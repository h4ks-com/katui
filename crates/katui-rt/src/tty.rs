//! The controlling-terminal transport: the probe's read and write side,
//! in raw mode for the exchange and restored after it.

use std::time::{Duration, Instant};

use katui_proto::kitty::IdSpace;
use katui_proto::probe::CellPixels;
use katui_proto::probe::{Capabilities, Env};
use katui_proto::probe::{Transport, probe, sniff};

#[cfg(unix)]
mod unix;
#[cfg(unix)]
use unix::measured_cell;
#[cfg(unix)]
pub use unix::{RawInput, TtyTransport, size};

#[cfg(windows)]
mod windows;
#[cfg(windows)]
use windows::measured_cell;
#[cfg(windows)]
pub use windows::{RawInput, TtyTransport, size};

/// The end of a listening budget that starts now.
fn deadline_after(budget: Duration) -> Instant {
    // a budget overflowing the clock means a limit nobody reaches
    Instant::now()
        .checked_add(budget)
        .unwrap_or_else(|| Instant::now() + Duration::from_secs(86_400 * 365))
}

/// The probe every demo and binary runs over the controlling terminal,
/// falling back to the environment sniff when no terminal answers.
#[must_use]
pub fn probe_default() -> Capabilities {
    let mut caps = probe_terminal();
    if caps.multiplexed() {
        caps.id_space = IdSpace::of_process(std::process::id());
    }
    caps
}

fn probe_terminal() -> Capabilities {
    let env = Env::capture();
    let caps = probe_once(&env, Duration::from_millis(600));
    if worth_retrying(&env, &caps) {
        let retried = probe_once(&env, Duration::from_millis(1200));
        if retried.kitty {
            return retried;
        }
    }
    caps
}

/// A multiplexer inside kitty sometimes relays kitty's answer too late for
/// the first budget, and losing it drops a kitty terminal to blocks, so we
/// ask once more with a longer one.
fn worth_retrying(env: &Env, caps: &Capabilities) -> bool {
    env.inside_multiplexer()
        && env.launched_from_kitty()
        && env.forced_graphics().is_none()
        && !caps.kitty
}

fn probe_once(env: &Env, budget: Duration) -> Capabilities {
    match TtyTransport::open(budget) {
        Ok(mut transport) => {
            let mut caps = probe(&mut transport, env);
            caps.local_files = crate::stash::reads_local_files(&mut transport, &caps);
            // answers racing the budget boundary would otherwise leak
            // into the parent shell as typed junk; spend a quiet window
            // draining whatever still arrives, and drop's input flush
            // then clears the rest
            let until = Instant::now() + Duration::from_millis(200);
            while Instant::now() < until {
                let _ = transport.read_timeout(Duration::from_millis(50));
            }
            caps
        }
        Err(_) => sniff(env),
    }
}

/// The pixel size of one cell: what the terminal driver reports now,
/// else what the probe answered, else the probed text area over the cell
/// count, else a typical cell. The driver comes first because it follows
/// a font zoom, which changes the cell after the probe. A multiplexer
/// leaves all of them unanswered at times, and a placement's cell span
/// scales whatever we choose, so the typical cell is a safe guess.
#[must_use]
pub fn cell_size(caps: &Capabilities) -> CellPixels {
    pick_cell(measured_cell(), caps.cell, caps.text_area.zip(size().ok()))
}

/// Device pixels per point. No terminal reports it, so we guess from the
/// cell the probe reported: one 30 pixels tall or more is a double
/// density screen.
#[must_use]
pub fn screen_scale(caps: &Capabilities) -> f32 {
    match caps.cell {
        Some(cell) if cell.height >= 30 => 2.0,
        _ => 1.0,
    }
}

fn pick_cell(
    measured: Option<CellPixels>,
    probed: Option<CellPixels>,
    area: Option<(CellPixels, katui_scene::CellSize)>,
) -> CellPixels {
    measured
        .or(probed)
        .or_else(|| area.and_then(|(area, cells)| cell_from_area(area, cells)))
        .unwrap_or(CellPixels {
            width: 10,
            height: 20,
        })
}

fn cell_from_area(area: CellPixels, cells: katui_scene::CellSize) -> Option<CellPixels> {
    let width = area.width.checked_div(cells.columns)?;
    let height = area.height.checked_div(cells.rows)?;
    (width > 0 && height > 0).then_some(CellPixels { width, height })
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use katui_proto::probe::CellPixels;
    use katui_scene::CellSize;

    use super::TtyTransport;

    #[test]
    fn a_text_area_over_its_cells_gives_the_cell() {
        let area = CellPixels {
            width: 560,
            height: 700,
        };
        let cells = CellSize {
            columns: 80,
            rows: 50,
        };
        assert_eq!(
            super::cell_from_area(area, cells),
            Some(CellPixels {
                width: 7,
                height: 14
            })
        );
        let empty = CellSize {
            columns: 0,
            rows: 0,
        };
        assert_eq!(super::cell_from_area(area, empty), None);
    }

    #[test]
    fn a_tall_reported_cell_means_a_double_density_screen() {
        let reported = |height| katui_proto::probe::Capabilities {
            cell: Some(CellPixels { width: 7, height }),
            ..katui_proto::probe::Capabilities::default()
        };
        assert!((super::screen_scale(&reported(35)) - 2.0).abs() < f32::EPSILON);
        assert!((super::screen_scale(&reported(30)) - 2.0).abs() < f32::EPSILON);
        assert!((super::screen_scale(&reported(17)) - 1.0).abs() < f32::EPSILON);
        let unknown = katui_proto::probe::Capabilities::default();
        assert!((super::screen_scale(&unknown) - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn opening_the_controlling_terminal_never_panics() {
        // a controlling terminal in the test environment is fine too: the
        // point is that open never panics, and both arms restore state
        if TtyTransport::open(Duration::from_millis(1)).is_ok() {}
    }

    #[test]
    fn the_driver_cell_wins_then_the_probe_then_the_area() {
        let cell = |width, height| CellPixels { width, height };
        let cells = CellSize {
            columns: 10,
            rows: 10,
        };
        let area = Some((cell(90, 180), cells));
        assert_eq!(
            super::pick_cell(Some(cell(8, 16)), Some(cell(7, 14)), area),
            cell(8, 16)
        );
        assert_eq!(super::pick_cell(None, Some(cell(7, 14)), area), cell(7, 14));
        assert_eq!(super::pick_cell(None, None, area), cell(9, 18));
        assert_eq!(super::pick_cell(None, None, None), cell(10, 20));
    }

    #[test]
    fn only_a_kitty_launched_multiplexer_without_graphics_retries() {
        use katui_proto::probe::{Capabilities, Env};

        let tmux_in_kitty = Env {
            tmux: Some("pane".to_owned()),
            kitty_window_id: Some("1".to_owned()),
            ..Env::default()
        };
        let lost = Capabilities::default();
        assert!(super::worth_retrying(&tmux_in_kitty, &lost));
        let found = Capabilities {
            kitty: true,
            ..Capabilities::default()
        };
        assert!(!super::worth_retrying(&tmux_in_kitty, &found));
        let forced = Env {
            graphics: Some("blocks".to_owned()),
            ..tmux_in_kitty.clone()
        };
        assert!(!super::worth_retrying(&forced, &lost));
        let bare_kitty = Env {
            tmux: None,
            ..tmux_in_kitty
        };
        assert!(!super::worth_retrying(&bare_kitty, &lost));
    }
}
