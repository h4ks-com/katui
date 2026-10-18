//! The ratatui compatibility backend: ratatui's cell changes flow into a
//! katui scene buffer and out through the katui-present cell presenter,
//! so a ratatui app runs on katui by swapping its backend generic. Pixel
//! planes and image placements join on this same seam. The cursor is
//! tracked, never queried: reading the terminal's answer belongs to the
//! runtime's transport.

use std::io;

use katui_present::cells::render_cell;
use katui_scene::CellSize;
use katui_scene::cell::{Attrs, Buffer, Cell as SceneCell, Color, Position};
use ratatui::backend::{Backend, ClearType, WindowSize};
use ratatui::buffer::Cell as RatCell;
use ratatui::layout::{Position as RatPosition, Size};
use ratatui::style::{Color as RatColor, Modifier};

/// The katui backend for ratatui, writing bytes to any [`io::Write`]:
/// `Vec<u8>` in tests, stdout in applications.
pub struct KatuiBackend<W: io::Write> {
    out: W,
    screen: Buffer,
    pixels: Size,
    cursor: RatPosition,
}

impl<W: io::Write> KatuiBackend<W> {
    /// A backend for a `columns` x `rows` terminal, writing to `out`.
    pub fn new(columns: u16, rows: u16, out: W) -> Self {
        Self {
            out,
            screen: Buffer::blank(CellSize { columns, rows }),
            pixels: Size::ZERO,
            cursor: RatPosition::ORIGIN,
        }
    }

    /// Set the pixel size a probe reported; `Size::ZERO` until then.
    #[must_use]
    pub fn with_pixel_size(mut self, pixels: Size) -> Self {
        self.pixels = pixels;
        self
    }

    /// The scene mirror of everything drawn so far.
    pub fn screen(&self) -> &Buffer {
        &self.screen
    }

    /// The writer back.
    pub fn into_inner(self) -> W {
        self.out
    }

    fn write_str(&mut self, text: &str) -> io::Result<()> {
        self.out.write_all(text.as_bytes())
    }

    fn blank_cells(&mut self, from: Position, to: Position) {
        for y in from.y..=to.y {
            for x in from.x..=to.x {
                self.screen.set(Position { x, y }, SceneCell::default());
            }
        }
    }
}

fn map_color(color: RatColor) -> Color {
    match color {
        RatColor::Reset => Color::Reset,
        RatColor::Black => Color::Indexed(0),
        RatColor::Red => Color::Indexed(1),
        RatColor::Green => Color::Indexed(2),
        RatColor::Yellow => Color::Indexed(3),
        RatColor::Blue => Color::Indexed(4),
        RatColor::Magenta => Color::Indexed(5),
        RatColor::Cyan => Color::Indexed(6),
        RatColor::Gray => Color::Indexed(7),
        RatColor::DarkGray => Color::Indexed(8),
        RatColor::LightRed => Color::Indexed(9),
        RatColor::LightGreen => Color::Indexed(10),
        RatColor::LightYellow => Color::Indexed(11),
        RatColor::LightBlue => Color::Indexed(12),
        RatColor::LightMagenta => Color::Indexed(13),
        RatColor::LightCyan => Color::Indexed(14),
        RatColor::White => Color::Indexed(15),
        RatColor::Indexed(index) => Color::Indexed(index),
        RatColor::Rgb(r, g, b) => Color::Rgb(r, g, b),
    }
}

fn map_modifier(modifier: Modifier) -> Attrs {
    let mut attrs = Attrs::EMPTY;
    let pairs = [
        (Modifier::BOLD, Attrs::BOLD),
        (Modifier::DIM, Attrs::DIM),
        (Modifier::ITALIC, Attrs::ITALIC),
        (Modifier::UNDERLINED, Attrs::UNDERLINE),
        (Modifier::SLOW_BLINK, Attrs::BLINK),
        (Modifier::RAPID_BLINK, Attrs::BLINK),
        (Modifier::REVERSED, Attrs::REVERSE),
        (Modifier::HIDDEN, Attrs::HIDDEN),
        (Modifier::CROSSED_OUT, Attrs::CROSSED),
    ];
    for (modifier_bit, attr) in pairs {
        if modifier.contains(modifier_bit) {
            attrs = attrs.union(attr);
        }
    }
    attrs
}

/// One ratatui cell as a scene cell. The underline color has no scene
/// counterpart yet and is dropped.
fn convert(cell: &RatCell) -> SceneCell {
    SceneCell {
        symbol: cell.symbol().to_owned(),
        fg: map_color(cell.fg),
        bg: map_color(cell.bg),
        attrs: map_modifier(cell.modifier),
    }
}

impl<W: io::Write> Backend for KatuiBackend<W> {
    type Error = io::Error;

    fn draw<'a, I>(&mut self, content: I) -> io::Result<()>
    where
        I: Iterator<Item = (u16, u16, &'a RatCell)>,
    {
        for (x, y, cell) in content {
            let at = Position { x, y };
            let converted = convert(cell);
            self.screen.set(at, converted.clone());
            render_cell(&mut self.out, at, &converted)?;
        }
        Ok(())
    }

    fn hide_cursor(&mut self) -> io::Result<()> {
        self.write_str("\x1b[?25l")
    }

    fn show_cursor(&mut self) -> io::Result<()> {
        self.write_str("\x1b[?25h")
    }

    fn get_cursor_position(&mut self) -> io::Result<RatPosition> {
        Ok(self.cursor)
    }

    fn set_cursor_position<P: Into<RatPosition>>(&mut self, position: P) -> io::Result<()> {
        let position = position.into();
        self.cursor = position;
        self.write_str(&format!("\x1b[{};{}H", position.y + 1, position.x + 1))
    }

    fn clear(&mut self) -> io::Result<()> {
        let size = self.screen.size();
        self.screen = Buffer::blank(size);
        self.write_str("\x1b[2J")
    }

    fn clear_region(&mut self, clear_type: ClearType) -> io::Result<()> {
        let origin = Position { x: 0, y: 0 };
        let end = Position {
            x: self.screen.size().columns.saturating_sub(1),
            y: self.screen.size().rows.saturating_sub(1),
        };
        let line_end = Position {
            x: end.x,
            y: self.cursor.y,
        };
        let cursor = Position {
            x: self.cursor.x,
            y: self.cursor.y,
        };
        match clear_type {
            ClearType::All => self.clear(),
            ClearType::AfterCursor => {
                self.blank_cells(cursor, end);
                self.write_str("\x1b[0J")
            }
            ClearType::BeforeCursor => {
                self.blank_cells(origin, cursor);
                self.write_str("\x1b[1J")
            }
            ClearType::CurrentLine => {
                self.blank_cells(
                    Position { x: 0, y: cursor.y },
                    Position {
                        x: end.x,
                        y: cursor.y,
                    },
                );
                self.write_str("\x1b[2K")
            }
            ClearType::UntilNewLine => {
                self.blank_cells(cursor, line_end);
                self.write_str("\x1b[0K")
            }
        }
    }

    fn size(&self) -> io::Result<Size> {
        let size = self.screen.size();
        Ok(Size::new(size.columns, size.rows))
    }

    fn window_size(&mut self) -> io::Result<WindowSize> {
        Ok(WindowSize {
            columns_rows: self.size()?,
            pixels: self.pixels,
        })
    }

    fn flush(&mut self) -> io::Result<()> {
        self.out.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::KatuiBackend;
    use katui_scene::cell::{Attrs as SceneAttrs, Color as SceneColor, Position as ScenePosition};
    use ratatui::Terminal;
    use ratatui::backend::{Backend as _, TestBackend};
    use ratatui::buffer::Cell as RatCell;
    use ratatui::layout::{Position as RatPosition, Size};
    use ratatui::style::{Color, Modifier, Style};
    use ratatui::text::{Line, Span};
    use ratatui::widgets::{Block, Paragraph, Widget as _};

    fn render<B: ratatui::backend::Backend>(terminal: &mut Terminal<B>) {
        let drawn = terminal.draw(|frame| {
            Paragraph::new(vec![
                Line::from(Span::styled("plain text", Style::new())),
                Line::from(vec![
                    Span::styled(
                        "bold red ",
                        Style::new().fg(Color::Red).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        "truecolor underline",
                        Style::new()
                            .bg(Color::Rgb(10, 20, 30))
                            .add_modifier(Modifier::UNDERLINED),
                    ),
                ]),
                Line::from(Span::styled(
                    "italic gray",
                    Style::new().fg(Color::Gray).add_modifier(Modifier::ITALIC),
                )),
            ])
            .block(Block::bordered())
            .render(frame.area(), frame.buffer_mut());
        });
        if let Err(error) = drawn {
            panic!("drawing failed: {error}");
        }
    }

    #[test]
    fn cell_parity_with_the_ratatui_testbackend() {
        let (columns, rows) = (24u16, 6u16);
        let mut expected_terminal =
            Terminal::new(TestBackend::new(columns, rows)).expect("terminal constructs");
        render(&mut expected_terminal);
        let mut ours = Terminal::new(KatuiBackend::new(columns, rows, Vec::new()))
            .expect("terminal constructs");
        render(&mut ours);

        let expected = expected_terminal.backend().buffer();
        let screen = ours.backend().screen();
        for y in 0..rows {
            for x in 0..columns {
                let at = RatPosition { x, y };
                let expected_cell = expected
                    .cell(at)
                    .expect("every compared position is inside the buffer");
                let our_cell = screen
                    .get(ScenePosition { x, y })
                    .expect("every position is inside the mirror");
                assert_eq!(our_cell.symbol, expected_cell.symbol(), "{x},{y} symbol");
                assert_eq!(
                    our_cell.fg,
                    super::map_color(expected_cell.fg),
                    "{x},{y} fg"
                );
                assert_eq!(
                    our_cell.bg,
                    super::map_color(expected_cell.bg),
                    "{x},{y} bg"
                );
                assert_eq!(
                    our_cell.attrs,
                    super::map_modifier(expected_cell.modifier),
                    "{x},{y} attrs"
                );
            }
        }
    }

    #[test]
    fn draw_writes_sgr_bytes_and_updates_the_mirror() {
        let mut backend = KatuiBackend::new(4, 2, Vec::new());
        let mut cell = RatCell::default();
        cell.set_symbol("X");
        backend
            .draw(vec![(1, 0, &cell)].into_iter())
            .expect("writing to a Vec cannot fail");
        let out = backend.screen();
        let mirror = out
            .get(ScenePosition { x: 1, y: 0 })
            .expect("the drawn cell is in the mirror");
        assert_eq!(mirror.symbol, "X");
        assert!(matches!(mirror.fg, SceneColor::Reset));
    }

    #[test]
    fn the_cursor_is_tracked_and_emitted_one_based() {
        let mut backend = KatuiBackend::new(10, 10, Vec::new());
        backend.set_cursor_position((3, 2)).expect("vec write");
        assert_eq!(
            backend.get_cursor_position().expect("tracked position"),
            RatPosition { x: 3, y: 2 }
        );
        let bytes = backend.into_inner();
        assert_eq!(
            String::from_utf8_lossy(&bytes),
            "\x1b[3;4H",
            "row then column, both one-based"
        );
    }

    #[test]
    fn clear_blankens_the_mirror_and_writes_the_sequence() {
        let mut backend = KatuiBackend::new(4, 2, Vec::new());
        let mut cell = RatCell::default();
        cell.set_symbol("X");
        backend
            .draw(vec![(0, 0, &cell), (3, 1, &cell)].into_iter())
            .expect("writing to a Vec cannot fail");
        backend.clear().expect("writing to a Vec cannot fail");
        for y in 0..2u16 {
            for x in 0..4u16 {
                assert_eq!(
                    backend
                        .screen()
                        .get(ScenePosition { x, y })
                        .expect("inside the mirror"),
                    &katui_scene::cell::Cell::default()
                );
            }
        }
    }

    #[test]
    fn window_size_carries_the_probed_pixels() {
        let mut backend =
            KatuiBackend::new(80, 24, Vec::new()).with_pixel_size(Size::new(700, 400));
        let window = backend.window_size().expect("reading fields cannot fail");
        assert_eq!(window.columns_rows, Size::new(80, 24));
        assert_eq!(window.pixels, Size::new(700, 400));
    }

    #[test]
    fn attrs_map_both_directions_of_the_shared_bits() {
        assert_eq!(
            super::map_modifier(Modifier::BOLD | Modifier::ITALIC),
            SceneAttrs::BOLD.union(SceneAttrs::ITALIC)
        );
        assert_eq!(super::map_modifier(Modifier::empty()), SceneAttrs::EMPTY);
    }
}
