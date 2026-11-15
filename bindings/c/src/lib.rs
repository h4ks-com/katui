//! The C ABI behind `katui.h`: apps, widgets, documents and games over
//! opaque pointers and function-pointer callbacks.
#![cfg(unix)]
// a C ABI reads raw pointers from its callers, which takes unsafe
#![allow(unsafe_code)]
// every function shares one safety contract, stated once in the header
#![allow(clippy::missing_safety_doc)]

mod app;
mod element;
mod ffi;
mod game;
mod html;
mod message;
mod state;
mod testing;
mod theme;
mod widgets;

pub use app::*;
pub use element::*;
pub use ffi::*;
pub use game::*;
pub use html::*;
pub use message::*;
pub use state::*;
pub use testing::*;
pub use theme::*;
pub use widgets::*;
