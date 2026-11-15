//! The patched kitty strategy: one whole transmit, then per-frame
//! retransmits of only the tiles whose pixels changed, each under a
//! stable tile image id whose placement it replaces.

use crate::Sink;
use std::collections::HashMap;
use std::io::{self, Write as _};

use katui_proto::kitty::{
    Delete, Format, IdSpace, ImageId as KittyImage, Place, PlacementId as KittyPlacement, Transmit,
};
use katui_proto::probe::CellPixels;
use katui_scene::cell::Position;
use katui_scene::placement::CellRect;
use katui_scene::surface::{PixelRect, Surface, diff_tiles};

use super::{damage_is_dense, write_whole_image};
use crate::anchor::Anchor;
use crate::cells::move_cursor;
use crate::kitty::crop_rect;

/// The patched kitty surface presenter: transmits the whole surface once,
/// then per frame retransmits only the tiles whose pixels changed, each
/// as its own small image under a stable tile id, placed over the base.
/// The kitty protocol has no in-place pixel update (retransmitting an id
/// replaces the image and its placements), so a tile id is simply
/// retransmitted and its placement replaced without flicker. A frame
/// with more damaged tiles than the patch budget, a resize, or anything
/// that loses the base image falls back to one whole retransmit.
/// An unchanged frame writes nothing.
///
/// The default tile side is `cell.width * cell.height` pixels, a
/// multiple of both cell axes, so every tile maps to whole cells and
/// patch edges never tear.
#[derive(Debug)]
pub struct PatchedKitty {
    base: u32,
    tile_side: u32,
    budget: usize,
    prev: Option<Surface>,
    base_shown: Option<KittyImage>,
    tile_generations: HashMap<u32, bool>,
    anchor: Option<Anchor>,
    repaint: bool,
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

    /// A presenter with an explicit `tile_side` in surface pixels and
    /// `budget`, the damaged-tile count past which one whole retransmit
    /// wins. Fine-grained surfaces want small tiles and a high budget;
    /// the tile's cell span always follows the surface-to-area scale.
    #[must_use]
    pub fn with_tiles(base: u32, _cell: CellPixels, tile_side: u32, budget: usize) -> Self {
        Self {
            base,
            tile_side: tile_side.max(1),
            budget,
            prev: None,
            base_shown: None,
            tile_generations: HashMap::new(),
            anchor: None,
            repaint: false,
        }
    }

    /// Send every pixel again on the next frame, for when the terminal
    /// may have lost what it was showing.
    pub fn repaint(&mut self) {
        self.repaint = true;
    }

    /// Free every image this presenter sent and repaint on the next frame.
    pub fn free<W: Sink>(&mut self, out: &mut W) -> io::Result<()> {
        let mut bytes = Vec::new();
        self.free_tiles(&mut bytes)?;
        for image in [self.base, self.base + 1] {
            bytes.write_all(&Delete::Free(KittyImage(image)).encode())?;
        }
        self.base_shown = None;
        self.repaint = true;
        out.write_all(&bytes)
    }

    /// Send every frame as small tiles placed relative to a placeholder
    /// anchor, for a multiplexer: it drops uploads bigger than it buffers,
    /// and it hides the anchor, and so every tile, with its pane.
    #[must_use]
    pub fn for_multiplexer(mut self, space: IdSpace) -> Self {
        self.base = space.image(self.base).0;
        self.anchor = Some(Anchor::new(space));
        self
    }

    /// Write one frame into `area` cells and return the bytes written.
    pub fn render_frame<W: Sink>(
        &mut self,
        out: &mut W,
        surface: &Surface,
        area: CellRect,
    ) -> io::Result<usize> {
        let resized = self.prev.as_ref().is_none_or(|prev| {
            prev.width() != surface.width() || prev.height() != surface.height()
        });
        let repaint = std::mem::take(&mut self.repaint);
        if self.anchor.is_some() {
            return self.render_tiles(out, surface, area, resized, repaint);
        }
        let fresh_start = resized || repaint;
        let damage = match self.prev.as_ref() {
            Some(prev) if !fresh_start => diff_tiles(prev, surface, self.tile_side),
            _ => Vec::new(),
        };
        let dense = damage_is_dense(surface, self.tile_side, damage.len());
        let patching = !fresh_start && !dense && damage.len() <= self.budget;

        let mut bytes = Vec::new();
        if !patching {
            self.full_frame(&mut bytes, surface, area)?;
        } else if damage.is_empty() {
            self.prev = Some(surface.clone());
            return Ok(0);
        } else {
            self.patch_all(&mut bytes, surface, &damage, area)?;
        }
        self.prev = Some(surface.clone());
        let written = bytes.len();
        out.write_all(&bytes)?;
        Ok(written)
    }

    /// Every damaged rectangle as one synchronized update.
    fn patch_all(
        &mut self,
        bytes: &mut Vec<u8>,
        surface: &Surface,
        damage: &[PixelRect],
        area: CellRect,
    ) -> io::Result<()> {
        bytes.write_all(b"\x1b[?2026h")?;
        for rect in damage {
            self.patch(bytes, surface, *rect, area)?;
        }
        bytes.write_all(b"\x1b[?2026l")
    }

    /// A repaint keeps the old tiles up until each one is replaced.
    fn render_tiles<W: Sink>(
        &mut self,
        out: &mut W,
        surface: &Surface,
        area: CellRect,
        resized: bool,
        repaint: bool,
    ) -> io::Result<usize> {
        let blank = Surface::new(surface.width(), surface.height());
        let before = self
            .prev
            .as_ref()
            .filter(|_| !resized && !repaint)
            .unwrap_or(&blank);
        let damage = diff_tiles(before, surface, self.tile_side);
        let mut bytes = Vec::new();
        if resized {
            self.free_tiles(&mut bytes)?;
        }
        if !damage.is_empty()
            && let Some(anchor) = self.anchor.as_mut()
        {
            anchor.write(&mut bytes)?;
        }
        for rect in &damage {
            self.patch(&mut bytes, surface, *rect, area)?;
        }
        self.prev = Some(surface.clone());
        out.write_all(&bytes)?;
        Ok(bytes.len())
    }

    fn free_tiles(&mut self, bytes: &mut Vec<u8>) -> io::Result<()> {
        if let Some(last) = self.tile_generations.keys().max().copied() {
            let range = Delete::FreeRange(self.tile_image(0, false), self.tile_image(last, true));
            bytes.write_all(&range.encode())?;
            self.tile_generations.clear();
        }
        Ok(())
    }

    /// The whole surface under the base id not on screen, so the old
    /// image stays visible until the new one is placed: multiplexers
    /// drop synchronized updates, and retransmitting a visible id
    /// removes its placement for a moment.
    fn full_frame(
        &mut self,
        bytes: &mut Vec<u8>,
        surface: &Surface,
        area: CellRect,
    ) -> io::Result<()> {
        let image = if self.base_shown == Some(KittyImage(self.base)) {
            KittyImage(self.base + 1)
        } else {
            KittyImage(self.base)
        };
        bytes.write_all(b"\x1b[?2026h")?;
        write_whole_image(bytes, image, surface, area)?;
        // old tiles sit above the base and would hide the new pixels
        self.free_tiles(bytes)?;
        if let Some(shown) = self.base_shown.replace(image) {
            bytes.write_all(&Delete::Free(shown).encode())?;
        }
        bytes.write_all(b"\x1b[?2026l")?;
        Ok(())
    }

    fn tile_index(&self, x: u32, y: u32) -> u32 {
        x / self.tile_side + (y / self.tile_side) * 4096
    }

    fn tile_image(&self, index: u32, second_generation: bool) -> KittyImage {
        KittyImage(self.base + 2 + index * 2 + u32::from(second_generation))
    }

    /// One damaged tile as its own small image, placed over the base at
    /// the tile's cell rectangle. Each tile alternates between two ids
    /// so its previous pixels stay visible until the new ones are placed.
    fn patch(
        &mut self,
        bytes: &mut Vec<u8>,
        surface: &Surface,
        rect: PixelRect,
        area: CellRect,
    ) -> io::Result<()> {
        // the tile's cell span comes from the surface-to-area scale, not
        // the terminal's cell pixels: the caller's surface may run at any
        // resolution and the base image already proved the scale
        let columns = u16::try_from(rect.width * u32::from(area.columns) / surface.width())
            .unwrap_or(u16::MAX);
        let rows = u16::try_from(rect.height * u32::from(area.rows) / surface.height())
            .unwrap_or(u16::MAX);
        let origin_x = area.x.saturating_add(
            u16::try_from(rect.x * u32::from(area.columns) / surface.width()).unwrap_or(u16::MAX),
        );
        let origin_y = area.y.saturating_add(
            u16::try_from(rect.y * u32::from(area.rows) / surface.height()).unwrap_or(u16::MAX),
        );
        let index = self.tile_index(rect.x, rect.y);
        let shown = self.tile_generations.get(&index).copied();
        let generation = shown.is_some_and(|generation| !generation);
        let image = self.tile_image(index, generation);

        let padded = with_apron(rect, surface);
        let pixels = tile_pixels(surface, padded);
        let mut transmit = Transmit::new(image, Format::Rgba, padded.width, padded.height, &pixels);
        transmit.compressed = true;
        for frame in transmit.encode()? {
            bytes.write_all(&frame)?;
        }
        let origin = Position {
            x: origin_x,
            y: origin_y,
        };
        let mut place = Place::new(image);
        place.columns = u32::from(columns);
        place.rows = u32::from(rows);
        place.placement = KittyPlacement(2 + index);
        place.crop = Some(crop_rect(PixelRect {
            x: rect.x - padded.x,
            y: rect.y - padded.y,
            width: rect.width,
            height: rect.height,
        }));
        if let Some(anchor) = &self.anchor {
            place.parent = Some(anchor.offset_to(origin));
        } else {
            move_cursor(bytes, origin)?;
        }
        bytes.write_all(&place.encode())?;
        if let Some(previous) = shown {
            bytes.write_all(&Delete::Free(self.tile_image(index, previous)).encode())?;
        }
        self.tile_generations.insert(index, generation);
        Ok(())
    }
}

/// `rect` grown by one pixel on every side the surface has room for. A
/// terminal filters each image on its own, so we send a tile with its
/// neighbours' edge pixels and crop them away, which blends tile edges the
/// way one whole image would.
fn with_apron(rect: PixelRect, surface: &Surface) -> PixelRect {
    let left = rect.x.saturating_sub(1);
    let top = rect.y.saturating_sub(1);
    let right = (rect.x + rect.width + 1).min(surface.width());
    let bottom = (rect.y + rect.height + 1).min(surface.height());
    PixelRect {
        x: left,
        y: top,
        width: right - left,
        height: bottom - top,
    }
}

/// The tile's RGBA rows out of the surface, row-major, tile width per
/// row: exactly the payload a tile transmit declares.
pub(super) fn tile_pixels(surface: &Surface, rect: PixelRect) -> Vec<u8> {
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

    fn plain_surface(width: u32, height: u32) -> Surface {
        crate::surface::test_support::solid(width, height, Rgba::solid(10, 10, 30))
    }
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

    #[test]
    fn the_first_frame_transmits_the_whole_surface() {
        let mut presenter = PatchedKitty::new(90, cell());
        let mut out = Vec::new();
        presenter
            .render_frame(
                &mut out,
                &plain_surface(64, 64),
                crate::surface::test_support::area(),
            )
            .expect("writing to a Vec cannot fail");
        let text = String::from_utf8_lossy(&out);
        assert!(text.contains("a=t"), "the whole image travels");
        assert!(text.contains("o=z"), "compressed");
        assert!(text.contains("a=p,i=90"), "and is placed");
    }

    #[test]
    fn an_identical_frame_writes_nothing() {
        let mut presenter = PatchedKitty::new(90, cell());
        let surface = plain_surface(64, 64);
        let mut first = Vec::new();
        presenter
            .render_frame(&mut first, &surface, crate::surface::test_support::area())
            .expect("writing to a Vec cannot fail");
        let mut second = Vec::new();
        let written = presenter
            .render_frame(&mut second, &surface, crate::surface::test_support::area())
            .expect("writing to a Vec cannot fail");
        assert_eq!(written, 0, "no damage means no bytes");
        assert!(second.is_empty());
    }

    #[test]
    fn a_small_change_retransmits_only_its_tile() {
        let mut presenter = PatchedKitty::new(90, cell());
        let mut surface = plain_surface(64, 64);
        let mut warmup = Vec::new();
        presenter
            .render_frame(&mut warmup, &surface, crate::surface::test_support::area())
            .expect("writing to a Vec cannot fail");
        surface.set(
            katui_scene::surface::PixelPos::new(40, 40),
            Rgba::solid(255, 255, 255),
        );
        let mut out = Vec::new();
        presenter
            .render_frame(&mut out, &surface, crate::surface::test_support::area())
            .expect("writing to a Vec cannot fail");
        let text = String::from_utf8_lossy(&out);
        assert!(
            !text.contains("s=64"),
            "the whole surface does not travel again"
        );
        assert!(
            text.contains("s=10,v=10"),
            "the damaged 8x8 tile travels with a pixel of apron on each side"
        );
        assert!(
            text.contains("x=1,y=1,w=8,h=8"),
            "the placement crops the apron away"
        );
        assert!(
            text.contains("a=p,i=41062") && text.contains("c=2,r=2"),
            "the tile covers its 2x2 cells at the surface's scale"
        );
    }

    #[test]
    fn a_whole_frame_frees_the_tiles_that_would_cover_it() {
        let mut presenter = PatchedKitty::new(90, cell());
        let mut surface = plain_surface(64, 64);
        let mut wire = Vec::new();
        presenter
            .render_frame(&mut wire, &surface, crate::surface::test_support::area())
            .expect("writing to a Vec cannot fail");
        assert!(
            !String::from_utf8_lossy(&wire).contains("d=R"),
            "no tiles exist yet"
        );
        surface.set(
            katui_scene::surface::PixelPos::new(40, 40),
            Rgba::solid(255, 255, 255),
        );
        presenter
            .render_frame(&mut wire, &surface, crate::surface::test_support::area())
            .expect("writing to a Vec cannot fail");
        wire.clear();
        for y in (0..64).step_by(8) {
            for x in (0..64).step_by(8) {
                surface.set(
                    katui_scene::surface::PixelPos::new(x, y),
                    Rgba::solid(1, 2, 3),
                );
            }
        }
        presenter
            .render_frame(&mut wire, &surface, crate::surface::test_support::area())
            .expect("writing to a Vec cannot fail");
        let text = String::from_utf8_lossy(&wire);
        let placed = text.find("a=p,i=").expect("the new base is placed");
        let freed = text.find("d=R").expect("the stale tiles are freed");
        assert!(placed < freed, "the new base appears before the tiles go");
    }

    fn space() -> katui_proto::kitty::IdSpace {
        katui_proto::kitty::IdSpace::of_process(4242)
    }

    fn anchored() -> String {
        let anchor = space().image(0x00ff_fff0);
        format!(",P={},", anchor.0)
    }

    #[test]
    fn a_multiplexer_grid_places_tiles_on_the_anchor_and_never_a_whole_image() {
        let mut presenter = PatchedKitty::new(90, cell()).for_multiplexer(space());
        let surface = plain_surface(64, 64);
        let mut wire = Vec::new();
        presenter
            .render_frame(&mut wire, &surface, crate::surface::test_support::area())
            .expect("writing to a Vec cannot fail");
        let text = String::from_utf8_lossy(&wire);
        assert!(!text.contains("s=64"), "no whole image travels");
        assert_eq!(
            text.matches(&anchored()).count(),
            64,
            "every tile sits on the anchor"
        );
        assert_eq!(
            text.matches('\u{10eeee}').count(),
            1,
            "one placeholder cell anchors the grid"
        );
    }

    #[test]
    fn a_repainted_grid_resends_every_tile_and_keeps_the_anchor() {
        let mut presenter = PatchedKitty::new(90, cell()).for_multiplexer(space());
        let surface = plain_surface(64, 64);
        let mut wire = Vec::new();
        presenter
            .render_frame(&mut wire, &surface, crate::surface::test_support::area())
            .expect("writing to a Vec cannot fail");
        presenter.repaint();
        wire.clear();
        presenter
            .render_frame(&mut wire, &surface, crate::surface::test_support::area())
            .expect("writing to a Vec cannot fail");
        let text = String::from_utf8_lossy(&wire);
        assert_eq!(text.matches(&anchored()).count(), 64, "every tile returns");
        assert_eq!(
            text.matches("a=t,").count(),
            64,
            "only the tiles travel, because sending the anchor again would drop every tile on it"
        );
        assert!(!text.contains("d=R"), "old tiles stay until replaced");
        wire.clear();
        presenter
            .render_frame(&mut wire, &surface, crate::surface::test_support::area())
            .expect("writing to a Vec cannot fail");
        assert!(wire.is_empty(), "one repaint request repaints once");
    }

    #[test]
    fn a_multiplexer_grid_frees_the_old_tiles_on_a_resize() {
        let mut presenter = PatchedKitty::new(90, cell()).for_multiplexer(space());
        let mut wire = Vec::new();
        presenter
            .render_frame(
                &mut wire,
                &plain_surface(64, 64),
                crate::surface::test_support::area(),
            )
            .expect("writing to a Vec cannot fail");
        wire.clear();
        presenter
            .render_frame(
                &mut wire,
                &plain_surface(32, 32),
                crate::surface::test_support::area(),
            )
            .expect("writing to a Vec cannot fail");
        let text = String::from_utf8_lossy(&wire);
        let freed = text.find("d=R").expect("the old grid is freed");
        let placed = text.find("a=p,").expect("the new grid is placed");
        assert!(freed < placed);
    }

    #[test]
    fn a_base_retransmit_never_reuses_the_visible_id() {
        let mut presenter = PatchedKitty::with_tiles(90, cell(), 8, 0);
        let mut surface = plain_surface(16, 16);
        let mut first = Vec::new();
        presenter
            .render_frame(&mut first, &surface, crate::surface::test_support::area())
            .expect("writing to a Vec cannot fail");
        surface.set(
            katui_scene::surface::PixelPos::new(1, 1),
            Rgba::solid(9, 9, 9),
        );
        let mut second = Vec::new();
        presenter
            .render_frame(&mut second, &surface, crate::surface::test_support::area())
            .expect("writing to a Vec cannot fail");
        let text = String::from_utf8_lossy(&second);
        let placed = text.find("a=p,i=91").expect("the alternate id is placed");
        let freed = text.find("d=I,i=90").expect("the old base is freed");
        assert!(placed < freed, "something is always on screen");
    }

    #[test]
    fn a_tile_alternates_ids_and_frees_the_previous_one_after_placing() {
        let mut presenter = PatchedKitty::new(90, cell());
        let mut surface = plain_surface(64, 64);
        let mut wire = Vec::new();
        presenter
            .render_frame(&mut wire, &surface, crate::surface::test_support::area())
            .expect("writing to a Vec cannot fail");
        for shade in [10, 20] {
            surface.set(
                katui_scene::surface::PixelPos::new(40, 40),
                Rgba::solid(shade, shade, shade),
            );
            wire.clear();
            presenter
                .render_frame(&mut wire, &surface, crate::surface::test_support::area())
                .expect("writing to a Vec cannot fail");
        }
        let text = String::from_utf8_lossy(&wire);
        let placed = text.find("a=p,i=41063").expect("the second generation");
        let freed = text.find("d=I,i=41062").expect("the first is freed");
        assert!(placed < freed, "the old tile stays until the new one shows");
    }

    #[test]
    fn a_whole_surface_change_falls_back_to_one_retransmit() {
        // 128x128 at 8px tiles is 256 tiles: everything dirty blows
        // the patch budget
        let mut presenter = PatchedKitty::new(90, cell());
        let mut surface = plain_surface(128, 128);
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
            .render_frame(
                &mut out,
                &plain_surface(64, 64),
                crate::surface::test_support::area(),
            )
            .expect("writing to a Vec cannot fail");
        let mut resized = Vec::new();
        presenter
            .render_frame(
                &mut resized,
                &plain_surface(32, 32),
                crate::surface::test_support::area(),
            )
            .expect("writing to a Vec cannot fail");
        let text = String::from_utf8_lossy(&resized);
        assert!(text.contains("a=t"), "a resize retransmits whole");
    }
}
