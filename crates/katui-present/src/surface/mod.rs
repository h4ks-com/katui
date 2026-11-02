//! Surface presenters: two kitty strategies, the whole-frame baseline
//! with alternating image ids and the tile-patched refinement, plus the
//! damage-tracked blocks fallback every truecolor terminal runs.

mod animated;
mod blocks;
mod buffer;
mod full;
mod patched;
mod screen;

pub use animated::AnimatedKitty;
pub use blocks::{BlocksSurface, render_blocks};
pub use buffer::BufferSurface;
pub use full::KittySurface;
pub use patched::PatchedKitty;
pub use screen::PixelScreen;

use crate::Sink;
use std::io;

use katui_proto::kitty::{
    Format, ImageId as KittyImage, Place, PlacementId as KittyPlacement, Transmit,
};
use katui_scene::cell::Position;
use katui_scene::placement::CellRect;
use katui_scene::surface::Surface;

use crate::cells::move_cursor;

/// One whole surface as a frame image: a compressed transmit under
/// `image`, the cursor at the area origin, one placement filling `area`.
/// The caller owns the sync wrapper and any previous-placement delete.
pub(crate) fn write_whole_image<W: Sink>(
    bytes: &mut W,
    image: KittyImage,
    surface: &Surface,
    area: CellRect,
) -> io::Result<()> {
    let mut transmit = Transmit::new(
        image,
        Format::Rgba,
        surface.width(),
        surface.height(),
        surface.pixels(),
    );
    transmit.compressed = true;
    for frame in transmit.encode()? {
        bytes.write_all(&frame)?;
    }
    move_cursor(
        bytes,
        Position {
            x: area.x,
            y: area.y,
        },
    )?;
    let mut place = Place::new(image);
    place.placement = KittyPlacement(1);
    place.columns = u32::from(area.columns);
    place.rows = u32::from(area.rows);
    bytes.write_all(&place.encode())
}

/// The solid-surface fixture the surface tests share.
#[cfg(test)]
pub(crate) mod test_support {
    use katui_scene::surface::{Rgba, Surface};

    pub(crate) fn solid(width: u32, height: u32, color: Rgba) -> Surface {
        let mut surface = Surface::new(width, height);
        for y in 0..height {
            for x in 0..width {
                surface.set(katui_scene::surface::PixelPos::new(x, y), color);
            }
        }
        surface
    }
}
