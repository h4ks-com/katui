//! The terminal input codec: raw bytes in, keys, mouse, paste, focus and
//! graphics answers out. Pure and total: any byte stream parses without
//! panicking, and a sequence split across reads waits for its tail.

use crate::probe::CellPixels;

mod parse;
#[cfg(test)]
mod tests;

pub use parse::InputParser;

/// Modifier keys held during a key or mouse event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Mods {
    pub shift: bool,
    pub alt: bool,
    pub ctrl: bool,
}

impl Mods {
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
}

/// The keys with a name of their own, by the name `Key::name` gives them.
const NAMED_KEYS: [(&str, Key); 14] = [
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
];

impl Key {
    /// The key's name: the character itself, `"enter"`, `"page_up"`, `"f1"`
    /// and so on.
    #[must_use]
    pub fn name(self) -> String {
        match self {
            Self::Char(character) => character.to_string(),
            Self::Function(number) => format!("f{number}"),
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
