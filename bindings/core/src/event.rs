//! Input events as the bindings read them: what kind each is, and its key,
//! modifiers, text, pointer and focus.

use katui_proto::input::{Button, InputEvent, Key, Mods, MouseAction};

use crate::Error;

/// The key named `name`: one character, `"enter"`, `"up"`, `"f1"` and so on.
pub fn key(name: &str) -> Result<Key, Error> {
    Key::from_name(name).ok_or_else(|| Error::Key(name.to_owned()))
}

/// A press of the key named `name` with `mods` held.
pub fn press(name: &str, mods: Mods) -> Result<InputEvent, Error> {
    Ok(InputEvent::Key(key(name)?, mods))
}

/// One key press per character of `text`.
pub fn typed(text: &str) -> impl Iterator<Item = InputEvent> + '_ {
    text.chars()
        .map(|character| InputEvent::Key(Key::Char(character), Mods::NONE))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventKind {
    Key,
    KeyUp,
    Mouse,
    Paste,
    Focus,
    Other,
}

impl EventKind {
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Key => "key",
            Self::KeyUp => "key_up",
            Self::Mouse => "mouse",
            Self::Paste => "paste",
            Self::Focus => "focus",
            Self::Other => "other",
        }
    }
}

#[must_use]
pub fn kind(event: &InputEvent) -> EventKind {
    match event {
        InputEvent::Key(..) => EventKind::Key,
        InputEvent::KeyUp(..) => EventKind::KeyUp,
        InputEvent::Mouse(_) => EventKind::Mouse,
        InputEvent::Paste(_) => EventKind::Paste,
        InputEvent::Focus(_) => EventKind::Focus,
        _ => EventKind::Other,
    }
}

/// The name of the key pressed or released.
#[must_use]
pub fn key_name(event: &InputEvent) -> Option<String> {
    match event {
        InputEvent::Key(key, _) | InputEvent::KeyUp(key, _) => Some(key.name()),
        _ => None,
    }
}

/// The modifiers held with a key or a mouse event.
#[must_use]
pub fn mods(event: &InputEvent) -> Mods {
    match event {
        InputEvent::Key(_, mods) | InputEvent::KeyUp(_, mods) => *mods,
        InputEvent::Mouse(mouse) => mouse.mods,
        _ => Mods::NONE,
    }
}

#[must_use]
pub fn pasted(event: &InputEvent) -> Option<&str> {
    match event {
        InputEvent::Paste(text) => Some(text),
        _ => None,
    }
}

/// Whether the window has the focus, for a focus event.
#[must_use]
pub fn focused(event: &InputEvent) -> Option<bool> {
    match event {
        InputEvent::Focus(focused) => Some(*focused),
        _ => None,
    }
}

/// The cell under the pointer, `(column, row)`, for a mouse event.
#[must_use]
pub fn position(event: &InputEvent) -> Option<(u32, u32)> {
    match event {
        InputEvent::Mouse(mouse) => Some((mouse.x, mouse.y)),
        _ => None,
    }
}

#[must_use]
pub fn action(event: &InputEvent) -> Option<MouseAction> {
    match event {
        InputEvent::Mouse(mouse) => Some(mouse.action),
        _ => None,
    }
}

/// The button of a press, release or drag.
#[must_use]
pub fn button(event: &InputEvent) -> Option<Button> {
    action(event).and_then(MouseAction::button)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_press_reads_back_its_key_and_mods() {
        let ctrl = Mods {
            ctrl: true,
            ..Mods::NONE
        };
        let event = press("enter", ctrl).ok();
        let event = event.as_ref();
        assert_eq!(event.map(kind), Some(EventKind::Key));
        assert_eq!(event.and_then(key_name).as_deref(), Some("enter"));
        assert_eq!(event.map(|event| mods(event).ctrl), Some(true));
        assert!(event.and_then(position).is_none());
        assert!(press("nope", Mods::NONE).is_err());
    }

    #[test]
    fn typed_text_is_one_press_per_character() {
        let names: Vec<_> = typed("hi").filter_map(|event| key_name(&event)).collect();
        assert_eq!(names, ["h", "i"]);
    }
}
