//! The cell presenter: scene cells as SGR sequences, plus the cursor and
//! clear commands every presenter consumer needs. Every cell
//! re-establishes its whole style because it cannot know what the cell
//! before it left behind; run coalescing belongs to the frame presenter.

use std::fmt::Write as _;

use katui_scene::cell::{Attrs, Cell, Color, Position};

/// SGR codes for the attributes, in emission order.
const ATTR_CODES: &[(Attrs, u8)] = &[
    (Attrs::BOLD, 1),
    (Attrs::DIM, 2),
    (Attrs::ITALIC, 3),
    (Attrs::UNDERLINE, 4),
    (Attrs::BLINK, 5),
    (Attrs::RAPID_BLINK, 6),
    (Attrs::REVERSE, 7),
    (Attrs::HIDDEN, 8),
    (Attrs::CROSSED, 9),
];

/// The region a clear erases.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClearKind {
    /// The whole visible screen.
    All,
    /// From the cursor through the end of the screen.
    AfterCursor,
    /// From the start of the screen through the cursor.
    BeforeCursor,
    /// The cursor's whole line.
    CurrentLine,
    /// From the cursor through the end of its line.
    UntilNewLine,
}

/// Write `cell` at `at`: cursor addressing, reset, colors, attributes,
/// symbol.
pub fn render_cell<W: std::io::Write>(
    out: &mut W,
    at: Position,
    cell: &Cell,
) -> std::io::Result<()> {
    let mut text = String::new();
    push_cursor(&mut text, at);
    text.push_str("\x1b[0m");
    write_color(&mut text, 38, 39, cell.fg);
    write_color(&mut text, 48, 49, cell.bg);
    for (attr, code) in ATTR_CODES {
        if cell.attrs.contains(*attr) {
            let _ = write!(text, "\x1b[{code}m");
        }
    }
    text.push_str(&cell.symbol);
    out.write_all(text.as_bytes())
}

/// Address the cursor to `at`, one-based through `u32` so the extremes
/// cannot wrap.
pub fn move_cursor<W: std::io::Write>(out: &mut W, at: Position) -> std::io::Result<()> {
    let mut text = String::new();
    push_cursor(&mut text, at);
    out.write_all(text.as_bytes())
}

/// Hide the cursor.
pub fn hide_cursor<W: std::io::Write>(out: &mut W) -> std::io::Result<()> {
    out.write_all(b"\x1b[?25l")
}

/// Show the cursor.
pub fn show_cursor<W: std::io::Write>(out: &mut W) -> std::io::Result<()> {
    out.write_all(b"\x1b[?25h")
}

/// Erase `kind`'s region.
pub fn clear_region<W: std::io::Write>(out: &mut W, kind: ClearKind) -> std::io::Result<()> {
    let sequence = match kind {
        ClearKind::All => "\x1b[2J",
        ClearKind::AfterCursor => "\x1b[0J",
        ClearKind::BeforeCursor => "\x1b[1J",
        ClearKind::CurrentLine => "\x1b[2K",
        ClearKind::UntilNewLine => "\x1b[0K",
    };
    out.write_all(sequence.as_bytes())
}

fn push_cursor(text: &mut String, at: Position) {
    let _ = write!(
        text,
        "\x1b[{};{}H",
        u32::from(at.y) + 1,
        u32::from(at.x) + 1
    );
}

fn write_color(text: &mut String, set: u8, reset: u8, color: Color) {
    match color {
        Color::Reset => {
            let _ = write!(text, "\x1b[{reset}m");
        }
        Color::Indexed(index) => {
            let _ = write!(text, "\x1b[{set};5;{index}m");
        }
        Color::Rgb(r, g, b) => {
            let _ = write!(text, "\x1b[{set};2;{r};{g};{b}m");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ClearKind, clear_region, hide_cursor, move_cursor, render_cell, show_cursor};
    use katui_scene::cell::{Attrs, Cell, Color, Position};

    #[test]
    fn a_styled_cell_writes_golden_bytes() {
        let cell = Cell {
            symbol: "A".to_owned(),
            fg: Color::Rgb(1, 2, 3),
            bg: Color::Indexed(4),
            attrs: Attrs::BOLD.union(Attrs::UNDERLINE),
        };
        let mut out = Vec::new();
        render_cell(&mut out, Position { x: 2, y: 1 }, &cell)
            .expect("writing to a Vec cannot fail");
        assert_eq!(
            String::from_utf8_lossy(&out),
            "\x1b[2;3H\x1b[0m\x1b[38;2;1;2;3m\x1b[48;5;4m\x1b[1m\x1b[4mA"
        );
    }

    #[test]
    fn a_plain_cell_resets_colors() {
        let mut out = Vec::new();
        render_cell(&mut out, Position { x: 0, y: 0 }, &Cell::default())
            .expect("writing to a Vec cannot fail");
        assert_eq!(
            String::from_utf8_lossy(&out),
            "\x1b[1;1H\x1b[0m\x1b[39m\x1b[49m "
        );
    }

    #[test]
    fn cursor_addressing_survives_the_u16_extremes() {
        let mut out = Vec::new();
        move_cursor(
            &mut out,
            Position {
                x: u16::MAX,
                y: u16::MAX,
            },
        )
        .expect("writing to a Vec cannot fail");
        assert_eq!(String::from_utf8_lossy(&out), "\x1b[65536;65536H");
    }

    #[test]
    fn cursor_visibility_and_clears_have_one_sequence_each() {
        let mut out = Vec::new();
        hide_cursor(&mut out).expect("writing to a Vec cannot fail");
        show_cursor(&mut out).expect("writing to a Vec cannot fail");
        for (kind, sequence) in [
            (ClearKind::All, "\x1b[2J"),
            (ClearKind::AfterCursor, "\x1b[0J"),
            (ClearKind::BeforeCursor, "\x1b[1J"),
            (ClearKind::CurrentLine, "\x1b[2K"),
            (ClearKind::UntilNewLine, "\x1b[0K"),
        ] {
            clear_region(&mut out, kind).expect("writing to a Vec cannot fail");
            assert!(String::from_utf8_lossy(&out).ends_with(sequence));
        }
        assert_eq!(
            String::from_utf8_lossy(&out),
            "\x1b[?25l\x1b[?25h\x1b[2J\x1b[0J\x1b[1J\x1b[2K\x1b[0K"
        );
    }
}
