//! The kitty placement presenter: a placement diff as graphics APC
//! frames, wrapped in synchronized output. Transmits ride along with
//! added placements; re-transmitting an existing image id replaces its
//! data, which is exactly the spec's update path, so no transmission
//! state lives here.

use std::collections::HashMap;
use std::io::{self, Write as _};

use katui_proto::kitty::{self, ImageId as KittyImage, PlacementId as KittyPlacement};
use katui_scene::cell::Position;
use katui_scene::placement::{ImageId, Placement, PlacementDelta};

/// One image's RGBA payload, ready to transmit.
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
    map: HashMap<u32, RgbaImage<'a>>,
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

/// Write `delta` as kitty graphics frames: every added placement with a
/// registered image transmits it and places it, moves re-place their pair
/// at the new origin, removed pairs delete. The whole exchange sits in
/// DEC 2026 synchronized output and leaves in one write, so a terminal
/// never shows half of it. An empty diff writes nothing.
///
/// An added placement whose image is missing from `images` is skipped:
/// its pixels are not on the wire, so its placement would show nothing.
pub fn render_placements<W: io::Write>(
    out: &mut W,
    delta: &PlacementDelta,
    images: &Images<'_>,
) -> io::Result<usize> {
    if delta.is_empty() {
        return Ok(0);
    }
    let mut bytes = Vec::new();
    bytes.write_all(b"\x1b[?2026h")?;
    for placement in &delta.added {
        if let Some(image) = images.get(placement.image) {
            let transmit = kitty::Transmit::new(
                KittyImage(placement.image.0),
                kitty::Format::Rgba,
                image.width,
                image.height,
                image.pixels,
            );
            for frame in transmit.encode() {
                bytes.write_all(&frame)?;
            }
            place(&mut bytes, placement)?;
        }
    }
    for placement in &delta.moved {
        place(&mut bytes, placement)?;
    }
    for removed in &delta.removed {
        bytes.write_all(
            &kitty::Delete::Placement(KittyImage(removed.image.0), KittyPlacement(removed.id.0))
                .encode(),
        )?;
    }
    bytes.write_all(b"\x1b[?2026l")?;
    let written = bytes.len();
    out.write_all(&bytes)?;
    Ok(written)
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

    use super::{Images, RgbaImage, render_placements};
    use katui_proto::kitty::{self, ImageId as KittyImage, PlacementId as KittyPlacement};
    use katui_scene::placement::{CellRect, ImageId, Placement, PlacementDelta, PlacementId};

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

    fn one_pixel_images() -> Images<'static> {
        let mut images = Images::default();
        images.insert(ImageId(7), RgbaImage::new(1, 1, &[0xff, 0, 0, 0xff]));
        images
    }

    #[test]
    fn an_empty_diff_writes_nothing() {
        let mut out = Vec::new();
        let written = render_placements(&mut out, &delta(&[], &[], &[]), &Images::default())
            .expect("writing to a Vec cannot fail");
        assert_eq!(written, 0);
        assert!(out.is_empty());
    }

    #[test]
    fn an_added_placement_transmits_then_places_inside_sync() {
        let mut out = Vec::new();
        let added = [placement(7, 1, 2, 3)];
        render_placements(&mut out, &delta(&added, &[], &[]), &one_pixel_images())
            .expect("writing to a Vec cannot fail");
        let text = String::from_utf8_lossy(&out);
        assert!(text.starts_with("\x1b[?2026h"));
        assert!(text.ends_with("\x1b[?2026l"));
        let transmit = kitty::Transmit::new(
            KittyImage(7),
            kitty::Format::Rgba,
            1,
            1,
            &[0xff, 0, 0, 0xff],
        )
        .encode();
        for frame in transmit {
            assert!(
                text.contains(String::from_utf8_lossy(&frame).as_ref()),
                "the transmit frame is on the wire"
            );
        }
        assert!(text.contains("\x1b[4;3H"), "cursor at the origin first");
        let mut place = kitty::Place::new(KittyImage(7));
        place.placement = KittyPlacement(1);
        place.columns = 4;
        place.rows = 2;
        assert!(
            text.contains(String::from_utf8_lossy(&place.encode()).as_ref()),
            "the place frame follows"
        );
    }

    #[test]
    fn a_move_replaces_the_pair_without_transmitting() {
        let mut out = Vec::new();
        let moved = [placement(7, 1, 9, 9)];
        render_placements(&mut out, &delta(&[], &moved, &[]), &one_pixel_images())
            .expect("writing to a Vec cannot fail");
        let text = String::from_utf8_lossy(&out);
        assert!(!text.contains(";AAAA"), "no payload on a move");
        assert!(text.contains("\x1b[10;10H"));
    }

    #[test]
    fn a_removed_pair_deletes_its_placement() {
        let mut out = Vec::new();
        let removed = [placement(7, 1, 0, 0)];
        render_placements(&mut out, &delta(&[], &[], &removed), &one_pixel_images())
            .expect("writing to a Vec cannot fail");
        let text = String::from_utf8_lossy(&out);
        let delete = kitty::Delete::Placement(KittyImage(7), KittyPlacement(1)).encode();
        assert!(text.contains(String::from_utf8_lossy(&delete).as_ref()));
    }

    #[test]
    fn an_added_placement_without_pixels_is_skipped() {
        let mut out = Vec::new();
        let added = [placement(99, 1, 0, 0)];
        render_placements(&mut out, &delta(&added, &[], &[]), &one_pixel_images())
            .expect("writing to a Vec cannot fail");
        let text = String::from_utf8_lossy(&out);
        assert_eq!(text, "\x1b[?2026h\x1b[?2026l", "only the sync wrapper");
    }
}
