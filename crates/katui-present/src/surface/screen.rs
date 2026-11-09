//! The composite pixel strategy: the patched kitty presenter when the
//! terminal speaks kitty graphics, sixel tiles when it speaks sixel, the
//! damage-tracked blocks otherwise.

use crate::Sink;
use std::io;

use katui_proto::probe::Capabilities;

use crate::select::{Backend, select};
use katui_scene::placement::CellRect;
use katui_scene::surface::Surface;

use super::{AnimatedKitty, BlocksSurface, PatchedKitty, SixelSurface};

/// The pixel strategy a [`PixelScreen`] drives on a graphics terminal.
#[derive(Debug)]
pub(super) enum PixelStrategy {
    /// Patched tile retransmits, each frame displayed as it arrives.
    Patched(PatchedKitty),
    /// Terminal-side animation frames, played by the compositor.
    Animated(AnimatedKitty),
    /// Sixel tiles, for terminals without kitty graphics.
    Sixel(SixelSurface),
}

/// The demo-grade pixel presenter for a terminal: one object that
/// renders a pixel surface full-screen through whatever the terminal
/// offers, so callers stop hand-picking strategies per capability. The
/// kitty strategy comes pre-built by the caller (tiles and budget are
/// policy); it is used only when the terminal speaks kitty graphics,
/// sixel takes its cell size, and the blocks path carries every other
/// terminal.
#[derive(Debug, Default)]
pub struct PixelScreen {
    graphics: Option<PixelStrategy>,
    blocks: BlocksSurface,
}

impl PixelScreen {
    /// The screen for `caps`: `patched` when the terminal speaks the
    /// kitty graphics protocol, sixel tiles at `patched`'s cell size when
    /// it speaks sixel, quarter blocks otherwise.
    #[must_use]
    pub fn new(caps: &Capabilities, patched: PatchedKitty) -> Self {
        let strategy = match select(caps) {
            Backend::Kitty => Some(PixelStrategy::Patched(
                patched.for_multiplexer(caps.multiplexed()),
            )),
            Backend::Sixel => Some(PixelStrategy::Sixel(SixelSurface::new(patched.cell()))),
            Backend::Blocks => None,
        };
        Self {
            graphics: strategy,
            blocks: BlocksSurface::new(),
        }
    }

    /// Whether frames go out as sixel, which shows the surface at the
    /// screen's own pixels.
    #[must_use]
    pub fn draws_sixel(&self) -> bool {
        matches!(self.graphics, Some(PixelStrategy::Sixel(_)))
    }

    /// The same screen driven by a terminal-side animation, falling back
    /// to `fallback` patches when the terminal speaks graphics but not
    /// the animation extension, a multiplexer that swallows frames
    /// included.
    #[must_use]
    pub fn animated(caps: &Capabilities, animated: AnimatedKitty, fallback: PatchedKitty) -> Self {
        let strategy = if caps.animation && !caps.multiplexed() {
            PixelStrategy::Animated(animated)
        } else {
            PixelStrategy::Patched(fallback.for_multiplexer(caps.multiplexed()))
        };
        Self {
            graphics: caps.kitty.then_some(strategy),
            blocks: BlocksSurface::new(),
        }
    }

    /// Send every graphics pixel again on the next frame, for when the
    /// terminal may have lost its images, a window resize for one.
    pub fn repaint(&mut self) {
        match &mut self.graphics {
            Some(PixelStrategy::Patched(patched)) => patched.repaint(),
            Some(PixelStrategy::Animated(animated)) => animated.repaint(),
            Some(PixelStrategy::Sixel(sixel)) => sixel.repaint(),
            None => {}
        }
    }

    /// Free every graphics image this screen sent and repaint on the next
    /// frame, for a screen that went out of view or is being replaced.
    pub fn free<W: Sink>(&mut self, out: &mut W) -> io::Result<()> {
        match &mut self.graphics {
            Some(PixelStrategy::Patched(patched)) => patched.free(out),
            Some(PixelStrategy::Animated(animated)) => animated.free(out),
            Some(PixelStrategy::Sixel(sixel)) => {
                sixel.repaint();
                Ok(())
            }
            None => Ok(()),
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
        match &mut self.graphics {
            Some(PixelStrategy::Patched(patched)) => {
                patched.render_frame(out, surface, CellRect::full(columns, rows))
            }
            Some(PixelStrategy::Animated(animated)) => {
                animated.render_frame(out, surface, CellRect::full(columns, rows))
            }
            Some(PixelStrategy::Sixel(sixel)) => sixel.render_frame(out, surface, columns, rows),
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
            animation: false,
            sixel: false,
            truecolor: false,
            identity: None,
            cell: None,
            text_area: None,
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
    fn a_sixel_terminal_gets_sixel_tiles() {
        let patched = crate::surface::PatchedKitty::new(
            90,
            CellPixels {
                width: 2,
                height: 4,
            },
        );
        let mut sixel = caps(false);
        sixel.sixel = true;
        let mut screen = PixelScreen::new(&sixel, patched);
        assert!(screen.draws_sixel());
        let mut out = Vec::new();
        screen
            .render_frame(&mut out, &plain(8, 4), 4, 2)
            .expect("writing to a Vec cannot fail");
        assert!(String::from_utf8_lossy(&out).contains("\x1bP0;1;0q"));
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

    #[test]
    fn an_animation_averse_graphics_terminal_gets_patches() {
        let mut no_animation = caps(true);
        no_animation.animation = false;
        let animated = crate::surface::AnimatedKitty::new(90, 20, 8, 16);
        let fallback = crate::surface::PatchedKitty::new(
            90,
            CellPixels {
                width: 2,
                height: 4,
            },
        );
        let mut screen = PixelScreen::animated(&no_animation, animated, fallback);
        let mut out = Vec::new();
        screen
            .render_frame(&mut out, &plain(8, 8), 4, 2)
            .expect("writing to a Vec cannot fail");
        let text = String::from_utf8_lossy(&out);
        assert!(!text.contains("a=f"), "no animation frames");
        assert!(text.contains("a=t"), "patch transmits carry the surface");
    }

    #[test]
    fn an_animation_capable_terminal_gets_frames() {
        let mut animated_caps = caps(true);
        animated_caps.animation = true;
        let animated = crate::surface::AnimatedKitty::new(90, 20, 8, 16);
        let fallback = crate::surface::PatchedKitty::new(
            90,
            CellPixels {
                width: 2,
                height: 4,
            },
        );
        let mut screen = PixelScreen::animated(&animated_caps, animated, fallback);
        let mut out = Vec::new();
        screen
            .render_frame(&mut out, &plain(8, 8), 4, 2)
            .expect("writing to a Vec cannot fail");
        let text = String::from_utf8_lossy(&out);
        assert!(text.contains("a=a"), "playback is terminal-driven");
    }
}
