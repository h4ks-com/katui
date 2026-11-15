//! `KatuiEvent`: the key presses, mouse events, pastes and focus changes
//! apps and widgets read.

use std::ffi::c_char;

use katui_bindings::event::{self, EventKind};
use katui_proto::input::{Button, InputEvent, Key, Mods, MouseAction};

use crate::ffi::{Error, borrow, borrow_mut, c_string, free, owned, owned_or_null, raise, text};

/// A key press, a release, a mouse event, a paste or a focus change.
pub struct KatuiEvent(pub(crate) InputEvent);

/// The key named `name`: one character, `"enter"`, `"up"`, `"f1"` and so on.
pub(crate) fn key(name: &str) -> Result<Key, Error> {
    event::key(name).map_err(raise)
}

/// What kind of event a `KatuiEvent` is.
pub type KatuiEventKind = u32;
pub const KATUI_EVENT_OTHER: KatuiEventKind = 0;
pub const KATUI_EVENT_KEY: KatuiEventKind = 1;
pub const KATUI_EVENT_KEY_UP: KatuiEventKind = 2;
pub const KATUI_EVENT_MOUSE: KatuiEventKind = 3;
pub const KATUI_EVENT_PASTE: KatuiEventKind = 4;
pub const KATUI_EVENT_FOCUS: KatuiEventKind = 5;

/// What the mouse did.
pub type KatuiMouseAction = u32;
pub const KATUI_MOUSE_NONE: KatuiMouseAction = 0;
pub const KATUI_MOUSE_PRESS: KatuiMouseAction = 1;
pub const KATUI_MOUSE_RELEASE: KatuiMouseAction = 2;
pub const KATUI_MOUSE_DRAG: KatuiMouseAction = 3;
pub const KATUI_MOUSE_MOVE: KatuiMouseAction = 4;
pub const KATUI_MOUSE_WHEEL_UP: KatuiMouseAction = 5;
pub const KATUI_MOUSE_WHEEL_DOWN: KatuiMouseAction = 6;

/// Which mouse button.
pub type KatuiButton = u32;
pub const KATUI_BUTTON_NONE: KatuiButton = 0;
pub const KATUI_BUTTON_LEFT: KatuiButton = 1;
pub const KATUI_BUTTON_MIDDLE: KatuiButton = 2;
pub const KATUI_BUTTON_RIGHT: KatuiButton = 3;

/// # Safety
/// `event` is NULL or a live event.
pub(crate) unsafe fn event<'a>(event: *const KatuiEvent) -> Option<&'a InputEvent> {
    unsafe { borrow(event) }.map(|event| &event.0)
}

/// A press of the key `name`, for tests and for handing keys to widgets.
/// NULL when `name` names no key.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_event_press(
    name: *const c_char,
    ctrl: bool,
    alt: bool,
    shift: bool,
) -> *mut KatuiEvent {
    let mods = Mods { shift, alt, ctrl };
    owned_or_null(
        event::press(&unsafe { text(name) }, mods)
            .map(KatuiEvent)
            .map_err(raise),
    )
}

/// A paste of `text`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_event_paste(text: *const c_char) -> *mut KatuiEvent {
    owned(KatuiEvent(InputEvent::Paste(
        unsafe { self::text(text) }.into_owned(),
    )))
}

/// A copy of `event` the caller owns.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_event_clone(event: *const KatuiEvent) -> *mut KatuiEvent {
    unsafe { self::event(event) }.map_or(std::ptr::null_mut(), |event| {
        owned(KatuiEvent(event.clone()))
    })
}

/// Free an event the caller owns. NULL does nothing.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_event_free(event: *mut KatuiEvent) {
    unsafe { free(event) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_event_kind(event: *const KatuiEvent) -> KatuiEventKind {
    match unsafe { self::event(event) }.map(event::kind) {
        Some(EventKind::Key) => KATUI_EVENT_KEY,
        Some(EventKind::KeyUp) => KATUI_EVENT_KEY_UP,
        Some(EventKind::Mouse) => KATUI_EVENT_MOUSE,
        Some(EventKind::Paste) => KATUI_EVENT_PASTE,
        Some(EventKind::Focus) => KATUI_EVENT_FOCUS,
        Some(EventKind::Other) | None => KATUI_EVENT_OTHER,
    }
}

/// The key's name: one character, `"enter"`, `"up"`, `"f1"` and so on, or
/// NULL for an event without a key. The caller frees it with
/// `katui_string_free`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_event_key(event: *const KatuiEvent) -> *mut c_char {
    unsafe { self::event(event) }
        .and_then(event::key_name)
        .map_or(std::ptr::null_mut(), |name| c_string(&name))
}

/// # Safety
/// `event` is NULL or a live event.
unsafe fn mods(event: *const KatuiEvent) -> Mods {
    unsafe { self::event(event) }.map_or(Mods::NONE, event::mods)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_event_ctrl(event: *const KatuiEvent) -> bool {
    unsafe { mods(event) }.ctrl
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_event_alt(event: *const KatuiEvent) -> bool {
    unsafe { mods(event) }.alt
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_event_shift(event: *const KatuiEvent) -> bool {
    unsafe { mods(event) }.shift
}

/// The pasted text, or NULL for another event. The caller frees it with
/// `katui_string_free`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_event_text(event: *const KatuiEvent) -> *mut c_char {
    unsafe { self::event(event) }
        .and_then(event::pasted)
        .map_or(std::ptr::null_mut(), c_string)
}

/// Whether the window has the focus, for a focus event.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_event_focused(event: *const KatuiEvent) -> bool {
    unsafe { self::event(event) }.and_then(event::focused) == Some(true)
}

/// Write the cell under the pointer into `column` and `row`. False for an
/// event without a pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_event_position(
    event: *const KatuiEvent,
    column: *mut u32,
    row: *mut u32,
) -> bool {
    let Some((x, y)) = unsafe { self::event(event) }.and_then(event::position) else {
        return false;
    };
    if let Some(column) = unsafe { borrow_mut(column) } {
        *column = x;
    }
    if let Some(row) = unsafe { borrow_mut(row) } {
        *row = y;
    }
    true
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_event_action(event: *const KatuiEvent) -> KatuiMouseAction {
    match unsafe { self::event(event) }.and_then(event::action) {
        Some(MouseAction::Press(_)) => KATUI_MOUSE_PRESS,
        Some(MouseAction::Release(_)) => KATUI_MOUSE_RELEASE,
        Some(MouseAction::Drag(_)) => KATUI_MOUSE_DRAG,
        Some(MouseAction::Move) => KATUI_MOUSE_MOVE,
        Some(MouseAction::WheelUp) => KATUI_MOUSE_WHEEL_UP,
        Some(MouseAction::WheelDown) => KATUI_MOUSE_WHEEL_DOWN,
        None => KATUI_MOUSE_NONE,
    }
}

/// The button of a press, release or drag.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_event_button(event: *const KatuiEvent) -> KatuiButton {
    match unsafe { self::event(event) }.and_then(event::button) {
        Some(Button::Left) => KATUI_BUTTON_LEFT,
        Some(Button::Middle) => KATUI_BUTTON_MIDDLE,
        Some(Button::Right) => KATUI_BUTTON_RIGHT,
        None => KATUI_BUTTON_NONE,
    }
}
