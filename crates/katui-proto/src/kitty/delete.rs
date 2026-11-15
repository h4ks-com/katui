//! Deletion (`a=d`) of placements and images.

use std::fmt::Write as _;

use super::{ImageId, PlacementId, apc};

/// Delete graphics (`a=d`). Deletes are one-way (`q=2`): nothing waits on
/// a delete, so the terminal never answers one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Delete {
    /// Every placement visible on screen (`d=a`).
    AllVisible,
    /// One image's placements, keeping its data for re-display (`d=i`).
    Image(ImageId),
    /// One image and its stored data (`d=I`).
    Free(ImageId),
    /// One placement of one image (`d=i` with `p`).
    Placement(ImageId, PlacementId),
    /// Every image with an id in this inclusive range, data included
    /// (`d=R`).
    FreeRange(ImageId, ImageId),
}

impl Delete {
    /// Encode as one APC frame.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut command = match self {
            Self::AllVisible => String::from("a=d,d=a"),
            Self::Free(_) => String::from("a=d,d=I"),
            Self::Image(_) | Self::Placement(..) => String::from("a=d,d=i"),
            Self::FreeRange(first, last) => format!("a=d,d=R,x={},y={}", first.0, last.0),
        };
        if let Self::Image(id) | Self::Free(id) | Self::Placement(id, _) = self
            && id.0 > 0
        {
            let _ = write!(command, ",i={}", id.0);
        }
        if let Self::Placement(_, placement) = self
            && placement.0 > 0
        {
            let _ = write!(command, ",p={}", placement.0);
        }
        command.push_str(",q=2");
        apc(&command).into_bytes()
    }
}
