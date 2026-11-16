//! Arcade games on katui: draw into a [`Light`] buffer, read [`HeldKeys`]
//! and let [`play`] run the fixed-step loop.

mod arcade;
mod font;
mod keys;
mod light;
mod space;

pub use arcade::{BASE_IMAGE, Game, MULTIPLEXED_SHRINK, play};
pub use katui_proto::input::{Button, Key, MouseAction, MouseEvent};
pub use keys::HeldKeys;
pub use light::{Blend, Light, P2, Rgb, lerp3, scaled, srgb};
pub use space::{Camera, Rng, Spark, Sparks, V3, v3};
