//! The console on Windows: its input and output in virtual terminal mode,
//! so escape sequences travel both ways as on a Unix terminal.

use std::time::{Duration, Instant};

use crossterm_winapi::{Console, ConsoleMode, Handle, InputRecord, ScreenBuffer};
use katui_proto::probe::{CellPixels, Transport};

use crate::RtError;

const ENABLE_PROCESSED_INPUT: u32 = 0x1;
const ENABLE_LINE_INPUT: u32 = 0x2;
const ENABLE_ECHO_INPUT: u32 = 0x4;
const ENABLE_VIRTUAL_TERMINAL_INPUT: u32 = 0x200;
const ENABLE_VIRTUAL_TERMINAL_PROCESSING: u32 = 0x4;
const DISABLE_NEWLINE_AUTO_RETURN: u32 = 0x8;

/// How long a read sleeps between looks at the console. The console wait
/// call has no safe wrapper, so we poll the pending event count instead.
const INPUT_POLL: Duration = Duration::from_millis(2);

/// The console in raw virtual terminal mode until drop: no echo, no line
/// buffering, keys and terminal answers arriving as escape sequences.
/// `with_signals` keeps Ctrl-C a signal for the probe and makes it the
/// byte 0x03 for raw input.
struct RawConsole {
    input: Console,
    input_mode: ConsoleMode,
    output_mode: ConsoleMode,
    saved_input: u32,
    saved_output: u32,
}

impl RawConsole {
    fn enter(with_signals: bool) -> Result<Self, RtError> {
        let input = Handle::current_in_handle()?;
        let input_mode = ConsoleMode::from(input.clone());
        let output_mode = ConsoleMode::from(Handle::current_out_handle()?);
        let console = Self {
            input: Console::from(input),
            saved_input: input_mode.mode()?,
            saved_output: output_mode.mode()?,
            input_mode,
            output_mode,
        };
        let mut cleared = ENABLE_LINE_INPUT | ENABLE_ECHO_INPUT;
        if !with_signals {
            cleared |= ENABLE_PROCESSED_INPUT;
        }
        console
            .input_mode
            .set_mode((console.saved_input | ENABLE_VIRTUAL_TERMINAL_INPUT) & !cleared)?;
        console.output_mode.set_mode(
            console.saved_output | ENABLE_VIRTUAL_TERMINAL_PROCESSING | DISABLE_NEWLINE_AUTO_RETURN,
        )?;
        Ok(console)
    }

    /// The next input before `end`; `None` on silence.
    fn read_until(&self, end: Instant) -> Option<Vec<u8>> {
        loop {
            let records = self.input.read_console_input().ok()?;
            let bytes = utf8(records.iter().flat_map(typed));
            if !bytes.is_empty() {
                return Some(bytes);
            }
            let left = end.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return None;
            }
            std::thread::sleep(INPUT_POLL.min(left));
        }
    }
}

impl Drop for RawConsole {
    fn drop(&mut self) {
        // replies and keys that arrived after the last read would reach
        // the parent shell as typed junk, so we discard them first; drop
        // cannot report, so the restore is best effort
        let _ = self.input.read_console_input();
        let _ = self.input_mode.set_mode(self.saved_input);
        let _ = self.output_mode.set_mode(self.saved_output);
    }
}

/// The UTF-16 units a console record types: key presses only, each as
/// often as it repeats.
fn typed(record: &InputRecord) -> impl Iterator<Item = u16> {
    let (unit, count) = match record {
        InputRecord::KeyEvent(key) if key.key_down => (key.u_char, key.repeat_count.max(1)),
        _ => (0, 0),
    };
    pressed(unit, count)
}

fn pressed(unit: u16, count: u16) -> impl Iterator<Item = u16> {
    std::iter::repeat_n(unit, usize::from(count)).filter(|unit| *unit != 0)
}

fn utf8(units: impl Iterator<Item = u16>) -> Vec<u8> {
    char::decode_utf16(units)
        .map(|decoded| decoded.unwrap_or(char::REPLACEMENT_CHARACTER))
        .collect::<String>()
        .into_bytes()
}

/// Raw keyboard input for the duration of the guard: `Ctrl-C`, `q` and
/// `Esc` arrive as plain bytes, and the saved console modes restore on
/// drop whatever ends the run.
pub struct RawInput(RawConsole);

impl RawInput {
    /// Grab the console in raw mode.
    pub fn open() -> Result<Self, RtError> {
        Ok(Self(RawConsole::enter(false)?))
    }

    /// Wait up to `wait` for input; `None` on silence.
    pub fn read_chunk(&mut self, wait: Duration) -> Option<Vec<u8>> {
        self.0.read_until(Instant::now() + wait)
    }
}

/// The probe transport over the console, raw for its lifetime and
/// restored on drop however the exchange ends. The total listening budget
/// lives here: reads stop returning once it is spent.
pub struct TtyTransport {
    console: RawConsole,
    output: Console,
    deadline: Instant,
}

impl TtyTransport {
    /// Open the console and switch it to raw mode. `budget` bounds the
    /// whole exchange, writes included.
    pub fn open(budget: Duration) -> Result<Self, RtError> {
        Ok(Self {
            console: RawConsole::enter(true)?,
            output: Console::from(Handle::current_out_handle()?),
            deadline: super::deadline_after(budget),
        })
    }
}

impl Transport for TtyTransport {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<()> {
        self.output.write_char_buffer(bytes).map(|_| ())
    }

    fn read_timeout(&mut self, wait: Duration) -> Option<Vec<u8>> {
        let start = Instant::now();
        if start >= self.deadline {
            return None;
        }
        self.console.read_until((start + wait).min(self.deadline))
    }
}

/// The console window's size in cells.
pub fn size() -> Result<katui_scene::CellSize, RtError> {
    let window = ScreenBuffer::current()?.info()?.terminal_size();
    Ok(katui_scene::CellSize {
        columns: window.width.saturating_add(1).unsigned_abs(),
        rows: window.height.saturating_add(1).unsigned_abs(),
    })
}

/// The console reports no pixels, so the cell comes from the probe.
pub(super) fn measured_cell() -> Option<CellPixels> {
    None
}

#[cfg(test)]
mod tests {
    use super::{pressed, utf8};

    #[test]
    fn key_presses_become_utf8_and_a_bare_modifier_types_nothing() {
        let crab = [0xD83E, 0xDD80];
        let units = pressed(u16::from(b'q'), 2)
            .chain(pressed(0, 1))
            .chain(pressed(0x1B, 1))
            .chain(crab);
        assert_eq!(utf8(units), "qq\u{1b}\u{1f980}".as_bytes());
    }

    #[test]
    fn a_released_key_types_nothing() {
        assert_eq!(utf8(pressed(u16::from(b'x'), 0)), b"");
    }
}
