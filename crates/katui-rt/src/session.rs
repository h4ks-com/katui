//! The alternate-screen session guard: enter on construction, leave on
//! drop, including unwinds. A session owns its writer; drawing through
//! katui happens beside it, on the same stdout.

use std::io;
#[cfg(unix)]
use std::sync::Arc;
#[cfg(unix)]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(unix)]
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use katui_present::cells;
use katui_present::session;
use katui_proto::kitty;
use katui_proto::probe::Capabilities;

use crate::RtError;

/// An alternate-screen session. Drop leaves the screen, whatever ended
/// it: normal return, error, panic.
pub struct Session<W: io::Write> {
    out: Option<W>,
}

impl<W: io::Write> Session<W> {
    /// Enter the alternate screen on `out`.
    pub fn enter(out: W) -> Result<Self, RtError> {
        let mut session = Self { out: Some(out) };
        if let Some(out) = session.out.as_mut() {
            session::enter(out)?;
        }
        Ok(session)
    }

    /// Leave the screen and return the writer.
    pub fn leave(mut self) -> io::Result<W> {
        let out = self.out.take();
        let Some(mut out) = out else {
            unreachable!("only leave takes the writer and it runs once");
        };
        session::leave(&mut out)?;
        Ok(out)
    }
}

impl<W: io::Write> Drop for Session<W> {
    fn drop(&mut self) {
        // drop cannot report, so the leave is best effort
        if let Some(out) = &mut self.out {
            let _ = session::leave(out);
        }
    }
}

/// What a [`Stage`] run owes the terminal when it ends: which teardown
/// pieces to send after the always-on resets.
#[derive(Debug, Clone, Copy, Default)]
pub struct Teardown {
    /// Delete every visible kitty image; runs that transmitted any.
    pub graphics: bool,
    /// Home the cursor; runs that kept painting the screen.
    pub home: bool,
    /// Clear the whole screen after homing.
    pub clear: bool,
}

impl Teardown {
    /// Send the restore sequence: leave sync mode, reset styles, show
    /// the cursor, then whatever this teardown still owes.
    fn finish<W: io::Write>(self, out: &mut W) -> io::Result<()> {
        out.write_all(b"\x1b[?2026l\x1b[0m")?;
        cells::show_cursor(out)?;
        if self.graphics {
            out.write_all(&kitty::Delete::AllVisible.encode())?;
        }
        if self.home {
            out.write_all(b"\x1b[H")?;
        }
        if self.clear {
            out.write_all(b"\x1b[2J")?;
        }
        out.flush()
    }
}

/// The main-screen stage: hides the cursor on enter and restores the
/// terminal on [`Stage::leave`] or drop. Unlike [`Session`] it never
/// switches to the alternate screen: a run draws beside it, through
/// [`Stage::writer`], on the same stdout.
pub struct Stage<W: io::Write> {
    out: Option<W>,
    teardown: Teardown,
    pacing: Pacing,
}

/// How a run keeps its output in step with the terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Pacing {
    /// The frame clock alone, for terminals that answer no queries.
    #[default]
    Timer,
    /// Wait for the terminal after every frame.
    EachFrame,
    /// Wait after every batch of commands, sized for a multiplexer.
    Batches,
}

/// A batch size well inside what a multiplexer buffers: rmux 0.10 drops
/// graphics commands past about 64 KB in flight, and 32 KB batches still
/// lost the odd tile.
#[cfg(unix)]
const MULTIPLEXER_BATCH: usize = 16 * 1024;

impl Pacing {
    /// The pacing `caps` can support.
    #[must_use]
    pub fn for_terminal(caps: &Capabilities) -> Self {
        match (caps.kitty, caps.multiplexed()) {
            (false, _) => Self::Timer,
            (true, false) => Self::EachFrame,
            (true, true) => Self::Batches,
        }
    }
}

/// What a draw callback knows about its frame.
#[derive(Debug, Clone, Copy)]
pub struct FrameContext {
    /// Frames drawn before this one.
    pub number: u64,
    /// The terminal's current size, when it can be read.
    pub size: Option<katui_scene::CellSize>,
}

/// What a [`ResizeWatch`] noticed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResizeEvent {
    /// The terminal has a new size; rebuild for it.
    Resized(katui_scene::CellSize),
    /// The size has held for a while; repaint, because the terminal may
    /// have dropped images while it was resizing.
    Settled,
}

/// How long after the last size change a watch asks for each repaint.
/// Kitty debounces its own resize for up to half a second, so the second
/// repaint lands after that.
const SETTLE_REPAINTS: [Duration; 2] = [Duration::from_millis(250), Duration::from_millis(1000)];

/// Turns the per-frame terminal size into resize and settle events.
#[derive(Debug)]
pub struct ResizeWatch {
    size: katui_scene::CellSize,
    changed_at: Option<Instant>,
    repaints_done: usize,
}

impl ResizeWatch {
    /// A watch starting from `size`.
    #[must_use]
    pub fn new(size: katui_scene::CellSize) -> Self {
        Self {
            size,
            changed_at: None,
            repaints_done: 0,
        }
    }

    /// Report what changed given the size read at `at`.
    pub fn observe(
        &mut self,
        size: Option<katui_scene::CellSize>,
        at: Instant,
    ) -> Option<ResizeEvent> {
        if let Some(new_size) = size
            && (new_size.columns, new_size.rows) != (self.size.columns, self.size.rows)
            && new_size.columns > 0
            && new_size.rows > 0
        {
            self.size = new_size;
            self.changed_at = Some(at);
            self.repaints_done = 0;
            return Some(ResizeEvent::Resized(new_size));
        }
        let waited = at.duration_since(self.changed_at?);
        let due = *SETTLE_REPAINTS.get(self.repaints_done)?;
        if waited < due {
            return None;
        }
        self.repaints_done += 1;
        Some(ResizeEvent::Settled)
    }
}

/// The first graphics query id a paced run waits on; each query takes
/// its own id, so an extra answer from a second attached client can
/// never stand in for the one being waited on.
#[cfg(unix)]
const PACING_IMAGES_FROM: u32 = 1_000_000;

/// How long a paced run keeps listening for answers still on their way
/// when it ends, and the silence that ends the listening sooner.
#[cfg(unix)]
const LATE_ANSWER_LIMIT: Duration = Duration::from_millis(500);
#[cfg(unix)]
const LATE_ANSWER_SILENCE: Duration = Duration::from_millis(120);

/// How long a paced run waits for one answer before it stops asking.
#[cfg(unix)]
const PACING_WAIT: Duration = Duration::from_millis(500);

/// The frames and timings of one run.
#[derive(Debug, Clone, Default)]
pub struct RunStats {
    frame_times: Vec<Duration>,
    elapsed: Duration,
    terminal_paced: bool,
    pacing_lost_at: Option<usize>,
}

impl RunStats {
    /// Frames completed.
    #[must_use]
    pub fn frames(&self) -> usize {
        self.frame_times.len()
    }

    /// Frames completed per second over the whole run.
    #[must_use]
    pub fn frames_per_second(&self) -> f64 {
        let frames = u32::try_from(self.frames()).unwrap_or(u32::MAX);
        f64::from(frames) / self.elapsed.as_secs_f64().max(f64::EPSILON)
    }

    fn percentile_ms(&self, percent: usize) -> f64 {
        let mut sorted = self.frame_times.clone();
        sorted.sort_unstable();
        let index = (sorted.len() * percent / 100).min(sorted.len().saturating_sub(1));
        sorted
            .get(index)
            .map_or(0.0, |time| time.as_secs_f64() * 1000.0)
    }
}

impl std::fmt::Display for RunStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} frames, {:.1} fps, frame work p50 {:.1}ms p95 {:.1}ms, terminal pacing {}",
            self.frames(),
            self.frames_per_second(),
            self.percentile_ms(50),
            self.percentile_ms(95),
            if self.terminal_paced { "on" } else { "off" },
        )?;
        if let Some(frame) = self.pacing_lost_at {
            write!(f, " (no answer at frame {frame})")?;
        }
        Ok(())
    }
}

impl<W: io::Write> Stage<W> {
    /// Hide the cursor and hold `out` for the run.
    pub fn enter(mut out: W, teardown: Teardown) -> Result<Self, RtError> {
        cells::hide_cursor(&mut out)?;
        Ok(Self {
            out: Some(out),
            teardown,
            pacing: Pacing::Timer,
        })
    }

    /// Set how [`Stage::run`] waits for the terminal. Frames then never
    /// queue up behind a slow terminal, so input stays responsive.
    #[must_use]
    pub fn paced(mut self, pacing: Pacing) -> Self {
        self.pacing = pacing;
        self
    }

    /// The writer the run draws through.
    pub fn writer(&mut self) -> &mut W {
        let Some(out) = self.out.as_mut() else {
            unreachable!("only leave takes the writer and it runs once");
        };
        out
    }

    /// Restore the terminal and return the writer.
    pub fn leave(mut self) -> io::Result<W> {
        let teardown = self.teardown;
        let Some(mut out) = self.out.take() else {
            unreachable!("only leave takes the writer and it runs once");
        };
        teardown.finish(&mut out)?;
        Ok(out)
    }

    /// Draw at `frames_per_second` for at most `frames`, stopping early
    /// when `draw` returns `false` or a quit key (`q`, `Esc`, `Ctrl-C`)
    /// lands on its own thread. The stage is restored on every exit
    /// path and the drawing error propagates.
    #[cfg(unix)] // the quit-key reader rides the unix raw-input guard
    pub fn run<F>(self, frames_per_second: u32, frames: u64, mut draw: F) -> io::Result<RunStats>
    where
        F: FnMut(&mut Vec<u8>, FrameContext) -> io::Result<bool>,
    {
        let running = Arc::new(AtomicBool::new(true));
        let stop = Arc::new(AtomicBool::new(false));
        let (answer_sender, answers) = mpsc::channel();
        let reader = crate::tty::spawn_input_reader(running.clone(), stop.clone(), answer_sender);
        let mut pacing = if reader.is_some() {
            self.pacing
        } else {
            Pacing::Timer
        };
        let mut clock = crate::clock::FrameClock::new(frames_per_second);
        let mut stage = self;
        let mut stats = RunStats {
            terminal_paced: pacing != Pacing::Timer,
            ..RunStats::default()
        };
        let mut wire = Vec::new();
        let listens_for_answers = pacing != Pacing::Timer;
        let mut next_query = PACING_IMAGES_FROM;
        let started = Instant::now();
        let result = (|| {
            for frame in 0..frames {
                if !running.load(Ordering::Relaxed) {
                    break;
                }
                let began = Instant::now();
                wire.clear();
                let context = FrameContext {
                    number: frame,
                    size: crate::tty::size().ok(),
                };
                let keep_going = draw(&mut wire, context)?;
                if !deliver(stage.writer(), &wire, pacing, &answers, &mut next_query)? {
                    pacing = Pacing::Timer;
                    stats.terminal_paced = false;
                    stats.pacing_lost_at = Some(stats.frames());
                }
                stats.frame_times.push(began.elapsed());
                if !keep_going {
                    break;
                }
                clock.wait_for_next_frame();
            }
            Ok(())
        })();
        stats.elapsed = started.elapsed();
        running.store(false, Ordering::Relaxed);
        if listens_for_answers {
            drain_late_answers(&answers);
        }
        stop.store(true, Ordering::Relaxed);
        if let Some(handle) = reader {
            let _ = handle.join();
        }
        // the stage restores the terminal whatever ended the run; the
        // original error still propagates
        let cleanup = stage.leave().map(|_| ());
        result.and(cleanup).map(|()| stats)
    }
}

/// Write one frame under `pacing`; `false` once the terminal stops
/// answering, with the frame still written whole.
#[cfg(unix)]
fn deliver<W: io::Write>(
    out: &mut W,
    frame: &[u8],
    pacing: Pacing,
    answers: &Receiver<u32>,
    next_query: &mut u32,
) -> io::Result<bool> {
    if frame.is_empty() {
        return Ok(true);
    }
    let parts = match pacing {
        Pacing::Timer => {
            out.write_all(frame)?;
            out.flush()?;
            return Ok(true);
        }
        Pacing::EachFrame => vec![frame],
        Pacing::Batches => kitty::split_at_commands(frame, MULTIPLEXER_BATCH),
    };
    let mut answering = true;
    for part in parts {
        out.write_all(part)?;
        if answering {
            let query = *next_query;
            *next_query = next_query.wrapping_add(1).max(PACING_IMAGES_FROM);
            session::terminal_query(out, query)?;
            out.flush()?;
            answering = await_terminal(answers, query);
        } else {
            out.flush()?;
        }
    }
    Ok(answering)
}

/// Keep raw mode up until the answers still travelling have arrived, so
/// none of them reach the shell.
#[cfg(unix)]
fn drain_late_answers(answers: &Receiver<u32>) {
    let deadline = Instant::now() + LATE_ANSWER_LIMIT;
    while Instant::now() < deadline && answers.recv_timeout(LATE_ANSWER_SILENCE).is_ok() {}
}

/// Wait for the answer to `query`; `false` once the terminal stops
/// answering.
#[cfg(unix)]
fn await_terminal(answers: &Receiver<u32>, query: u32) -> bool {
    while let Ok(image) = answers.recv_timeout(PACING_WAIT) {
        if image == query {
            return true;
        }
    }
    false
}

impl<W: io::Write> Drop for Stage<W> {
    fn drop(&mut self) {
        // drop cannot report, so the restore is best effort; it only
        // runs when leave never did, on a panic for example
        if let Some(out) = &mut self.out {
            let _ = self.teardown.finish(out);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io;
    use std::io::Write as _;

    use std::time::{Duration, Instant};

    use katui_proto::probe::{Capabilities, Identity};

    use super::{ResizeEvent, ResizeWatch, Session, Stage, Teardown};
    /// A writer that records into shared storage, so bytes survive the
    /// session's drop.
    #[derive(Clone)]
    struct Recorder(std::rc::Rc<std::cell::RefCell<Vec<u8>>>);

    impl io::Write for Recorder {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.0.borrow_mut().extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn a_session_leaves_on_drop() {
        let log = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let session = Session::enter(Recorder(log.clone())).expect("recorder cannot fail");
        drop(session);
        assert_eq!(
            String::from_utf8_lossy(&log.borrow()),
            "\x1b[?1049h\x1b[?25l\x1b[?25h\x1b[?1049l",
            "drop leaves the screen"
        );
    }

    #[test]
    fn leave_after_use_returns_the_writer_with_the_leave_bytes() {
        let session = Session::enter(Vec::new()).expect("writing to a Vec cannot fail");
        let bytes = session.leave().expect("writing to a Vec cannot fail");
        assert_eq!(
            String::from_utf8_lossy(&bytes),
            "\x1b[?1049h\x1b[?25l\x1b[?25h\x1b[?1049l"
        );
    }

    #[test]
    fn a_failing_writer_fails_the_enter() {
        struct Broken;
        impl io::Write for Broken {
            fn write(&mut self, _: &[u8]) -> io::Result<usize> {
                Err(io::Error::other("broken"))
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        assert!(Session::enter(Broken).is_err());
    }

    #[test]
    fn a_full_stage_restores_everything_it_owes() {
        let stage = Stage::enter(
            Vec::new(),
            Teardown {
                graphics: true,
                home: true,
                clear: true,
            },
        )
        .expect("writing to a Vec cannot fail");
        let bytes = stage.leave().expect("writing to a Vec cannot fail");
        assert_eq!(
            String::from_utf8_lossy(&bytes),
            "\x1b[?25l\x1b[?2026l\x1b[0m\x1b[?25h\x1b_Ga=d,d=a,q=2\x1b\\\x1b[H\x1b[2J",
            "hide on enter; sync off, styles, cursor, graphics, home, clear on leave"
        );
    }

    #[test]
    fn a_bare_stage_only_resets_the_basics() {
        let stage =
            Stage::enter(Vec::new(), Teardown::default()).expect("writing to a Vec cannot fail");
        let bytes = stage.leave().expect("writing to a Vec cannot fail");
        assert_eq!(
            String::from_utf8_lossy(&bytes),
            "\x1b[?25l\x1b[?2026l\x1b[0m\x1b[?25h",
            "no graphics, home or clear owed"
        );
    }

    #[test]
    fn a_stage_left_unleft_still_restores_on_drop() {
        let log = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let stage =
            Stage::enter(Recorder(log.clone()), Teardown::default()).expect("recorder cannot fail");
        drop(stage);
        assert_eq!(
            String::from_utf8_lossy(&log.borrow()),
            "\x1b[?25l\x1b[?2026l\x1b[0m\x1b[?25h",
            "drop restores when leave never ran"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_run_draws_until_the_callback_stops() {
        let stage =
            Stage::enter(Vec::new(), Teardown::default()).expect("writing to a Vec cannot fail");
        let mut drawn = 0;
        let stats = stage
            .run(1000, 100, |out, frame| {
                drawn = frame.number + 1;
                out.write_all(b"x")?;
                Ok(frame.number < 3)
            })
            .expect("writing to a Vec cannot fail");
        assert_eq!(drawn, 4, "the callback's false ends the run");
        assert_eq!(stats.frames(), 4, "every drawn frame is counted");
        assert!(stats.frames_per_second() > 0.0);
    }

    fn size(columns: u16, rows: u16) -> katui_scene::CellSize {
        katui_scene::CellSize { columns, rows }
    }

    #[test]
    fn a_resize_reports_once_then_repaints_as_it_settles() {
        let start = Instant::now();
        let mut watch = ResizeWatch::new(size(80, 24));
        assert_eq!(watch.observe(Some(size(80, 24)), start), None);
        let later = |millis| start + Duration::from_millis(millis);
        assert_eq!(
            watch.observe(Some(size(100, 30)), start),
            Some(ResizeEvent::Resized(size(100, 30)))
        );
        assert_eq!(watch.observe(Some(size(100, 30)), later(100)), None);
        assert_eq!(
            watch.observe(Some(size(100, 30)), later(300)),
            Some(ResizeEvent::Settled)
        );
        assert_eq!(watch.observe(Some(size(100, 30)), later(400)), None);
        assert_eq!(
            watch.observe(Some(size(100, 30)), later(1100)),
            Some(ResizeEvent::Settled)
        );
        assert_eq!(watch.observe(Some(size(100, 30)), later(5000)), None);
    }

    #[test]
    fn a_new_resize_restarts_the_settling() {
        let start = Instant::now();
        let mut watch = ResizeWatch::new(size(80, 24));
        watch.observe(Some(size(100, 30)), start);
        let dragged = start + Duration::from_millis(200);
        watch.observe(Some(size(110, 30)), dragged);
        assert_eq!(
            watch.observe(Some(size(110, 30)), start + Duration::from_millis(300)),
            None,
            "the clock runs from the last change"
        );
        assert_eq!(
            watch.observe(Some(size(110, 30)), start + Duration::from_millis(500)),
            Some(ResizeEvent::Settled)
        );
    }

    #[test]
    fn a_zero_or_unreadable_size_is_ignored() {
        let mut watch = ResizeWatch::new(size(80, 24));
        let now = Instant::now();
        assert_eq!(watch.observe(None, now), None);
        assert_eq!(watch.observe(Some(size(0, 0)), now), None);
    }

    #[cfg(unix)]
    #[test]
    fn multiplexers_get_batches_and_plain_terminals_no_queries() {
        let plain = Capabilities::default();
        assert_eq!(super::Pacing::for_terminal(&plain), super::Pacing::Timer);
        let kitty = Capabilities {
            kitty: true,
            ..Capabilities::default()
        };
        assert_eq!(
            super::Pacing::for_terminal(&kitty),
            super::Pacing::EachFrame
        );
        let muxed = Capabilities {
            identity: Some(Identity {
                name: "rmux".to_owned(),
                version: None,
            }),
            ..kitty
        };
        assert!(matches!(
            super::Pacing::for_terminal(&muxed),
            super::Pacing::Batches
        ));
    }

    #[cfg(unix)]
    #[test]
    fn a_run_propagates_a_drawing_error_and_still_restores() {
        let stage =
            Stage::enter(Vec::new(), Teardown::default()).expect("writing to a Vec cannot fail");
        let result = stage.run(1000, 10, |_, _| Err(io::Error::other("broken")));
        assert!(result.is_err(), "the drawing error propagates");
    }
}
