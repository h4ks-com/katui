//! `Event`: the key presses, mouse events, pastes and focus changes apps
//! and widgets read.

use katui_bindings::event;
use katui_proto::input::{Button, InputEvent, Key, Mods, MouseAction};
use napi::Result;

use crate::convert::raise;

/// The key named `name`: one character, `"enter"`, `"up"`, `"f1"` and so on.
pub(crate) fn key(name: &str) -> Result<Key> {
    event::key(name).map_err(raise)
}

/// The modifiers held with a key press.
#[napi(object)]
#[derive(Default)]
pub struct Modifiers {
    pub ctrl: Option<bool>,
    pub alt: Option<bool>,
    pub shift: Option<bool>,
    pub super_key: Option<bool>,
    pub hyper: Option<bool>,
    pub meta: Option<bool>,
}

/// A key press, a release, a mouse event, a paste or a focus change.
#[napi]
pub struct Event {
    pub(crate) inner: InputEvent,
}

impl Event {
    pub(crate) fn new(inner: InputEvent) -> Self {
        Self { inner }
    }
}

#[napi]
impl Event {
    /// A press of the key `name`, for tests and for handing keys to widgets.
    #[napi(factory)]
    pub fn press(name: String, modifiers: Option<Modifiers>) -> Result<Self> {
        let modifiers = modifiers.unwrap_or_default();
        let mods = Mods {
            shift: modifiers.shift.unwrap_or(false),
            alt: modifiers.alt.unwrap_or(false),
            ctrl: modifiers.ctrl.unwrap_or(false),
            super_key: modifiers.super_key.unwrap_or(false),
            hyper: modifiers.hyper.unwrap_or(false),
            meta: modifiers.meta.unwrap_or(false),
        };
        event::press(&name, mods).map(Self::new).map_err(raise)
    }

    #[napi(factory)]
    pub fn paste(text: String) -> Self {
        Self::new(InputEvent::Paste(text))
    }

    /// `"key"`, `"key_repeat"`, `"key_up"`, `"mouse"`, `"paste"`, `"focus"`
    /// or `"other"`.
    #[napi(
        getter,
        ts_return_type = "'key' | 'key_repeat' | 'key_up' | 'mouse' | 'paste' | 'focus' | 'other'"
    )]
    pub fn kind(&self) -> &'static str {
        event::kind(&self.inner).name()
    }

    /// The key's name: one character, `"enter"`, `"up"`, `"f1"` and so on.
    #[napi(getter)]
    pub fn key(&self) -> Option<String> {
        event::key_name(&self.inner)
    }

    #[napi(getter)]
    pub fn ctrl(&self) -> bool {
        event::mods(&self.inner).ctrl
    }

    #[napi(getter)]
    pub fn alt(&self) -> bool {
        event::mods(&self.inner).alt
    }

    #[napi(getter)]
    pub fn shift(&self) -> bool {
        event::mods(&self.inner).shift
    }

    #[napi(getter)]
    pub fn super_key(&self) -> bool {
        event::mods(&self.inner).super_key
    }

    #[napi(getter)]
    pub fn hyper(&self) -> bool {
        event::mods(&self.inner).hyper
    }

    #[napi(getter)]
    pub fn meta(&self) -> bool {
        event::mods(&self.inner).meta
    }

    /// The pasted text.
    #[napi(getter)]
    pub fn text(&self) -> Option<String> {
        event::pasted(&self.inner).map(str::to_owned)
    }

    /// Whether the window has the focus, for a focus event.
    #[napi(getter)]
    pub fn focused(&self) -> Option<bool> {
        event::focused(&self.inner)
    }

    /// The cell under the pointer, `[column, row]`, for a mouse event.
    #[napi(getter)]
    pub fn position(&self) -> Option<(u32, u32)> {
        event::position(&self.inner)
    }

    /// `"press"`, `"release"`, `"drag"`, `"move"`, `"wheel_up"`,
    /// `"wheel_down"`, `"wheel_left"` or `"wheel_right"`, for a mouse event.
    #[napi(
        getter,
        ts_return_type = "'press' | 'release' | 'drag' | 'move' | 'wheel_up' | 'wheel_down' | 'wheel_left' | 'wheel_right' | null"
    )]
    pub fn action(&self) -> Option<&'static str> {
        event::action(&self.inner).map(MouseAction::name)
    }

    /// `"left"`, `"middle"`, `"right"`, `"back"` or `"forward"`, for a press,
    /// release or drag.
    #[napi(
        getter,
        ts_return_type = "'left' | 'middle' | 'right' | 'back' | 'forward' | null"
    )]
    pub fn button(&self) -> Option<&'static str> {
        event::button(&self.inner).map(Button::name)
    }

    #[napi(js_name = "toString")]
    pub fn describe(&self) -> String {
        format!("Event({:?})", self.inner)
    }
}
