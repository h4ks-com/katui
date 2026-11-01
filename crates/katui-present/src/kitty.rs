//! The kitty placement presenter: a placement diff as graphics APC
//! frames, wrapped in synchronized output.
//!
//! Image ids are content-stable: new pixels get a new id, so an id is
//! transmitted once per session and never re-sent. Re-transmitting is the
//! one operation the protocol defines destructively (it deletes the image
//! and every placement of it), so the presenter tracks what it has sent
//! and which pairs are live, and retires an image once its last placement
//! leaves.

use crate::Sink;
use std::collections::HashSet;
use std::io::{self, Write as _};

use katui_proto::kitty::{self, ImageId as KittyImage, PlacementId as KittyPlacement};
use katui_scene::cell::Position;
use katui_scene::placement::{ImageId, Placement, PlacementDelta};

/// One image's RGBA payload, ready to transmit. `pixels` holds exactly
/// `4 * width * height` bytes.
#[derive(Debug, Clone, Copy)]
pub struct RgbaImage<'a> {
    width: u32,
    height: u32,
    pixels: &'a [u8],
}

impl<'a> RgbaImage<'a> {
    /// An image of `width` x `height` RGBA pixels.
    #[must_use]
    pub fn new(width: u32, height: u32, pixels: &'a [u8]) -> Self {
        Self {
            width,
            height,
            pixels,
        }
    }
}

/// The images a diff's added placements draw from, keyed by scene id.
#[derive(Debug, Default)]
pub struct Images<'a> {
    map: std::collections::HashMap<u32, RgbaImage<'a>>,
}

impl<'a> Images<'a> {
    /// Register an image under its scene id.
    pub fn insert(&mut self, id: ImageId, image: RgbaImage<'a>) {
        self.map.insert(id.0, image);
    }

    fn get(&self, id: ImageId) -> Option<RgbaImage<'_>> {
        self.map.get(&id.0).copied()
    }
}

/// The kitty presenter. Holds wire state only: which images were sent and
/// which placement pairs are live.
#[derive(Debug, Default)]
pub struct Kitty {
    sent: HashSet<u32>,
    live: HashSet<(u32, u32)>,
}

impl Kitty {
    /// A fresh presenter for a fresh terminal session.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Write `delta` as kitty graphics frames and return the byte count,
    /// the input of the frame budget pacing. The whole exchange sits in
    /// DEC 2026 synchronized output and leaves in one write, so a
    /// terminal never shows half of it. An empty diff writes nothing.
    ///
    /// Added placements place their image, transmitting it once if this
    /// presenter has not sent it yet; an added placement whose image is
    /// missing from `images` is skipped, because its pixels are not on
    /// the wire. Moves re-place their pair at the new origin. Removed
    /// pairs delete themselves, and an image whose last placement left is
    /// deleted whole.
    pub fn render<W: Sink>(
        &mut self,
        out: &mut W,
        delta: &PlacementDelta,
        images: &Images<'_>,
    ) -> io::Result<usize> {
        if delta.is_empty() {
            return Ok(0);
        }
        let mut bytes = Vec::new();
        bytes.write_all(b"\x1b[?2026h")?;
        let mut transmitted: HashSet<u32> = HashSet::new();
        for placement in &delta.added {
            let image = placement.image;
            if !self.sent.contains(&image.0)
                && !transmitted.contains(&image.0)
                && let Some(payload) = images.get(image)
            {
                let transmit = kitty::Transmit::new(
                    KittyImage(image.0),
                    kitty::Format::Rgba,
                    payload.width,
                    payload.height,
                    payload.pixels,
                );
                for frame in transmit.encode()? {
                    bytes.write_all(&frame)?;
                }
                transmitted.insert(image.0);
            }
            if self.sent.contains(&image.0) || transmitted.contains(&image.0) {
                place(&mut bytes, placement)?;
                self.live.insert((image.0, placement.id.0));
            }
        }
        for placement in &delta.moved {
            if self.live.contains(&(placement.image.0, placement.id.0)) {
                place(&mut bytes, placement)?;
            }
        }
        for removed in &delta.removed {
            let pair = (removed.image.0, removed.id.0);
            self.live.remove(&pair);
            bytes.write_all(
                &kitty::Delete::Placement(
                    KittyImage(removed.image.0),
                    KittyPlacement(removed.id.0),
                )
                .encode(),
            )?;
        }
        let mut emptied: Vec<u32> = self
            .sent
            .iter()
            .copied()
            .filter(|image| !self.live.iter().any(|(i, _)| i == image))
            .collect();
        emptied.sort_unstable();
        for image in emptied {
            bytes.write_all(&kitty::Delete::Image(KittyImage(image)).encode())?;
            self.sent.remove(&image);
        }
        self.sent.extend(transmitted);
        bytes.write_all(b"\x1b[?2026l")?;
        let written = bytes.len();
        out.write_all(&bytes)?;
        Ok(written)
    }
}

/// Address the placement origin, then anchor the place frame there.
fn place(bytes: &mut Vec<u8>, placement: &Placement) -> io::Result<()> {
    crate::cells::move_cursor(
        bytes,
        Position {
            x: placement.area.x,
            y: placement.area.y,
        },
    )?;
    let mut command = kitty::Place::new(KittyImage(placement.image.0));
    command.placement = KittyPlacement(placement.id.0);
    command.columns = u32::from(placement.area.columns);
    command.rows = u32::from(placement.area.rows);
    command.crop = placement.crop.map(|crop| kitty::PixelRect {
        x: crop.x,
        y: crop.y,
        width: crop.width,
        height: crop.height,
    });
    bytes.write_all(&command.encode())
}

#[cfg(test)]
mod tests {
    use super::{Images, Kitty, RgbaImage};
    use katui_proto::kitty::{self, ImageId as KittyImage, PlacementId as KittyPlacement};
    use katui_scene::placement::{CellRect, ImageId, Placement, PlacementDelta, PlacementId};

    const PIXELS: &[u8] = &[0xff, 0, 0, 0xff];

    fn placement(image: u32, id: u32, x: u16, y: u16) -> Placement {
        Placement {
            image: ImageId(image),
            id: PlacementId(id),
            area: CellRect {
                x,
                y,
                columns: 4,
                rows: 2,
            },
            crop: None,
        }
    }

    fn delta(added: &[Placement], moved: &[Placement], removed: &[Placement]) -> PlacementDelta {
        PlacementDelta {
            added: added.to_vec(),
            moved: moved.to_vec(),
            removed: removed.to_vec(),
        }
    }

    fn images() -> Images<'static> {
        let mut images = Images::default();
        images.insert(ImageId(7), RgbaImage::new(1, 1, PIXELS));
        images.insert(ImageId(8), RgbaImage::new(1, 1, PIXELS));
        images
    }

    fn transmit_frame(id: u32) -> String {
        let frames = kitty::Transmit::new(KittyImage(id), kitty::Format::Rgba, 1, 1, PIXELS)
            .encode()
            .expect("a Vec encoder cannot fail");
        let mut joined = String::new();
        for frame in frames {
            joined.push_str(&String::from_utf8_lossy(&frame));
        }
        joined
    }

    fn occurrences(text: &str, needle: &str) -> usize {
        text.matches(needle).count()
    }

    /// Draw the first frame of `image` into a throwaway writer, the
    /// starting state of every later-frame test.
    fn seed(presenter: &mut Kitty, image: u32) {
        let first = [placement(image, 1, 0, 0)];
        presenter
            .render(&mut Vec::new(), &delta(&first, &[], &[]), &images())
            .expect("writing to a Vec cannot fail");
    }

    #[test]
    fn an_empty_diff_writes_nothing() {
        let mut out = Vec::new();
        let written = Kitty::new()
            .render(&mut out, &delta(&[], &[], &[]), &images())
            .expect("writing to a Vec cannot fail");
        assert_eq!(written, 0);
        assert!(out.is_empty());
    }

    #[test]
    fn an_added_placement_transmits_once_then_places_inside_sync() {
        let mut out = Vec::new();
        let added = [placement(7, 1, 2, 3)];
        Kitty::new()
            .render(&mut out, &delta(&added, &[], &[]), &images())
            .expect("writing to a Vec cannot fail");
        let text = String::from_utf8_lossy(&out);
        assert!(text.starts_with("\x1b[?2026h"));
        assert!(text.ends_with("\x1b[?2026l"));
        let transmit_at = text
            .find(&transmit_frame(7))
            .expect("the transmit frame is on the wire");
        let cursor_at = text.find("\x1b[4;3H").expect("cursor at the origin");
        let mut place = kitty::Place::new(KittyImage(7));
        place.placement = KittyPlacement(1);
        place.columns = 4;
        place.rows = 2;
        let place_at = text
            .find(String::from_utf8_lossy(&place.encode()).as_ref())
            .expect("the place frame follows");
        assert!(transmit_at < cursor_at, "payload before addressing");
        assert!(cursor_at < place_at, "addressing before the place frame");
    }

    #[test]
    fn two_pairs_of_one_image_share_one_transmission() {
        let mut out = Vec::new();
        let added = [placement(7, 1, 0, 0), placement(7, 2, 6, 0)];
        let mut presenter = Kitty::new();
        presenter
            .render(&mut out, &delta(&added, &[], &[]), &images())
            .expect("writing to a Vec cannot fail");
        let text = String::from_utf8_lossy(&out);
        assert_eq!(
            occurrences(&text, &transmit_frame(7)),
            1,
            "one transmission per image per session"
        );
        assert!(text.contains("\x1b[1;1H"));
        assert!(text.contains("\x1b[1;7H"));
    }

    #[test]
    fn a_later_pair_places_without_retransmitting_the_live_image() {
        let mut presenter = Kitty::new();
        seed(&mut presenter, 7);
        let second = [placement(7, 2, 6, 0)];
        let mut out = Vec::new();
        presenter
            .render(&mut out, &delta(&second, &[], &[]), &images())
            .expect("writing to a Vec cannot fail");
        let text = String::from_utf8_lossy(&out);
        assert!(
            !text.contains("/wAA"),
            "the live image is never re-transmitted"
        );
        assert!(text.contains("\x1b[1;7H"), "the new pair places");
    }

    #[test]
    fn a_move_replaces_the_pair_without_transmitting() {
        let mut presenter = Kitty::new();
        seed(&mut presenter, 7);
        let moved = [placement(7, 1, 9, 9)];
        let mut out = Vec::new();
        presenter
            .render(&mut out, &delta(&[], &moved, &[]), &images())
            .expect("writing to a Vec cannot fail");
        let text = String::from_utf8_lossy(&out);
        assert!(!text.contains("/wAA"), "no payload on a move");
        assert!(text.contains("\x1b[10;10H"));
    }

    #[test]
    fn removing_the_last_pair_retires_the_image() {
        let mut presenter = Kitty::new();
        seed(&mut presenter, 7);
        let removed = [placement(7, 1, 0, 0)];
        let mut out = Vec::new();
        presenter
            .render(&mut out, &delta(&[], &[], &removed), &images())
            .expect("writing to a Vec cannot fail");
        let text = String::from_utf8_lossy(&out);
        let pair_delete = kitty::Delete::Placement(KittyImage(7), KittyPlacement(1)).encode();
        let image_delete = kitty::Delete::Image(KittyImage(7)).encode();
        let pair_at = text
            .find(String::from_utf8_lossy(&pair_delete).as_ref())
            .expect("the pair deletes");
        let image_at = text
            .find(String::from_utf8_lossy(&image_delete).as_ref())
            .expect("the emptied image retires");
        assert!(pair_at < image_at, "pairs go before their image");
    }

    #[test]
    fn an_added_placement_without_pixels_is_skipped() {
        let mut out = Vec::new();
        let added = [placement(99, 1, 0, 0)];
        Kitty::new()
            .render(&mut out, &delta(&added, &[], &[]), &images())
            .expect("writing to a Vec cannot fail");
        assert_eq!(
            String::from_utf8_lossy(&out),
            "\x1b[?2026h\x1b[?2026l",
            "only the sync wrapper"
        );
    }
}
