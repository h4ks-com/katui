//! Placement (`a=p`): where a transmitted image shows, and which part.

use std::fmt::Write as _;

use super::{ImageId, PlacementId, Quiet, apc};

/// A rectangle of the source image, in pixels (the `x y w h` crop keys).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PixelRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// Display a transmitted image (`a=p`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Place {
    /// The image to display; required, a zero id makes the terminal ignore
    /// the command.
    pub image: ImageId,
    /// Placement id: re-sending the same pair moves it without flicker.
    pub placement: PlacementId,
    /// Display width in columns (`c`); with `rows` alone the terminal
    /// scales keeping the aspect ratio.
    pub columns: u32,
    /// Display height in rows (`r`).
    pub rows: u32,
    /// Z-order; negative draws under the text.
    pub z: i32,
    /// Source rectangle crop, [`None`] displays the whole image.
    pub crop: Option<PixelRect>,
    /// Answer suppression, [`Quiet::Silent`] from the constructor. A silent
    /// placement is the default because a render loop re-places every
    /// frame, and one answer per placement floods the input stream.
    pub quiet: Quiet,
    /// Keep the cursor from advancing past the image (`C=1`).
    pub hold_cursor: bool,
    /// A virtual placement that Unicode placeholder text shows (`U=1`).
    pub virtual_placement: bool,
    /// The placement this one sits relative to (`P`, `Q`), offset by
    /// whole cells (`H`, `V`).
    pub parent: Option<Relative>,
}

/// Where a placement sits relative to another placement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Relative {
    /// The parent's image.
    pub image: ImageId,
    /// The parent's placement.
    pub placement: PlacementId,
    /// Cells to the right of the parent's top-left cell.
    pub columns: i32,
    /// Cells below the parent's top-left cell.
    pub rows: i32,
}

impl Place {
    /// Build a silent placement holding the cursor, the presenter default.
    #[must_use]
    pub fn new(image: ImageId) -> Self {
        Self {
            image,
            placement: PlacementId(0),
            columns: 0,
            rows: 0,
            z: 0,
            crop: None,
            quiet: Quiet::Silent,
            hold_cursor: true,
            virtual_placement: false,
            parent: None,
        }
    }

    /// Encode as one APC frame.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut command = String::from("a=p");
        if self.image.0 > 0 {
            let _ = write!(command, ",i={}", self.image.0);
        }
        if self.placement.0 > 0 {
            let _ = write!(command, ",p={}", self.placement.0);
        }
        if self.columns > 0 {
            let _ = write!(command, ",c={}", self.columns);
        }
        if self.rows > 0 {
            let _ = write!(command, ",r={}", self.rows);
        }
        if self.z != 0 {
            let _ = write!(command, ",z={}", self.z);
        }
        if let Some(crop) = self.crop {
            let _ = write!(
                command,
                ",x={},y={},w={},h={}",
                crop.x, crop.y, crop.width, crop.height
            );
        }
        let _ = write!(command, ",q={}", self.quiet.key());
        if self.hold_cursor {
            command.push_str(",C=1");
        }
        if self.virtual_placement {
            command.push_str(",U=1");
        }
        if let Some(parent) = self.parent {
            let _ = write!(
                command,
                ",P={},Q={},H={},V={}",
                parent.image.0, parent.placement.0, parent.columns, parent.rows
            );
        }
        apc(&command).into_bytes()
    }
}

#[cfg(test)]
mod tests {

    use crate::kitty::{ImageId, Place, PlacementId};

    #[test]
    fn place_encodes_a_golden_frame() {
        let mut place = Place::new(ImageId(7));
        place.placement = PlacementId(3);
        place.columns = 4;
        place.rows = 2;
        place.z = -1;
        assert_eq!(
            place.encode(),
            b"\x1b_Ga=p,i=7,p=3,c=4,r=2,z=-1,q=2,C=1\x1b\\".to_vec()
        );
    }

    #[test]
    fn a_relative_placement_names_its_parent_and_offset() {
        let mut place = Place::new(ImageId(7));
        place.placement = PlacementId(3);
        place.parent = Some(super::Relative {
            image: ImageId(9),
            placement: PlacementId(1),
            columns: 4,
            rows: 2,
        });
        assert_eq!(
            place.encode(),
            b"\x1b_Ga=p,i=7,p=3,q=2,C=1,P=9,Q=1,H=4,V=2\x1b\\".to_vec()
        );
    }

    #[test]
    fn place_appends_the_crop_keys() {
        let mut place = Place::new(ImageId(7));
        place.crop = Some(super::PixelRect {
            x: 10,
            y: 5,
            width: 100,
            height: 200,
        });
        assert_eq!(
            place.encode(),
            b"\x1b_Ga=p,i=7,x=10,y=5,w=100,h=200,q=2,C=1\x1b\\".to_vec()
        );
    }
}
