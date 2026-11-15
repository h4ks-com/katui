//! The parser that turns terminal input bytes into events.

use super::{Button, InputEvent, Key, Mods, MouseAction, MouseEvent};
use crate::kitty;

/// The longest incomplete sequence kept before it is discarded.
const MAX_PENDING: usize = 4 * 1024 * 1024;
const PASTE_START: &[u8] = b"\x1b[200~";
/// Where the kitty keyboard protocol numbers its non-text keys.
const FUNCTIONAL_KEYS: std::ops::RangeInclusive<u32> = 57344..=63743;
const KEYPAD_ENTER: u32 = 57414;

enum Parsed {
    Event(InputEvent, usize),
    Skip(usize),
    Incomplete,
}

/// Turns input bytes into events, holding partial sequences across reads.
#[derive(Debug, Default)]
pub struct InputParser {
    pending: Vec<u8>,
}

impl InputParser {
    /// Parse `bytes` along with anything held from earlier reads.
    pub fn feed(&mut self, bytes: &[u8]) -> Vec<InputEvent> {
        self.pending.extend_from_slice(bytes);
        let mut events = Vec::new();
        let mut consumed = 0;
        while let Some(rest) = self.pending.get(consumed..)
            && !rest.is_empty()
        {
            match parse_one(rest) {
                Parsed::Event(event, length) => {
                    events.push(event);
                    consumed += length;
                }
                Parsed::Skip(length) => consumed += length,
                Parsed::Incomplete => break,
            }
        }
        self.pending.drain(..consumed);
        if self.pending.len() > MAX_PENDING {
            self.pending.clear();
        }
        events
    }

    /// Input went quiet: an escape with nothing after it is the Esc key,
    /// and any other open sequence was an Alt chord, because terminals
    /// write their replies whole.
    pub fn idle(&mut self) -> Option<InputEvent> {
        if self.pending.starts_with(PASTE_START) {
            return None;
        }
        let lone_escape = self.pending == [0x1b];
        self.pending.clear();
        lone_escape.then_some(InputEvent::Key(Key::Escape, Mods::NONE))
    }
}

fn key(key: Key, mods: Mods, length: usize) -> Parsed {
    Parsed::Event(InputEvent::Key(key, mods), length)
}

fn parse_one(bytes: &[u8]) -> Parsed {
    match bytes {
        [] => Parsed::Incomplete,
        [0x1b, rest @ ..] => parse_escape(bytes, rest),
        [byte, ..] if *byte < 0x20 || *byte == 0x7f => parse_control(*byte),
        _ => parse_character(bytes),
    }
}

fn parse_control(byte: u8) -> Parsed {
    let ctrl = Mods {
        ctrl: true,
        ..Mods::NONE
    };
    match byte {
        b'\r' | b'\n' => key(Key::Enter, Mods::NONE, 1),
        b'\t' => key(Key::Tab, Mods::NONE, 1),
        0x7f | 0x08 => key(Key::Backspace, Mods::NONE, 1),
        0x01..=0x1a => key(Key::Char(char::from(byte - 1 + b'a')), ctrl, 1),
        _ => Parsed::Skip(1),
    }
}

fn parse_character(bytes: &[u8]) -> Parsed {
    let width = match bytes.first() {
        Some(0xc0..=0xdf) => 2,
        Some(0xe0..=0xef) => 3,
        Some(0xf0..=0xf7) => 4,
        _ => 1,
    };
    let Some(encoded) = bytes.get(..width) else {
        return Parsed::Incomplete;
    };
    match std::str::from_utf8(encoded)
        .ok()
        .and_then(|text| text.chars().next())
    {
        Some(character) => key(Key::Char(character), Mods::NONE, width),
        None => Parsed::Skip(1),
    }
}

fn parse_escape(bytes: &[u8], rest: &[u8]) -> Parsed {
    match rest {
        [] => Parsed::Incomplete,
        [0x1b, ..] => key(Key::Escape, Mods::NONE, 1),
        [b'[', tail @ ..] => parse_csi(bytes, tail),
        [b'O', tail @ ..] => parse_ss3(tail),
        [b'_' | b'P' | b']', ..] => parse_string(bytes),
        _ => match parse_one(rest) {
            Parsed::Event(InputEvent::Key(pressed, mods), length) => {
                key(pressed, Mods { alt: true, ..mods }, length + 1)
            }
            Parsed::Incomplete => Parsed::Incomplete,
            Parsed::Event(..) | Parsed::Skip(_) => Parsed::Skip(2),
        },
    }
}

fn parse_ss3(tail: &[u8]) -> Parsed {
    let Some(letter) = tail.first() else {
        return Parsed::Incomplete;
    };
    match letter_key(*letter) {
        Some(pressed) => key(pressed, Mods::NONE, 3),
        None => Parsed::Skip(3),
    }
}

fn letter_key(letter: u8) -> Option<Key> {
    Some(match letter {
        b'A' => Key::Up,
        b'B' => Key::Down,
        b'C' => Key::Right,
        b'D' => Key::Left,
        b'H' => Key::Home,
        b'F' => Key::End,
        b'P' => Key::Function(1),
        b'Q' => Key::Function(2),
        b'R' => Key::Function(3),
        b'S' => Key::Function(4),
        _ => return None,
    })
}

/// An APC, DCS or OSC string: graphics answers are reported, the rest
/// are consumed.
fn parse_string(bytes: &[u8]) -> Parsed {
    let terminated = bytes
        .windows(2)
        .position(|pair| pair == b"\x1b\\")
        .map(|at| at + 2);
    let Some(length) = terminated else {
        return Parsed::Incomplete;
    };
    let frame = bytes.get(..length).unwrap_or_default();
    match kitty::parse_responses(frame).into_iter().next() {
        Some(kitty::Response::Ok { image, .. } | kitty::Response::Error { image, .. }) => {
            Parsed::Event(InputEvent::GraphicsAnswer(image.0), length)
        }
        None => Parsed::Skip(length),
    }
}

fn parse_csi(bytes: &[u8], tail: &[u8]) -> Parsed {
    let Some(final_at) = tail.iter().position(|byte| (0x40..=0x7e).contains(byte)) else {
        return Parsed::Incomplete;
    };
    let parameters = std::str::from_utf8(tail.get(..final_at).unwrap_or_default()).unwrap_or("");
    let final_byte = tail.get(final_at).copied().unwrap_or_default();
    let length = final_at + 3;
    if let Some(special) = parse_special_csi(bytes, parameters, final_byte, length) {
        return special;
    }
    let parsed = match final_byte {
        b'I' => Parsed::Event(InputEvent::Focus(true), length),
        b'O' => Parsed::Event(InputEvent::Focus(false), length),
        b'~' => parse_tilde(parameters, length),
        b'u' => parse_unicode_key(parameters, length),
        letter => match letter_key(letter) {
            Some(pressed) => key(pressed, modifiers_at(parameters, 1), length),
            None => Parsed::Skip(length),
        },
    };
    if reports_release(parameters) {
        as_release(parsed)
    } else {
        parsed
    }
}

/// The CSI sequences that are not plain keys: bracketed paste, mouse
/// reports, mode reports, device attributes and Shift+Tab.
fn parse_special_csi(
    bytes: &[u8],
    parameters: &str,
    final_byte: u8,
    length: usize,
) -> Option<Parsed> {
    if final_byte == b'~' && parameters == "200" {
        return Some(parse_paste(bytes, length));
    }
    if let Some(mouse) = parameters.strip_prefix('<') {
        return Some(parse_mouse(mouse, final_byte, length));
    }
    let private = parameters.starts_with('?');
    match final_byte {
        b'y' if private => Some(parse_mode_report(parameters, length)),
        b'c' if private => Some(Parsed::Event(InputEvent::DeviceAttributes, length)),
        b'Z' => {
            let back = Mods {
                shift: true,
                ..Mods::NONE
            };
            Some(key(Key::Tab, back, length))
        }
        _ => None,
    }
}

/// The kitty keyboard protocol puts the event type after the modifiers,
/// and type 3 is a release.
fn reports_release(parameters: &str) -> bool {
    parameters
        .split(';')
        .nth(1)
        .and_then(|modifiers| modifiers.split(':').nth(1))
        == Some("3")
}

fn as_release(parsed: Parsed) -> Parsed {
    match parsed {
        Parsed::Event(InputEvent::Key(released, mods), length) => {
            Parsed::Event(InputEvent::KeyUp(released, mods), length)
        }
        other => other,
    }
}

/// The number before any `:` sub-parameters in field `index`.
fn field(parameters: &str, index: usize) -> Option<u32> {
    parameters
        .split(';')
        .nth(index)?
        .split(':')
        .next()?
        .parse()
        .ok()
}

fn modifiers_at(parameters: &str, index: usize) -> Mods {
    field(parameters, index).map_or(Mods::NONE, Mods::from_xterm)
}

fn parse_paste(bytes: &[u8], start: usize) -> Parsed {
    const END: &[u8] = b"\x1b[201~";
    let body = bytes.get(start..).unwrap_or_default();
    let Some(at) = body.windows(END.len()).position(|window| window == END) else {
        return Parsed::Incomplete;
    };
    let text = String::from_utf8_lossy(body.get(..at).unwrap_or_default()).into_owned();
    Parsed::Event(InputEvent::Paste(text), start + at + END.len())
}

fn parse_mode_report(parameters: &str, length: usize) -> Parsed {
    let report = parameters.trim_start_matches('?').trim_end_matches('$');
    match (field(report, 0), field(report, 1)) {
        (Some(mode), Some(state)) => {
            Parsed::Event(InputEvent::Mode(mode, matches!(state, 1 | 3)), length)
        }
        _ => Parsed::Skip(length),
    }
}

fn parse_tilde(parameters: &str, length: usize) -> Parsed {
    let pressed = match field(parameters, 0) {
        Some(1 | 7) => Key::Home,
        Some(2) => Key::Insert,
        Some(3) => Key::Delete,
        Some(4 | 8) => Key::End,
        Some(5) => Key::PageUp,
        Some(6) => Key::PageDown,
        Some(11) => Key::Function(1),
        Some(12) => Key::Function(2),
        Some(13) => Key::Function(3),
        Some(14) => Key::Function(4),
        Some(15) => Key::Function(5),
        Some(number @ 17..=21) => Key::Function(u8::try_from(number - 11).unwrap_or(0)),
        Some(number @ 23..=24) => Key::Function(u8::try_from(number - 12).unwrap_or(0)),
        _ => return Parsed::Skip(length),
    };
    key(pressed, modifiers_at(parameters, 1), length)
}

fn parse_unicode_key(parameters: &str, length: usize) -> Parsed {
    let mods = modifiers_at(parameters, 1);
    match field(parameters, 0) {
        Some(27) => key(Key::Escape, mods, length),
        Some(13 | KEYPAD_ENTER) => key(Key::Enter, mods, length),
        Some(9) => key(Key::Tab, mods, length),
        Some(127) => key(Key::Backspace, mods, length),
        Some(code) if FUNCTIONAL_KEYS.contains(&code) => Parsed::Skip(length),
        Some(code) => match char::from_u32(code) {
            Some(character) => key(Key::Char(character), mods, length),
            None => Parsed::Skip(length),
        },
        None => Parsed::Skip(length),
    }
}

fn parse_mouse(fields: &str, final_byte: u8, length: usize) -> Parsed {
    let (Some(code), Some(column), Some(row)) =
        (field(fields, 0), field(fields, 1), field(fields, 2))
    else {
        return Parsed::Skip(length);
    };
    let Some(action) = mouse_action(code, final_byte == b'm') else {
        return Parsed::Skip(length);
    };
    Parsed::Event(
        InputEvent::Mouse(MouseEvent {
            action,
            x: column.saturating_sub(1),
            y: row.saturating_sub(1),
            mods: Mods {
                shift: code & 4 != 0,
                alt: code & 8 != 0,
                ctrl: code & 16 != 0,
            },
        }),
        length,
    )
}

fn mouse_action(code: u32, released: bool) -> Option<MouseAction> {
    if code & 64 != 0 {
        return match code & 3 {
            0 => Some(MouseAction::WheelUp),
            1 => Some(MouseAction::WheelDown),
            _ => None,
        };
    }
    let button = match code & 3 {
        0 => Some(Button::Left),
        1 => Some(Button::Middle),
        2 => Some(Button::Right),
        _ => None,
    };
    match (code & 32 != 0, released, button) {
        (true, _, Some(held)) => Some(MouseAction::Drag(held)),
        (true, _, None) => Some(MouseAction::Move),
        (false, true, Some(held)) => Some(MouseAction::Release(held)),
        (false, false, Some(held)) => Some(MouseAction::Press(held)),
        (false, _, None) => None,
    }
}
