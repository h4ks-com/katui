//! The anchor that keeps graphics inside a multiplexer pane: one Unicode
//! placeholder cell at the pane's top-left, which the multiplexer moves,
//! clears and hides like any text. Every image sits relative to it, so
//! the terminal shows our images only while the multiplexer shows the
//! anchor, and in the pane's place.

use std::io::{self, Write as _};

use katui_proto::kitty::{
    Format, IdSpace, ImageId as KittyImage, Place, PlacementId as KittyPlacement, Relative,
    Transmit,
};
use katui_proto::placeholder::placeholder_row;
use katui_scene::cell::Position;

use crate::cells::move_cursor;

/// One anchor per program, so every presenter shares its image id,
/// chosen high in the local ids to stay clear of scene and surface ids.
const ANCHOR_IMAGE: u32 = 0x00ff_fff0;
const ANCHOR_PLACEMENT: KittyPlacement = KittyPlacement(1);
const TRANSPARENT_PIXEL: [u8; 4] = [0; 4];

#[derive(Debug)]
pub(crate) struct Anchor {
    image: KittyImage,
    established: bool,
}

impl Anchor {
    pub(crate) fn new(space: IdSpace) -> Self {
        Self {
            image: space.image(ANCHOR_IMAGE),
            established: false,
        }
    }

    /// Send the anchor again on the next frame, for a terminal that may
    /// have lost it.
    pub(crate) fn forget(&mut self) {
        self.established = false;
    }

    /// Write the anchor cell, creating the anchor image first when the
    /// terminal lacks it. A frame writes this before any placement that
    /// sits relative to the anchor.
    pub(crate) fn write(&mut self, bytes: &mut Vec<u8>) -> io::Result<()> {
        if !self.established {
            let transmit = Transmit::new(self.image, Format::Rgba, 1, 1, &TRANSPARENT_PIXEL);
            for frame in transmit.encode()? {
                bytes.write_all(&frame)?;
            }
            let mut place = Place::new(self.image);
            place.placement = ANCHOR_PLACEMENT;
            place.columns = 1;
            place.rows = 1;
            place.virtual_placement = true;
            bytes.write_all(&place.encode())?;
            self.established = true;
        }
        if let Some(cell) = placeholder_row(self.image, 0, 1) {
            move_cursor(bytes, Position { x: 0, y: 0 })?;
            bytes.write_all(cell.as_bytes())?;
        }
        Ok(())
    }

    /// Where a placement at `origin` sits relative to the anchor.
    pub(crate) fn offset_to(&self, origin: Position) -> Relative {
        Relative {
            image: self.image,
            placement: ANCHOR_PLACEMENT,
            columns: i32::from(origin.x),
            rows: i32::from(origin.y),
        }
    }
}

#[cfg(test)]
mod tests {
    use katui_scene::cell::Position;

    use super::Anchor;

    #[test]
    fn the_anchor_image_goes_out_once_and_its_cell_every_frame() {
        let mut anchor = Anchor::new(katui_proto::kitty::IdSpace::of_process());
        let (mut first, mut second) = (Vec::new(), Vec::new());
        anchor
            .write(&mut first)
            .expect("writing to a Vec cannot fail");
        anchor
            .write(&mut second)
            .expect("writing to a Vec cannot fail");
        let first = String::from_utf8_lossy(&first);
        let second = String::from_utf8_lossy(&second);
        assert!(first.contains("a=t,") && first.contains("U=1"));
        assert!(!second.contains("\x1b_G"), "the image is already there");
        assert!(second.contains('\u{10eeee}'), "the cell is drawn again");
        let offset = anchor.offset_to(Position { x: 5, y: 3 });
        assert_eq!((offset.columns, offset.rows), (5, 3));
    }
}
