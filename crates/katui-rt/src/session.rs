//! The alternate-screen session guard: enter on construction, leave on
//! drop, including unwinds. A session owns its writer; drawing through
//! katui happens beside it, on the same stdout.

use std::io;

use katui_present::session;

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

#[cfg(test)]
mod tests {
    use std::io;

    use super::Session;

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
}
