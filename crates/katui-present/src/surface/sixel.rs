//! The sixel strategy: the surface scaled to the screen's real pixels,
//! sent as one sixel image per changed tile at the tile's cell.

use std::io::{self, Write as _};

use katui_proto::probe::CellPixels;
use katui_proto::sixel;
use katui_scene::cell::Position;
use katui_scene::placement::CellRect;
use katui_scene::surface::{PixelRect, Surface, diff_tiles};

use super::damage_is_dense;
use super::patched::tile_pixels;
use crate::Sink;
use crate::cells::move_cursor;

/// The sixel surface presenter. Sixel images have no scaling, so we scale
/// the surface to whole cells ourselves, nearest pixel. We leave the
/// bottom row out, because a sixel touching it scrolls the screen.
#[derive(Debug)]
pub struct SixelSurface {
    cell: CellPixels,
    prev: Option<Surface>,
}

impl SixelSurface {
    /// A presenter for a terminal whose cells are `cell` pixels.
    #[must_use]
    pub fn new(cell: CellPixels) -> Self {
        Self { cell, prev: None }
    }

    /// Send every pixel again on the next frame.
    pub fn repaint(&mut self) {
        self.prev = None;
    }

    /// Write `surface` scaled over `columns` x `rows` cells, minus the
    /// bottom row, and return the bytes written.
    pub fn render_frame<W: Sink>(
        &mut self,
        out: &mut W,
        surface: &Surface,
        columns: u16,
        rows: u16,
    ) -> io::Result<usize> {
        let area = CellRect::full(columns, rows.saturating_sub(1));
        self.render_frame_in(out, surface, area)
    }

    /// Write `surface` scaled over the `area` cells and return the bytes
    /// written. A sixel that touches the screen's bottom row scrolls the
    /// screen, so `area` must stay above it.
    pub fn render_frame_in<W: Sink>(
        &mut self,
        out: &mut W,
        surface: &Surface,
        area: CellRect,
    ) -> io::Result<usize> {
        let width = u32::from(area.columns) * u32::from(self.cell.width);
        let height = u32::from(area.rows) * u32::from(self.cell.height);
        let origin = Position {
            x: area.x,
            y: area.y,
        };
        let Some(screen) = surface.scaled(width, height) else {
            return Ok(0);
        };
        let tile_side = u32::from(self.cell.width) * u32::from(self.cell.height);
        let damage = self.prev.as_ref().map_or_else(
            || vec![whole(width, height)],
            |prev| diff_tiles(prev, &screen, tile_side),
        );
        let damage = if damage_is_dense(&screen, tile_side, damage.len()) {
            vec![whole(width, height)]
        } else {
            damage
        };
        let mut bytes = Vec::new();
        if !damage.is_empty() {
            bytes.write_all(b"\x1b[?2026h")?;
            for rect in &damage {
                self.write_tile(&mut bytes, &screen, *rect, origin)?;
            }
            bytes.write_all(b"\x1b[?2026l")?;
        }
        self.prev = Some(screen);
        out.write_all(&bytes)?;
        Ok(bytes.len())
    }

    fn write_tile(
        &self,
        bytes: &mut Vec<u8>,
        screen: &Surface,
        rect: PixelRect,
        origin: Position,
    ) -> io::Result<()> {
        let pixels = tile_pixels(screen, rect);
        let Some(image) = sixel::encode(&pixels, rect.width, rect.height) else {
            return Ok(());
        };
        let at = Position {
            x: origin.x.saturating_add(
                u16::try_from(rect.x / u32::from(self.cell.width)).unwrap_or(u16::MAX),
            ),
            y: origin.y.saturating_add(
                u16::try_from(rect.y / u32::from(self.cell.height)).unwrap_or(u16::MAX),
            ),
        };
        move_cursor(bytes, at)?;
        bytes.write_all(&image)
    }
}

fn whole(width: u32, height: u32) -> PixelRect {
    PixelRect {
        x: 0,
        y: 0,
        width,
        height,
    }
}

#[cfg(test)]
mod tests {
    use katui_proto::probe::CellPixels;
    use katui_scene::surface::{PixelPos, Rgba};

    use super::SixelSurface;
    use crate::surface::test_support::solid;

    const CELL: CellPixels = CellPixels {
        width: 4,
        height: 6,
    };

    #[test]
    fn the_first_frame_fills_the_screen_but_the_bottom_row() {
        let mut presenter = SixelSurface::new(CELL);
        let mut out = Vec::new();
        presenter
            .render_frame(&mut out, &solid(10, 10, Rgba::solid(255, 0, 0)), 20, 6)
            .expect("writing to a Vec cannot fail");
        let text = String::from_utf8_lossy(&out);
        assert!(
            text.contains("\x1b[1;1H\x1bP0;1;0q\"1;1;80;30"),
            "80x30 pixels from the top left: {text:.60}"
        );
        assert_eq!(text.matches("\x1bP").count(), 1, "one image");
    }

    #[test]
    fn a_changed_pixel_redraws_one_tile_at_its_cell() {
        let mut presenter = SixelSurface::new(CELL);
        let mut surface = solid(80, 60, Rgba::solid(0, 0, 0));
        let mut out = Vec::new();
        presenter
            .render_frame(&mut out, &surface, 20, 11)
            .expect("writing to a Vec cannot fail");
        surface.set(PixelPos::new(50, 30), Rgba::solid(255, 255, 255));
        out.clear();
        presenter
            .render_frame(&mut out, &surface, 20, 11)
            .expect("writing to a Vec cannot fail");
        let text = String::from_utf8_lossy(&out);
        assert_eq!(text.matches("\x1bP").count(), 1, "one tile");
        assert!(
            text.contains("\x1b[5;13H\x1bP0;1;0q\"1;1;24;24"),
            "the 24 pixel tile at x 48, y 24: {text:.80}"
        );
        out.clear();
        let written = presenter
            .render_frame(&mut out, &surface, 20, 11)
            .expect("writing to a Vec cannot fail");
        assert_eq!(written, 0, "an unchanged frame writes nothing");
    }
}
