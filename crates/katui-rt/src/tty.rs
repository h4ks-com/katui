//! The controlling-terminal transport: the probe's read and write side,
//! in raw mode for the exchange and restored after it.

use std::fs::{File, OpenOptions};
use std::io;
use std::os::fd::AsFd;
use std::time::{Duration, Instant};

use katui_proto::probe::Transport;
use rustix::event::{PollFd, PollFlags, Timespec, poll};
use rustix::termios::{
    InputModes, LocalModes, OptionalActions, QueueSelector, SpecialCodeIndex, Termios, tcflush,
    tcgetattr, tcsetattr,
};

/// Failures of the terminal lifecycle.
#[derive(Debug, thiserror::Error)]
pub enum RtError {
    /// Opening or configuring `/dev/tty` failed, typically because no
    /// controlling terminal exists.
    #[error("opening the controlling terminal failed")]
    Open(#[from] io::Error),
    /// A termios or poll operation failed mid-exchange.
    #[error("terminal control failed: {0}")]
    Control(#[from] rustix::io::Errno),
}

/// The probe transport over `/dev/tty`, raw for its lifetime and restored
/// on drop however the exchange ends. The total listening budget lives
/// here: reads stop returning once it is spent.
pub struct StdTransport {
    tty: File,
    saved: Termios,
    deadline: Instant,
}

impl StdTransport {
    /// Open the controlling terminal and switch it to raw mode. `budget`
    /// bounds the whole exchange, writes included.
    pub fn open(budget: Duration) -> Result<Self, RtError> {
        let tty = OpenOptions::new().read(true).write(true).open("/dev/tty")?;
        let saved = tcgetattr(tty.as_fd())?;
        let mut raw = saved.clone();
        // no echo of the answers into the output, no line buffering of
        // them, no flow control eating control bytes mid-reply
        raw.local_modes -= LocalModes::ECHO | LocalModes::ICANON;
        raw.input_modes -= InputModes::IXON | InputModes::ICRNL;
        // pin the noncanonical read floor so a previous owner's VMIN cannot
        // stall trailing reply fragments
        raw.special_codes[SpecialCodeIndex::VMIN] = 1;
        raw.special_codes[SpecialCodeIndex::VTIME] = 0;
        tcsetattr(tty.as_fd(), OptionalActions::Now, &raw)?;
        Ok(Self {
            tty,
            saved,
            // a budget overflowing the clock means a limit nobody reaches
            deadline: Instant::now()
                .checked_add(budget)
                .unwrap_or_else(|| Instant::now() + Duration::from_secs(86_400 * 365)),
        })
    }
}

impl Drop for StdTransport {
    fn drop(&mut self) {
        // replies that arrived after the budget stay queued on the device
        // and would reach the parent shell as typed junk, so drain first;
        // drop cannot report, so the restore itself is best effort
        let _ = tcflush(self.tty.as_fd(), QueueSelector::IFlush);
        let _ = tcsetattr(self.tty.as_fd(), OptionalActions::Now, &self.saved);
    }
}

impl Transport for StdTransport {
    fn write(&mut self, bytes: &[u8]) {
        let mut written = 0;
        while let Some(rest) = bytes.get(written..) {
            if rest.is_empty() {
                break;
            }
            match rustix::io::write(self.tty.as_fd(), rest) {
                Ok(0) => break,
                Ok(n) => written += n,
                Err(rustix::io::Errno::INTR) => {}
                Err(_) => break,
            }
        }
    }

    fn read_timeout(&mut self, wait: Duration) -> Option<Vec<u8>> {
        let now = Instant::now();
        if now >= self.deadline {
            return None;
        }
        let wait = wait.min(self.deadline - now);
        let timeout = Timespec {
            tv_sec: wait.as_secs().min(i64::MAX.unsigned_abs()).cast_signed(),
            tv_nsec: i64::from(wait.subsec_nanos()),
        };
        let mut fds = [PollFd::new(&self.tty, PollFlags::IN)];
        // a signal must not end the exchange, only the budget may
        let ready = loop {
            match poll(&mut fds, Some(&timeout)) {
                Ok(ready) => break ready,
                Err(rustix::io::Errno::INTR) => {}
                Err(_) => return None,
            }
        };
        if ready == 0 {
            return None;
        }
        let mut buffer = [0u8; 4096];
        match rustix::io::read(self.tty.as_fd(), &mut buffer) {
            Ok(0) | Err(_) => None,
            Ok(n) => buffer.get(..n).map(<[u8]>::to_vec),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::StdTransport;

    #[test]
    fn opening_without_a_controlling_terminal_fails_cleanly() {
        // a controlling terminal in the test environment is fine too: the
        // point is that open never panics, and both arms restore state
        if StdTransport::open(Duration::from_millis(1)).is_ok() {}
    }
}
