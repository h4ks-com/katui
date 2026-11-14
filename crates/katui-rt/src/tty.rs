//! The controlling-terminal transport: the probe's read and write side,
//! in raw mode for the exchange and restored after it.

use std::fs::{File, OpenOptions};
use std::os::fd::AsFd;
use std::time::{Duration, Instant};

use katui_proto::kitty::IdSpace;
use katui_proto::probe::CellPixels;
use katui_proto::probe::{Capabilities, Env};
use katui_proto::probe::{Transport, probe, sniff};
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
            // a budget overflowing the clock means a limit nobody reaches
            deadline: Instant::now()
                .checked_add(budget)
                .unwrap_or_else(|| Instant::now() + Duration::from_secs(86_400 * 365)),
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

/// The probe every demo and binary runs over the controlling terminal,
/// falling back to the environment sniff when no terminal answers.
pub fn probe_default() -> Capabilities {
    let mut caps = probe_terminal();
    if caps.multiplexed() {
        caps.id_space = IdSpace::of_process(std::process::id());
    }
    caps
}

fn probe_terminal() -> Capabilities {
    let env = Env::capture();
    let caps = probe_once(&env, Duration::from_millis(600));
    if worth_retrying(&env, &caps) {
        let retried = probe_once(&env, Duration::from_millis(1200));
        if retried.kitty {
            return retried;
        }
    }
    caps
}

/// A multiplexer inside kitty sometimes relays kitty's answer too late for
/// the first budget, and losing it drops a kitty terminal to blocks, so we
/// ask once more with a longer one.
fn worth_retrying(env: &Env, caps: &Capabilities) -> bool {
    env.inside_multiplexer()
        && env.launched_from_kitty()
        && env.forced_graphics().is_none()
        && !caps.kitty
}

fn probe_once(env: &Env, budget: Duration) -> Capabilities {
    match TtyTransport::open(budget) {
        Ok(mut transport) => {
            let caps = probe(&mut transport, env);
            // answers racing the budget boundary would otherwise leak
            // into the parent shell as typed junk; spend a quiet window
            // draining whatever still arrives, and drop's input flush
            // then clears the rest
            let until = Instant::now() + Duration::from_millis(200);
            while Instant::now() < until {
                let _ = transport.read_timeout(Duration::from_millis(50));
            }
            caps
        }
        Err(_) => sniff(env),
    }
}

/// The cell size the terminal driver reports, when it reports pixels.
#[must_use]
fn measured_cell() -> Option<CellPixels> {
    let window = window_size().ok()?;
    let area = CellPixels {
        width: window.ws_xpixel,
        height: window.ws_ypixel,
    };
    let cells = katui_scene::CellSize {
        columns: window.ws_col,
        rows: window.ws_row,
    };
    cell_from_area(area, cells)
}

/// The pixel size of one cell: what the terminal driver reports now,
/// else what the probe answered, else the probed text area over the cell
/// count, else a typical cell. The driver comes first because it follows
/// a font zoom, which changes the cell after the probe. A multiplexer
/// leaves all of them unanswered at times, and a placement's cell span
/// scales whatever we choose, so the typical cell is a safe guess.
#[must_use]
pub fn cell_size(caps: &Capabilities) -> CellPixels {
    pick_cell(measured_cell(), caps.cell, caps.text_area.zip(size().ok()))
}

fn pick_cell(
    measured: Option<CellPixels>,
    probed: Option<CellPixels>,
    area: Option<(CellPixels, katui_scene::CellSize)>,
) -> CellPixels {
    measured
        .or(probed)
        .or_else(|| area.and_then(|(area, cells)| cell_from_area(area, cells)))
        .unwrap_or(CellPixels {
            width: 10,
            height: 20,
        })
}

fn cell_from_area(area: CellPixels, cells: katui_scene::CellSize) -> Option<CellPixels> {
    let width = area.width.checked_div(cells.columns)?;
    let height = area.height.checked_div(cells.rows)?;
    (width > 0 && height > 0).then_some(CellPixels { width, height })
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
mod area_tests {
    use katui_proto::probe::CellPixels;
    use katui_scene::CellSize;

    #[test]
    fn a_text_area_over_its_cells_gives_the_cell() {
        let area = CellPixels {
            width: 560,
            height: 700,
        };
        let cells = CellSize {
            columns: 80,
            rows: 50,
        };
        assert_eq!(
            super::cell_from_area(area, cells),
            Some(CellPixels {
                width: 7,
                height: 14
            })
        );
        let empty = CellSize {
            columns: 0,
            rows: 0,
        };
        assert_eq!(super::cell_from_area(area, empty), None);
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::{LocalModes, OpenOptions, TtyTransport, tcgetattr};
    use katui_proto::probe::CellPixels;
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

    #[test]
    fn opening_the_controlling_terminal_never_panics() {
        // a controlling terminal in the test environment is fine too: the
        // point is that open never panics, and both arms restore state
        if TtyTransport::open(Duration::from_millis(1)).is_ok() {}
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

    #[test]
    fn the_driver_cell_wins_then_the_probe_then_the_area() {
        let cell = |width, height| CellPixels { width, height };
        let cells = katui_scene::CellSize {
            columns: 10,
            rows: 10,
        };
        let area = Some((cell(90, 180), cells));
        assert_eq!(
            super::pick_cell(Some(cell(8, 16)), Some(cell(7, 14)), area),
            cell(8, 16)
        );
        assert_eq!(super::pick_cell(None, Some(cell(7, 14)), area), cell(7, 14));
        assert_eq!(super::pick_cell(None, None, area), cell(9, 18));
        assert_eq!(super::pick_cell(None, None, None), cell(10, 20));
    }

    #[test]
    fn only_a_kitty_launched_multiplexer_without_graphics_retries() {
        use katui_proto::probe::{Capabilities, Env};

        let tmux_in_kitty = Env {
            tmux: Some("pane".to_owned()),
            kitty_window_id: Some("1".to_owned()),
            ..Env::default()
        };
        let lost = Capabilities::default();
        assert!(super::worth_retrying(&tmux_in_kitty, &lost));
        let found = Capabilities {
            kitty: true,
            ..Capabilities::default()
        };
        assert!(!super::worth_retrying(&tmux_in_kitty, &found));
        let forced = Env {
            graphics: Some("blocks".to_owned()),
            ..tmux_in_kitty.clone()
        };
        assert!(!super::worth_retrying(&forced, &lost));
        let bare_kitty = Env {
            tmux: None,
            ..tmux_in_kitty
        };
        assert!(!super::worth_retrying(&bare_kitty, &lost));
    }
}
