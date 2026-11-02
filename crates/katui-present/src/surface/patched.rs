//! The patched kitty strategy: one whole transmit, then per-frame
//! retransmits of only the tiles whose pixels changed, each under a
//! stable tile image id whose placement it replaces.

use crate::Sink;
use std::io::{self, Write as _};

use katui_proto::kitty::{
    Format, ImageId as KittyImage, Place, PlacementId as KittyPlacement, Transmit,
};
use katui_proto::probe::CellPixels;
use katui_scene::cell::Position;
use katui_scene::placement::CellRect;
use katui_scene::surface::Surface;
use katui_scene::surface::diff_tiles;

use super::write_whole_image;
use crate::cells::move_cursor;

/// The patched kitty surface presenter: transmits the whole surface once,
/// then per frame retransmits only the tiles whose pixels changed, each
/// as its own small image under a stable tile id, placed over the base.
/// The kitty protocol has no in-place pixel update (retransmitting an id
/// replaces the image and its placements), so a tile id is simply
/// retransmitted and its placement replaced without flicker. A frame
/// with more damaged tiles than the patch budget, a resize, or anything
/// that invalidates the base image falls back to one whole retransmit.
/// An unchanged frame writes nothing.
///
/// The default tile side is `cell.width * cell.height` pixels, a
/// multiple of both cell axes, so every tile maps to whole cells and
/// patch edges never tear. This is the bandwidth workhorse: a moving
/// sprite on a static background costs a handful of small tile
/// retransmits per frame.
#[derive(Debug)]
pub struct PatchedKitty {
    base: u32,
    cell: CellPixels,
    tile_side: u32,
    budget: usize,
    prev: Option<Surface>,
}

impl PatchedKitty {
    /// A presenter for surfaces drawn at `cell` pixel resolution, handing
    /// out image ids from `base` upward, tiling at the cell-derived side
    /// with the sprite-friendly budget of 64 tiles.
    #[must_use]
    pub fn new(base: u32, cell: CellPixels) -> Self {
        Self::with_tiles(
            base,
            cell,
            u32::from(cell.width) * u32::from(cell.height),
            64,
        )
    }

    /// A presenter with an explicit `tile_side` in surface pixels (a
    /// multiple of both cell axes keeps tile edges on cell boundaries)
    /// and `budget`, the damaged-tile count past which one whole
    /// retransmit wins. Fine-grained surfaces want small tiles and a
    /// high budget.
    #[must_use]
    pub fn with_tiles(base: u32, cell: CellPixels, tile_side: u32, budget: usize) -> Self {
        Self {
            base,
            cell,
            tile_side: tile_side.max(1),
            budget,
            prev: None,
        }
    }

    /// Write one frame into `area` cells and return the bytes written.
    pub fn render_frame<W: Sink>(
        &mut self,
        out: &mut W,
        surface: &Surface,
        area: CellRect,
    ) -> io::Result<usize> {
        let fresh_start = self.prev.as_ref().is_none_or(|prev| {
            prev.width() != surface.width() || prev.height() != surface.height()
        });
        let damage = match self.prev.as_ref() {
            Some(prev) if !fresh_start => diff_tiles(prev, surface, self.tile_side),
            _ => Vec::new(),
        };
        let patching = !fresh_start && damage.len() <= self.budget;

        let mut bytes = Vec::new();
        if !patching {
            self.full_frame(&mut bytes, surface, area)?;
        } else if damage.is_empty() {
            self.prev = Some(surface.clone());
            return Ok(0);
        } else {
            bytes.write_all(b"\x1b[?2026h")?;
            for rect in &damage {
                self.patch(&mut bytes, surface, *rect, area)?;
            }
            bytes.write_all(b"\x1b[?2026l")?;
        }
        self.prev = Some(surface.clone());
        let written = bytes.len();
        out.write_all(&bytes)?;
        Ok(written)
    }

    fn full_frame(&self, bytes: &mut Vec<u8>, surface: &Surface, area: CellRect) -> io::Result<()> {
        bytes.write_all(b"\x1b[?2026h")?;
        write_whole_image(bytes, KittyImage(self.base), surface, area)?;
        bytes.write_all(b"\x1b[?2026l")?;
        Ok(())
    }

    /// One damaged tile as its own small image: the tile's pixels are
    /// retransmitted under a stable tile id (which the protocol defines
    /// as replacing that image and its placements), then placed over the
    /// base at the tile's cell rectangle. No flicker: the replacement
    /// placement shares the id pair with the one it replaces.
    fn patch(
        &self,
        bytes: &mut Vec<u8>,
        surface: &Surface,
        rect: katui_scene::surface::PixelRect,
        area: CellRect,
    ) -> io::Result<()> {
        // narrowing through try_from saturating, the file's one idiom
        let columns =
            u16::try_from(rect.width.div_ceil(u32::from(self.cell.width))).unwrap_or(u16::MAX);
        let rows =
            u16::try_from(rect.height.div_ceil(u32::from(self.cell.height))).unwrap_or(u16::MAX);
        let origin_x = area
            .x
            .saturating_add(u16::try_from(rect.x / u32::from(self.cell.width)).unwrap_or(u16::MAX));
        let origin_y = area.y.saturating_add(
            u16::try_from(rect.y / u32::from(self.cell.height)).unwrap_or(u16::MAX),
        );
        let tile_index = (rect.x / self.tile_side, rect.y / self.tile_side);
        let image = KittyImage(self.base + 2 + tile_index.0 + tile_index.1 * 4096);

        let pixels = tile_pixels(surface, rect);
        let mut transmit = Transmit::new(image, Format::Rgba, rect.width, rect.height, &pixels);
        transmit.compressed = true;
        for frame in transmit.encode()? {
            bytes.write_all(&frame)?;
        }
        move_cursor(
            bytes,
            Position {
                x: origin_x,
                y: origin_y,
            },
        )?;
        let mut place = Place::new(image);
        place.placement = KittyPlacement(2 + tile_index.0 + tile_index.1 * 4096);
        place.columns = u32::from(columns);
        place.rows = u32::from(rows);
        bytes.write_all(&place.encode())
    }
}

/// The tile's RGBA rows out of the surface, row-major, tile width per
/// row: exactly the payload a tile transmit declares.
fn tile_pixels(surface: &Surface, rect: katui_scene::surface::PixelRect) -> Vec<u8> {
    let row_bytes = (surface.width() * 4) as usize;
    let mut tile = Vec::with_capacity((rect.width * rect.height * 4) as usize);
    for (row_index, row) in surface.pixels().chunks(row_bytes).enumerate() {
        if (row_index as u32) < rect.y || (row_index as u32) >= rect.y + rect.height {
            continue;
        }
        let start = (rect.x * 4) as usize;
        let end = start + (rect.width * 4) as usize;
        if let Some(slice) = row.get(start..end) {
            tile.extend_from_slice(slice);
        }
    }
    tile
}

#[cfg(test)]
mod tests {
    use super::PatchedKitty;
    use katui_proto::probe::CellPixels;
    use katui_scene::placement::CellRect;
    use katui_scene::surface::{Rgba, Surface};

    // 8x8 px tiles with a 2x4 cell: 2*4=8, a multiple of both axes
    fn cell() -> CellPixels {
        CellPixels {
            width: 2,
            height: 4,
        }
    }

    fn area() -> CellRect {
        CellRect {
            x: 0,
            y: 0,
            columns: 16,
            rows: 16,
        }
    }

    fn plain(width: u32, height: u32) -> Surface {
        let mut surface = Surface::new(width, height);
        for y in 0..height {
            for x in 0..width {
                surface.set(
                    katui_scene::surface::PixelPos::new(x, y),
                    Rgba::solid(10, 10, 30),
                );
            }
        }
        surface
    }

    #[test]
    fn the_first_frame_transmits_the_whole_surface() {
        let mut presenter = PatchedKitty::new(90, cell());
        let mut out = Vec::new();
        presenter
            .render_frame(&mut out, &plain(64, 64), area())
            .expect("writing to a Vec cannot fail");
        let text = String::from_utf8_lossy(&out);
        assert!(text.contains("a=t"), "the whole image travels");
        assert!(text.contains("o=z"), "compressed");
        assert!(text.contains("a=p,i=90"), "and is placed");
    }

    #[test]
    fn an_identical_frame_writes_nothing() {
        let mut presenter = PatchedKitty::new(90, cell());
        let surface = plain(64, 64);
        let mut first = Vec::new();
        presenter
            .render_frame(&mut first, &surface, area())
            .expect("writing to a Vec cannot fail");
        let mut second = Vec::new();
        let written = presenter
            .render_frame(&mut second, &surface, area())
            .expect("writing to a Vec cannot fail");
        assert_eq!(written, 0, "no damage means no bytes");
        assert!(second.is_empty());
    }

    #[test]
    fn a_small_change_retransmits_only_its_tile() {
        let mut presenter = PatchedKitty::new(90, cell());
        let mut surface = plain(64, 64);
        let mut warmup = Vec::new();
        presenter
            .render_frame(&mut warmup, &surface, area())
            .expect("writing to a Vec cannot fail");
        surface.set(
            katui_scene::surface::PixelPos::new(40, 40),
            Rgba::solid(255, 255, 255),
        );
        let mut out = Vec::new();
        presenter
            .render_frame(&mut out, &surface, area())
            .expect("writing to a Vec cannot fail");
        let text = String::from_utf8_lossy(&out);
        assert!(
            !text.contains("s=64"),
            "the whole surface does not travel again"
        );
        assert!(
            text.contains("s=8,v=8"),
            "the damaged tile travels, 8x8 pixels at tile side 8"
        );
        assert!(
            text.contains("a=p,i=20577") && text.contains("c=4,r=2"),
            "the tile image is placed over its 4x2 cells"
        );
    }

    #[test]
    fn a_whole_surface_change_falls_back_to_one_retransmit() {
        // 128x128 at 8px tiles is 256 tiles: everything dirty blows
        // the patch budget
        let mut presenter = PatchedKitty::new(90, cell());
        let mut surface = plain(128, 128);
        let big_area = CellRect {
            x: 0,
            y: 0,
            columns: 32,
            rows: 32,
        };
        let mut warmup = Vec::new();
        presenter
            .render_frame(&mut warmup, &surface, big_area)
            .expect("writing to a Vec cannot fail");
        for y in 0..128u32 {
            for x in 0..128u32 {
                surface.set(
                    katui_scene::surface::PixelPos::new(x, y),
                    Rgba::solid(200, 40, 40),
                );
            }
        }
        let mut out = Vec::new();
        presenter
            .render_frame(&mut out, &surface, big_area)
            .expect("writing to a Vec cannot fail");
        let text = String::from_utf8_lossy(&out);
        assert!(
            text.contains("a=t"),
            "everything dirty means one full frame"
        );
    }

    #[test]
    fn a_size_change_falls_back_to_one_retransmit() {
        let mut presenter = PatchedKitty::new(90, cell());
        let mut out = Vec::new();
        presenter
            .render_frame(&mut out, &plain(64, 64), area())
            .expect("writing to a Vec cannot fail");
        let mut resized = Vec::new();
        presenter
            .render_frame(&mut resized, &plain(32, 32), area())
            .expect("writing to a Vec cannot fail");
        let text = String::from_utf8_lossy(&resized);
        assert!(text.contains("a=t"), "a resize retransmits whole");
    }
}
