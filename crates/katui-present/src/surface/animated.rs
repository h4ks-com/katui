//! The animated kitty strategy: the surface's generations become frames
//! of one terminal-side animation, played by the terminal's own
//! compositor while the wire only carries what changed.

use crate::Sink;
use std::io::{self, Write as _};

use katui_proto::kitty::{
    Animate, AnimationState, Canvas, Delete, FrameTransmit, ImageId as KittyImage,
};
use katui_scene::placement::CellRect;
use katui_scene::surface::diff_tiles;
use katui_scene::surface::{PixelRect, Surface};

use super::patched::tile_pixels;
use super::{damage_is_dense, write_whole_image};
use crate::kitty::crop_rect;

/// How many frames one animated image holds before a fresh root frame
/// replaces it: a bound on the terminal's frame storage.
const RESET_EVERY: u32 = 96;

/// The animated kitty surface presenter: the first frame becomes the
/// root frame of one image, placed once, and every later surface
/// appends a frame composed onto the previous one plus the rectangles
/// that changed. Playback runs in the terminal: loading mode plays the
/// frames it holds at the gap pace and waits for more, so the wire is a
/// buffer, jitter never reaches the screen, and motion happens at the
/// compositor's rate rather than the client's. An unchanged frame
/// writes nothing.
///
/// Frames accumulate terminal-side, so every 96 frames a fresh root frame
/// (one whole compressed transmit) goes up under the alternate id, and
/// then the finished reel stops and is freed.
#[derive(Debug)]
pub struct AnimatedKitty {
    base: u32,
    current: u32,
    tile_side: u32,
    budget: usize,
    gap_ms: i32,
    frame: u32,
    since_reset: u32,
    prev: Option<Surface>,
}

impl AnimatedKitty {
    /// A presenter handing out image ids from `base` upward, playing at
    /// `frames_per_second`, tiling changed rectangles at `tile_side`
    /// surface pixels. Past `budget` damaged rectangles one whole frame
    /// wins over hundreds of edits.
    #[must_use]
    pub fn new(base: u32, frames_per_second: u32, tile_side: u32, budget: usize) -> Self {
        Self {
            base,
            current: base,
            tile_side: tile_side.max(1),
            budget,
            gap_ms: (1000 / frames_per_second.max(1)).max(1).cast_signed(),
            frame: 0,
            since_reset: 0,
            prev: None,
        }
    }

    /// Start a fresh reel on the next frame, for when the terminal may
    /// have lost what it was showing.
    pub fn repaint(&mut self) {
        self.prev = None;
    }

    /// Free both reels and start a fresh one on the next frame.
    pub fn free<W: Sink>(&mut self, out: &mut W) -> io::Result<()> {
        for image in [self.base, self.base + 1] {
            out.write_all(&Delete::Free(KittyImage(image)).encode())?;
        }
        self.repaint();
        Ok(())
    }

    /// Write one frame's data and return the bytes written.
    pub fn render_frame<W: Sink>(
        &mut self,
        out: &mut W,
        surface: &Surface,
        area: CellRect,
    ) -> io::Result<usize> {
        let same_size = self.prev.as_ref().is_some_and(|prev| {
            prev.width() == surface.width() && prev.height() == surface.height()
        });
        if !same_size {
            return self.start_reel(out, surface, area);
        }
        if self.since_reset >= RESET_EVERY {
            return self.rotate_reel(out, surface, area);
        }
        let damage = self
            .prev
            .as_ref()
            .map(|prev| diff_tiles(prev, surface, self.tile_side))
            .unwrap_or_default();
        self.prev = Some(surface.clone());
        if damage.is_empty() {
            return Ok(0);
        }
        let next = self.frame + 1;
        let mut bytes = Vec::new();
        let dense = damage_is_dense(surface, self.tile_side, damage.len());
        if !dense && damage.len() <= self.budget {
            self.edited_frame(&mut bytes, surface, &damage, next)?;
        } else {
            self.whole_frame(&mut bytes, surface)?;
        }
        self.frame = next;
        self.since_reset += 1;
        let written = bytes.len();
        out.write_all(&bytes)?;
        Ok(written)
    }

    /// Start the next reel under the other id, then free the finished one,
    /// so we always have a frame on screen.
    fn rotate_reel<W: Sink>(
        &mut self,
        out: &mut W,
        surface: &Surface,
        area: CellRect,
    ) -> io::Result<usize> {
        let finished = self.current;
        self.current = if finished == self.base {
            self.base + 1
        } else {
            self.base
        };
        let written = self.start_reel(out, surface, area)?;
        Self::stop_reel(out, finished)?;
        Ok(written)
    }

    /// Frame `next` as a copy of its predecessor with one anchor pixel of
    /// real data, which the damaged rectangles then edit in place.
    fn edited_frame(
        &self,
        bytes: &mut Vec<u8>,
        surface: &Surface,
        damage: &[PixelRect],
        next: u32,
    ) -> io::Result<()> {
        let anchor = surface.pixels().get(..4).unwrap_or(&[0, 0, 0, 255]);
        let create = FrameTransmit {
            id: KittyImage(self.current),
            edit: None,
            canvas: Canvas::Frame(self.frame),
            rect: Some(crop_rect(PixelRect {
                x: 0,
                y: 0,
                width: 1,
                height: 1,
            })),
            width: 1,
            height: 1,
            payload: anchor,
            compressed: false,
            gap_ms: Some(self.gap_ms),
        };
        for frame in create.encode()? {
            bytes.write_all(&frame)?;
        }
        for rect in damage {
            let pixels = tile_pixels(surface, *rect);
            let edit = FrameTransmit {
                id: KittyImage(self.current),
                edit: Some(next),
                canvas: Canvas::Blank,
                rect: Some(crop_rect(*rect)),
                width: rect.width,
                height: rect.height,
                payload: &pixels,
                compressed: true,
                gap_ms: None,
            };
            for frame in edit.encode()? {
                bytes.write_all(&frame)?;
            }
        }
        Ok(())
    }

    /// The whole surface as the next frame.
    fn whole_frame(&self, bytes: &mut Vec<u8>, surface: &Surface) -> io::Result<()> {
        let whole = FrameTransmit {
            id: KittyImage(self.current),
            edit: None,
            canvas: Canvas::Blank,
            rect: None,
            width: surface.width(),
            height: surface.height(),
            payload: surface.pixels(),
            compressed: true,
            gap_ms: Some(self.gap_ms),
        };
        for frame in whole.encode()? {
            bytes.write_all(&frame)?;
        }
        Ok(())
    }

    /// Transmit the root frame, place it once, and hand playback to the
    /// terminal.
    fn start_reel<W: Sink>(
        &mut self,
        out: &mut W,
        surface: &Surface,
        area: CellRect,
    ) -> io::Result<usize> {
        let mut bytes = Vec::new();
        write_whole_image(&mut bytes, KittyImage(self.current), surface, area, None)?;
        let run = Animate {
            id: KittyImage(self.current),
            state: Some(AnimationState::Loading),
            frame_gap: Some((1, self.gap_ms)),
        };
        bytes.write_all(&run.encode())?;
        self.prev = Some(surface.clone());
        self.frame = 1;
        self.since_reset = 1;
        let written = bytes.len();
        out.write_all(&bytes)?;
        Ok(written)
    }

    /// Stop playback of `image` and free its data.
    fn stop_reel<W: Sink>(out: &mut W, image: u32) -> io::Result<()> {
        let stop = Animate {
            id: KittyImage(image),
            state: Some(AnimationState::Stop),
            frame_gap: None,
        };
        out.write_all(&stop.encode())?;
        out.write_all(&Delete::Free(KittyImage(image)).encode())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::AnimatedKitty;

    fn plain_surface(width: u32, height: u32) -> Surface {
        crate::surface::test_support::solid(width, height, Rgba::solid(10, 10, 30))
    }

    use katui_scene::surface::{Rgba, Surface};

    #[test]
    fn the_first_frame_is_a_root_frame_placed_and_playing() {
        let mut presenter = AnimatedKitty::new(90, 20, 8, 16);
        let mut out = Vec::new();
        presenter
            .render_frame(
                &mut out,
                &plain_surface(64, 64),
                crate::surface::test_support::area(),
            )
            .expect("writing to a Vec cannot fail");
        let text = String::from_utf8_lossy(&out);
        assert!(
            text.starts_with("\x1b_Ga=t,"),
            "an image must exist before kitty accepts frames for it"
        );
        assert!(!text.contains("a=f"), "the root is no animation frame");
        assert!(text.contains("o=z"), "compressed");
        assert!(text.contains("a=p,i=90"), "placed once");
        assert!(text.contains("a=a,i=90,s=2"), "playback is terminal-driven");
        assert!(text.contains("r=1,z=50"), "the root gap is 50ms at 20fps");
    }

    #[test]
    fn a_changed_frame_is_a_copy_plus_its_rectangles() {
        let mut presenter = AnimatedKitty::new(90, 20, 8, 16);
        let mut out = Vec::new();
        presenter
            .render_frame(
                &mut out,
                &plain_surface(64, 64),
                crate::surface::test_support::area(),
            )
            .expect("writing to a Vec cannot fail");
        let mut surface = plain_surface(64, 64);
        surface.set(
            katui_scene::surface::PixelPos::new(40, 40),
            Rgba::solid(255, 255, 255),
        );
        let mut second = Vec::new();
        presenter
            .render_frame(&mut second, &surface, crate::surface::test_support::area())
            .expect("writing to a Vec cannot fail");
        let text = String::from_utf8_lossy(&second);
        assert!(
            !text.contains("s=64,v=64,"),
            "the whole surface does not travel again"
        );
        assert!(
            text.contains("c=1,x=0,y=0,s=1,v=1"),
            "the frame is born a copy of its predecessor with an anchor pixel"
        );
        assert!(
            text.contains("r=2,x=40,y=40,s=8,v=8"),
            "the damaged tile edits frame two in place"
        );
    }

    #[test]
    fn an_unchanged_frame_writes_nothing() {
        let mut presenter = AnimatedKitty::new(90, 20, 8, 16);
        let mut out = Vec::new();
        let surface = plain_surface(64, 64);
        presenter
            .render_frame(&mut out, &surface, crate::surface::test_support::area())
            .expect("writing to a Vec cannot fail");
        let mut second = Vec::new();
        let written = presenter
            .render_frame(&mut second, &surface, crate::surface::test_support::area())
            .expect("writing to a Vec cannot fail");
        assert_eq!(written, 0);
        assert!(second.is_empty());
    }

    #[test]
    fn a_reel_resets_after_its_frame_budget() {
        let mut presenter = AnimatedKitty::new(90, 20, 8, 16);
        let mut surface = plain_surface(32, 32);
        let mut out = Vec::new();
        presenter
            .render_frame(&mut out, &surface, crate::surface::test_support::area())
            .expect("writing to a Vec cannot fail");
        for _ in 0..super::RESET_EVERY {
            surface.set(
                katui_scene::surface::PixelPos::new(4, 4),
                Rgba::solid(
                    (std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map_or(1, |d| d.subsec_nanos()) as u8)
                        .saturating_mul(0)
                        + 200,
                    40,
                    40,
                ),
            );
            surface.set(
                katui_scene::surface::PixelPos::new(5, 5),
                Rgba::solid(40, 200, 40),
            );
            presenter
                .render_frame(&mut out, &surface, crate::surface::test_support::area())
                .expect("writing to a Vec cannot fail");
            surface.set(
                katui_scene::surface::PixelPos::new(4, 4),
                Rgba::solid(40, 40, 200),
            );
            presenter
                .render_frame(&mut out, &surface, crate::surface::test_support::area())
                .expect("writing to a Vec cannot fail");
        }
        let text = String::from_utf8_lossy(&out);
        assert!(text.contains("a=a,i=90,s=1"), "the finished reel stops");
        assert!(
            text.contains("a=t,f=32,t=d,o=z,s=32,v=32,i=91"),
            "a fresh root frame starts the next reel"
        );
    }
}
