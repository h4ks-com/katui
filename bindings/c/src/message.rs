//! What clicks, drags, links and timers carry back to C: the key the
//! caller chose, and a payload with what was picked.

use std::ffi::{CString, c_char};

use katui_bindings::Msg as Shared;
use katui_ui::Scroll;

// cbindgen resolves a generic path by its last segment, so naming the
// alias after the shared type would make it read `Msg<u64>` as this alias.
pub(crate) type Msg = Shared<u64>;

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

/// Call `deliver` with the C form of `message`, whose link lives during the
/// call.
pub(crate) fn with_message<R>(message: &Msg, deliver: impl FnOnce(&KatuiMessage) -> R) -> R {
    let link = match message {
        Msg::Link(_, href) | Msg::Href(href) => {
            Some(CString::new(href.replace('\0', "")).unwrap_or_default())
        }
        _ => None,
    };
    let mut out = match *message {
        Msg::Value(key) => KatuiMessage::new(key, KATUI_PAYLOAD_NONE),
        Msg::Index(key, index) => KatuiMessage {
            index,
            ..KatuiMessage::new(key, KATUI_PAYLOAD_INDEX)
        },
        Msg::Id(key, id) => KatuiMessage {
            id,
            ..KatuiMessage::new(key, KATUI_PAYLOAD_ID)
        },
        Msg::Float(key, value) => KatuiMessage {
            value,
            ..KatuiMessage::new(key, KATUI_PAYLOAD_FLOAT)
        },
        Msg::Point(key, x, y) => KatuiMessage {
            x,
            y,
            ..KatuiMessage::new(key, KATUI_PAYLOAD_POINT)
        },
        Msg::Scroll(key, scroll) => KatuiMessage {
            scroll: scroll.into(),
            ..KatuiMessage::new(key, KATUI_PAYLOAD_SCROLL)
        },
        Msg::Link(key, _) => KatuiMessage::new(key, KATUI_PAYLOAD_LINK),
        Msg::Href(_) => KatuiMessage::new(0, KATUI_PAYLOAD_LINK),
        Msg::Stop => KatuiMessage::new(0, KATUI_PAYLOAD_NONE),
    };
    if let Some(link) = &link {
        out.link = link.as_ptr();
    }
    deliver(&out)
}
