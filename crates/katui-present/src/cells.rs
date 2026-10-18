//! The cell presenter: one changed cell as SGR sequences. Every cell
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
    (Attrs::REVERSE, 7),
    (Attrs::HIDDEN, 8),
    (Attrs::CROSSED, 9),
];

/// Write `cell` at `at`: cursor addressing, reset, colors, attributes,
/// symbol.
pub fn render_cell<W: std::io::Write>(
    out: &mut W,
    at: Position,
    cell: &Cell,
) -> std::io::Result<()> {
    let mut text = String::new();
    let _ = write!(text, "\x1b[{};{}H\x1b[0m", at.y + 1, at.x + 1);
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
    use super::render_cell;
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
}
