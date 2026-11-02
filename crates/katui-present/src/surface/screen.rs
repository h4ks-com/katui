//! The composite pixel strategy: the patched kitty presenter when the
//! terminal speaks graphics, the damage-tracked blocks otherwise.

use crate::Sink;
use std::io;

use katui_proto::probe::Capabilities;
use katui_scene::placement::CellRect;
use katui_scene::surface::Surface;

use super::{BlocksSurface, PatchedKitty};

/// The demo-grade pixel presenter for a terminal: one object that
/// renders a pixel surface full-screen through whatever the terminal
/// offers, so callers stop hand-picking strategies per capability. The
/// kitty strategy comes pre-built by the caller (tile side and budget
/// are policy); it is used only when the terminal actually speaks
/// graphics, and the blocks path carries every other terminal.
#[derive(Debug, Default)]
pub struct PixelScreen {
    kitty: Option<PatchedKitty>,
    blocks: BlocksSurface,
}

impl PixelScreen {
    /// The screen for `caps`: `patched` when the terminal speaks the
    /// kitty graphics protocol, quarter blocks otherwise.
    #[must_use]
    pub fn new(caps: &Capabilities, patched: PatchedKitty) -> Self {
        Self {
            kitty: caps.kitty.then_some(patched),
            blocks: BlocksSurface::new(),
        }
    }

    /// Write `surface` scaled to the whole `columns` x `rows` screen and
    /// return the bytes written.
    pub fn render_frame<W: Sink>(
        &mut self,
        out: &mut W,
        surface: &Surface,
        columns: u16,
        rows: u16,
    ) -> io::Result<usize> {
        match &mut self.kitty {
            Some(patched) => patched.render_frame(out, surface, CellRect::full(columns, rows)),
            None => self.blocks.render_frame(out, surface),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::PixelScreen;
    use katui_proto::probe::{Capabilities, CellPixels, ProbeSource};
    use katui_scene::surface::{Rgba, Surface};

    fn caps(kitty: bool) -> Capabilities {
        Capabilities {
            kitty,
            sixel: false,
            truecolor: false,
            identity: None,
            cell: None,
            trust: ProbeSource::Sniffed,
        }
    }

    fn plain(width: u32, height: u32) -> Surface {
        let mut surface = Surface::new(width, height);
        for y in 0..height {
            for x in 0..width {
                surface.set(
                    katui_scene::surface::PixelPos::new(x, y),
                    Rgba::solid(255, 80, 40),
                );
            }
        }
        surface
    }

    #[test]
    fn a_kitty_terminal_gets_graphics_frames() {
        let patched = crate::surface::PatchedKitty::new(
            90,
            CellPixels {
                width: 2,
                height: 4,
            },
        );
        let mut screen = PixelScreen::new(&caps(true), patched);
        let mut out = Vec::new();
        screen
            .render_frame(&mut out, &plain(8, 8), 4, 2)
            .expect("writing to a Vec cannot fail");
        let text = String::from_utf8_lossy(&out);
        assert!(text.contains("a=t"), "the surface transmits as an image");
        assert!(text.contains("a=p"), "and is placed");
    }

    #[test]
    fn a_plain_terminal_gets_quarter_blocks() {
        let patched = crate::surface::PatchedKitty::new(
            90,
            CellPixels {
                width: 2,
                height: 4,
            },
        );
        let mut screen = PixelScreen::new(&caps(false), patched);
        let mut out = Vec::new();
        screen
            .render_frame(&mut out, &plain(8, 4), 4, 2)
            .expect("writing to a Vec cannot fail");
        let text = String::from_utf8_lossy(&out);
        assert!(!text.contains("a=t"), "no graphics on a plain terminal");
        assert!(text.contains("\x1b[38;2;"), "quarter blocks render");
    }
}
