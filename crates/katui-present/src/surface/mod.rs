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

/// Whether so much of a fine tile grid changed that one whole image is
/// the better package for the frame.
pub(crate) fn damage_is_dense(surface: &Surface, tile_side: u32, damaged: usize) -> bool {
    // on a coarse grid a tile is already most of a frame, so patching
    // stays cheaper there
    const FINE_GRID_TILES: u64 = 8;
    let across = surface.width().div_ceil(tile_side);
    let down = surface.height().div_ceil(tile_side);
    let total = u64::from(across.saturating_mul(down).max(1));
    total >= FINE_GRID_TILES && 4 * u64::try_from(damaged).unwrap_or(u64::MAX) >= total
}

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

/// Byte budgets per frame for the case every game hits: a small sprite
/// moving over a static scene. A presenter that regresses into sending
/// whole frames fails here long before anyone notices a slow game.
#[cfg(test)]
mod budget_tests {
    use katui_proto::probe::CellPixels;
    use katui_scene::placement::CellRect;
    use katui_scene::surface::{PixelPos, Rgba, Surface};

    use super::{AnimatedKitty, BlocksSurface, PatchedKitty};

    const WIDTH: u32 = 640;
    const HEIGHT: u32 = 360;
    const SPRITE: u32 = 16;
    const FRAMES: u32 = 30;
    /// About four times what each strategy spends today, and a hundredth
    /// of one whole frame.
    const BUDGET: usize = 8 * 1024;

    fn background() -> Surface {
        let mut surface = Surface::new(WIDTH, HEIGHT);
        for y in 0..HEIGHT {
            for x in 0..WIDTH {
                let shade = Rgba::solid((x / 3) as u8, (y / 2) as u8, 90);
                surface.set(PixelPos::new(x, y), shade);
            }
        }
        surface
    }

    fn scene(background: &Surface, frame: u32) -> Surface {
        let mut surface = background.clone();
        let left = 40 + frame * 4;
        for y in 100..100 + SPRITE {
            for x in left..left + SPRITE {
                surface.set(PixelPos::new(x, y), Rgba::solid(255, 240, 40));
            }
        }
        surface
    }

    /// The largest frame after the first, in bytes.
    fn worst_frame(mut render: impl FnMut(&mut Vec<u8>, &Surface) -> usize) -> usize {
        let background = background();
        let mut out = Vec::new();
        render(&mut out, &scene(&background, 0));
        (1..FRAMES)
            .map(|frame| {
                out.clear();
                render(&mut out, &scene(&background, frame))
            })
            .max()
            .unwrap_or(0)
    }

    fn area() -> CellRect {
        CellRect::full(80, 20)
    }

    #[test]
    fn a_moving_sprite_costs_a_few_kilobytes_a_frame_in_every_strategy() {
        let cell = CellPixels {
            width: 8,
            height: 18,
        };
        let mut patched = PatchedKitty::with_tiles(900, cell, 32, 64);
        let patched_worst = worst_frame(|out, surface| {
            patched
                .render_frame(out, surface, area())
                .expect("writing to a Vec cannot fail")
        });
        let mut tiled = PatchedKitty::with_tiles(900, cell, 32, 64).for_multiplexer(true);
        let tiled_worst = worst_frame(|out, surface| {
            tiled
                .render_frame(out, surface, area())
                .expect("writing to a Vec cannot fail")
        });
        let mut animated = AnimatedKitty::new(900, 60, 32, 64);
        let animated_worst = worst_frame(|out, surface| {
            animated
                .render_frame(out, surface, area())
                .expect("writing to a Vec cannot fail")
        });
        let mut blocks = BlocksSurface::new();
        let blocks_worst = worst_frame(|out, surface| {
            blocks
                .render_frame(out, surface)
                .expect("writing to a Vec cannot fail")
        });
        let worst = [
            ("patched", patched_worst),
            ("multiplexer tiles", tiled_worst),
            ("animated", animated_worst),
            ("blocks", blocks_worst),
        ];
        for (strategy, bytes) in worst {
            assert!(bytes > 0, "{strategy} sends the moved sprite");
            assert!(
                bytes <= BUDGET,
                "{strategy} spent {bytes} bytes on one frame"
            );
        }
    }
}
