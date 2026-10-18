//! Surface presenters: the full-frame kitty strategy with alternating
//! image ids, and the blocks fallback every truecolor terminal runs.

use std::io::{self, Write as _};

use katui_proto::kitty::{
    Delete, Format, ImageId as KittyImage, Place, PlacementId as KittyPlacement, Transmit,
};
use katui_scene::cell::Position;
use katui_scene::placement::CellRect;
use katui_scene::surface::Surface;

use crate::blocks::{Rgb, render_line};
use crate::cells::move_cursor;

/// The full-frame kitty surface presenter: every frame transmits the
/// whole surface compressed under one of two alternating image ids and
/// places it, deleting the previous frame's placement, so the terminal
/// never shows a half-updated image. Patched tile presenters refine this
/// later; whole-frame is the correctness baseline.
#[derive(Debug)]
pub struct KittySurface {
    base: u32,
    parity: bool,
}

impl KittySurface {
    /// A presenter handing out image ids from `base` upward; pick a base
    /// no other katui presenter in the session uses.
    #[must_use]
    pub fn new(base: u32) -> Self {
        Self {
            base,
            parity: false,
        }
    }

    /// Write `surface` scaled into `area` cells as one synchronized
    /// frame and return the bytes written.
    pub fn render_frame<W: io::Write>(
        &mut self,
        out: &mut W,
        surface: &Surface,
        area: CellRect,
    ) -> io::Result<usize> {
        let current = self.base + u32::from(self.parity);
        let previous = self.base + u32::from(!self.parity);
        let mut bytes = Vec::new();
        bytes.write_all(b"\x1b[?2026h")?;
        let mut transmit = Transmit::new(
            KittyImage(current),
            Format::Rgba,
            surface.width(),
            surface.height(),
            surface.pixels(),
        );
        transmit.compressed = true;
        for frame in transmit.encode() {
            bytes.write_all(&frame)?;
        }
        move_cursor(
            &mut bytes,
            Position {
                x: area.x,
                y: area.y,
            },
        )?;
        let mut place = Place::new(KittyImage(current));
        place.placement = KittyPlacement(1);
        place.columns = u32::from(area.columns);
        place.rows = u32::from(area.rows);
        bytes.write_all(&place.encode())?;
        bytes.write_all(&Delete::Placement(KittyImage(previous), KittyPlacement(1)).encode())?;
        bytes.write_all(b"\x1b[?2026l")?;
        self.parity = !self.parity;
        let written = bytes.len();
        out.write_all(&bytes)?;
        Ok(written)
    }
}

/// Write a surface as quarter-block lines from the home cursor, the
/// pixel path for terminals without a graphics protocol. Alpha is
/// ignored: block glyphs carry no transparency.
pub fn render_blocks<W: io::Write>(out: &mut W, surface: &Surface) -> io::Result<usize> {
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
            surface.get(x, y).map(|pixel| Rgb {
                r: pixel.r,
                g: pixel.g,
                b: pixel.b,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {

    use super::{KittySurface, render_blocks};
    use katui_scene::placement::CellRect;
    use katui_scene::surface::{Rgba, Surface};

    fn solid(width: u32, height: u32, color: Rgba) -> Surface {
        let mut surface = Surface::new(width, height);
        for y in 0..height {
            for x in 0..width {
                surface.set(x, y, color);
            }
        }
        surface
    }

    fn area() -> CellRect {
        CellRect {
            x: 2,
            y: 1,
            columns: 20,
            rows: 10,
        }
    }

    #[test]
    fn frames_alternate_ids_and_delete_the_previous_frame() {
        let surface = solid(4, 4, Rgba::solid(255, 0, 0));
        let mut presenter = KittySurface::new(90);
        let mut first = Vec::new();
        presenter
            .render_frame(&mut first, &surface, area())
            .expect("writing to a Vec cannot fail");
        let text = String::from_utf8_lossy(&first);
        assert!(text.contains("i=90,"), "the first frame uses the base id");
        assert!(text.contains("d=i,i=91"), "the other id's placement clears");
        assert!(text.contains("o=z"), "frames travel compressed");

        let mut second = Vec::new();
        presenter
            .render_frame(&mut second, &surface, area())
            .expect("writing to a Vec cannot fail");
        let text = String::from_utf8_lossy(&second);
        assert!(text.contains("i=91,"), "the second frame alternates");
        assert!(
            text.contains("d=i,i=90"),
            "the first frame's placement clears"
        );
    }

    #[test]
    fn frames_stay_inside_synchronized_output() {
        let surface = solid(2, 2, Rgba::solid(0, 255, 0));
        let mut presenter = KittySurface::new(10);
        let mut out = Vec::new();
        presenter
            .render_frame(&mut out, &surface, area())
            .expect("writing to a Vec cannot fail");
        let text = String::from_utf8_lossy(&out);
        assert!(text.starts_with("\x1b[?2026h"));
        assert!(text.ends_with("\x1b[?2026l"));
    }

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
}
