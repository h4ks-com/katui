//! The terminal input codec: raw bytes in, keys, mouse, paste, focus and
//! graphics answers out. Pure and total: any byte stream parses without
//! panicking, and a sequence split across reads waits for its tail.

use crate::kitty;
use crate::probe::CellPixels;

/// The longest incomplete sequence kept before it is discarded.
const MAX_PENDING: usize = 4 * 1024 * 1024;

const PASTE_START: &[u8] = b"\x1b[200~";

/// Where the kitty keyboard protocol numbers its non-text keys.
const FUNCTIONAL_KEYS: std::ops::RangeInclusive<u32> = 57344..=63743;
const KEYPAD_ENTER: u32 = 57414;

/// Modifier keys held during a key or mouse event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Mods {
    /// Shift.
    pub shift: bool,
    /// Alt or Option.
    pub alt: bool,
    /// Control.
    pub ctrl: bool,
}

impl Mods {
    /// No modifiers.
    pub const NONE: Self = Self {
        shift: false,
        alt: false,
        ctrl: false,
    };

    /// From the xterm parameter form, one plus a bit mask.
    fn from_xterm(parameter: u32) -> Self {
        let bits = parameter.saturating_sub(1);
        Self {
            shift: bits & 1 != 0,
            alt: bits & 2 != 0,
            ctrl: bits & 4 != 0,
        }
    }
}

/// A key on the keyboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    /// A character, already shifted by the terminal.
    Char(char),
    /// Enter.
    Enter,
    /// Tab.
    Tab,
    /// Backspace.
    Backspace,
    /// Delete.
    Delete,
    /// Insert.
    Insert,
    /// Escape.
    Escape,
    /// Up arrow.
    Up,
    /// Down arrow.
    Down,
    /// Left arrow.
    Left,
    /// Right arrow.
    Right,
    /// Home.
    Home,
    /// End.
    End,
    /// Page up.
    PageUp,
    /// Page down.
    PageDown,
    /// A function key, 1-based.
    Function(u8),
}

/// A mouse button.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Button {
    /// Left.
    Left,
    /// Middle.
    Middle,
    /// Right.
    Right,
}

/// What the mouse did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseAction {
    /// A button went down.
    Press(Button),
    /// A button came up.
    Release(Button),
    /// The pointer moved with a button held.
    Drag(Button),
    /// The pointer moved with no button held.
    Move,
    /// The wheel turned up.
    WheelUp,
    /// The wheel turned down.
    WheelDown,
}

/// One mouse event. The position is zero-based, in pixels when the
/// terminal honored pixel mouse reporting and in cells otherwise.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MouseEvent {
    /// What happened.
    pub action: MouseAction,
    /// Column or pixel from the left.
    pub x: u32,
    /// Row or pixel from the top.
    pub y: u32,
    /// Modifiers held.
    pub mods: Mods,
}

/// One thing the terminal reported.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputEvent {
    /// A key press or auto-repeat.
    Key(Key, Mods),
    /// A key release, reported only after the application asks for them.
    KeyUp(Key, Mods),
    /// A mouse event.
    Mouse(MouseEvent),
    /// Text pasted as one block.
    Paste(String),
    /// The terminal window gained or lost focus.
    Focus(bool),
    /// A kitty graphics answer naming this image id.
    GraphicsAnswer(u32),
    /// The terminal's primary device attributes answer, which arrives
    /// after everything sent before the query was handled.
    DeviceAttributes,
    /// A private mode report: the mode and whether it is on.
    Mode(u32, bool),
}

impl InputEvent {
    /// Whether this is one of the keys every katui demo quits on: `q`,
    /// `Esc` or `Ctrl-C`.
    #[must_use]
    pub fn is_quit_key(&self) -> bool {
        match self {
            Self::Key(Key::Escape | Key::Char('q'), Mods::NONE) => true,
            Self::Key(Key::Char('c'), mods) => mods.ctrl,
            _ => false,
        }
    }
}

/// The pixel mode we ask for in mouse reports.
const PIXEL_MOUSE_MODE: u32 = 1016;

/// Places mouse reports on the pixel surface, whichever unit the terminal
/// reports in.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Pointer {
    pixels: bool,
    reported_cell: Option<CellPixels>,
    columns: u16,
    rows: u16,
}

impl Pointer {
    /// A pointer for a terminal that reported its cell as `reported_cell`.
    /// Pixel reports come in that unit, which can be a fraction of the
    /// surface pixel on a high density screen.
    #[must_use]
    pub fn new(reported_cell: Option<CellPixels>) -> Self {
        Self {
            reported_cell,
            ..Self::default()
        }
    }

    /// Learn the report unit from a pixel mode report.
    pub fn observe(&mut self, event: &InputEvent) {
        if let InputEvent::Mode(PIXEL_MOUSE_MODE, on) = event {
            self.pixels = *on;
        }
    }

    /// Track the screen grid the reports point into.
    pub fn resize(&mut self, columns: u16, rows: u16) {
        self.columns = columns;
        self.rows = rows;
    }

    /// The surface pixel `mouse` points at, with cells of `cell` pixels.
    /// Some terminals send pixels while reporting the pixel mode as
    /// unsupported, so we switch to pixels once a report lands at twice
    /// the grid, which a cell report from a grid that just grew never
    /// reaches.
    pub fn locate(&mut self, mouse: &MouseEvent, cell: CellPixels) -> (f32, f32) {
        let beyond = |value: u32, cells: u16| cells > 0 && value >= u32::from(cells) * 2;
        if beyond(mouse.x, self.columns) || beyond(mouse.y, self.rows) {
            self.pixels = true;
        }
        let coordinate = |value: u32| f32::from(u16::try_from(value).unwrap_or(u16::MAX));
        let (x, y) = (coordinate(mouse.x), coordinate(mouse.y));
        if self.pixels {
            let reported = self
                .reported_cell
                .filter(|reported| reported.width > 0 && reported.height > 0)
                .unwrap_or(cell);
            (
                x * f32::from(cell.width) / f32::from(reported.width),
                y * f32::from(cell.height) / f32::from(reported.height),
            )
        } else {
            (
                (x + 0.5) * f32::from(cell.width),
                (y + 0.5) * f32::from(cell.height),
            )
        }
    }
}

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
    if final_byte == b'~' && parameters == "200" {
        return parse_paste(bytes, length);
    }
    if let Some(mouse) = parameters.strip_prefix('<') {
        return parse_mouse(mouse, final_byte, length);
    }
    if final_byte == b'y' && parameters.starts_with('?') {
        return parse_mode_report(parameters, length);
    }
    if final_byte == b'c' && parameters.starts_with('?') {
        return Parsed::Event(InputEvent::DeviceAttributes, length);
    }
    if final_byte == b'Z' {
        let back = Mods {
            shift: true,
            ..Mods::NONE
        };
        return key(Key::Tab, back, length);
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

#[cfg(test)]
mod tests {
    use super::{Button, InputEvent, InputParser, Key, Mods, MouseAction, MouseEvent, Pointer};
    use crate::probe::CellPixels;

    fn parse(bytes: &[u8]) -> Vec<InputEvent> {
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
        let mut scaled = Pointer::new(Some(cell));
        scaled.resize(80, 25);
        assert_eq!(
            scaled.locate(&click(211, 92), retina),
            (422.0, 184.0),
            "reports in points land on the double density surface"
        );
    }

    const CTRL: Mods = Mods {
        shift: false,
        alt: false,
        ctrl: true,
    };

    fn plain(pressed: Key) -> InputEvent {
        InputEvent::Key(pressed, Mods::NONE)
    }

    #[test]
    fn key_event_types_separate_releases_from_presses() {
        let mut parser = InputParser::default();
        assert_eq!(
            parser.feed(b"\x1b[97;1:1u\x1b[97;1:2u\x1b[97;1:3u\x1b[1;1:3A"),
            vec![
                InputEvent::Key(Key::Char('a'), Mods::NONE),
                InputEvent::Key(Key::Char('a'), Mods::NONE),
                InputEvent::KeyUp(Key::Char('a'), Mods::NONE),
                InputEvent::KeyUp(Key::Up, Mods::NONE),
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
    fn keypad_enter_sends_and_other_functional_keys_type_nothing() {
        assert_eq!(parse(b"\x1b[57414u"), vec![plain(Key::Enter)]);
        assert_eq!(parse(b"\x1b[57376u"), vec![]);
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
    fn the_quit_keys_are_q_escape_and_ctrl_c() {
        assert!(plain(Key::Char('q')).is_quit_key());
        assert!(plain(Key::Escape).is_quit_key());
        assert!(InputEvent::Key(Key::Char('c'), CTRL).is_quit_key());
        assert!(!plain(Key::Char('c')).is_quit_key());
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
