//! The interactive event loop: redraws when something changed and
//! sleeps otherwise, on the alternate screen with the mouse enabled.

use std::io;
use std::time::{Duration, Instant};

use katui_present::{cells, session};
use katui_proto::input::InputEvent;
use katui_proto::kitty;
use katui_proto::probe::Capabilities;
use katui_scene::CellSize;

use crate::RtError;
use crate::events::{EventPump, Signal, Waker};
use crate::pacing::{Pacer, Pacing};
use crate::pane::PaneView;
use crate::session::{ResizeEvent, ResizeWatch};
use crate::stop::StopRequest;

/// How often an idle loop checks the terminal size.
const IDLE_POLL: Duration = Duration::from_millis(100);

/// What an [`App`] is told about.
#[derive(Debug, PartialEq, Eq)]
pub enum AppEvent<M> {
    /// Something the terminal reported.
    Input(InputEvent),
    /// A message from one of the application's threads.
    Message(M),
    /// The terminal has a new size.
    Resized(CellSize),
    /// The terminal may have lost what we showed: a resize has settled,
    /// or our multiplexer pane is back in view after the loop paused its
    /// drawing. Everything on screen needs sending again.
    Repaint,
}

/// What the loop does next.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Control {
    /// Sleep until something happens.
    Wait,
    /// Draw once.
    Redraw,
    /// Keep drawing at the frame rate.
    Animate,
    /// End the loop.
    Quit,
}

/// An interactive terminal application.
pub trait App {
    /// What the application's own threads send to it.
    type Message: Send + 'static;

    /// Called once before the first draw, with the handle for queuing
    /// messages from other threads.
    fn start(&mut self, waker: Waker<Self::Message>);

    /// React to one event.
    fn update(&mut self, event: AppEvent<Self::Message>) -> Control;

    /// Encode the current state as terminal bytes into `out`.
    fn draw(&mut self, out: &mut Vec<u8>, size: CellSize) -> io::Result<Control>;
}

/// Run `app` on `out` until it asks to quit, drawing at most
/// `frames_per_second` times a second. The terminal is restored on every
/// exit path that returns.
pub fn run_app<A: App, W: io::Write>(
    app: &mut A,
    mut out: W,
    caps: &Capabilities,
    frames_per_second: u32,
) -> Result<(), RtError> {
    let mut pump = EventPump::start();
    let pacing = if pump.reads_terminal() {
        Pacing::for_terminal(caps)
    } else {
        Pacing::Timer
    };
    if let Some(identity) = caps.identity.as_ref().filter(|_| caps.multiplexed()) {
        pump.watch_pane_visibility(&identity.name);
    }
    let size = crate::tty::size()?;
    let stop = StopRequest::watch()?;
    let terminal = RestoringTerminal { out: &mut out };
    session::enter(terminal.out)?;
    session::enable_input(terminal.out)?;
    terminal.out.flush()?;
    app.start(pump.waker());
    let mut event_loop = EventLoop {
        pacer: Pacer::new(pacing, caps.multiplexed()),
        resize: ResizeWatch::new(size),
        size,
        frame_time: Duration::from_secs_f64(1.0 / f64::from(frames_per_second.max(1))),
        next_draw: Instant::now(),
        pending: Control::Redraw,
        pane: PaneView::Active,
        wire: Vec::new(),
        stop,
    };
    let result = event_loop.run(app, terminal.out, &mut pump);
    // we stop mouse reports before leaving raw mode, or the ones already
    // on their way would reach the shell as typed junk
    session::disable_input(terminal.out)?;
    terminal.out.flush()?;
    pump.finish();
    drop(terminal);
    result
}

/// Restores the terminal when dropped, a panic in the application included.
struct RestoringTerminal<'a, W: io::Write> {
    out: &'a mut W,
}

impl<W: io::Write> Drop for RestoringTerminal<'_, W> {
    fn drop(&mut self) {
        // drop cannot report, so the restore is best effort
        let _ = restore(self.out);
    }
}

fn restore<W: io::Write>(out: &mut W) -> io::Result<()> {
    out.write_all(b"\x1b[?2026l\x1b[0m")?;
    out.write_all(&kitty::Delete::AllVisible.encode())?;
    session::disable_input(out)?;
    cells::show_cursor(out)?;
    session::leave(out)?;
    out.flush()
}

/// The most demanding of two requests made since the last frame.
pub(crate) fn merge(pending: Control, control: Control) -> Control {
    match (pending, control) {
        (Control::Quit, _) | (_, Control::Quit) => Control::Quit,
        (Control::Animate, _) | (_, Control::Animate) => Control::Animate,
        (Control::Redraw, _) | (_, Control::Redraw) => Control::Redraw,
        (Control::Wait, Control::Wait) => Control::Wait,
    }
}

struct EventLoop {
    pacer: Pacer,
    resize: ResizeWatch,
    size: CellSize,
    frame_time: Duration,
    next_draw: Instant,
    pending: Control,
    pane: PaneView,
    wire: Vec<u8>,
    stop: StopRequest,
}

impl EventLoop {
    fn run<A: App, W: io::Write>(
        &mut self,
        app: &mut A,
        out: &mut W,
        pump: &mut EventPump<A::Message>,
    ) -> Result<(), RtError> {
        while self.pending != Control::Quit && !self.stop.requested() {
            let first = pump.next(self.wait());
            self.handle_signals(app, first, pump);
            self.handle_resize(app);
            let wants_frame = matches!(self.pending, Control::Redraw | Control::Animate)
                && self.pane == PaneView::Active;
            if self.pending != Control::Quit && wants_frame && Instant::now() >= self.next_draw {
                self.draw(app, out, pump)?;
            }
        }
        Ok(())
    }

    fn wait(&self) -> Duration {
        if self.pane != PaneView::Active {
            return IDLE_POLL;
        }
        match self.pending {
            Control::Redraw | Control::Animate => self
                .next_draw
                .saturating_duration_since(Instant::now())
                .min(IDLE_POLL),
            Control::Wait | Control::Quit => IDLE_POLL,
        }
    }

    fn handle_signals<A: App>(
        &mut self,
        app: &mut A,
        first: Option<Signal<A::Message>>,
        pump: &mut EventPump<A::Message>,
    ) {
        let mut signal = first;
        while let Some(current) = signal {
            match current {
                Signal::Input(InputEvent::GraphicsAnswer(_)) => {}
                Signal::Input(event) => self.apply(app, AppEvent::Input(event)),
                Signal::Message(message) => self.apply(app, AppEvent::Message(message)),
                Signal::Pane(view) => {
                    self.pane = view;
                    if view == PaneView::Active {
                        self.pacer.restore();
                        self.apply(app, AppEvent::Repaint);
                    }
                }
            }
            signal = pump.next(Duration::ZERO);
        }
    }

    fn handle_resize<A: App>(&mut self, app: &mut A) {
        match self.resize.observe(crate::tty::size().ok(), Instant::now()) {
            Some(ResizeEvent::Resized(size)) => {
                self.size = size;
                self.apply(app, AppEvent::Resized(size));
                self.merge(Control::Redraw);
            }
            Some(ResizeEvent::Settled) => {
                self.apply(app, AppEvent::Repaint);
                self.merge(Control::Redraw);
            }
            None => {}
        }
    }

    fn apply<A: App>(&mut self, app: &mut A, event: AppEvent<A::Message>) {
        let control = app.update(event);
        self.merge(control);
    }

    fn merge(&mut self, control: Control) {
        self.pending = merge(self.pending, control);
    }

    fn draw<A: App, W: io::Write>(
        &mut self,
        app: &mut A,
        out: &mut W,
        pump: &mut EventPump<A::Message>,
    ) -> Result<(), RtError> {
        self.next_draw = Instant::now() + self.frame_time;
        self.wire.clear();
        let after = app.draw(&mut self.wire, self.size)?;
        self.pending = after;
        self.pacer.deliver(out, &self.wire, self.size, pump)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use katui_scene::CellSize;

    use super::{Control, EventLoop};
    use crate::pacing::{Pacer, Pacing};
    use crate::session::ResizeWatch;

    fn event_loop(pending: Control) -> EventLoop {
        let size = CellSize {
            columns: 80,
            rows: 24,
        };
        EventLoop {
            pacer: Pacer::new(Pacing::Timer, false),
            resize: ResizeWatch::new(size),
            size,
            frame_time: Duration::from_millis(16),
            next_draw: Instant::now(),
            pending,
            pane: super::PaneView::Active,
            wire: Vec::new(),
            stop: crate::stop::StopRequest::default(),
        }
    }

    #[test]
    fn the_most_demanding_request_wins() {
        let mut state = event_loop(Control::Wait);
        state.merge(Control::Redraw);
        assert_eq!(state.pending, Control::Redraw);
        state.merge(Control::Wait);
        assert_eq!(state.pending, Control::Redraw, "a redraw is never lost");
        state.merge(Control::Animate);
        assert_eq!(state.pending, Control::Animate);
        state.merge(Control::Quit);
        state.merge(Control::Redraw);
        assert_eq!(state.pending, Control::Quit, "a quit is never undone");
    }

    #[test]
    fn an_idle_loop_only_polls() {
        let state = event_loop(Control::Wait);
        assert_eq!(state.wait(), super::IDLE_POLL);
    }
}
