//! The whole-frame kitty strategy: every frame travels complete, under
//! alternating image ids.

use crate::Sink;
use std::io::{self, Write as _};

use katui_proto::kitty::{Delete, ImageId as KittyImage, PlacementId as KittyPlacement};
use katui_scene::placement::CellRect;
use katui_scene::surface::Surface;

use super::write_whole_image;

/// The full-frame kitty surface presenter: every frame transmits the
/// whole surface compressed under one of two alternating image ids and
/// places it, deleting the previous frame's placement, so the terminal
/// never shows a half-updated image.
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
    pub fn render_frame<W: Sink>(
        &mut self,
        out: &mut W,
        surface: &Surface,
        area: CellRect,
    ) -> io::Result<usize> {
        let current = self.base + u32::from(self.parity);
        let previous = self.base + u32::from(!self.parity);
        let mut bytes = Vec::new();
        bytes.write_all(b"\x1b[?2026h")?;
        write_whole_image(&mut bytes, KittyImage(current), surface, area, None)?;
        bytes.write_all(&Delete::Placement(KittyImage(previous), KittyPlacement(1)).encode())?;
        bytes.write_all(b"\x1b[?2026l")?;
        self.parity = !self.parity;
        let written = bytes.len();
        out.write_all(&bytes)?;
        Ok(written)
    }
}

#[cfg(test)]
mod tests {
    use super::KittySurface;
    use crate::surface::test_support::solid;
    use katui_scene::placement::CellRect;
    use katui_scene::surface::Rgba;

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
}
