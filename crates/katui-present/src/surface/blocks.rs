//! The blocks strategy: quarter-block cells for terminals without a
//! graphics protocol, damage-tracked so a moving sprite costs a handful
//! of cells per frame.

use crate::Sink;
use std::io::{self, Write as _};

use katui_scene::cell::diff;
use katui_scene::cell::{Attrs, Buffer, Color, Position};
use katui_scene::surface::Surface;

use crate::blocks::{Rgb, quarter, render_line};

/// The stateful blocks presenter: renders a pixel surface as
/// quarter-block cells once, then repaints only the cells whose pixels
/// changed. This is the blocks path's answer to
/// [`PatchedKitty`](crate::surface::PatchedKitty): a moving sprite on a static
/// background costs a handful of cursor-addressed SGR cells per frame
/// instead of a full-screen rewrite, which is what makes the fallback
/// survivable inside multiplexers.
#[derive(Debug, Default)]
pub struct BlocksSurface {
    prev: Option<Buffer>,
}

impl BlocksSurface {
    /// A fresh presenter with no previous frame.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Write one frame's cells and return the bytes written. A fresh or
    /// resized frame clears the screen and repaints every cell; a later
    /// frame repaints only the cells whose pixels changed, inside
    /// synchronized output, with no clear that would erase the rest.
    /// An unchanged frame writes nothing.
    pub fn render_frame<W: Sink>(&mut self, out: &mut W, surface: &Surface) -> io::Result<usize> {
        let next = cells_for_surface(surface);
        let (changed, full): (Vec<_>, bool) = match self.prev.as_ref() {
            Some(prev) if prev.size() == next.size() => (
                diff(prev, &next)
                    .into_iter()
                    .map(|(position, cell)| (position, cell.clone()))
                    .collect(),
                false,
            ),
            Some(_) | None => (
                all_cells(&next)
                    .into_iter()
                    .map(|(position, cell)| (position, cell.clone()))
                    .collect(),
                true,
            ),
        };
        self.prev = Some(next);
        if changed.is_empty() {
            return Ok(0);
        }
        let mut bytes = Vec::new();
        bytes.write_all(b"\x1b[?2026h")?;
        if full {
            bytes.write_all(b"\x1b[H\x1b[2J")?;
        }
        for (position, cell) in &changed {
            crate::cells::render_cell(&mut bytes, *position, cell)?;
        }
        bytes.write_all(b"\x1b[0m\x1b[?2026l")?;
        let written = bytes.len();
        out.write_all(&bytes)?;
        Ok(written)
    }
}

/// Every cell of a buffer, row-major.
fn all_cells(buffer: &Buffer) -> Vec<(Position, &katui_scene::cell::Cell)> {
    let mut cells = Vec::new();
    for row in 0..buffer.size().rows {
        for column in 0..buffer.size().columns {
            if let Some(cell) = buffer.get(Position { x: column, y: row }) {
                cells.push((Position { x: column, y: row }, cell));
            }
        }
    }
    cells
}

/// Classify every cell of a surface into the quarter-block grid.
fn cells_for_surface(surface: &Surface) -> Buffer {
    let columns = u16::try_from(surface.width() / 2).unwrap_or(u16::MAX);
    let rows = u16::try_from(surface.height() / 2).unwrap_or(u16::MAX);
    let mut buffer = Buffer::blank(katui_scene::CellSize { columns, rows });
    for row in 0..rows {
        for column in 0..columns {
            let pixel = |dx: u32, dy: u32| -> Rgb {
                let x = u32::from(column) * 2 + dx;
                let y = u32::from(row) * 2 + dy;
                surface
                    .get(katui_scene::surface::PixelPos::new(x, y))
                    .map_or(Rgb { r: 0, g: 0, b: 0 }, |color| Rgb {
                        r: color.r,
                        g: color.g,
                        b: color.b,
                    })
            };
            let block = quarter([[pixel(0, 0), pixel(1, 0)], [pixel(0, 1), pixel(1, 1)]]);
            let cell = katui_scene::cell::Cell {
                symbol: block.glyph.to_string(),
                fg: Color::Rgb(block.fg.r, block.fg.g, block.fg.b),
                bg: Color::Rgb(block.bg.r, block.bg.g, block.bg.b),
                attrs: Attrs::EMPTY,
            };
            buffer.set(Position { x: column, y: row }, cell);
        }
    }
    buffer
}

/// Write a surface as quarter-block lines from the home cursor, the
/// pixel path for terminals without a graphics protocol. Alpha is
/// ignored: block glyphs carry no transparency.
pub fn render_blocks<W: Sink>(out: &mut W, surface: &Surface) -> io::Result<usize> {
    let mut bytes = Vec::new();
    bytes.write_all(b"\x1b[H")?;
    let mut y = 0;
    while y + 1 < surface.height() {
        let top = pixel_row(surface, y);
        let bottom = pixel_row(surface, y + 1);
        render_line(&mut bytes, &top, &bottom)?;
        bytes.write_all(b"\x1b[K\r\n")?;
        y += 2;
    }
    let written = bytes.len();
    out.write_all(&bytes)?;
    Ok(written)
}

fn pixel_row(surface: &Surface, y: u32) -> Vec<Rgb> {
    (0..surface.width())
        .filter_map(|x| {
            surface
                .get(katui_scene::surface::PixelPos::new(x, y))
                .map(|pixel| Rgb {
                    r: pixel.r,
                    g: pixel.g,
                    b: pixel.b,
                })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::render_blocks;
    use crate::surface::test_support::solid;
    use katui_scene::surface::Rgba;

    #[test]
    fn blocks_render_a_solid_surface_as_full_cells() {
        let surface = solid(4, 2, Rgba::solid(255, 0, 0));
        let mut out = Vec::new();
        render_blocks(&mut out, &surface).expect("writing to a Vec cannot fail");
        let text = String::from_utf8_lossy(&out);
        assert!(text.starts_with("\x1b[H"));
        assert!(text.contains("\x1b[38;2;255;0;0m"));
        assert!(text.contains('█'));
        assert!(text.contains("\x1b[K\r\n"));
    }

    mod diffed_blocks {
        use super::super::BlocksSurface;
        use super::solid;
        use katui_scene::surface::Rgba;

        #[test]
        fn an_identical_frame_writes_nothing() {
            let mut presenter = BlocksSurface::new();
            let surface = solid(16, 16, Rgba::solid(30, 40, 50));
            let mut first = Vec::new();
            presenter
                .render_frame(&mut first, &surface)
                .expect("writing to a Vec cannot fail");
            let mut second = Vec::new();
            let written = presenter
                .render_frame(&mut second, &surface)
                .expect("writing to a Vec cannot fail");
            assert_eq!(written, 0, "no pixel change means no bytes");
            assert!(second.is_empty());
        }

        #[test]
        fn a_small_change_costs_a_handful_of_cells() {
            let mut presenter = BlocksSurface::new();
            let mut surface = solid(32, 32, Rgba::solid(10, 10, 10));
            let mut first = Vec::new();
            presenter
                .render_frame(&mut first, &surface)
                .expect("writing to a Vec cannot fail");
            surface.set(
                katui_scene::surface::PixelPos::new(20, 20),
                Rgba::solid(255, 0, 0),
            );
            let mut second = Vec::new();
            let written = presenter
                .render_frame(&mut second, &surface)
                .expect("writing to a Vec cannot fail");
            assert!(
                written < first.len() / 8,
                "one pixel change is far cheaper than a full frame"
            );
            let text = String::from_utf8_lossy(&second);
            assert!(
                !text.contains("\x1b[2J"),
                "a diffed frame never erases the screen"
            );
            assert!(
                text.matches("\x1b[").count() < 10,
                "a couple of cells repaint, not the screen"
            );
        }

        #[test]
        fn a_size_change_repaints_everything() {
            let mut presenter = BlocksSurface::new();
            let mut first = Vec::new();
            presenter
                .render_frame(&mut first, &solid(16, 16, Rgba::solid(9, 9, 9)))
                .expect("writing to a Vec cannot fail");
            let mut second = Vec::new();
            let written = presenter
                .render_frame(&mut second, &solid(32, 32, Rgba::solid(9, 9, 9)))
                .expect("writing to a Vec cannot fail");
            assert!(written > 0, "a resize repaints the whole grid");
        }
    }
}
