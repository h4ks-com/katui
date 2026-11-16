//! The composite pixel strategy: patched kitty tiles or terminal-side
//! animation when the terminal speaks kitty graphics, sixel tiles when it
//! speaks sixel, the damage-tracked blocks otherwise.

use crate::Sink;
use std::io;

use katui_proto::probe::{Capabilities, CellPixels};

use crate::select::{Backend, select};
use katui_scene::cell::Position;
use katui_scene::placement::CellRect;
use katui_scene::surface::{PixelRect, Surface};

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

/// Renders a pixel surface full-screen through the best strategy the
/// terminal offers.
#[derive(Debug, Default)]
pub struct PixelScreen {
    graphics: Option<PixelStrategy>,
    blocks: BlocksSurface,
}

impl PixelScreen {
    /// The screen for `caps` with cells of `cell` pixels: `patched` when
    /// the terminal speaks the kitty graphics protocol, sixel tiles when it
    /// speaks sixel, quarter blocks otherwise.
    #[must_use]
    pub fn new(caps: &Capabilities, cell: CellPixels, patched: PatchedKitty) -> Self {
        let patched = if caps.multiplexed() {
            patched.for_multiplexer(caps.id_space, caps.relative_placements)
        } else {
            patched
        };
        let graphics = match select(caps) {
            Backend::Kitty => Some(PixelStrategy::Patched(patched)),
            Backend::Sixel => Some(PixelStrategy::Sixel(SixelSurface::new(cell))),
            Backend::Blocks => None,
        };
        Self {
            graphics,
            blocks: BlocksSurface::new(),
        }
    }

    /// The screen for a terminal without kitty graphics: sixel tiles when
    /// it speaks sixel, quarter blocks otherwise.
    #[must_use]
    pub fn picture(caps: &Capabilities, cell: CellPixels) -> Self {
        let graphics =
            (select(caps) == Backend::Sixel).then(|| PixelStrategy::Sixel(SixelSurface::new(cell)));
        Self {
            graphics,
            blocks: BlocksSurface::new(),
        }
    }

    /// How many of `rows` screen rows a frame covers. Sixel leaves the
    /// bottom row free, because a sixel touching it scrolls the screen.
    #[must_use]
    pub fn rows_shown(&self, rows: u16) -> u16 {
        match self.graphics {
            Some(PixelStrategy::Sixel(_)) => rows.saturating_sub(1),
            _ => rows,
        }
    }

    /// The same screen driven by a terminal-side animation when the
    /// terminal stores animation frames outside a multiplexer, which
    /// swallows frames, and by [`Self::new`] with `fallback` otherwise.
    #[must_use]
    pub fn animated(
        caps: &Capabilities,
        cell: CellPixels,
        animated: AnimatedKitty,
        fallback: PatchedKitty,
    ) -> Self {
        if select(caps) == Backend::Kitty && caps.animation && !caps.multiplexed() {
            Self {
                graphics: Some(PixelStrategy::Animated(animated)),
                blocks: BlocksSurface::new(),
            }
        } else {
            Self::new(caps, cell, fallback)
        }
    }

    /// Draw for cells of `cell` pixels from now on, after a font zoom.
    pub fn set_cell(&mut self, cell: CellPixels) {
        if let Some(PixelStrategy::Patched(patched)) = &mut self.graphics {
            patched.set_cell(cell);
        }
    }

    /// Send every graphics pixel again on the next frame, for when the
    /// terminal may have lost its images, such as after a window resize.
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
    /// return the bytes written. Blocks show two by two pixels a cell, so
    /// a surface of another size is resized to that first.
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
            None => {
                let (width, height) = (u32::from(columns) * 2, u32::from(rows) * 2);
                if (surface.width(), surface.height()) == (width, height) {
                    self.blocks.render_frame(out, surface)
                } else {
                    surface
                        .scaled(width, height)
                        .map_or(Ok(0), |fitted| self.blocks.render_frame(out, &fitted))
                }
            }
        }
    }

    /// Write `surface` scaled over the `area` cells, leaving the rest of the
    /// screen to the app's own cells, and return the bytes written. On a
    /// sixel terminal `area` must stay above the bottom row.
    pub fn render_frame_in<W: Sink>(
        &mut self,
        out: &mut W,
        surface: &Surface,
        area: CellRect,
    ) -> io::Result<usize> {
        match &mut self.graphics {
            Some(PixelStrategy::Patched(patched)) => patched.render_frame(out, surface, area),
            Some(PixelStrategy::Animated(animated)) => animated.render_frame(out, surface, area),
            Some(PixelStrategy::Sixel(sixel)) => sixel.render_frame_in(out, surface, area),
            None => {
                let (width, height) = (u32::from(area.columns) * 2, u32::from(area.rows) * 2);
                let origin = Position {
                    x: area.x,
                    y: area.y,
                };
                surface.scaled(width, height).map_or(Ok(0), |fitted| {
                    self.blocks.render_frame_at(out, &fitted, origin)
                })
            }
        }
    }

    /// [`Self::render_frame_in`] for a frame whose changes since the last lie
    /// inside `damage`, in surface pixels, so kitty graphics compare only
    /// the tiles those rectangles touch.
    pub fn render_frame_damaged<W: Sink>(
        &mut self,
        out: &mut W,
        surface: &Surface,
        area: CellRect,
        damage: &[PixelRect],
    ) -> io::Result<usize> {
        match &mut self.graphics {
            Some(PixelStrategy::Patched(patched)) => {
                patched.render_frame_damaged(out, surface, area, damage)
            }
            _ => self.render_frame_in(out, surface, area),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::PixelScreen;
    use katui_proto::probe::{Capabilities, CellPixels, ProbeSource};
    use katui_scene::surface::{Rgba, Surface};

    const CELL: CellPixels = CellPixels {
        width: 2,
        height: 4,
    };

    fn caps(kitty: bool) -> Capabilities {
        Capabilities {
            kitty,
            animation: false,
            sixel: false,
            truecolor: false,
            identity: None,
            cell: None,
            text_area: None,
            relative_placements: false,
            local_files: false,
            key_releases: false,
            id_space: katui_proto::kitty::IdSpace::default(),
            trust: ProbeSource::Sniffed,
        }
    }

    fn plain(width: u32, height: u32) -> Surface {
        crate::surface::test_support::solid(width, height, Rgba::solid(255, 80, 40))
    }

    #[test]
    fn a_picture_in_a_cell_rectangle_starts_at_its_origin_and_keeps_the_rest() {
        let area = katui_scene::placement::CellRect {
            x: 3,
            y: 2,
            columns: 4,
            rows: 2,
        };
        let mut blocks = PixelScreen::picture(&caps(false), CELL);
        let mut out = Vec::new();
        blocks
            .render_frame_in(&mut out, &plain(8, 4), area)
            .expect("writing to a Vec cannot fail");
        let text = String::from_utf8_lossy(&out);
        assert!(
            text.contains("\x1b[3;4H"),
            "the first cell sits at row 3, column 4"
        );
        assert!(!text.contains("\x1b[2J"), "the rest of the screen stays");
        let sixel = Capabilities {
            sixel: true,
            ..caps(false)
        };
        let mut sixels = PixelScreen::picture(&sixel, CELL);
        out.clear();
        sixels
            .render_frame_in(&mut out, &plain(8, 8), area)
            .expect("writing to a Vec cannot fail");
        assert!(String::from_utf8_lossy(&out).contains("\x1b[3;4H"));
    }

    #[test]
    fn a_sixel_picture_covers_every_row_but_the_last_and_blocks_cover_all() {
        let sixel = Capabilities {
            sixel: true,
            ..caps(false)
        };
        assert_eq!(PixelScreen::picture(&sixel, CELL).rows_shown(24), 23);
        assert_eq!(PixelScreen::picture(&caps(false), CELL).rows_shown(24), 24);
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
        let mut screen = PixelScreen::new(&caps(true), CELL, patched);
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
        let mut screen = PixelScreen::new(&sixel, CELL, patched);
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
        let mut screen = PixelScreen::new(&caps(false), CELL, patched);
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
        let mut screen = PixelScreen::animated(&no_animation, CELL, animated, fallback);
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
        let mut screen = PixelScreen::animated(&animated_caps, CELL, animated, fallback);
        let mut out = Vec::new();
        screen
            .render_frame(&mut out, &plain(8, 8), 4, 2)
            .expect("writing to a Vec cannot fail");
        let text = String::from_utf8_lossy(&out);
        assert!(text.contains("a=a"), "playback is terminal-driven");
    }
}
