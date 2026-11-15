//! The kitty graphics protocol codec: transmit, place, delete and the
//! answer parser, translated from the specification at
//! <https://sw.kovidgoyal.net/kitty/graphics-protocol/>.
//!
//! Pure bytes in and out. Control-data key order is canonical
//! (a, f, t, s, v, i, q, m for transmit) so golden tests can pin exact
//! output, and every command defaults to one-way emission (`q=2`): the
//! callers that want answers opt in, the ones inside host applications
//! stay silent.

mod delete;
mod frames;
mod passthrough;
mod place;
mod response;
#[cfg(test)]
mod tests;
mod transmit;

pub use delete::Delete;
pub use frames::{Animate, AnimationState, Canvas, FrameTransmit, banded_upload};
pub use passthrough::{split_at_commands, wrap_for_passthrough};
pub use place::{PixelRect, Place, Relative};
pub use response::{Response, barrier, parse_responses};
pub use transmit::Transmit;

/// The protocol's maximum chunk payload, in base64 characters.
pub const CHUNK: usize = 4096;

/// An image id in the terminal's global id namespace. Zero means "no id":
/// transmit then uploads without one and place and delete omit the key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImageId(pub u32);

/// The image ids one program owns on a terminal it shares with others:
/// its own high byte over 24-bit local ids. The high byte fits the
/// placeholder diacritic, so anchored ids keep working. The default space
/// leaves local ids as they are.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct IdSpace(u8);

impl IdSpace {
    /// The space a process with id `process` takes: a nonzero high byte.
    // Two processes whose ids match modulo 255 share a space, a rare clash we accept.
    #[must_use]
    pub fn of_process(process: u32) -> Self {
        Self(u8::try_from(process % 255).unwrap_or(0) + 1)
    }

    /// The id `local` takes inside this space.
    #[must_use]
    pub fn image(self, local: u32) -> ImageId {
        ImageId(u32::from(self.0) << 24 | (local & 0x00ff_ffff))
    }
}

/// A placement id, unique per image. Zero omits the key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlacementId(pub u32);

/// Pixel data format (the `f` key).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// 24-bit RGB, three bytes per pixel (`f=24`).
    Rgb,
    /// 32-bit RGBA, four bytes per pixel (`f=32`).
    Rgba,
    /// PNG, dimensions read from the data (`f=100`).
    Png,
}

impl Format {
    fn key(self) -> u32 {
        match self {
            Self::Rgb => 24,
            Self::Rgba => 32,
            Self::Png => 100,
        }
    }
}

/// Answer suppression (the `q` key).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Quiet {
    /// Answer every command.
    Normal,
    /// Suppress success answers, keep errors.
    ErrorsOnly,
    /// Suppress everything, the default here.
    Silent,
}

impl Quiet {
    fn key(self) -> u32 {
        match self {
            Self::Normal => 0,
            Self::ErrorsOnly => 1,
            Self::Silent => 2,
        }
    }
}

/// Frame `command` as a kitty graphics APC: `ESC _ G` + command + `ESC \`.
fn apc(command: &str) -> String {
    let mut out = String::with_capacity(command.len() + 8);
    out.push_str("\x1b_G");
    out.push_str(command);
    out.push_str("\x1b\\");
    out
}
