//! The controlling-terminal transport: the probe's read and write side,
//! in raw mode for the exchange and restored after it.

use std::fs::{File, OpenOptions};
use std::os::fd::AsFd;
use std::time::{Duration, Instant};

use katui_proto::probe::{Capabilities, Env};
use katui_proto::probe::{Transport, probe, sniff};
use rustix::event::{PollFd, PollFlags, Timespec, poll};
use rustix::termios::{
    InputModes, LocalModes, OptionalActions, QueueSelector, SpecialCodeIndex, Termios, tcflush,
    tcgetattr, tcgetwinsize, tcsetattr,
};

use crate::RtError;

/// The shared raw-mode entry: no echo, no line buffering, no flow
/// control, a one-byte read floor. `with_signals` keeps ISIG on for the
/// probe (Ctrl-C must stay a signal there) and drops it for raw input,
/// where Ctrl-C arrives as a byte.
fn enter_raw(tty: &File, with_signals: bool) -> Result<Termios, RtError> {
    let saved = tcgetattr(tty.as_fd())?;
    let mut raw = saved.clone();
    let mut modes = LocalModes::ECHO | LocalModes::ICANON;
    if !with_signals {
        modes |= LocalModes::ISIG;
    }
    raw.local_modes -= modes;
    raw.input_modes -= InputModes::IXON | InputModes::ICRNL;
    // pin the noncanonical read floor so a previous owner's VMIN cannot
    // stall trailing fragments
    raw.special_codes[SpecialCodeIndex::VMIN] = 1;
    raw.special_codes[SpecialCodeIndex::VTIME] = 0;
    tcsetattr(tty.as_fd(), OptionalActions::Now, &raw)?;
    Ok(saved)
}

/// Raw keyboard input for the duration of the guard: the controlling
/// terminal delivers bytes one at a time, with `Ctrl-C`, `q` and `Esc`
/// arriving as plain bytes instead of signals, and the saved state
/// restores on drop whatever ends the run.
pub struct RawInput {
    tty: File,
    saved: Termios,
}

impl RawInput {
    /// Grab the controlling terminal in raw mode.
    pub fn open() -> Result<Self, RtError> {
        let tty = OpenOptions::new().read(true).write(true).open("/dev/tty")?;
        let saved = enter_raw(&tty, false)?;
        Ok(Self { tty, saved })
    }

    /// Wait up to `wait` for one key byte; `None` on silence.
    pub fn read_timeout(&mut self, wait: Duration) -> Option<u8> {
        let timeout = timespec(wait);
        let mut fds = [PollFd::new(&self.tty, PollFlags::IN)];
        let Ok(ready) = poll(&mut fds, Some(&timeout)) else {
            return None;
        };
        if ready == 0 {
            return None;
        }
        let mut buffer = [0u8; 1];
        match rustix::io::read(self.tty.as_fd(), &mut buffer) {
            Ok(1) => Some(buffer[0]),
            _ => None,
        }
    }
}

impl Drop for RawInput {
    fn drop(&mut self) {
        // drop cannot report, so the restore is best effort; bytes that
        // arrived after the last read are drained so the shell sees none
        let _ = tcflush(self.tty.as_fd(), QueueSelector::IFlush);
        let _ = tcsetattr(self.tty.as_fd(), OptionalActions::Now, &self.saved);
    }
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
        let saved = enter_raw(&tty, true)?;
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

/// The terminal's size in columns and rows, asked of the controlling
/// terminal, as the named [`katui_scene::CellSize`] the scene uses.
pub fn size() -> Result<katui_scene::CellSize, RtError> {
    let tty = OpenOptions::new().read(true).write(true).open("/dev/tty")?;
    let winsize = tcgetwinsize(tty.as_fd())?;
    Ok(katui_scene::CellSize {
        columns: winsize.ws_col,
        rows: winsize.ws_row,
    })
}

/// The one probe every demo and binary runs: the standard 300 ms budget
/// over the controlling terminal, falling back to the environment sniff
/// when no terminal answers.
pub fn probe_default() -> Capabilities {
    let env = Env::capture();
    match StdTransport::open(Duration::from_millis(300)) {
        Ok(mut transport) => probe(&mut transport, &env),
        Err(_) => sniff(&env),
    }
}

fn timespec(wait: Duration) -> Timespec {
    Timespec {
        tv_sec: wait.as_secs().min(i64::MAX.unsigned_abs()).cast_signed(),
        tv_nsec: i64::from(wait.subsec_nanos()),
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
        let timeout = timespec(wait);
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
