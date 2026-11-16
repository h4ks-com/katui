//! The terminal input codec: raw bytes in, keys, mouse, paste, focus and
//! graphics answers out. Pure and total: any byte stream parses without
//! panicking, and a sequence split across reads waits for its tail.

use crate::probe::CellPixels;

mod parse;

pub use parse::InputParser;

/// Modifier keys held during a key or mouse event. Only the kitty keyboard
/// protocol reports Super, Hyper and Meta.
#[allow(clippy::struct_excessive_bools)] // a modifier set is flags
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Mods {
    pub shift: bool,
    pub alt: bool,
    pub ctrl: bool,
    pub super_key: bool,
    pub hyper: bool,
    pub meta: bool,
}

impl Mods {
    pub const NONE: Self = Self {
        shift: false,
        alt: false,
        ctrl: false,
        super_key: false,
        hyper: false,
        meta: false,
    };

    /// From the xterm parameter form, one plus a bit mask.
    fn from_xterm(parameter: u32) -> Self {
        let bits = parameter.saturating_sub(1);
        Self {
            shift: bits & 1 != 0,
            alt: bits & 2 != 0,
            ctrl: bits & 4 != 0,
            super_key: bits & 8 != 0,
            hyper: bits & 16 != 0,
            meta: bits & 32 != 0,
        }
    }
}

/// A key on the keyboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    /// A character, already shifted by the terminal.
    Char(char),
    Enter,
    Tab,
    Backspace,
    Delete,
    Insert,
    Escape,
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
    /// A function key, 1-based.
    Function(u8),
    /// A key on the numeric keypad. The keypad Enter reports as
    /// [`Key::Enter`].
    Keypad(KeypadKey),
    /// A media or volume key.
    Media(MediaKey),
    /// A modifier key pressed or released on its own.
    Modifier(ModifierKey),
    CapsLock,
    NumLock,
    ScrollLock,
    PrintScreen,
    Pause,
    Menu,
}

/// A key on the numeric keypad.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeypadKey {
    /// A digit key, 0 to 9.
    Digit(u8),
    Decimal,
    Divide,
    Multiply,
    Subtract,
    Add,
    Equal,
    Separator,
    Left,
    Right,
    Up,
    Down,
    PageUp,
    PageDown,
    Home,
    End,
    Insert,
    Delete,
    /// The middle key, 5 with Num Lock off.
    Begin,
}

/// A media or volume key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaKey {
    Play,
    Pause,
    PlayPause,
    Reverse,
    Stop,
    FastForward,
    Rewind,
    Next,
    Previous,
    Record,
    LowerVolume,
    RaiseVolume,
    Mute,
}

/// A modifier key, left or right.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModifierKey {
    LeftShift,
    RightShift,
    LeftControl,
    RightControl,
    LeftAlt,
    RightAlt,
    LeftSuper,
    RightSuper,
    LeftHyper,
    RightHyper,
    LeftMeta,
    RightMeta,
}

/// The keys with a name of their own, by the name `Key::name` gives them.
const NAMED_KEYS: [(&str, Key); 63] = [
    ("enter", Key::Enter),
    ("tab", Key::Tab),
    ("backspace", Key::Backspace),
    ("delete", Key::Delete),
    ("insert", Key::Insert),
    ("escape", Key::Escape),
    ("up", Key::Up),
    ("down", Key::Down),
    ("left", Key::Left),
    ("right", Key::Right),
    ("home", Key::Home),
    ("end", Key::End),
    ("page_up", Key::PageUp),
    ("page_down", Key::PageDown),
    ("caps_lock", Key::CapsLock),
    ("num_lock", Key::NumLock),
    ("scroll_lock", Key::ScrollLock),
    ("print_screen", Key::PrintScreen),
    ("pause", Key::Pause),
    ("menu", Key::Menu),
    ("kp_decimal", Key::Keypad(KeypadKey::Decimal)),
    ("kp_divide", Key::Keypad(KeypadKey::Divide)),
    ("kp_multiply", Key::Keypad(KeypadKey::Multiply)),
    ("kp_subtract", Key::Keypad(KeypadKey::Subtract)),
    ("kp_add", Key::Keypad(KeypadKey::Add)),
    ("kp_equal", Key::Keypad(KeypadKey::Equal)),
    ("kp_separator", Key::Keypad(KeypadKey::Separator)),
    ("kp_left", Key::Keypad(KeypadKey::Left)),
    ("kp_right", Key::Keypad(KeypadKey::Right)),
    ("kp_up", Key::Keypad(KeypadKey::Up)),
    ("kp_down", Key::Keypad(KeypadKey::Down)),
    ("kp_page_up", Key::Keypad(KeypadKey::PageUp)),
    ("kp_page_down", Key::Keypad(KeypadKey::PageDown)),
    ("kp_home", Key::Keypad(KeypadKey::Home)),
    ("kp_end", Key::Keypad(KeypadKey::End)),
    ("kp_insert", Key::Keypad(KeypadKey::Insert)),
    ("kp_delete", Key::Keypad(KeypadKey::Delete)),
    ("kp_begin", Key::Keypad(KeypadKey::Begin)),
    ("media_play", Key::Media(MediaKey::Play)),
    ("media_pause", Key::Media(MediaKey::Pause)),
    ("media_play_pause", Key::Media(MediaKey::PlayPause)),
    ("media_reverse", Key::Media(MediaKey::Reverse)),
    ("media_stop", Key::Media(MediaKey::Stop)),
    ("media_fast_forward", Key::Media(MediaKey::FastForward)),
    ("media_rewind", Key::Media(MediaKey::Rewind)),
    ("media_next", Key::Media(MediaKey::Next)),
    ("media_previous", Key::Media(MediaKey::Previous)),
    ("media_record", Key::Media(MediaKey::Record)),
    ("media_lower_volume", Key::Media(MediaKey::LowerVolume)),
    ("media_raise_volume", Key::Media(MediaKey::RaiseVolume)),
    ("media_mute", Key::Media(MediaKey::Mute)),
    ("left_shift", Key::Modifier(ModifierKey::LeftShift)),
    ("right_shift", Key::Modifier(ModifierKey::RightShift)),
    ("left_control", Key::Modifier(ModifierKey::LeftControl)),
    ("right_control", Key::Modifier(ModifierKey::RightControl)),
    ("left_alt", Key::Modifier(ModifierKey::LeftAlt)),
    ("right_alt", Key::Modifier(ModifierKey::RightAlt)),
    ("left_super", Key::Modifier(ModifierKey::LeftSuper)),
    ("right_super", Key::Modifier(ModifierKey::RightSuper)),
    ("left_hyper", Key::Modifier(ModifierKey::LeftHyper)),
    ("right_hyper", Key::Modifier(ModifierKey::RightHyper)),
    ("left_meta", Key::Modifier(ModifierKey::LeftMeta)),
    ("right_meta", Key::Modifier(ModifierKey::RightMeta)),
];

impl Key {
    /// The key's name: the character itself, `"enter"`, `"page_up"`, `"f1"`,
    /// `"kp_5"`, `"media_play"`, `"left_shift"` and so on.
    #[must_use]
    pub fn name(self) -> String {
        match self {
            Self::Char(character) => character.to_string(),
            Self::Function(number) => format!("f{number}"),
            Self::Keypad(KeypadKey::Digit(digit)) => format!("kp_{digit}"),
            named => NAMED_KEYS
                .iter()
                .find(|(_, key)| *key == named)
                .map_or_else(String::new, |(name, _)| (*name).to_owned()),
        }
    }

    /// The key called `name`, the way [`Key::name`] spells it; `"space"`
    /// also names the space bar.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        let mut characters = name.chars();
        if let (Some(character), None) = (characters.next(), characters.next()) {
            return Some(Self::Char(character));
        }
        if name == "space" {
            return Some(Self::Char(' '));
        }
        NAMED_KEYS
            .iter()
            .find(|(known, _)| *known == name)
            .map(|(_, key)| *key)
            .or_else(|| name.strip_prefix('f')?.parse().ok().map(Self::Function))
            .or_else(|| {
                let digit = name.strip_prefix("kp_")?.parse().ok().filter(|d| *d < 10)?;
                Some(Self::Keypad(KeypadKey::Digit(digit)))
            })
    }
}

/// A mouse button.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Button {
    Left,
    Middle,
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

impl Button {
    /// `"left"`, `"middle"` or `"right"`.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Middle => "middle",
            Self::Right => "right",
        }
    }
}

impl MouseAction {
    /// `"press"`, `"release"`, `"drag"`, `"move"`, `"wheel_up"` or
    /// `"wheel_down"`.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Press(_) => "press",
            Self::Release(_) => "release",
            Self::Drag(_) => "drag",
            Self::Move => "move",
            Self::WheelUp => "wheel_up",
            Self::WheelDown => "wheel_down",
        }
    }

    /// The button pressed, released or held, for those actions.
    #[must_use]
    pub const fn button(self) -> Option<Button> {
        match self {
            Self::Press(button) | Self::Release(button) | Self::Drag(button) => Some(button),
            Self::Move | Self::WheelUp | Self::WheelDown => None,
        }
    }
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
    /// A key press. Terminals that report no event types send auto-repeats
    /// as presses too.
    Key(Key, Mods),
    /// A key auto-repeating while held, reported only after the
    /// application asks for releases.
    KeyRepeat(Key, Mods),
    /// A key release, reported only after the application asks for them.
    KeyUp(Key, Mods),
    /// The key at the same place on a US layout as the key of the event
    /// that follows, which shortcuts can match whatever layout is active.
    /// Only the kitty keyboard protocol reports it.
    BaseLayoutKey(char),
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
    /// The cell the terminal reported at the start, with the surface cell
    /// we drew it as then. Their ratio is the screen density, which a
    /// font zoom leaves alone, so it still holds after the cell changes.
    density: Option<(CellPixels, CellPixels)>,
    columns: u16,
    rows: u16,
}

impl Pointer {
    /// A pointer for a terminal that reported its cell as `reported_cell`
    /// while we drew cells of `cell` pixels. Pixel reports come in the
    /// reported unit, which can be a fraction of the surface pixel on a
    /// high density screen.
    #[must_use]
    pub fn new(reported_cell: Option<CellPixels>, cell: CellPixels) -> Self {
        Self {
            density: reported_cell
                .filter(|reported| reported.width > 0 && reported.height > 0)
                .map(|reported| (reported, cell)),
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
            let (reported, drawn) = self.density.unwrap_or((cell, cell));
            (
                x * f32::from(drawn.width) / f32::from(reported.width),
                y * f32::from(drawn.height) / f32::from(reported.height),
            )
        } else {
            (
                (x + 0.5) * f32::from(cell.width),
                (y + 0.5) * f32::from(cell.height),
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::input::{InputEvent, Key, KeypadKey};

    use super::parse::tests::{CTRL, plain};

    #[test]
    fn every_key_name_reads_back_as_its_key() {
        let keys = [
            Key::Char('a'),
            Key::Char(' '),
            Key::Function(12),
            Key::Keypad(KeypadKey::Digit(7)),
        ]
        .into_iter()
        .chain(super::NAMED_KEYS.iter().map(|(_, key)| *key));
        for key in keys {
            assert_eq!(Key::from_name(&key.name()), Some(key), "{key:?}");
        }
        assert_eq!(Key::from_name("space"), Some(Key::Char(' ')));
        assert_eq!(Key::from_name("nothing"), None);
        assert_eq!(Key::from_name("fx"), None);
        assert_eq!(Key::from_name("kp_10"), None);
    }

    #[test]
    fn the_quit_keys_are_q_escape_and_ctrl_c() {
        assert!(plain(Key::Char('q')).is_quit_key());
        assert!(plain(Key::Escape).is_quit_key());
        assert!(InputEvent::Key(Key::Char('c'), CTRL).is_quit_key());
        assert!(!plain(Key::Char('c')).is_quit_key());
    }
}
