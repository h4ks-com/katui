//! The parser that turns terminal input bytes into events.

use super::{
    Button, InputEvent, Key, KeypadKey, MediaKey, ModifierKey, Mods, MouseAction, MouseEvent,
};
use crate::kitty;

/// The longest incomplete sequence kept before it is discarded.
const MAX_PENDING: usize = 4 * 1024 * 1024;
const PASTE_START: &[u8] = b"\x1b[200~";
/// Where the kitty keyboard protocol numbers its non-text keys.
const FUNCTIONAL_KEYS: std::ops::RangeInclusive<u32> = 57344..=63743;
const KEYPAD_ENTER: u32 = 57414;

enum Parsed {
    Event(InputEvent, usize),
    /// A key event with the base layout key the terminal reported for it.
    Based(char, InputEvent, usize),
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
                Parsed::Based(base, event, length) => {
                    events.extend([InputEvent::BaseLayoutKey(base), event]);
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
            Parsed::Event(..) | Parsed::Based(..) | Parsed::Skip(_) => Parsed::Skip(2),
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
        b'E' => Key::Keypad(KeypadKey::Begin),
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
    let typed = with_event_type(parsed, parameters);
    match (typed, base_layout_key(parameters)) {
        (Parsed::Event(event, length), Some(base)) => Parsed::Based(base, event, length),
        (typed, _) => typed,
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

/// The kitty keyboard protocol puts the event type after the modifiers:
/// 1 is a press, 2 a repeat and 3 a release.
fn with_event_type(parsed: Parsed, parameters: &str) -> Parsed {
    let Parsed::Event(InputEvent::Key(pressed, mods), length) = parsed else {
        return parsed;
    };
    let event_type = parameters
        .split(';')
        .nth(1)
        .and_then(|modifiers| modifiers.split(':').nth(1));
    let event = match event_type {
        Some("2") => InputEvent::KeyRepeat(pressed, mods),
        Some("3") => InputEvent::KeyUp(pressed, mods),
        _ => InputEvent::Key(pressed, mods),
    };
    Parsed::Event(event, length)
}

/// The third sub-parameter of the key code, sent under the kitty "report
/// alternate keys" flag.
fn base_layout_key(parameters: &str) -> Option<char> {
    let code = parameters
        .split(';')
        .next()?
        .split(':')
        .nth(2)?
        .parse()
        .ok()?;
    char::from_u32(code)
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
        Some(57427) => Key::Keypad(KeypadKey::Begin),
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
        Some(code) if FUNCTIONAL_KEYS.contains(&code) => match functional_key(code) {
            Some(pressed) => key(pressed, mods, length),
            None => Parsed::Skip(length),
        },
        Some(code) => match char::from_u32(code) {
            Some(character) => key(Key::Char(character), mods, length),
            None => Parsed::Skip(length),
        },
        None => Parsed::Skip(length),
    }
}

const LOCK_KEYS: [Key; 6] = [
    Key::CapsLock,
    Key::ScrollLock,
    Key::NumLock,
    Key::PrintScreen,
    Key::Pause,
    Key::Menu,
];
const KEYPAD_OPERATORS: [KeypadKey; 5] = [
    KeypadKey::Decimal,
    KeypadKey::Divide,
    KeypadKey::Multiply,
    KeypadKey::Subtract,
    KeypadKey::Add,
];
const KEYPAD_REST: [KeypadKey; 13] = [
    KeypadKey::Equal,
    KeypadKey::Separator,
    KeypadKey::Left,
    KeypadKey::Right,
    KeypadKey::Up,
    KeypadKey::Down,
    KeypadKey::PageUp,
    KeypadKey::PageDown,
    KeypadKey::Home,
    KeypadKey::End,
    KeypadKey::Insert,
    KeypadKey::Delete,
    KeypadKey::Begin,
];
const MEDIA_KEYS: [MediaKey; 13] = [
    MediaKey::Play,
    MediaKey::Pause,
    MediaKey::PlayPause,
    MediaKey::Reverse,
    MediaKey::Stop,
    MediaKey::FastForward,
    MediaKey::Rewind,
    MediaKey::Next,
    MediaKey::Previous,
    MediaKey::Record,
    MediaKey::LowerVolume,
    MediaKey::RaiseVolume,
    MediaKey::Mute,
];
const MODIFIER_KEYS: [ModifierKey; 12] = [
    ModifierKey::LeftShift,
    ModifierKey::LeftControl,
    ModifierKey::LeftAlt,
    ModifierKey::LeftSuper,
    ModifierKey::LeftHyper,
    ModifierKey::LeftMeta,
    ModifierKey::RightShift,
    ModifierKey::RightControl,
    ModifierKey::RightAlt,
    ModifierKey::RightSuper,
    ModifierKey::RightHyper,
    ModifierKey::RightMeta,
];

/// A key from the kitty functional key table, which numbers each group
/// consecutively.
fn functional_key(code: u32) -> Option<Key> {
    let nth = |first: u32| usize::try_from(code.checked_sub(first)?).ok();
    match code {
        57358..=57363 => LOCK_KEYS.get(nth(57358)?).copied(),
        57376..=57398 => u8::try_from(code - 57363).ok().map(Key::Function),
        57399..=57408 => u8::try_from(code - 57399)
            .ok()
            .map(|digit| Key::Keypad(KeypadKey::Digit(digit))),
        57409..=57413 => KEYPAD_OPERATORS.get(nth(57409)?).copied().map(Key::Keypad),
        57415..=57427 => KEYPAD_REST.get(nth(57415)?).copied().map(Key::Keypad),
        57428..=57440 => MEDIA_KEYS.get(nth(57428)?).copied().map(Key::Media),
        57441..=57452 => MODIFIER_KEYS.get(nth(57441)?).copied().map(Key::Modifier),
        _ => None,
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
                ..Mods::NONE
            },
        }),
        length,
    )
}

fn mouse_action(code: u32, released: bool) -> Option<MouseAction> {
    let button = match (code & 0xc0, code & 3) {
        (0x40, 0) => return Some(MouseAction::WheelUp),
        (0x40, 1) => return Some(MouseAction::WheelDown),
        (0x40, 2) => return Some(MouseAction::WheelLeft),
        (0x40, 3) => return Some(MouseAction::WheelRight),
        (0, 0) => Some(Button::Left),
        (0, 1) => Some(Button::Middle),
        (0, 2) => Some(Button::Right),
        (0x80, 0) => Some(Button::Back),
        (0x80, 1) => Some(Button::Forward),
        (0, _) => None,
        _ => return None,
    };
    match (code & 32 != 0, released, button) {
        (true, _, Some(held)) => Some(MouseAction::Drag(held)),
        (true, _, None) => Some(MouseAction::Move),
        (false, true, Some(held)) => Some(MouseAction::Release(held)),
        (false, false, Some(held)) => Some(MouseAction::Press(held)),
        (false, _, None) => None,
    }
}

#[cfg(test)]
pub(in crate::input) mod tests {
    use crate::input::{
        Button, InputEvent, InputParser, Key, KeypadKey, MediaKey, ModifierKey, Mods, MouseAction,
        MouseEvent, Pointer,
    };

    const SHIFT: Mods = Mods {
        shift: true,
        ..Mods::NONE
    };
    use crate::probe::CellPixels;

    pub(in crate::input) fn parse(bytes: &[u8]) -> Vec<InputEvent> {
        InputParser::default().feed(bytes)
    }

    #[test]
    fn a_report_beyond_the_grid_switches_the_pointer_to_pixels() {
        let cell = CellPixels {
            width: 7,
            height: 17,
        };
        let click = |x, y| MouseEvent {
            action: MouseAction::Press(Button::Left),
            x,
            y,
            mods: Mods::NONE,
        };
        let mut pointer = Pointer::default();
        pointer.resize(80, 25);
        pointer.observe(&InputEvent::Mode(1016, false));
        assert_eq!(pointer.locate(&click(2, 1), cell), (17.5, 25.5));
        assert_eq!(
            pointer.locate(&click(85, 26), cell),
            (598.5, 450.5),
            "a cell report just past a grid that grew stays in cells"
        );
        assert_eq!(pointer.locate(&click(210, 159), cell), (210.0, 159.0));
        assert_eq!(
            pointer.locate(&click(2, 1), cell),
            (2.0, 1.0),
            "later reports stay in pixels"
        );
        let mut honored = Pointer::default();
        honored.observe(&InputEvent::Mode(1016, true));
        assert_eq!(honored.locate(&click(2, 1), cell), (2.0, 1.0));
        let retina = CellPixels {
            width: 14,
            height: 34,
        };
        let mut scaled = Pointer::new(Some(cell), retina);
        scaled.resize(80, 25);
        assert_eq!(
            scaled.locate(&click(211, 92), retina),
            (422.0, 184.0),
            "reports in points land on the double density surface"
        );
        let zoomed = CellPixels {
            width: 18,
            height: 44,
        };
        assert_eq!(
            scaled.locate(&click(211, 92), zoomed),
            (422.0, 184.0),
            "a font zoom changes the cell and leaves the density alone"
        );
    }

    pub(in crate::input) const CTRL: Mods = Mods {
        ctrl: true,
        ..Mods::NONE
    };

    pub(in crate::input) fn plain(pressed: Key) -> InputEvent {
        InputEvent::Key(pressed, Mods::NONE)
    }

    #[test]
    fn key_event_types_separate_presses_repeats_and_releases() {
        let mut parser = InputParser::default();
        assert_eq!(
            parser.feed(b"\x1b[97;1:1u\x1b[97;1:2u\x1b[97;1:3u\x1b[1;1:3A\x1b[1;5:2B"),
            vec![
                InputEvent::Key(Key::Char('a'), Mods::NONE),
                InputEvent::KeyRepeat(Key::Char('a'), Mods::NONE),
                InputEvent::KeyUp(Key::Char('a'), Mods::NONE),
                InputEvent::KeyUp(Key::Up, Mods::NONE),
                InputEvent::KeyRepeat(Key::Down, CTRL),
            ]
        );
    }

    #[test]
    fn a_reported_base_layout_key_comes_right_before_its_key() {
        assert_eq!(
            parse(b"\x1b[1089::99;5u\x1b[1089:1057:99;6:3u\x1b[97:65;2u"),
            vec![
                InputEvent::BaseLayoutKey('c'),
                InputEvent::Key(Key::Char('с'), CTRL),
                InputEvent::BaseLayoutKey('c'),
                InputEvent::KeyUp(
                    Key::Char('с'),
                    Mods {
                        shift: true,
                        ctrl: true,
                        ..Mods::NONE
                    }
                ),
                InputEvent::Key(Key::Char('a'), SHIFT),
            ]
        );
    }

    #[test]
    fn plain_text_and_utf8_become_characters() {
        assert_eq!(
            parse("aé🔥".as_bytes()),
            vec![
                plain(Key::Char('a')),
                plain(Key::Char('é')),
                plain(Key::Char('🔥'))
            ]
        );
    }

    #[test]
    fn a_character_split_across_reads_waits_for_its_tail() {
        let mut parser = InputParser::default();
        let bytes = "🔥".as_bytes();
        assert_eq!(parser.feed(&bytes[..2]), vec![]);
        assert_eq!(parser.feed(&bytes[2..]), vec![plain(Key::Char('🔥'))]);
    }

    #[test]
    fn control_bytes_are_named_keys_or_ctrl_chords() {
        assert_eq!(
            parse(b"\r\t\x7f\x03"),
            vec![
                plain(Key::Enter),
                plain(Key::Tab),
                plain(Key::Backspace),
                InputEvent::Key(Key::Char('c'), CTRL),
            ]
        );
    }

    #[test]
    fn arrows_and_editing_keys_parse_in_every_form() {
        assert_eq!(
            parse(b"\x1b[A\x1bOB\x1b[3~\x1b[5~\x1b[H"),
            vec![
                plain(Key::Up),
                plain(Key::Down),
                plain(Key::Delete),
                plain(Key::PageUp),
                plain(Key::Home),
            ]
        );
    }

    #[test]
    fn modified_arrows_report_their_modifiers() {
        assert_eq!(parse(b"\x1b[1;5C"), vec![InputEvent::Key(Key::Right, CTRL)]);
    }

    #[test]
    fn kitty_keyboard_form_reports_keys_and_modifiers() {
        assert_eq!(
            parse(b"\x1b[27u\x1b[13;2u\x1b[97;5u"),
            vec![
                plain(Key::Escape),
                InputEvent::Key(
                    Key::Enter,
                    Mods {
                        shift: true,
                        ..Mods::NONE
                    }
                ),
                InputEvent::Key(Key::Char('a'), CTRL),
            ]
        );
    }

    #[test]
    fn sgr_mouse_reports_press_release_motion_and_wheel() {
        let at = |action, x, y| {
            InputEvent::Mouse(MouseEvent {
                action,
                x,
                y,
                mods: Mods::NONE,
            })
        };
        assert_eq!(
            parse(
                b"\x1b[<0;11;21M\x1b[<0;11;21m\x1b[<35;5;6M\x1b[<32;7;8M\x1b[<64;1;1M\x1b[<65;1;1M"
            ),
            vec![
                at(MouseAction::Press(Button::Left), 10, 20),
                at(MouseAction::Release(Button::Left), 10, 20),
                at(MouseAction::Move, 4, 5),
                at(MouseAction::Drag(Button::Left), 6, 7),
                at(MouseAction::WheelUp, 0, 0),
                at(MouseAction::WheelDown, 0, 0),
            ]
        );
    }

    #[test]
    fn sgr_mouse_reports_the_horizontal_wheel_and_the_side_buttons() {
        let at = |action| {
            InputEvent::Mouse(MouseEvent {
                action,
                x: 2,
                y: 3,
                mods: Mods::NONE,
            })
        };
        assert_eq!(
            parse(
                b"\x1b[<66;3;4M\x1b[<67;3;4M\x1b[<128;3;4M\x1b[<128;3;4m\x1b[<129;3;4M\
                  \x1b[<161;3;4M\x1b[<192;3;4M"
            ),
            vec![
                at(MouseAction::WheelLeft),
                at(MouseAction::WheelRight),
                at(MouseAction::Press(Button::Back)),
                at(MouseAction::Release(Button::Back)),
                at(MouseAction::Press(Button::Forward)),
                at(MouseAction::Drag(Button::Forward)),
            ]
        );
    }

    #[test]
    fn a_paste_arrives_as_one_block_even_across_reads() {
        let mut parser = InputParser::default();
        assert_eq!(parser.feed(b"\x1b[200~hello\x1b[A"), vec![]);
        assert_eq!(
            parser.feed(b" world\x1b[201~q"),
            vec![
                InputEvent::Paste("hello\x1b[A world".to_owned()),
                plain(Key::Char('q'))
            ]
        );
    }

    #[test]
    fn a_paste_that_stalls_stays_one_paste() {
        let mut parser = InputParser::default();
        assert_eq!(parser.feed(b"\x1b[200~one\n"), vec![]);
        assert_eq!(parser.idle(), None);
        assert_eq!(
            parser.feed(b"two\x1b[201~"),
            vec![InputEvent::Paste("one\ntwo".to_owned())]
        );
    }

    #[test]
    fn keypad_enter_sends_and_unknown_functional_keys_type_nothing() {
        assert_eq!(parse(b"\x1b[57414u"), vec![plain(Key::Enter)]);
        assert_eq!(parse(b"\x1b[57364u\x1b[57453u"), vec![]);
    }

    #[test]
    fn modifier_keys_pressed_alone_report_their_side() {
        assert_eq!(
            parse(b"\x1b[57441;2u\x1b[57447;2:3u\x1b[57442u\x1b[57450u\x1b[57446u\x1b[57452u"),
            vec![
                InputEvent::Key(Key::Modifier(ModifierKey::LeftShift), SHIFT),
                InputEvent::KeyUp(Key::Modifier(ModifierKey::RightShift), SHIFT),
                plain(Key::Modifier(ModifierKey::LeftControl)),
                plain(Key::Modifier(ModifierKey::RightSuper)),
                plain(Key::Modifier(ModifierKey::LeftMeta)),
                plain(Key::Modifier(ModifierKey::RightMeta)),
            ]
        );
    }

    #[test]
    fn super_hyper_and_meta_bits_parse_and_lock_bits_are_ignored() {
        let all = Mods {
            shift: true,
            alt: true,
            ctrl: true,
            super_key: true,
            hyper: true,
            meta: true,
        };
        assert_eq!(
            parse(b"\x1b[97;9u\x1b[97;17u\x1b[97;33u\x1b[97;64u\x1b[97;257u"),
            vec![
                InputEvent::Key(
                    Key::Char('a'),
                    Mods {
                        super_key: true,
                        ..Mods::NONE
                    }
                ),
                InputEvent::Key(
                    Key::Char('a'),
                    Mods {
                        hyper: true,
                        ..Mods::NONE
                    }
                ),
                InputEvent::Key(
                    Key::Char('a'),
                    Mods {
                        meta: true,
                        ..Mods::NONE
                    }
                ),
                InputEvent::Key(Key::Char('a'), all),
                plain(Key::Char('a')),
            ]
        );
    }

    #[test]
    fn keypad_media_lock_and_high_function_keys_parse() {
        assert_eq!(
            parse(
                b"\x1b[57399u\x1b[57404u\x1b[57409u\x1b[57413u\x1b[57415u\x1b[57417u\x1b[57426u\
                  \x1b[57427u\x1b[1E\x1b[57427~\x1b[57428u\x1b[57430u\x1b[57440u\
                  \x1b[57358u\x1b[57360u\x1b[57361u\x1b[57363u\x1b[57376u\x1b[57398u"
            ),
            vec![
                plain(Key::Keypad(KeypadKey::Digit(0))),
                plain(Key::Keypad(KeypadKey::Digit(5))),
                plain(Key::Keypad(KeypadKey::Decimal)),
                plain(Key::Keypad(KeypadKey::Add)),
                plain(Key::Keypad(KeypadKey::Equal)),
                plain(Key::Keypad(KeypadKey::Left)),
                plain(Key::Keypad(KeypadKey::Delete)),
                plain(Key::Keypad(KeypadKey::Begin)),
                plain(Key::Keypad(KeypadKey::Begin)),
                plain(Key::Keypad(KeypadKey::Begin)),
                plain(Key::Media(MediaKey::Play)),
                plain(Key::Media(MediaKey::PlayPause)),
                plain(Key::Media(MediaKey::Mute)),
                plain(Key::CapsLock),
                plain(Key::NumLock),
                plain(Key::PrintScreen),
                plain(Key::Menu),
                plain(Key::Function(13)),
                plain(Key::Function(35)),
            ]
        );
    }

    #[test]
    fn focus_and_mode_reports_parse() {
        assert_eq!(
            parse(b"\x1b[I\x1b[O\x1b[?1016;1$y\x1b[?1016;0$y"),
            vec![
                InputEvent::Focus(true),
                InputEvent::Focus(false),
                InputEvent::Mode(1016, true),
                InputEvent::Mode(1016, false),
            ]
        );
    }

    #[test]
    fn graphics_answers_report_their_image_even_when_they_fail() {
        assert_eq!(
            parse(b"\x1b_Gi=29;OK\x1b\\\x1b_Gi=30,r=1;ENOENT:gone\x1b\\"),
            vec![
                InputEvent::GraphicsAnswer(29),
                InputEvent::GraphicsAnswer(30)
            ]
        );
    }

    #[test]
    fn a_legacy_back_tab_is_shift_tab() {
        let back = Mods {
            shift: true,
            ..Mods::NONE
        };
        assert_eq!(parse(b"\x1b[Z"), vec![InputEvent::Key(Key::Tab, back)]);
    }

    #[test]
    fn a_device_attributes_answer_is_one_event_and_no_key() {
        assert_eq!(parse(b"\x1b[?62;4;22c"), vec![InputEvent::DeviceAttributes]);
    }

    #[test]
    fn a_q_inside_an_answer_is_no_key() {
        assert_eq!(
            parse(b"\x1b_Gi=5;qqq\x1b\\"),
            vec![InputEvent::GraphicsAnswer(5)]
        );
    }

    #[test]
    fn a_lone_escape_is_the_esc_key_once_input_goes_quiet() {
        let mut parser = InputParser::default();
        assert_eq!(parser.feed(b"\x1b"), vec![]);
        assert_eq!(parser.idle(), Some(plain(Key::Escape)));
        assert_eq!(parser.idle(), None);
    }

    #[test]
    fn escape_right_before_an_answer_is_still_the_esc_key() {
        assert_eq!(
            parse(b"\x1b\x1b_Gi=7;OK\x1b\\"),
            vec![plain(Key::Escape), InputEvent::GraphicsAnswer(7)]
        );
    }

    #[test]
    fn an_alt_chord_reports_alt_and_cannot_wedge_the_parser() {
        assert_eq!(
            parse(b"\x1bx"),
            vec![InputEvent::Key(
                Key::Char('x'),
                Mods {
                    alt: true,
                    ..Mods::NONE
                }
            )]
        );
        let mut parser = InputParser::default();
        assert_eq!(parser.feed(b"\x1b]"), vec![]);
        assert_eq!(parser.idle(), None, "an open string is no Esc key");
        assert_eq!(parser.feed(b"q"), vec![plain(Key::Char('q'))]);
    }

    #[test]
    fn arrow_keys_never_read_as_quit_keys() {
        assert!(parse(b"\x1b[A").iter().all(|event| !event.is_quit_key()));
    }

    #[test]
    fn random_byte_streams_never_panic() {
        let mut rng = crate::fuzz::XorShift::new(0x1234_5678_9abc_def0);
        for _ in 0..3000 {
            let mut parser = InputParser::default();
            for _ in 0..4 {
                let length = (rng.next() % 64) as usize;
                let mut bytes: Vec<u8> = (0..length).map(|_| rng.byte()).collect();
                if rng.next().is_multiple_of(2) {
                    let at = (rng.next() as usize) % (length + 1);
                    bytes.splice(at..at, b"\x1b[<".iter().copied());
                }
                let _ = parser.feed(&bytes);
            }
            let _ = parser.idle();
        }
    }
}
