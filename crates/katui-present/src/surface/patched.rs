//! The patched kitty strategy: one whole transmit, then per-frame crop
//! placements of the tiles that changed.

use crate::Sink;
use std::io::{self, Write as _};

use katui_proto::kitty::{ImageId as KittyImage, Place, PlacementId as KittyPlacement};
use katui_proto::probe::CellPixels;
use katui_scene::cell::Position;
use katui_scene::placement::CellRect;
use katui_scene::surface::Surface;
use katui_scene::surface::diff_tiles;

use super::write_whole_image;
use crate::cells::move_cursor;

/// The patched kitty surface presenter: transmits the whole surface once,
/// then per frame re-places only the tiles whose pixels changed, as crop
/// placements of the transmitted image. A frame with more than 64
/// damaged tiles, a resize, or anything that invalidates the base image
/// falls back to one whole retransmit, which is cheaper than hundreds of
/// placements. An unchanged frame writes nothing.
///
/// The tile side is `cell.width * cell.height` pixels, a multiple of both
/// cell axes, so every tile maps to whole cells and patch edges never
/// tear. This is the bandwidth workhorse: a moving sprite on a static
/// background costs a handful of small placements per frame.
#[derive(Debug)]
pub struct PatchedKitty {
    base: u32,
    cell: CellPixels,
    prev: Option<Surface>,
}

impl PatchedKitty {
    /// A presenter for surfaces drawn at `cell` pixel resolution, handing
    /// out image ids from `base` upward.
    #[must_use]
    pub fn new(base: u32, cell: CellPixels) -> Self {
        Self {
            base,
            cell,
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
        let tile = self.tile_side();
        let damage = match self.prev.as_ref() {
            Some(prev) if !fresh_start => diff_tiles(prev, surface, tile),
            _ => Vec::new(),
        };
        let patching = !fresh_start && damage.len() <= 64;

        let mut bytes = Vec::new();
        if !patching {
            self.full_frame(&mut bytes, surface, area)?;
        } else if damage.is_empty() {
            self.prev = Some(surface.clone());
            return Ok(0);
        } else {
            bytes.write_all(b"\x1b[?2026h")?;
            for rect in &damage {
                self.patch(&mut bytes, *rect, area)?;
            }
            bytes.write_all(b"\x1b[?2026l")?;
        }
        self.prev = Some(surface.clone());
        let written = bytes.len();
        out.write_all(&bytes)?;
        Ok(written)
    }

    fn tile_side(&self) -> u32 {
        u32::from(self.cell.width) * u32::from(self.cell.height)
    }

    fn full_frame(&self, bytes: &mut Vec<u8>, surface: &Surface, area: CellRect) -> io::Result<()> {
        bytes.write_all(b"\x1b[?2026h")?;
        write_whole_image(bytes, KittyImage(self.base), surface, area)?;
        bytes.write_all(b"\x1b[?2026l")?;
        Ok(())
    }

    /// One damaged tile as a crop placement of the base image: the source
    /// rectangle selects the tile's pixels, the cell rectangle says where
    /// they land, the stable placement id makes re-paints overwrite in
    /// place.
    fn patch(
        &self,
        bytes: &mut Vec<u8>,
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
        move_cursor(
            bytes,
            Position {
                x: origin_x,
                y: origin_y,
            },
        )?;
        let tile_index = (rect.x / self.tile_side(), rect.y / self.tile_side());
        let mut place = Place::new(KittyImage(self.base));
        place.placement = KittyPlacement(2 + tile_index.0 + tile_index.1 * 4096);
        place.columns = u32::from(columns);
        place.rows = u32::from(rows);
        place.crop = Some(crate::kitty::crop_rect(rect));
        bytes.write_all(&place.encode())
    }
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
    fn a_small_change_patches_only_its_tile() {
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
        assert!(!text.contains("a=t"), "no retransmit");
        assert!(text.contains("a=p,i=90"), "a crop placement runs");
        assert!(text.contains("x=40,y=40"), "the crop names its tile");
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
