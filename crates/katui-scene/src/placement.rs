//! Image placements on the scene: stable ids, cell rectangles, and the
//! diff between two placement sets. The presenter layer turns a diff into
//! protocol bytes; the ids here are scene-level and map one to one onto
//! the protocol ids there.

/// An image id, stable for the image's lifetime on screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ImageId(pub u32);

/// A placement id, unique per image. Re-sending the same image and
/// placement pair moves the placement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PlacementId(pub u32);

/// A rectangle of cells: origin plus extent. Zero columns or rows leave
/// the display size to the terminal, which then draws the image at its
/// natural pixel size.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CellRect {
    /// Left edge, in columns.
    pub x: u16,
    /// Top edge, in rows.
    pub y: u16,
    /// Width, in columns.
    pub columns: u16,
    /// Height, in rows.
    pub rows: u16,
}

impl CellRect {
    /// A rectangle covering a whole `columns` x `rows` screen from the
    /// origin.
    #[must_use]
    pub const fn full(columns: u16, rows: u16) -> Self {
        Self {
            x: 0,
            y: 0,
            columns,
            rows,
        }
    }
}

/// A rectangle of the source image, in pixels. A zero-sized crop
/// intersects the image to nothing and displays no pixels. The one
/// pixel rectangle the scene exports: [`crate::surface::PixelRect`],
/// re-exported here so placement consumers need one import only.
pub use crate::surface::PixelRect;

/// One image on screen: which image, which placement of it, where, and
/// how much of the source pixels it shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Placement {
    /// The transmitted image this placement shows.
    pub image: ImageId,
    /// The placement id, for flicker-free moves.
    pub id: PlacementId,
    /// Where the placement sits, in cells.
    pub area: CellRect,
    /// The source rectangle, [`None`] for the whole image.
    pub crop: Option<PixelRect>,
    /// Stacking order; higher draws on top and negative draws under text.
    pub z: i32,
}

impl Placement {
    fn pair(self) -> (u32, u32) {
        (self.image.0, self.id.0)
    }
}

/// The difference between two placement sets.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PlacementDelta {
    /// Placements with no pair on the previous scene.
    pub added: Vec<Placement>,
    /// Placements whose pair existed but whose geometry changed, carrying
    /// the new geometry.
    pub moved: Vec<Placement>,
    /// Previous placements with no pair on the next scene, carrying the
    /// old geometry.
    pub removed: Vec<Placement>,
}

impl PlacementDelta {
    /// Whether nothing changed.
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.moved.is_empty() && self.removed.is_empty()
    }
}

/// Diff two placement sets by (image, placement) pair: a new pair adds, a
/// changed geometry moves, a vanished pair removes. Both sets hold each
/// pair at most once. Search is linear per entry; placement counts are
/// tens, never thousands.
pub fn diff(prev: &[Placement], next: &[Placement]) -> PlacementDelta {
    let mut delta = PlacementDelta::default();
    for placement in next {
        match prev.iter().find(|old| old.pair() == placement.pair()) {
            None => delta.added.push(*placement),
            Some(old)
                if old.area != placement.area
                    || old.crop != placement.crop
                    || old.z != placement.z =>
            {
                delta.moved.push(*placement);
            }
            Some(_) => {}
        }
    }
    for old in prev {
        if !next.iter().any(|p| p.pair() == old.pair()) {
            delta.removed.push(*old);
        }
    }
    delta
}

#[cfg(test)]
mod tests {
    use super::{CellRect, ImageId, Placement, PlacementId, diff};

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
            z: 0,
        }
    }

    #[test]
    fn identical_sets_diff_to_nothing() {
        let set = [placement(1, 1, 0, 0), placement(2, 1, 6, 0)];
        assert!(diff(&set, &set).is_empty());
    }

    #[test]
    fn a_changed_geometry_moves_the_pair() {
        let prev = [placement(1, 1, 0, 0)];
        let next = [placement(1, 1, 3, 4)];
        let delta = diff(&prev, &next);
        assert_eq!(delta.moved, vec![placement(1, 1, 3, 4)]);
        assert!(delta.added.is_empty());
        assert!(delta.removed.is_empty());
    }

    #[test]
    fn a_changed_crop_moves_the_pair() {
        let mut prev = [placement(1, 1, 0, 0)];
        prev[0].crop = Some(super::PixelRect::default());
        let mut next = [placement(1, 1, 0, 0)];
        next[0].crop = Some(super::PixelRect {
            x: 4,
            ..super::PixelRect::default()
        });
        let delta = diff(&prev, &next);
        assert_eq!(delta.moved.len(), 1);
        assert!(delta.added.is_empty());
        assert!(delta.removed.is_empty());
    }

    #[test]
    fn new_pairs_add_and_vanished_pairs_remove() {
        let prev = [placement(1, 1, 0, 0), placement(2, 1, 6, 0)];
        let next = [placement(1, 1, 0, 0), placement(3, 1, 6, 0)];
        let delta = diff(&prev, &next);
        assert_eq!(delta.added, vec![placement(3, 1, 6, 0)]);
        assert_eq!(delta.removed, vec![placement(2, 1, 6, 0)]);
        assert!(delta.moved.is_empty());
    }

    #[test]
    fn the_same_placement_id_under_another_image_is_another_pair() {
        let prev = [placement(1, 1, 0, 0)];
        let next = [placement(2, 1, 0, 0)];
        let delta = diff(&prev, &next);
        assert_eq!(delta.added.len(), 1);
        assert_eq!(delta.removed.len(), 1);
    }
}
