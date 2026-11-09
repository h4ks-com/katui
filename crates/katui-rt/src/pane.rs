//! Where our multiplexer pane stands. The terminal knows nothing of
//! multiplexer windows, and a multiplexer drops graphics from panes that
//! are unfocused or out of view, so an application has to watch for both.

use std::fs::{File, OpenOptions};
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

const POLL: Duration = Duration::from_millis(200);
const FORMAT: &str =
    "#{session_attached} #{window_active} #{pane_active} #{window_zoomed_flag} #{client_tty}";

/// What a multiplexer lets a pane show.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaneView {
    /// On screen and focused: graphics reach the terminal.
    Active,
    /// On screen but unfocused: the multiplexer drops graphics, so what
    /// the terminal shows stays as it was.
    Frozen,
    /// Off screen: another window, a zoomed sibling, or no client.
    Hidden,
}

/// A change in what our pane can show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaneChange {
    /// What the pane can show now.
    pub view: PaneView,
    client_tty: Option<String>,
}

impl PaneChange {
    /// The terminal showing our session, opened for writing. A hidden
    /// pane reaches the terminal only this way, because the multiplexer
    /// drops what it sends.
    #[must_use]
    pub fn open_client(&self) -> Option<File> {
        OpenOptions::new()
            .write(true)
            .open(self.client_tty.as_ref()?)
            .ok()
    }
}

/// Report through `report` each time multiplexer `program` changes what
/// our pane can show, until `stop` is set or `report` returns `false`.
pub(crate) fn watch(
    program: String,
    stop: Arc<AtomicBool>,
    report: impl Fn(PaneChange) -> bool + Send + 'static,
) {
    std::thread::spawn(move || {
        let mut shown = PaneView::Active;
        let mut client_tty = None;
        while !stop.load(Ordering::Relaxed) {
            std::thread::sleep(POLL);
            let Some(reading) = query(&program) else {
                continue;
            };
            // a session that loses its client reports no tty, so we keep
            // the last one to reach the terminal that still shows us
            client_tty = reading.client_tty.or(client_tty);
            if reading.view == shown {
                continue;
            }
            shown = reading.view;
            let change = PaneChange {
                view: shown,
                client_tty: client_tty.clone(),
            };
            if !report(change) {
                return;
            }
        }
    });
}

fn query(program: &str) -> Option<Reading> {
    let pane = std::env::var("TMUX_PANE").ok()?;
    let output = Command::new(program)
        .args(["display-message", "-p", "-t", &pane, FORMAT])
        .output()
        .ok()?;
    parse(std::str::from_utf8(&output.stdout).ok()?)
}

#[derive(Debug, PartialEq, Eq)]
struct Reading {
    view: PaneView,
    client_tty: Option<String>,
}

fn parse(fields: &str) -> Option<Reading> {
    let mut values = fields.split_whitespace();
    let mut flag = || values.next()?.parse::<u32>().ok();
    let attached = flag()?;
    let window_active = flag()?;
    let pane_active = flag()?;
    let zoomed = flag()?;
    let view = if attached == 0 || window_active == 0 || (zoomed == 1 && pane_active == 0) {
        PaneView::Hidden
    } else if pane_active == 0 {
        PaneView::Frozen
    } else {
        PaneView::Active
    };
    Some(Reading {
        view,
        client_tty: values.next().map(str::to_owned),
    })
}

#[cfg(test)]
mod tests {
    use super::{PaneView, Reading, parse};

    fn view(fields: &str) -> Option<PaneView> {
        parse(fields).map(|reading| reading.view)
    }

    #[test]
    fn a_pane_view_follows_focus_window_and_zoom() {
        assert_eq!(view("1 1 1 0 /dev/ttys1\n"), Some(PaneView::Active));
        assert_eq!(view("1 1 0 0 /dev/ttys1"), Some(PaneView::Frozen));
        assert_eq!(view("1 0 1 0 /dev/ttys1"), Some(PaneView::Hidden));
        assert_eq!(view("1 1 0 1 /dev/ttys1"), Some(PaneView::Hidden));
        assert_eq!(view("1 1 1 1 /dev/ttys1"), Some(PaneView::Active));
        assert_eq!(view("error"), None);
    }

    #[test]
    fn a_session_with_no_client_reports_no_tty() {
        assert_eq!(
            parse("0 1 1 0 \n"),
            Some(Reading {
                view: PaneView::Hidden,
                client_tty: None,
            })
        );
    }
}
