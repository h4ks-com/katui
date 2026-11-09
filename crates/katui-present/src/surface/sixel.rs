//! The sixel strategy: the surface scaled to the screen's real pixels,
//! sent as one sixel image per changed tile at the tile's cell.

use std::io::{self, Write as _};

use katui_proto::probe::CellPixels;
use katui_proto::sixel;
use katui_scene::cell::Position;
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
        let width = u32::from(columns) * u32::from(self.cell.width);
        let height = u32::from(rows.saturating_sub(1)) * u32::from(self.cell.height);
        let Some(screen) = scaled(surface, width, height) else {
            return Ok(0);
        };
        let tile_side = u32::from(self.cell.width) * u32::from(self.cell.height);
        let damage = match self.prev.as_ref() {
            Some(prev) if prev.width() == width && prev.height() == height => {
                diff_tiles(prev, &screen, tile_side)
            }
            _ => vec![whole(width, height)],
        };
        let damage = if damage_is_dense(&screen, tile_side, damage.len()) {
            vec![whole(width, height)]
        } else {
            damage
        };
        let mut bytes = Vec::new();
        if !damage.is_empty() {
            bytes.write_all(b"\x1b[?2026h")?;
            for rect in &damage {
                self.write_tile(&mut bytes, &screen, *rect)?;
            }
            bytes.write_all(b"\x1b[?2026l")?;
        }
        self.prev = Some(screen);
        out.write_all(&bytes)?;
        Ok(bytes.len())
    }

    fn write_tile(&self, bytes: &mut Vec<u8>, screen: &Surface, rect: PixelRect) -> io::Result<()> {
        let pixels = tile_pixels(screen, rect);
        let Some(image) = sixel::encode(&pixels, rect.width, rect.height) else {
            return Ok(());
        };
        let at = Position {
            x: u16::try_from(rect.x / u32::from(self.cell.width)).unwrap_or(u16::MAX),
            y: u16::try_from(rect.y / u32::from(self.cell.height)).unwrap_or(u16::MAX),
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

/// `surface` resized to `width` x `height`, nearest pixel; `None` for an
/// empty target.
fn scaled(surface: &Surface, width: u32, height: u32) -> Option<Surface> {
    if width == 0 || height == 0 || surface.width() == 0 || surface.height() == 0 {
        return None;
    }
    let source = surface.pixels();
    let source_row = surface.width() as usize * 4;
    let columns: Vec<usize> = (0..width)
        .map(|x| (u64::from(x) * u64::from(surface.width()) / u64::from(width)) as usize * 4)
        .collect();
    let mut pixels = Vec::with_capacity(width as usize * height as usize * 4);
    for y in 0..height {
        let row = (u64::from(y) * u64::from(surface.height()) / u64::from(height)) as usize;
        let start = row * source_row;
        for column in &columns {
            let at = start + column;
            pixels.extend_from_slice(source.get(at..at + 4).unwrap_or(&[0, 0, 0, 255]));
        }
    }
    Surface::from_rgba(width, height, pixels)
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
