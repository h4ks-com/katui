//! What the Python, Node and C bindings share, apart from their languages:
//! the message type, the app and game bridges, the headless screen, focus
//! over foreign values, and the parsing of names, images and documents.
//!
//! A binding implements [`app::AppHost`] or [`game::GameHost`] over its
//! own values, converts arguments and raises [`Error`]s in its own style.
#![cfg(unix)]

pub mod app;
pub mod event;
pub mod failure;
pub mod game;
pub mod html;
pub mod message;
pub mod names;
pub mod picture;
pub mod screen;
pub mod state;
pub mod widgets;

mod error;

pub use error::{Error, ErrorKind};
pub use message::{Msg, Node};
