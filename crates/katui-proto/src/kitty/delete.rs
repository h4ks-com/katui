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

#[cfg(test)]
mod tests {

    use crate::kitty::{Delete, ImageId, PlacementId};

    #[test]
    fn delete_encodes_golden_frames() {
        assert_eq!(
            Delete::AllVisible.encode(),
            b"\x1b_Ga=d,d=a,q=2\x1b\\".to_vec()
        );
        assert_eq!(
            Delete::Image(ImageId(10)).encode(),
            b"\x1b_Ga=d,d=i,i=10,q=2\x1b\\".to_vec()
        );
        assert_eq!(
            Delete::Placement(ImageId(10), PlacementId(2)).encode(),
            b"\x1b_Ga=d,d=i,i=10,p=2,q=2\x1b\\".to_vec()
        );
        assert_eq!(
            Delete::Free(ImageId(10)).encode(),
            b"\x1b_Ga=d,d=I,i=10,q=2\x1b\\".to_vec()
        );
        assert_eq!(
            Delete::FreeRange(ImageId(5), ImageId(9)).encode(),
            b"\x1b_Ga=d,d=R,x=5,y=9,q=2\x1b\\".to_vec()
        );
        assert_eq!(
            crate::kitty::barrier(ImageId(7)),
            b"\x1b_Gi=7,s=1,v=1,a=q,t=d,f=24;AAAA\x1b\\".to_vec()
        );
    }

    #[test]
    fn a_zero_id_omits_the_id_keys() {
        assert_eq!(
            Delete::Image(ImageId(0)).encode(),
            b"\x1b_Ga=d,d=i,q=2\x1b\\".to_vec()
        );
        assert_eq!(
            Delete::Placement(ImageId(0), PlacementId(2)).encode(),
            b"\x1b_Ga=d,d=i,p=2,q=2\x1b\\".to_vec()
        );
    }
}
