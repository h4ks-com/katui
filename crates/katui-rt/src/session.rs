//! The alternate-screen session guard: enter on construction, leave on
//! drop, including unwinds. A session owns its writer; drawing through
//! katui happens beside it, on the same stdout.

use std::io;
use std::time::{Duration, Instant};

use katui_present::cells;
use katui_present::session;
use katui_proto::kitty;

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

impl<W: io::Write> Stage<W> {
    /// Hide the cursor and hold `out` for the run.
    pub fn enter(mut out: W, teardown: Teardown) -> Result<Self, RtError> {
        cells::hide_cursor(&mut out)?;
        Ok(Self {
            out: Some(out),
            teardown,
        })
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

    use std::time::{Duration, Instant};

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
}
