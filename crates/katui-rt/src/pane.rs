//! Where our multiplexer pane stands. A multiplexer drops graphics from
//! panes that are unfocused or out of view, so an application pauses
//! there and sends everything again when the pane comes back.

use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

const POLL: Duration = Duration::from_millis(200);
const FORMAT: &str = "#{session_attached} #{window_active} #{pane_active} #{window_zoomed_flag}";

/// What a multiplexer lets a pane show.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaneView {
    /// On screen and focused: graphics reach the terminal.
    Active,
    /// On screen but unfocused: the multiplexer drops graphics, so what
    /// the terminal shows stays as it was.
    Frozen,
    /// Off screen: another window or a zoomed sibling.
    Hidden,
    /// No client attached: the multiplexer still keeps every cell we
    /// write, so the loop keeps drawing, without waiting for answers that
    /// no terminal sends.
    Detached,
}

impl PaneView {
    /// Whether the loop draws while the pane stands here.
    #[must_use]
    pub fn draws(self) -> bool {
        matches!(self, Self::Active | Self::Detached)
    }
}

/// Report through `report` each time multiplexer `program` changes what
/// our pane can show, until `stop` is set or `report` returns `false`.
pub(crate) fn watch(
    program: String,
    stop: Arc<AtomicBool>,
    report: impl Fn(PaneView) -> bool + Send + 'static,
) {
    std::thread::spawn(move || {
        let mut shown = PaneView::Active;
        while !stop.load(Ordering::Relaxed) {
            std::thread::sleep(POLL);
            let Some(view) = query(&program) else {
                continue;
            };
            if view == shown {
                continue;
            }
            shown = view;
            if !report(view) {
                return;
            }
        }
    });
}

fn query(program: &str) -> Option<PaneView> {
    let pane = std::env::var("TMUX_PANE").ok()?;
    let output = Command::new(program)
        .args(["display-message", "-p", "-t", &pane, FORMAT])
        .output()
        .ok()?;
    parse(std::str::from_utf8(&output.stdout).ok()?)
}

fn parse(fields: &str) -> Option<PaneView> {
    let mut values = fields.split_whitespace();
    let mut flag = || values.next()?.parse::<u32>().ok();
    let attached = flag()?;
    let window_active = flag()?;
    let pane_active = flag()?;
    let zoomed = flag()?;
    Some(if attached == 0 {
        PaneView::Detached
    } else if window_active == 0 || (zoomed == 1 && pane_active == 0) {
        PaneView::Hidden
    } else if pane_active == 0 {
        PaneView::Frozen
    } else {
        PaneView::Active
    })
}

#[cfg(test)]
mod tests {
    use super::{PaneView, parse};

    #[test]
    fn a_pane_view_follows_focus_window_and_zoom() {
        assert_eq!(parse("1 1 1 0\n"), Some(PaneView::Active));
        assert_eq!(parse("1 1 0 0"), Some(PaneView::Frozen));
        assert_eq!(parse("1 0 1 0"), Some(PaneView::Hidden));
        assert_eq!(parse("1 1 0 1"), Some(PaneView::Hidden));
        assert_eq!(parse("1 1 1 1"), Some(PaneView::Active));
        assert_eq!(parse("0 1 1 0"), Some(PaneView::Detached));
        assert_eq!(parse("error"), None);
    }
}
