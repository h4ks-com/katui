//! The composite pixel strategy: the patched kitty presenter when the
//! terminal speaks graphics, the damage-tracked blocks otherwise.

use crate::Sink;
use std::io;

use katui_proto::probe::Capabilities;
use katui_scene::placement::CellRect;
use katui_scene::surface::Surface;

use super::{AnimatedKitty, BlocksSurface, PatchedKitty};

/// The pixel strategy a [`PixelScreen`] drives on a graphics terminal.
#[derive(Debug)]
pub(super) enum PixelStrategy {
    /// Patched tile retransmits, each frame displayed as it arrives.
    Patched(PatchedKitty),
    /// Terminal-side animation frames, played by the compositor.
    Animated(AnimatedKitty),
}

/// The demo-grade pixel presenter for a terminal: one object that
/// renders a pixel surface full-screen through whatever the terminal
/// offers, so callers stop hand-picking strategies per capability. The
/// kitty strategy comes pre-built by the caller (tiles and budget are
/// policy); it is used only when the terminal actually speaks
/// graphics, and the blocks path carries every other terminal.
#[derive(Debug, Default)]
pub struct PixelScreen {
    kitty: Option<PixelStrategy>,
    blocks: BlocksSurface,
}

impl PixelScreen {
    /// The screen for `caps`: `patched` when the terminal speaks the
    /// kitty graphics protocol, quarter blocks otherwise.
    #[must_use]
    pub fn new(caps: &Capabilities, patched: PatchedKitty) -> Self {
        let patched = patched.tiles_only(caps.multiplexed());
        Self {
            kitty: caps.kitty.then_some(PixelStrategy::Patched(patched)),
            blocks: BlocksSurface::new(),
        }
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
            PixelStrategy::Patched(fallback.tiles_only(caps.multiplexed()))
        };
        Self {
            kitty: caps.kitty.then_some(strategy),
            blocks: BlocksSurface::new(),
        }
    }

    /// Send every graphics pixel again on the next frame, for when the
    /// terminal may have lost its images, a window resize for one.
    pub fn invalidate(&mut self) {
        match &mut self.kitty {
            Some(PixelStrategy::Patched(patched)) => patched.invalidate(),
            Some(PixelStrategy::Animated(animated)) => animated.invalidate(),
            None => {}
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
            Some(PixelStrategy::Patched(patched)) => {
                patched.render_frame(out, surface, CellRect::full(columns, rows))
            }
            Some(PixelStrategy::Animated(animated)) => {
                animated.render_frame(out, surface, CellRect::full(columns, rows))
            }
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
