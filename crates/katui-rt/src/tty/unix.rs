//! The controlling terminal on Unix: `/dev/tty` through termios.

use std::fs::{File, OpenOptions};
use std::os::fd::AsFd;
use std::time::{Duration, Instant};

use katui_proto::probe::CellPixels;
use katui_proto::probe::Transport;
use rustix::event::{PollFd, PollFlags, Timespec, poll};
use rustix::fd::OwnedFd;
use rustix::fs::{Mode, OFlags};
use rustix::termios::{
    InputModes, LocalModes, OptionalActions, QueueSelector, SpecialCodeIndex, Termios, Winsize,
    tcflush, tcgetattr, tcgetwinsize, tcsetattr,
};

use crate::RtError;

/// Open the controlling terminal for the read/write exchange,
/// non-blocking: a poll that reports readable without deliverable bytes
/// (macOS ptys do this right after the process's own write) must degrade
/// to a retry, never to a blocked read.
fn open_tty() -> Result<File, RtError> {
    let flags = OFlags::RDWR | OFlags::NONBLOCK;
    let fd: OwnedFd = rustix::fs::open("/dev/tty", flags, Mode::empty())?;
    Ok(File::from(fd))
}

/// The terminal in raw mode until drop: no echo, no line buffering, no
/// flow control, a one-byte read floor. `with_signals` keeps ISIG on for
/// the probe (Ctrl-C must stay a signal there) and drops it for raw
/// input, where Ctrl-C arrives as a byte.
struct RawTty {
    tty: File,
    saved: Termios,
}

impl RawTty {
    fn enter(tty: File, with_signals: bool) -> Result<Self, RtError> {
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
        Ok(Self { tty, saved })
    }
}

impl Drop for RawTty {
    fn drop(&mut self) {
        // replies and keys that arrived after the last read would reach
        // the parent shell as typed junk, so we flush them first; drop
        // cannot report, so the restore is best effort
        let _ = tcflush(self.tty.as_fd(), QueueSelector::IFlush);
        let _ = tcsetattr(self.tty.as_fd(), OptionalActions::Now, &self.saved);
    }
}

/// The pause after the terminal claims input it cannot deliver.
const SPURIOUS_READY_BACKOFF: Duration = Duration::from_millis(4);

/// Raw keyboard input for the duration of the guard: the controlling
/// terminal delivers bytes one at a time, with `Ctrl-C`, `q` and `Esc`
/// arriving as plain bytes instead of signals, and the saved state
/// restores on drop whatever ends the run.
pub struct RawInput(RawTty);

impl RawInput {
    /// Grab the controlling terminal in raw mode.
    pub fn open() -> Result<Self, RtError> {
        Ok(Self(RawTty::enter(open_tty()?, false)?))
    }

    /// Wait up to `wait` for input; `None` on silence.
    pub fn read_chunk(&mut self, wait: Duration) -> Option<Vec<u8>> {
        let deadline = Instant::now() + wait;
        let mut buffer = [0u8; 4096];
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            let mut fds = [PollFd::new(&self.0.tty, PollFlags::IN)];
            if !matches!(poll(&mut fds, Some(&timespec(remaining))), Ok(ready) if ready > 0) {
                return None;
            }
            match rustix::io::read(self.0.tty.as_fd(), &mut buffer) {
                Ok(count) if count > 0 => return buffer.get(..count).map(<[u8]>::to_vec),
                // macOS ptys can report readable with no bytes to deliver,
                // so we back off here or this loop would spin a whole core
                Err(rustix::io::Errno::AGAIN) if !remaining.is_zero() => {
                    std::thread::sleep(SPURIOUS_READY_BACKOFF);
                }
                _ => return None,
            }
        }
    }
}

/// The probe transport over `/dev/tty`, raw for its lifetime and restored
/// on drop however the exchange ends. The total listening budget lives
/// here: reads stop returning once it is spent.
pub struct TtyTransport {
    raw: RawTty,
    deadline: Instant,
}

impl TtyTransport {
    /// Open the controlling terminal and switch it to raw mode. `budget`
    /// bounds the whole exchange, writes included.
    pub fn open(budget: Duration) -> Result<Self, RtError> {
        let tty = open_tty()?;
        Self::from_tty(tty, budget)
    }

    /// The raw-mode lifecycle over an already-open terminal file: the
    /// seam the pty test drives, since /dev/tty cannot be pointed
    /// elsewhere.
    fn from_tty(tty: File, budget: Duration) -> Result<Self, RtError> {
        Ok(Self {
            raw: RawTty::enter(tty, true)?,
            deadline: super::deadline_after(budget),
        })
    }
}

fn window_size() -> Result<Winsize, RtError> {
    let tty = OpenOptions::new().read(true).write(true).open("/dev/tty")?;
    Ok(tcgetwinsize(tty.as_fd())?)
}

/// The terminal's size in cells.
pub fn size() -> Result<katui_scene::CellSize, RtError> {
    let winsize = window_size()?;
    Ok(katui_scene::CellSize {
        columns: winsize.ws_col,
        rows: winsize.ws_row,
    })
}

/// The cell size the terminal driver reports, when it reports pixels.
pub(super) fn measured_cell() -> Option<CellPixels> {
    let window = window_size().ok()?;
    let area = CellPixels {
        width: window.ws_xpixel,
        height: window.ws_ypixel,
    };
    let cells = katui_scene::CellSize {
        columns: window.ws_col,
        rows: window.ws_row,
    };
    super::cell_from_area(area, cells)
}

fn timespec(wait: Duration) -> Timespec {
    Timespec {
        tv_sec: wait.as_secs().min(i64::MAX.unsigned_abs()).cast_signed(),
        tv_nsec: i64::from(wait.subsec_nanos()),
    }
}

impl Transport for TtyTransport {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<()> {
        let mut written = 0;
        while let Some(rest) = bytes.get(written..) {
            if rest.is_empty() {
                break;
            }
            match rustix::io::write(self.raw.tty.as_fd(), rest) {
                Ok(0) => break,
                Ok(n) => written += n,
                Err(rustix::io::Errno::INTR) => {}
                // the fd is non-blocking and a pty buffer holds about a
                // kilobyte, so we wait on writability: a fixed sleep here
                // caps the channel at a few hundred KB/s
                Err(rustix::io::Errno::AGAIN) => {
                    let mut fds = [PollFd::new(&self.raw.tty, PollFlags::OUT)];
                    match poll(&mut fds, Some(&timespec(Duration::from_millis(100)))) {
                        Ok(_) | Err(rustix::io::Errno::INTR) => {}
                        Err(errno) => return Err(std::io::Error::from(errno)),
                    }
                }
                Err(errno) => return Err(std::io::Error::from(errno)),
            }
        }
        Ok(())
    }

    fn read_timeout(&mut self, wait: Duration) -> Option<Vec<u8>> {
        let start = Instant::now();
        if start >= self.deadline {
            return None;
        }
        let end = (start + wait).min(self.deadline);
        while Instant::now() < end {
            let timeout = timespec(end - Instant::now());
            let mut fds = [PollFd::new(&self.raw.tty, PollFlags::IN)];
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
            match rustix::io::read(self.raw.tty.as_fd(), &mut buffer) {
                Ok(0) => return None,
                Ok(n) => return buffer.get(..n).map(<[u8]>::to_vec),
                // the poll reported readable but no byte arrived; the
                // fd is non-blocking, so spend the rest of the wait
                // budget polling again instead of reporting silence
                Err(rustix::io::Errno::AGAIN) => {}
                Err(_) => return None,
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::{LocalModes, OpenOptions, TtyTransport, tcgetattr};
    use std::os::fd::AsFd as _;

    /// A pty pair for the terminal tests: the master keeps the device
    /// alive, the slave gets driven. `None` where the sandbox denies
    /// device nodes.
    #[allow(clippy::print_stderr)] // the skip notice belongs on stderr
    fn pty_slave() -> Option<(rustix::fd::OwnedFd, std::fs::File)> {
        use rustix::pty::{OpenptFlags, openpt, ptsname, unlockpt};

        let master = openpt(OpenptFlags::RDWR | OpenptFlags::NOCTTY)
            .expect("opening a pty pair cannot fail");
        unlockpt(&master).expect("unlocking the pty cannot fail");
        let name = [0u8; 64];
        let name = ptsname(&master, name).expect("the pty has a name");
        let path = name.to_string_lossy().into_owned();
        if let Ok(slave) = OpenOptions::new().read(true).write(true).open(&path) {
            Some((master, slave))
        } else {
            eprintln!("skipping pty test: no permission to open {path}");
            None
        }
    }

    /// The raw-mode lifecycle, verified against a real pty: attributes
    /// flip while the transport lives and return exactly on drop.
    #[test]
    fn raw_mode_flips_and_restores_on_a_pty() {
        let Some((_master, slave)) = pty_slave() else {
            return;
        };
        let witness = slave.try_clone().expect("duplicating the fd cannot fail");
        let before = tcgetattr(witness.as_fd()).expect("the pty answers tcgetattr");

        let transport = TtyTransport::from_tty(slave, Duration::from_millis(50))
            .expect("a pty enters raw mode");
        let during = tcgetattr(witness.as_fd()).expect("the pty answers tcgetattr");
        assert!(
            !during
                .local_modes
                .contains(LocalModes::ICANON | LocalModes::ECHO),
            "raw mode clears line buffering and echo"
        );
        drop(transport);

        let after = tcgetattr(witness.as_fd()).expect("the pty answers tcgetattr");
        assert_eq!(
            before.local_modes, after.local_modes,
            "drop restores the saved local modes"
        );
        assert_eq!(
            before.input_modes, after.input_modes,
            "drop restores the saved input modes"
        );
    }

    /// A silent terminal must end the exchange at the budget, never in a
    /// blocked read: macOS ptys report readable right after the probe's
    /// own write, so the fd is non-blocking and a lying poll retries.
    #[test]
    fn the_probe_budget_ends_a_silent_exchange() {
        use katui_proto::probe::ProbeSource;
        use katui_proto::probe::{Env, probe};

        let Some((_master, slave)) = pty_slave() else {
            return;
        };

        let env = Env {
            term: Some("xterm-256color".to_owned()),
            ..Env::default()
        };
        let mut transport = TtyTransport::from_tty(slave, Duration::from_millis(300))
            .expect("a pty enters raw mode");
        let start = Instant::now();
        let caps = probe(&mut transport, &env);
        assert_eq!(
            caps.trust,
            ProbeSource::Sniffed,
            "a silent pty proves nothing"
        );
        assert!(
            start.elapsed() < Duration::from_secs(2),
            "the budget ends the exchange, not a blocked read"
        );
    }
}
