//! Constraint layout: splitting a cell rectangle along one axis into
//! sized children. Percentages round down, lengths hold, fills share
//! the rest. A Fill absorbs whatever the other constraints leave, so
//! a split containing a Fill covers its area exactly once; length-only
//! splits cover only what they claim, and overlong claims saturate
//! rather than wrap.

use katui_scene::placement::CellRect;

/// One child's size claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Constraint {
    /// A fixed number of cells.
    Length(u16),
    /// A share of the whole axis, rounded down.
    Percentage(u8),
    /// An equal share of whatever the others leave.
    Fill,
}

/// Split `area` into columns, left to right.
#[must_use]
pub fn split_horizontal(area: CellRect, constraints: &[Constraint]) -> Vec<CellRect> {
    let sizes = resolve(u32::from(area.columns), constraints);
    let mut x = area.x;
    sizes
        .into_iter()
        .map(|width| {
            let rect = CellRect {
                x,
                y: area.y,
                columns: width,
                rows: area.rows,
            };
            x = x.saturating_add(width);
            rect
        })
        .collect()
}

/// Split `area` into rows, top to bottom.
#[must_use]
pub fn split_vertical(area: CellRect, constraints: &[Constraint]) -> Vec<CellRect> {
    let sizes = resolve(u32::from(area.rows), constraints);
    let mut y = area.y;
    sizes
        .into_iter()
        .map(|height| {
            let rect = CellRect {
                x: area.x,
                y,
                columns: area.columns,
                rows: height,
            };
            y = y.saturating_add(height);
            rect
        })
        .collect()
}

fn resolve(whole: u32, constraints: &[Constraint]) -> Vec<u16> {
    let whole = whole.min(u32::from(u16::MAX));
    let mut sizes = Vec::with_capacity(constraints.len());
    let mut fill_indexes = Vec::new();
    let mut used = 0u32;
    for (index, constraint) in constraints.iter().enumerate() {
        match constraint {
            Constraint::Length(cells) => {
                used = used.saturating_add(u32::from(*cells));
                sizes.push(*cells);
            }
            Constraint::Percentage(percent) => {
                let size = whole * u32::from(*percent) / 100;
                used = used.saturating_add(size);
                sizes.push(u16::try_from(size).unwrap_or(u16::MAX));
            }
            Constraint::Fill => {
                fill_indexes.push(index);
                sizes.push(0);
            }
        }
    }
    if !fill_indexes.is_empty() {
        let mut rest = whole.saturating_sub(used.min(whole));
        let share = u32::try_from(fill_indexes.len()).unwrap_or(1);
        let each = rest / share;
        for &index in &fill_indexes {
            if let Some(slot) = sizes.get_mut(index) {
                *slot = u16::try_from(each).unwrap_or(u16::MAX);
            }
            rest = rest.saturating_sub(each);
        }
        // the division's remainder goes to the first fills, one cell each
        for &index in &fill_indexes {
            if rest == 0 {
                break;
            }
            if let Some(slot) = sizes.get_mut(index) {
                *slot = (*slot).saturating_add(1);
            }
            rest -= 1;
        }
    }
    sizes
}

#[cfg(test)]
mod tests {
    use super::{Constraint, split_horizontal, split_vertical};
    use katui_scene::placement::CellRect;

    fn area(columns: u16, rows: u16) -> CellRect {
        CellRect {
            x: 0,
            y: 0,
            columns,
            rows,
        }
    }

    fn widths(rects: &[CellRect]) -> Vec<u16> {
        rects.iter().map(|rect| rect.columns).collect()
    }

    fn heights(rects: &[CellRect]) -> Vec<u16> {
        rects.iter().map(|rect| rect.rows).collect()
    }

    #[test]
    fn lengths_hold_their_cells() {
        let rects = split_horizontal(
            area(20, 5),
            &[
                Constraint::Length(5),
                Constraint::Fill,
                Constraint::Length(3),
            ],
        );
        assert_eq!(widths(&rects), vec![5, 12, 3]);
        assert_eq!(
            rects[2].x + rects[2].columns,
            20,
            "the split reaches the edge"
        );
    }

    #[test]
    fn percentages_round_down_and_fills_take_the_rest() {
        let rects = split_horizontal(
            area(13, 4),
            &[
                Constraint::Percentage(50),
                Constraint::Fill,
                Constraint::Fill,
            ],
        );
        assert_eq!(widths(&rects), vec![6, 4, 3]);
    }

    #[test]
    fn fills_share_a_leftover_cell_to_the_earliest() {
        let rects = split_horizontal(
            area(11, 4),
            &[Constraint::Fill, Constraint::Fill, Constraint::Length(4)],
        );
        assert_eq!(widths(&rects), vec![4, 3, 4]);
    }

    #[test]
    fn vertical_splits_stack_rows() {
        let rects = split_vertical(
            area(30, 10),
            &[
                Constraint::Length(1),
                Constraint::Fill,
                Constraint::Length(2),
            ],
        );
        assert_eq!(heights(&rects), vec![1, 7, 2]);
        assert_eq!(rects[1].y, 1);
        assert_eq!(rects[2].y, 8);
    }

    #[test]
    fn splits_start_at_the_areas_origin() {
        let rects = split_horizontal(
            CellRect {
                x: 5,
                y: 7,
                columns: 10,
                rows: 3,
            },
            &[Constraint::Fill, Constraint::Fill],
        );
        assert_eq!(rects[0].x, 5);
        assert_eq!(rects[1].x, 10);
        assert_eq!(rects[0].rows, 3);
    }

    #[test]
    fn an_overlong_claim_saturates_instead_of_wrapping() {
        let rects = split_horizontal(area(5, 2), &[Constraint::Length(9), Constraint::Fill]);
        assert_eq!(widths(&rects), vec![9, 0]);
    }
}
