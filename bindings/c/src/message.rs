//! What clicks, drags, links and timers carry back to C: the key the
//! caller chose, and a payload with what was picked.

use std::ffi::{CString, c_char};
use std::sync::Arc;

use katui_ui::Scroll;

#[derive(Clone)]
pub(crate) enum Payload {
    None,
    Index(usize),
    Id(u64),
    Float(f32),
    Point(f32, f32),
    Scroll(Scroll),
    Link(Arc<str>),
}

#[derive(Clone)]
pub(crate) enum Msg {
    User(u64, Payload),
    /// A callback broke its contract while we drew, so the app has to end.
    Stop,
}

pub(crate) fn plain(key: u64) -> Msg {
    Msg::User(key, Payload::None)
}

/// Which field of a `KatuiMessage` holds the payload.
pub type KatuiPayload = u32;
/// Only the key: buttons, checkboxes, `katui_waker_send`.
pub const KATUI_PAYLOAD_NONE: KatuiPayload = 0;
/// `index`: tabs, radio, select, menu and table rows.
pub const KATUI_PAYLOAD_INDEX: KatuiPayload = 1;
/// `id`: tree nodes.
pub const KATUI_PAYLOAD_ID: KatuiPayload = 2;
/// `value`: sliders and split dividers.
pub const KATUI_PAYLOAD_FLOAT: KatuiPayload = 3;
/// `x` and `y`, as fractions of the element's box: presses and drags.
pub const KATUI_PAYLOAD_POINT: KatuiPayload = 4;
/// `scroll`: scroll views and elements with `on_scroll`.
pub const KATUI_PAYLOAD_SCROLL: KatuiPayload = 5;
/// `link`: a link clicked in a document.
pub const KATUI_PAYLOAD_LINK: KatuiPayload = 6;

/// Where a scrolling element stands, in pixels.
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct KatuiScroll {
    /// How far the content is scrolled.
    pub offset: f32,
    /// The furthest the content can scroll.
    pub max: f32,
}

impl From<Scroll> for KatuiScroll {
    fn from(scroll: Scroll) -> Self {
        Self {
            offset: scroll.offset,
            max: scroll.max,
        }
    }
}

impl From<KatuiScroll> for Scroll {
    fn from(scroll: KatuiScroll) -> Self {
        Self {
            offset: scroll.offset,
            max: scroll.max,
        }
    }
}

/// A message as `update` receives it. Only the field `payload` names is
/// set; the others are zero. katui owns `link`, valid during the call.
#[repr(C)]
pub struct KatuiMessage {
    /// The key the element was built with.
    pub key: u64,
    pub payload: KatuiPayload,
    pub index: usize,
    pub id: u64,
    pub value: f32,
    pub x: f32,
    pub y: f32,
    pub scroll: KatuiScroll,
    pub link: *const c_char,
}

impl KatuiMessage {
    fn new(key: u64, payload: KatuiPayload) -> Self {
        Self {
            key,
            payload,
            index: 0,
            id: 0,
            value: 0.0,
            x: 0.0,
            y: 0.0,
            scroll: KatuiScroll::default(),
            link: std::ptr::null(),
        }
    }
}

/// Call `deliver` with the C form of `key` and `payload`.
pub(crate) fn with_message<R>(
    key: u64,
    payload: &Payload,
    deliver: impl FnOnce(&KatuiMessage) -> R,
) -> R {
    let mut message = KatuiMessage::new(key, KATUI_PAYLOAD_NONE);
    let link;
    match payload {
        Payload::None => {}
        Payload::Index(index) => {
            message.payload = KATUI_PAYLOAD_INDEX;
            message.index = *index;
        }
        Payload::Id(id) => {
            message.payload = KATUI_PAYLOAD_ID;
            message.id = *id;
        }
        Payload::Float(value) => {
            message.payload = KATUI_PAYLOAD_FLOAT;
            message.value = *value;
        }
        Payload::Point(x, y) => {
            message.payload = KATUI_PAYLOAD_POINT;
            (message.x, message.y) = (*x, *y);
        }
        Payload::Scroll(scroll) => {
            message.payload = KATUI_PAYLOAD_SCROLL;
            message.scroll = (*scroll).into();
        }
        Payload::Link(href) => {
            link = CString::new(href.replace('\0', "")).unwrap_or_default();
            message.payload = KATUI_PAYLOAD_LINK;
            message.link = link.as_ptr();
        }
    }
    deliver(&message)
}
