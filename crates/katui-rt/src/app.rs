//! The interactive event loop: redraws when something changed and
//! sleeps otherwise, on the alternate screen with the mouse enabled.

use std::io;
use std::time::{Duration, Instant};

use katui_present::session;
use katui_proto::input::InputEvent;
use katui_proto::probe::Capabilities;
use katui_scene::CellSize;

use crate::RtError;
use crate::events::{EventPump, Signal, Waker};
use crate::pacing::{Pacer, Pacing};
use crate::pane::PaneView;
use crate::session::{ResizeEvent, ResizeWatch, Teardown};
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

    /// How the last frame that wrote bytes went, for an app that sizes its
    /// frames to what the terminal keeps up with.
    fn delivered(&mut self, _delivery: &Delivery) {}
}

/// How one frame reached the terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Delivery {
    /// The bytes the frame wrote.
    pub bytes: usize,
    /// How long [`App::draw`] took to encode it.
    pub drawn: Duration,
    /// From the start of the draw until the terminal took the whole frame:
    /// until it answered our pacing query, or until the bytes were written
    /// on a terminal that answers none.
    pub delivered: Duration,
    /// Whether the terminal answered, so `delivered` covers its own work.
    pub answered: bool,
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
        pacer: Pacer::new(pacing, caps.multiplexed(), caps.id_space),
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
    Teardown {
        graphics: true,
        ..Teardown::default()
    }
    .finish(out)?;
    session::disable_input(out)?;
    session::leave(out)?;
    out.flush()
}

/// Whether `event` answers one of our pacing queries, which belongs to
/// the loop and never reaches the app.
pub(crate) fn is_terminal_answer(event: &InputEvent) -> bool {
    matches!(
        event,
        InputEvent::GraphicsAnswer(_) | InputEvent::DeviceAttributes
    )
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
            let wants_frame =
                matches!(self.pending, Control::Redraw | Control::Animate) && self.pane.draws();
            if self.pending != Control::Quit && wants_frame && Instant::now() >= self.next_draw {
                self.draw(app, out, pump)?;
            }
        }
        Ok(())
    }

    fn wait(&self) -> Duration {
        if !self.pane.draws() {
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
        // we stop draining after one frame time, and at once on a quit, so a
        // waker that sends faster than the app handles can never starve a frame
        let until = Instant::now() + self.frame_time;
        let mut signal = first;
        while let Some(current) = signal {
            match current {
                Signal::Input(event) if is_terminal_answer(&event) => {}
                Signal::Input(event) => self.apply(app, AppEvent::Input(event)),
                Signal::Message(message) => self.apply(app, AppEvent::Message(message)),
                Signal::Pane(view) => self.on_pane(app, view),
            }
            if self.pending == Control::Quit || Instant::now() >= until {
                break;
            }
            signal = pump.next(Duration::ZERO);
        }
    }

    /// The terminal lost what we drew while our pane was out of view, so we
    /// send everything again when it comes back.
    fn on_pane<A: App>(&mut self, app: &mut A, view: PaneView) {
        self.pane = view;
        match view {
            PaneView::Active => {
                self.pacer.restore();
                self.apply(app, AppEvent::Repaint);
            }
            PaneView::Detached => self.pacer.stop_asking(),
            PaneView::Frozen | PaneView::Hidden => {}
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
        let started = Instant::now();
        self.next_draw = started + self.frame_time;
        self.wire.clear();
        let after = app.draw(&mut self.wire, self.size)?;
        let drawn = started.elapsed();
        self.pending = after;
        let asks = self.pacer.asks();
        let answered = self.pacer.deliver(out, &self.wire, self.size, pump)?;
        if !self.wire.is_empty() {
            app.delivered(&Delivery {
                bytes: self.wire.len(),
                drawn,
                delivered: started.elapsed(),
                answered: asks && answered,
            });
        }
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
            pacer: Pacer::new(Pacing::Timer, false, katui_proto::kitty::IdSpace::default()),
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

    struct Counter(u32);

    impl super::App for Counter {
        type Message = ();

        fn start(&mut self, _waker: crate::events::Waker<()>) {}

        fn update(&mut self, _event: super::AppEvent<()>) -> Control {
            self.0 += 1;
            if self.0 == 3 {
                Control::Quit
            } else {
                Control::Redraw
            }
        }

        fn draw(&mut self, _out: &mut Vec<u8>, _size: CellSize) -> std::io::Result<Control> {
            Ok(Control::Wait)
        }
    }

    #[test]
    fn a_flood_of_messages_stops_draining_at_a_quit() {
        let mut pump = crate::events::EventPump::start();
        let waker = pump.waker();
        for _ in 0..1000 {
            waker.send(());
        }
        let mut state = event_loop(Control::Wait);
        let mut app = Counter(0);
        let first = pump.next(Duration::from_secs(1));
        state.handle_signals(&mut app, first, &mut pump);
        assert_eq!(state.pending, Control::Quit);
        assert_eq!(
            app.0, 3,
            "the loop stops at the quit, the rest stays queued"
        );
        pump.finish();
    }

    #[test]
    fn an_idle_loop_only_polls() {
        let state = event_loop(Control::Wait);
        assert_eq!(state.wait(), super::IDLE_POLL);
    }
}
