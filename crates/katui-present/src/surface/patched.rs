//! The patched kitty strategy: one whole transmit, then per-frame
//! retransmits of only the tiles whose pixels changed, each under a
//! stable tile image id whose placement it replaces.

use crate::Sink;
use std::collections::HashMap;
use std::io::{self, Write as _};

use katui_proto::kitty::{
    Delete, IdSpace, ImageId as KittyImage, Place, PlacementId as KittyPlacement,
};
use katui_proto::placeholder::placeholder_row;
use katui_proto::probe::CellPixels;
use katui_scene::cell::Position;
use katui_scene::placement::CellRect;
use katui_scene::surface::{PixelRect, Surface, diff_rects};

use crate::anchor::Anchor;

use super::{Stash, damage_is_dense, transmit_pixels, write_whole_image};
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
    grid: Grid,
    cell: CellPixels,
    stash: Option<Box<dyn Stash>>,
    repaint: bool,
}

/// Where tiles go under a multiplexer: nowhere special outside one, on a
/// placeholder anchor in kitty, or each under its own placeholder text.
#[derive(Debug)]
enum Grid {
    Plain,
    Anchored(Anchor),
    Placeholders,
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
    pub fn with_tiles(base: u32, cell: CellPixels, tile_side: u32, budget: usize) -> Self {
        Self {
            base,
            tile_side: tile_side.max(1),
            budget,
            prev: None,
            base_shown: None,
            tile_generations: HashMap::new(),
            grid: Grid::Plain,
            cell,
            stash: None,
            repaint: false,
        }
    }

    /// Send pixels through `stash`, for a terminal on this machine that
    /// reads them from there.
    #[must_use]
    pub fn with_stash(mut self, stash: Box<dyn Stash>) -> Self {
        self.stash = Some(stash);
        self
    }

    /// Draw placeholder tiles for cells of `cell` pixels from now on, after
    /// a font zoom changed them.
    pub fn set_cell(&mut self, cell: CellPixels) {
        self.cell = cell;
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

    /// Send every frame as small tiles for a multiplexer, which drops
    /// uploads bigger than it buffers and hides placeholder text with its
    /// pane. With `relative_placements` every tile sits on one placeholder
    /// anchor and stretches to its cells; without, each tile shows through
    /// placeholder text of its own, which keeps its aspect and so needs
    /// the true cell size.
    #[must_use]
    pub fn for_multiplexer(mut self, space: IdSpace, relative_placements: bool) -> Self {
        self.base = space.image(self.base).0;
        self.grid = if relative_placements {
            Grid::Anchored(Anchor::new(space))
        } else {
            Grid::Placeholders
        };
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
        if !matches!(self.grid, Grid::Plain) {
            if resized && matches!(self.grid, Grid::Placeholders) {
                self.fit_tiles_to_cells(surface, area);
            }
            return self.render_tiles(out, surface, area, resized, repaint);
        }
        let fresh_start = resized || repaint;
        let damage = match self.prev.as_ref() {
            Some(prev) if !fresh_start => self.damage(prev, surface, area),
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

    /// Make the tile side a whole number of cells again when a resize or a
    /// font zoom changed the cell. A placeholder tile covers whole cells,
    /// so a tile that cut a cell in two would leave cells uncovered.
    fn fit_tiles_to_cells(&mut self, surface: &Surface, area: CellRect) {
        let width = surface.width().checked_div(u32::from(area.columns));
        let height = surface.height().checked_div(u32::from(area.rows));
        if let (Some(width), Some(height)) = (width, height)
            && width > 0
            && height > 0
            && width * u32::from(area.columns) == surface.width()
            && height * u32::from(area.rows) == surface.height()
            && (!self.tile_side.is_multiple_of(width) || !self.tile_side.is_multiple_of(height))
        {
            let whole = least_common_multiple(width, height);
            self.tile_side = whole * SMALLEST_TILE.div_ceil(whole);
        }
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
        let mut shows = Vec::new();
        for rect in damage {
            self.patch(bytes, &mut shows, surface, *rect, area)?;
        }
        bytes.append(&mut shows);
        bytes.write_all(b"\x1b[?2026l")
    }

    /// A repaint keeps the old tiles up until each one is replaced. Every
    /// upload goes out before any tile shows, because a multiplexer gives
    /// us no synchronized update: the new frame then appears at once
    /// instead of in strips while the uploads trickle through.
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
        let damage = self.damage(before, surface, area);
        let mut bytes = Vec::new();
        if resized {
            self.free_tiles(&mut bytes)?;
        }
        if !damage.is_empty()
            && let Grid::Anchored(anchor) = &mut self.grid
        {
            anchor.write(&mut bytes)?;
        }
        let mut shows = Vec::new();
        for rect in &damage {
            self.patch(&mut bytes, &mut shows, surface, *rect, area)?;
        }
        bytes.append(&mut shows);
        self.prev = Some(surface.clone());
        out.write_all(&bytes)?;
        Ok(bytes.len())
    }

    /// Ghostty's range delete frees every image, the base included, so we
    /// free the shown tiles one by one.
    fn free_tiles(&mut self, bytes: &mut Vec<u8>) -> io::Result<()> {
        for (index, generation) in std::mem::take(&mut self.tile_generations) {
            bytes.write_all(&Delete::Free(self.tile_image(index, generation)).encode())?;
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
        write_whole_image(bytes, image, surface, area, self.stash.as_deref_mut())?;
        // old tiles sit above the base and would hide the new pixels
        self.free_tiles(bytes)?;
        if let Some(shown) = self.base_shown.replace(image) {
            bytes.write_all(&Delete::Free(shown).encode())?;
        }
        bytes.write_all(b"\x1b[?2026l")?;
        Ok(())
    }

    /// The damaged tiles since `before`.
    fn damage(&self, before: &Surface, surface: &Surface, area: CellRect) -> Vec<PixelRect> {
        diff_rects(before, surface, cell_tiles(surface, area, self.tile_side))
    }

    fn tile_image(&self, index: u32, second_generation: bool) -> KittyImage {
        KittyImage(self.base + 2 + index * 2 + u32::from(second_generation))
    }

    /// One damaged tile as its own small image at the tile's cell
    /// rectangle: its upload into `uploads`, and into `reveal` what puts
    /// it on screen and frees the image it replaces. Each tile alternates
    /// between two ids so its previous pixels stay visible until the new
    /// ones are placed.
    fn patch(
        &mut self,
        uploads: &mut Vec<u8>,
        reveal: &mut Vec<u8>,
        surface: &Surface,
        rect: PixelRect,
        area: CellRect,
    ) -> io::Result<()> {
        let (left, right) = column_span(rect, surface, area);
        let (top, bottom) = row_span(rect, surface, area);
        let columns = right.saturating_sub(left).max(1);
        let rows = bottom.saturating_sub(top).max(1);
        let origin_x = area.x.saturating_add(left);
        let origin_y = area.y.saturating_add(top);
        let index = tile_index(origin_x, origin_y);
        let shown = self.tile_generations.get(&index).copied();
        let generation = shown.is_some_and(|generation| !generation);
        let image = self.tile_image(index, generation);
        let origin = Position {
            x: origin_x,
            y: origin_y,
        };
        let cells = (columns, rows);
        if matches!(self.grid, Grid::Placeholders) {
            upload_placeholder_tile(uploads, image, surface, rect, cells, self.cell)?;
            show_placeholder_tile(reveal, image, origin, cells)?;
        } else {
            let padded = with_apron(rect, surface);
            transmit_pixels(
                uploads,
                image,
                &tile_pixels(surface, padded),
                (padded.width, padded.height),
                self.stash.as_deref_mut(),
            )?;
            self.place_tile(reveal, image, rect, padded, origin, cells)?;
        }
        if let Some(previous) = shown {
            reveal.write_all(&Delete::Free(self.tile_image(index, previous)).encode())?;
        }
        self.tile_generations.insert(index, generation);
        Ok(())
    }

    /// A tile placed over the base at its cell rectangle, cropped out of
    /// the `padded` copy with an apron around it.
    fn place_tile(
        &self,
        bytes: &mut Vec<u8>,
        image: KittyImage,
        rect: PixelRect,
        padded: PixelRect,
        origin: Position,
        cells: (u16, u16),
    ) -> io::Result<()> {
        let (columns, rows) = cells;
        let index = tile_index(origin.x, origin.y);
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
        if let Grid::Anchored(anchor) = &self.grid {
            place.parent = Some(anchor.offset_to(origin));
        } else {
            move_cursor(bytes, origin)?;
        }
        bytes.write_all(&place.encode())
    }
}

/// The tile whose top left cell is `column`, `row`.
fn tile_index(column: u16, row: u16) -> u32 {
    u32::from(column) + u32::from(row) * 4096
}

/// The surface cut into tiles about `tile_side` pixels square, on cell
/// edges: a surface at any resolution then has every tile cover whole
/// cells, and a tile placed on its cells lines up with the base image.
fn cell_tiles(surface: &Surface, area: CellRect, tile_side: u32) -> Vec<PixelRect> {
    let columns = u32::from(area.columns).max(1);
    let rows = u32::from(area.rows).max(1);
    let across = cells_per_tile(tile_side, surface.width(), columns);
    let down = cells_per_tile(tile_side, surface.height(), rows);
    let mut tiles = Vec::new();
    for row in (0..rows).step_by(down as usize) {
        let top = edge(row, surface.height(), rows);
        let bottom = edge((row + down).min(rows), surface.height(), rows);
        for column in (0..columns).step_by(across as usize) {
            let left = edge(column, surface.width(), columns);
            let right = edge((column + across).min(columns), surface.width(), columns);
            if right > left && bottom > top {
                tiles.push(PixelRect {
                    x: left,
                    y: top,
                    width: right - left,
                    height: bottom - top,
                });
            }
        }
    }
    tiles
}

/// How many of `cells` cells spanning `pixels` pixels make a tile about
/// `side` pixels long.
fn cells_per_tile(side: u32, pixels: u32, cells: u32) -> u32 {
    let per_tile = (u64::from(side) * u64::from(cells) * 2 + u64::from(pixels))
        / (2 * u64::from(pixels.max(1)));
    u32::try_from(per_tile).unwrap_or(cells).clamp(1, cells)
}

/// The pixel nearest where cell `cell` starts, with `cells` cells
/// spanning `pixels` pixels.
fn edge(cell: u32, pixels: u32, cells: u32) -> u32 {
    let at = (u64::from(cell) * u64::from(pixels) * 2 + u64::from(cells))
        / (2 * u64::from(cells.max(1)));
    u32::try_from(at).unwrap_or(pixels)
}

/// The cell edge nearest pixel `pixel`, the inverse of [`edge`].
fn cell_at(pixel: u32, pixels: u32, cells: u32) -> u16 {
    let at = (u64::from(pixel) * u64::from(cells) * 2 + u64::from(pixels))
        / (2 * u64::from(pixels.max(1)));
    u16::try_from(at).unwrap_or(u16::MAX)
}

/// The first and past-the-end columns `rect` covers.
fn column_span(rect: PixelRect, surface: &Surface, area: CellRect) -> (u16, u16) {
    let columns = u32::from(area.columns);
    (
        cell_at(rect.x, surface.width(), columns),
        cell_at(rect.x + rect.width, surface.width(), columns),
    )
}

/// The first and past-the-end rows `rect` covers.
fn row_span(rect: PixelRect, surface: &Surface, area: CellRect) -> (u16, u16) {
    let rows = u32::from(area.rows);
    (
        cell_at(rect.y, surface.height(), rows),
        cell_at(rect.y + rect.height, surface.height(), rows),
    )
}

/// The smallest tile side in pixels we refit to, so a whole frame stays a
/// few hundred tiles.
const SMALLEST_TILE: u32 = 64;

fn least_common_multiple(first: u32, second: u32) -> u32 {
    let (mut a, mut b) = (first, second);
    while b != 0 {
        (a, b) = (b, a % b);
    }
    first / a.max(1) * second
}

/// A terminal fits a placeholder image into its cells keeping its aspect,
/// so we stretch the tile's height to the aspect of its cells and keep its
/// width, which keeps a game's reduced resolution.
fn upload_placeholder_tile(
    bytes: &mut Vec<u8>,
    image: KittyImage,
    surface: &Surface,
    rect: PixelRect,
    (columns, rows): (u16, u16),
    cell: CellPixels,
) -> io::Result<()> {
    let cells_wide = (u32::from(columns) * u32::from(cell.width)).max(1);
    let cells_high = u32::from(rows) * u32::from(cell.height);
    let width = rect.width.max(1);
    let height = (u64::from(width) * u64::from(cells_high) / u64::from(cells_wide)).max(1);
    let height = u32::try_from(height).unwrap_or(rect.height);
    let pixels = resample(
        &tile_pixels(surface, rect),
        (rect.width, rect.height),
        (width, height),
    );
    transmit_pixels(bytes, image, &pixels, (width, height), None)
}

/// A virtual placement of the tile's cell size, and the placeholder text
/// that shows it written over its cells.
fn show_placeholder_tile(
    bytes: &mut Vec<u8>,
    image: KittyImage,
    origin: Position,
    (columns, rows): (u16, u16),
) -> io::Result<()> {
    let mut place = Place::new(image);
    place.columns = u32::from(columns);
    place.rows = u32::from(rows);
    place.virtual_placement = true;
    bytes.write_all(&place.encode())?;
    for row in 0..rows {
        if let Some(text) = placeholder_row(image, u32::from(row), u32::from(columns)) {
            move_cursor(
                bytes,
                Position {
                    x: origin.x,
                    y: origin.y.saturating_add(row),
                },
            )?;
            bytes.write_all(text.as_bytes())?;
        }
    }
    Ok(())
}

/// `pixels` of `from` size scaled to `to` by nearest neighbour, or as they
/// are when the sizes match.
fn resample(pixels: &[u8], from: (u32, u32), to: (u32, u32)) -> Vec<u8> {
    if from == to || from.0 == 0 || from.1 == 0 {
        return pixels.to_vec();
    }
    let mut scaled = Vec::with_capacity((to.0 * to.1 * 4) as usize);
    for y in 0..to.1 {
        let source_row = (y * from.1 / to.1) * from.0;
        for x in 0..to.0 {
            let start = ((source_row + x * from.0 / to.0) * 4) as usize;
            scaled.extend_from_slice(pixels.get(start..start + 4).unwrap_or(&[0; 4]));
        }
    }
    scaled
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
    fn tiles_of_a_scaled_surface_cover_whole_cells_edge_to_edge() {
        let surface = plain_surface(50, 37);
        let area = CellRect::full(16, 9);
        let tiles = super::cell_tiles(&surface, area, 8);
        let covered: u32 = tiles.iter().map(|tile| tile.width * tile.height).sum();
        assert_eq!(covered, 50 * 37, "the tiles cover the surface once");
        let mut next_column = 0;
        for tile in tiles.iter().filter(|tile| tile.y == 0) {
            let (left, right) = super::column_span(*tile, &surface, area);
            assert_eq!(left, next_column, "each tile starts where the last ended");
            assert!(right > left, "every tile covers a cell");
            next_column = right;
        }
        assert_eq!(next_column, 16, "the last tile ends at the last column");
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
            text.contains("a=p,i=82032") && text.contains("c=2,r=2"),
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
        let freed = text.find("d=I").expect("the stale tiles are freed");
        assert!(
            !text.contains("d=R"),
            "Ghostty frees every image on a range delete"
        );
        assert!(placed < freed, "the new base appears before the tiles go");
    }

    fn space() -> katui_proto::kitty::IdSpace {
        katui_proto::kitty::IdSpace::of_process(4242)
    }

    const VIRTUAL: &str = ",U=1";

    #[test]
    fn a_multiplexer_grid_shows_every_tile_through_placeholders_and_never_a_whole_image() {
        let mut presenter = PatchedKitty::new(90, cell()).for_multiplexer(space(), false);
        let surface = plain_surface(64, 64);
        let mut wire = Vec::new();
        presenter
            .render_frame(&mut wire, &surface, crate::surface::test_support::area())
            .expect("writing to a Vec cannot fail");
        let text = String::from_utf8_lossy(&wire);
        assert!(!text.contains("s=64"), "no whole image travels");
        assert_eq!(
            text.matches(VIRTUAL).count(),
            64,
            "every tile has a virtual placement"
        );
        assert_eq!(
            text.matches('\u{10eeee}').count(),
            16 * 16,
            "placeholder text covers every cell once"
        );
        assert!(!text.contains(",P="), "no tile needs a parent placement");
    }

    #[test]
    fn a_multiplexer_frame_uploads_every_tile_before_showing_any() {
        let mut presenter = PatchedKitty::new(90, cell()).for_multiplexer(space(), false);
        let mut wire = Vec::new();
        presenter
            .render_frame(
                &mut wire,
                &plain_surface(64, 64),
                crate::surface::test_support::area(),
            )
            .expect("writing to a Vec cannot fail");
        let text = String::from_utf8_lossy(&wire);
        let last_upload = text.rfind("a=t,").expect("tiles travel");
        let first_show = text.find(VIRTUAL).expect("tiles show");
        assert!(last_upload < first_show, "the frame appears at once");
    }

    #[test]
    fn a_repainted_grid_resends_every_tile() {
        let mut presenter = PatchedKitty::new(90, cell()).for_multiplexer(space(), false);
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
        assert_eq!(text.matches(VIRTUAL).count(), 64, "every tile returns");
        assert_eq!(text.matches("a=t,").count(), 64, "only the tiles travel");
        assert!(!text.contains("d=R"), "old tiles stay until replaced");
        wire.clear();
        presenter
            .render_frame(&mut wire, &surface, crate::surface::test_support::area())
            .expect("writing to a Vec cannot fail");
        assert!(wire.is_empty(), "one repaint request repaints once");
    }

    #[test]
    fn a_multiplexer_grid_frees_the_old_tiles_on_a_resize() {
        let mut presenter = PatchedKitty::new(90, cell()).for_multiplexer(space(), false);
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
        let freed = text.find("d=I").expect("the old grid is freed");
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
        let placed = text.find("a=p,i=82033").expect("the second generation");
        let freed = text.find("d=I,i=82032").expect("the first is freed");
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
