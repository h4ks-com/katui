//! The cell plane: a grid of styled symbols plus the diff presenters
//! translate into terminal bytes. The plane stores one symbol per cell and
//! diffs by value; grapheme shaping and wide-glyph layout are the text
//! layer's concern, never the plane's.

use crate::CellSize;

/// A cell coordinate: `x` counts columns from the left, `y` rows from the
/// top.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Position {
    /// Column.
    pub x: u16,
    /// Row.
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
    /// Direct RGB.
    Rgb(u8, u8, u8),
}

/// The SGR render states a cell can carry, one bit each.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Attrs(u16);

impl Attrs {
    /// No attributes.
    pub const EMPTY: Self = Self(0);
    /// Bold text.
    pub const BOLD: Self = Self(1 << 0);
    /// Dim text.
    pub const DIM: Self = Self(1 << 1);
    /// Italic text.
    pub const ITALIC: Self = Self(1 << 2);
    /// Underlined text.
    pub const UNDERLINE: Self = Self(1 << 3);
    /// Blinking text.
    pub const BLINK: Self = Self(1 << 4);
    /// Rapidly blinking text.
    pub const RAPID_BLINK: Self = Self(1 << 5);
    /// Foreground and background swapped.
    pub const REVERSE: Self = Self(1 << 6);
    /// Invisible text.
    pub const HIDDEN: Self = Self(1 << 7);
    /// Struck-through text.
    pub const CROSSED: Self = Self(1 << 8);

    /// Both attribute sets at once.
    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// Whether every attribute of `other` is set here.
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    /// Whether nothing is set.
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
    /// Foreground color.
    pub fg: Color,
    /// Background color.
    pub bg: Color,
    /// Text attributes.
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
    pub fn blank(size: CellSize) -> Self {
        Self {
            size,
            cells: vec![Cell::default(); cell_count(size)],
        }
    }

    /// The grid size.
    pub fn size(&self) -> CellSize {
        self.size
    }

    /// The cell at `at`, `None` outside the grid.
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

    /// Write `text` left to right from `at`, one character per cell,
    /// clipped at the right edge. Graphemes wider than one cell land with
    /// the text layer. Returns the number of cells written.
    pub fn set_string(&mut self, at: Position, text: &str, fg: Color, bg: Color) -> usize {
        let mut written = 0;
        for (step, symbol) in text.chars().enumerate() {
            let x = at.x.saturating_add(u16::try_from(step).unwrap_or(u16::MAX));
            let cell = Cell {
                symbol: symbol.to_string(),
                fg,
                bg,
                attrs: crate::cell::Attrs::EMPTY,
            };
            if !self.set(Position { x, y: at.y }, cell) {
                break;
            }
            written += 1;
        }
        written
    }
}

/// The changed cells between two buffers, row-major: every position where
/// `next` differs, carrying `next`'s cell. Buffers of different sizes diff
/// as a full redraw.
pub fn diff<'a>(prev: &Buffer, next: &'a Buffer) -> Vec<(Position, &'a Cell)> {
    if prev.size() != next.size() {
        return all_cells(next);
    }
    let mut changed = Vec::new();
    for (index, (old, new)) in prev.cells.iter().zip(&next.cells).enumerate() {
        if old != new {
            let position = position_of(next.size(), index);
            if let Some(cell) = next.cells.get(index) {
                changed.push((position, cell));
            }
        }
    }
    changed
}

fn all_cells(buffer: &Buffer) -> Vec<(Position, &Cell)> {
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
    use super::{Buffer, Cell, Color, Position, diff};
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
        let written =
            buffer.set_string(Position { x: 2, y: 0 }, "hello", Color::Reset, Color::Reset);
        assert_eq!(written, 2);
        assert_eq!(buffer.get(Position { x: 2, y: 0 }), Some(&text_cell("h")));
        assert_eq!(buffer.get(Position { x: 3, y: 0 }), Some(&text_cell("e")));
        assert_eq!(buffer.get(Position { x: 0, y: 1 }), Some(&Cell::default()));
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
