//! Arcade games on katui: draw into a [`Light`] buffer, read [`HeldKeys`]
//! and let [`play`] run the fixed-step loop.
#![cfg(unix)] // the runner drives /dev/tty through katui-rt's unix layer

mod arcade;
mod font;
mod keys;
mod light;
mod space;

pub use arcade::{Game, play};
pub use katui_proto::input::Key;
pub use keys::HeldKeys;
pub use light::{Blend, Light, P2, Rgb, scaled};
pub use space::{Camera, Rng, Spark, Sparks, V3, v3};
