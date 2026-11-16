//! The C ABI behind `katui.h`: apps, widgets, documents and games over
//! opaque pointers and function-pointer callbacks.
// a C ABI reads raw pointers from its callers, which takes unsafe
#![allow(unsafe_code)]
// every function shares one safety contract, stated once in the header
#![allow(clippy::missing_safety_doc)]
// other languages call these functions, where `#[must_use]` means nothing
#![allow(clippy::must_use_candidate)]
#![warn(clippy::undocumented_unsafe_blocks)]

pub mod app;
pub mod element;
pub mod ffi;
pub mod game;
pub mod html;
pub mod message;

pub mod event;
pub mod state;
pub mod testing;
pub mod theme;
pub mod widgets;
