//! The cell plane: a grid of styled symbols plus the diff presenters
//! translate into terminal bytes. A cell holds one grapheme; a wide
//! grapheme fills its cell and leaves the cells it covers empty, the way
//! ratatui's buffer does.

use unicode_segmentation::UnicodeSegmentation as _;
use unicode_width::UnicodeWidthStr as _;

use crate::CellSize;

/// A cell coordinate: `x` counts columns from the left, `y` rows from the
/// top.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Position {
    pub x: u16,
    pub y: u16,
}

/// A cell color: the terminal default, a palette index, or direct RGB.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Color {
    /// The terminal default color.
    #[default]
    Reset,
    /// A palette index, the 16 ANSI colors plus the 256-color cube.
    Indexed(u8),
    Rgb(u8, u8, u8),
}

/// The SGR render states a cell can carry, one bit each.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Attrs(u16);

impl Attrs {
    pub const EMPTY: Self = Self(0);
    pub const BOLD: Self = Self(1 << 0);
    pub const DIM: Self = Self(1 << 1);
    pub const ITALIC: Self = Self(1 << 2);
    pub const UNDERLINE: Self = Self(1 << 3);
    pub const BLINK: Self = Self(1 << 4);
    pub const RAPID_BLINK: Self = Self(1 << 5);
    /// Foreground and background swapped.
    pub const REVERSE: Self = Self(1 << 6);
    pub const HIDDEN: Self = Self(1 << 7);
    pub const CROSSED: Self = Self(1 << 8);

    /// Both attribute sets at once.
    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// Whether every attribute of `other` is set here.
    #[must_use]
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    /// Whether nothing is set.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }
}

/// One cell: a symbol, its colors and its attributes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cell {
    /// The symbol shown, usually one grapheme. A blank cell holds a
    /// single space.
    pub symbol: String,
    pub fg: Color,
    pub bg: Color,
    pub attrs: Attrs,
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            symbol: " ".to_owned(),
            fg: Color::Reset,
            bg: Color::Reset,
            attrs: Attrs::EMPTY,
        }
    }
}

/// A grid of cells, row-major, origin top-left.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Buffer {
    size: CellSize,
    cells: Vec<Cell>,
}

impl Buffer {
    /// A buffer of default cells: spaces in default colors.
    #[must_use]
    pub fn blank(size: CellSize) -> Self {
        Self {
            size,
            cells: vec![Cell::default(); cell_count(size)],
        }
    }

    #[must_use]
    pub fn size(&self) -> CellSize {
        self.size
    }

    /// The cell at `at`, `None` outside the grid.
    #[must_use]
    pub fn get(&self, at: Position) -> Option<&Cell> {
        self.cells.get(index_of(self.size, at)?)
    }

    /// Write `cell` at `at`; `false` and no change outside the grid.
    pub fn set(&mut self, at: Position, cell: Cell) -> bool {
        match index_of(self.size, at).and_then(|index| self.cells.get_mut(index)) {
            Some(slot) => {
                *slot = cell;
                true
            }
            None => false,
        }
    }

    /// Write `text` left to right from `at`, one grapheme per cell, a
    /// wide grapheme taking two cells, clipped at the right edge: a wide
    /// grapheme that does not fit stops the write. Zero-width graphemes
    /// are dropped. Returns the number of cells written.
    pub fn set_string(
        &mut self,
        at: Position,
        text: &str,
        fg: Color,
        bg: Color,
        attrs: Attrs,
    ) -> usize {
        let mut x = usize::from(at.x);
        let end = usize::from(self.size.columns);
        let styled = |symbol: &str| Cell {
            symbol: symbol.to_owned(),
            fg,
            bg,
            attrs,
        };
        for grapheme in text.graphemes(true) {
            let width = grapheme.width();
            if width == 0 {
                continue;
            }
            if x + width > end {
                break;
            }
            for (offset, symbol) in std::iter::once(grapheme)
                .chain(std::iter::repeat_n("", width - 1))
                .enumerate()
            {
                let column = u16::try_from(x + offset).unwrap_or(u16::MAX);
                self.set(Position { x: column, y: at.y }, styled(symbol));
            }
            x += width;
        }
        x - usize::from(at.x)
    }
}

/// The changed cells between two buffers, row-major: every position where
/// `next` differs, carrying `next`'s cell. Buffers of different sizes diff
/// as a full redraw.
#[must_use]
pub fn diff<'a>(prev: &Buffer, next: &'a Buffer) -> Vec<(Position, &'a Cell)> {
    if prev.size() != next.size() {
        return all_cells(next);
    }
    let mut changed = Vec::new();
    // we skip the cells a wide grapheme covers, and we redraw the cells a
    // wide grapheme used to cover, because the terminal blanked them
    let (mut covered, mut uncovered) = (0_usize, 0_usize);
    for (index, (old, new)) in prev.cells.iter().zip(&next.cells).enumerate() {
        if covered == 0 && (old != new || uncovered > 0) {
            changed.push((position_of(next.size(), index), new));
        }
        let (new_width, old_width) = (new.symbol.width(), old.symbol.width());
        covered = new_width.saturating_sub(1);
        uncovered = new_width.max(old_width).max(uncovered).saturating_sub(1);
    }
    changed
}

/// Every cell of a buffer with its position, row-major.
#[must_use]
pub fn all_cells(buffer: &Buffer) -> Vec<(Position, &Cell)> {
    buffer
        .cells
        .iter()
        .enumerate()
        .map(|(index, cell)| (position_of(buffer.size(), index), cell))
        .collect()
}

fn cell_count(size: CellSize) -> usize {
    usize::from(size.columns) * usize::from(size.rows)
}

fn index_of(size: CellSize, at: Position) -> Option<usize> {
    if at.x < size.columns && at.y < size.rows {
        Some(usize::from(at.y) * usize::from(size.columns) + usize::from(at.x))
    } else {
        None
    }
}

fn position_of(size: CellSize, index: usize) -> Position {
    let columns = usize::from(size.columns).max(1);
    Position {
        x: u16::try_from(index % columns).unwrap_or(u16::MAX),
        y: u16::try_from(index / columns).unwrap_or(u16::MAX),
    }
}

#[cfg(test)]
mod tests {
    use super::{Attrs, Buffer, Cell, Color, Position, diff};
    use crate::CellSize;

    fn size() -> CellSize {
        CellSize {
            columns: 4,
            rows: 3,
        }
    }

    fn text_cell(symbol: &str) -> Cell {
        Cell {
            symbol: symbol.to_owned(),
            fg: Color::Reset,
            bg: Color::Reset,
            attrs: super::Attrs::EMPTY,
        }
    }

    #[test]
    fn attrs_combine_and_test_membership() {
        let style = super::Attrs::BOLD.union(super::Attrs::UNDERLINE);
        assert!(style.contains(super::Attrs::BOLD));
        assert!(!style.contains(super::Attrs::ITALIC));
        assert!(super::Attrs::EMPTY.is_empty());
    }

    #[test]
    fn the_default_cell_is_a_blank_space() {
        assert_eq!(Cell::default().symbol, " ");
        assert_eq!(Cell::default().fg, Color::Reset);
    }

    #[test]
    fn diff_of_identical_buffers_is_empty() {
        let buffer = Buffer::blank(size());
        assert!(diff(&buffer, &buffer).is_empty());
    }

    #[test]
    fn diff_reports_exactly_the_change_in_row_major_order() {
        let mut prev = Buffer::blank(size());
        let mut next = prev.clone();
        prev.set(Position { x: 0, y: 1 }, text_cell("a"));
        next.set(Position { x: 0, y: 1 }, text_cell("a"));
        next.set(Position { x: 3, y: 1 }, text_cell("b"));
        next.set(Position { x: 1, y: 2 }, text_cell("c"));
        let changed = diff(&prev, &next);
        let positions: Vec<Position> = changed.iter().map(|(at, _)| *at).collect();
        assert_eq!(
            positions,
            vec![Position { x: 3, y: 1 }, Position { x: 1, y: 2 }],
            "storage order: rows before columns"
        );
        assert_eq!(changed[0].1, &text_cell("b"));
        assert_eq!(changed[1].1, &text_cell("c"));
    }

    #[test]
    fn out_of_bounds_writes_change_nothing() {
        let mut buffer = Buffer::blank(size());
        let before = buffer.clone();
        assert!(!buffer.set(Position { x: 4, y: 0 }, text_cell("x")));
        assert!(!buffer.set(Position { x: 0, y: 3 }, text_cell("x")));
        assert_eq!(buffer, before);
        assert_eq!(buffer.get(Position { x: 4, y: 0 }), None);
    }

    #[test]
    fn set_string_clips_at_the_right_edge() {
        let mut buffer = Buffer::blank(size());
        let written = buffer.set_string(
            Position { x: 2, y: 0 },
            "hello",
            Color::Reset,
            Color::Reset,
            Attrs::EMPTY,
        );
        assert_eq!(written, 2);
        assert_eq!(buffer.get(Position { x: 2, y: 0 }), Some(&text_cell("h")));
        assert_eq!(buffer.get(Position { x: 3, y: 0 }), Some(&text_cell("e")));
        assert_eq!(buffer.get(Position { x: 0, y: 1 }), Some(&Cell::default()));
    }

    fn written(text: &str) -> Buffer {
        let mut buffer = Buffer::blank(size());
        buffer.set_string(
            Position { x: 0, y: 0 },
            text,
            Color::Reset,
            Color::Reset,
            Attrs::EMPTY,
        );
        buffer
    }

    #[test]
    fn a_wide_grapheme_fills_two_cells_and_keeps_its_variation_selector() {
        let buffer = written("界\u{2764}\u{fe0f}");
        let symbols: Vec<&str> = (0..4)
            .filter_map(|x| buffer.get(Position { x, y: 0 }))
            .map(|cell| cell.symbol.as_str())
            .collect();
        assert_eq!(symbols, ["界", "", "\u{2764}\u{fe0f}", ""]);
        assert_eq!(
            written("a界界").get(Position { x: 3, y: 0 }),
            Some(&text_cell(" ")),
            "a wide grapheme past the edge stops the write"
        );
        assert_eq!(
            written("e\u{301}x").get(Position { x: 0, y: 0 }),
            Some(&text_cell("e\u{301}")),
            "combining marks stay in their cell"
        );
    }

    #[test]
    fn the_diff_skips_covered_cells_and_redraws_uncovered_ones() {
        let blank = Buffer::blank(size());
        let wide = written("界a");
        let positions = |changes: Vec<(Position, &Cell)>| -> Vec<u16> {
            changes.iter().map(|(at, _)| at.x).collect()
        };
        assert_eq!(
            positions(diff(&blank, &wide)),
            [0, 2],
            "the covered cell stays out"
        );
        let narrow = written("xya");
        assert_eq!(
            positions(diff(&wide, &narrow)),
            [0, 1],
            "the uncovered cell comes back"
        );
    }

    #[test]
    fn buffers_of_different_sizes_diff_as_a_full_redraw() {
        let small = Buffer::blank(crate::CellSize {
            columns: 2,
            rows: 2,
        });
        let large = Buffer::blank(size());
        let changed = diff(&small, &large);
        assert_eq!(changed.len(), 12, "every cell of the new buffer");
    }

    #[test]
    fn colors_compare_across_kinds() {
        assert_ne!(Color::Reset, Color::Indexed(4));
        assert_ne!(Color::Indexed(4), Color::Rgb(4, 4, 4));
        assert_eq!(Color::Rgb(1, 2, 3), Color::Rgb(1, 2, 3));
    }
}
