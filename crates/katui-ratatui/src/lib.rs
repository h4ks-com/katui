//! A ratatui backend that draws through katui's cell presenter and places
//! kitty images beside the cells.

pub mod image;
#[cfg(all(unix, feature = "run"))]
pub mod run;

use std::io;

use katui_present::cells::{
    ClearKind, clear_region, hide_cursor, move_cursor, render_cell, show_cursor,
};
use katui_present::kitty::{Images, Kitty};
use katui_scene::CellSize;
use katui_scene::cell::{Attrs, Buffer, Cell as SceneCell, Color, Position};
use katui_scene::placement::{self, Placement, PlacementId};

pub use katui_present::kitty::RgbaImage;
pub use katui_scene::placement::{CellRect, ImageId};
use ratatui::backend::{Backend, ClearType, WindowSize};
use ratatui::buffer::{Buffer as RatBuffer, Cell as RatCell, CellDiffOption};
use ratatui::layout::{Position as RatPosition, Rect, Size};
use ratatui::style::{Color as RatColor, Modifier};
use ratatui::widgets::Widget;

/// One image a frame shows: its scene id, its pixels and the cells it
/// covers.
#[derive(Debug, Clone, Copy)]
pub struct ImageFrame<'a> {
    pub image: ImageId,
    pub pixels: RgbaImage<'a>,
    pub area: CellRect,
}

/// A widget that hands its area to an image: ratatui skips those cells, so
/// the text layer never draws over the picture.
#[derive(Debug, Clone, Copy, Default)]
pub struct ImageSlot;

impl Widget for ImageSlot {
    fn render(self, area: Rect, buf: &mut RatBuffer) {
        for position in area.positions() {
            if let Some(cell) = buf.cell_mut(position) {
                cell.set_diff_option(CellDiffOption::Skip);
            }
        }
    }
}

/// The katui backend for ratatui, writing bytes to any [`io::Write`]:
/// `Vec<u8>` in tests, stdout in applications.
pub struct KatuiBackend<W: io::Write> {
    out: W,
    screen: Buffer,
    pixels: Size,
    cursor: RatPosition,
    kitty: Kitty,
    placements: Vec<Placement>,
}

impl<W: io::Write> KatuiBackend<W> {
    /// A backend for a `columns` x `rows` terminal, writing to `out`.
    #[must_use]
    pub fn new(columns: u16, rows: u16, out: W) -> Self {
        Self {
            out,
            screen: Buffer::blank(CellSize { columns, rows }),
            pixels: Size::ZERO,
            cursor: RatPosition::ORIGIN,
            kitty: Kitty::new(),
            placements: Vec::new(),
        }
    }

    /// Set the pixel size a probe reported; `Size::ZERO` until then.
    #[must_use]
    pub fn with_pixel_size(mut self, pixels: Size) -> Self {
        self.pixels = pixels;
        self
    }

    /// The scene mirror of every cell drawn so far.
    pub fn screen(&self) -> &Buffer {
        &self.screen
    }

    /// Consume the backend and return its writer.
    pub fn into_inner(self) -> W {
        self.out
    }

    /// The writer, for a caller that collects the bytes as it goes.
    pub fn writer_mut(&mut self) -> &mut W {
        &mut self.out
    }

    /// Draw one RGBA image at `area`, replacing every image drawn
    /// before; [`Self::draw_images`] with one entry.
    pub fn draw_image(
        &mut self,
        image: ImageId,
        pixels: &[u8],
        width: u32,
        height: u32,
        area: CellRect,
    ) -> io::Result<usize> {
        let Some(pixels) = RgbaImage::new(width, height, pixels) else {
            // a mismatched payload transmits nothing rather than a
            // corrupt frame; the caller learns it by the zero bytes
            return Ok(0);
        };
        self.draw_images(&[ImageFrame {
            image,
            pixels,
            area,
        }])
    }

    /// Show exactly `frames` this frame: the placement bookkeeping diffs
    /// against the previous frame, so an unchanged image costs nothing, a
    /// move re-places without re-transmitting, a new image id transmits
    /// once and an image left out is removed. Returns the number of bytes
    /// written. Render an [`ImageSlot`] over each area so
    /// ratatui leaves those cells to the image.
    pub fn draw_images(&mut self, frames: &[ImageFrame<'_>]) -> io::Result<usize> {
        let mut images = Images::default();
        let mut next = Vec::with_capacity(frames.len());
        for (index, frame) in frames.iter().enumerate() {
            images.insert(frame.image, frame.pixels);
            next.push(Placement {
                image: frame.image,
                id: PlacementId(u32::try_from(index + 1).unwrap_or(u32::MAX)),
                area: frame.area,
                crop: None,
                z: 0,
            });
        }
        let delta = placement::diff(&self.placements, &next);
        let written = self.kitty.render(&mut self.out, &delta, &images)?;
        self.placements = next;
        Ok(written)
    }

    fn blank_cells(&mut self, from: Position, to: Position) {
        for y in from.y..=to.y {
            for x in from.x..=to.x {
                self.screen.set(Position { x, y }, SceneCell::default());
            }
        }
    }
}

/// The 16 ANSI colors in palette order, so a color's position is its index.
const ANSI: [RatColor; 16] = [
    RatColor::Black,
    RatColor::Red,
    RatColor::Green,
    RatColor::Yellow,
    RatColor::Blue,
    RatColor::Magenta,
    RatColor::Cyan,
    RatColor::Gray,
    RatColor::DarkGray,
    RatColor::LightRed,
    RatColor::LightGreen,
    RatColor::LightYellow,
    RatColor::LightBlue,
    RatColor::LightMagenta,
    RatColor::LightCyan,
    RatColor::White,
];

fn map_color(color: RatColor) -> Color {
    match color {
        RatColor::Reset => Color::Reset,
        RatColor::Indexed(index) => Color::Indexed(index),
        RatColor::Rgb(r, g, b) => Color::Rgb(r, g, b),
        named => ANSI
            .iter()
            .position(|ansi| *ansi == named)
            .and_then(|index| u8::try_from(index).ok())
            .map_or(Color::Reset, Color::Indexed),
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
        (Modifier::RAPID_BLINK, Attrs::RAPID_BLINK),
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
            // every rendered cell addresses the cursor absolutely, so the
            // terminal parks on the last drawn cell and the mirror follows
            self.cursor = RatPosition { x, y };
            let converted = convert(cell);
            self.screen.set(at, converted.clone());
            render_cell(&mut self.out, at, &converted)?;
        }
        Ok(())
    }

    fn hide_cursor(&mut self) -> io::Result<()> {
        hide_cursor(&mut self.out)
    }

    fn show_cursor(&mut self) -> io::Result<()> {
        show_cursor(&mut self.out)
    }

    fn get_cursor_position(&mut self) -> io::Result<RatPosition> {
        Ok(self.cursor)
    }

    fn set_cursor_position<P: Into<RatPosition>>(&mut self, position: P) -> io::Result<()> {
        let position = position.into();
        self.cursor = position;
        move_cursor(
            &mut self.out,
            Position {
                x: position.x,
                y: position.y,
            },
        )
    }

    fn clear(&mut self) -> io::Result<()> {
        let size = self.screen.size();
        self.screen = Buffer::blank(size);
        clear_region(&mut self.out, ClearKind::All)
    }

    fn clear_region(&mut self, clear_type: ClearType) -> io::Result<()> {
        let end = Position {
            x: self.screen.size().columns.saturating_sub(1),
            y: self.screen.size().rows.saturating_sub(1),
        };
        let cursor = Position {
            x: self.cursor.x,
            y: self.cursor.y,
        };
        // the escapes erase reading-order ranges, not rectangles, so
        // each arm blanks exactly the cells its escape erases
        match clear_type {
            ClearType::All => self.clear(),
            ClearType::AfterCursor => {
                self.blank_cells(
                    cursor,
                    Position {
                        x: end.x,
                        y: cursor.y,
                    },
                );
                if cursor.y < end.y {
                    self.blank_cells(
                        Position {
                            x: 0,
                            y: cursor.y + 1,
                        },
                        end,
                    );
                }
                clear_region(&mut self.out, ClearKind::AfterCursor)
            }
            ClearType::BeforeCursor => {
                if cursor.y > 0 {
                    self.blank_cells(
                        Position { x: 0, y: 0 },
                        Position {
                            x: end.x,
                            y: cursor.y - 1,
                        },
                    );
                }
                self.blank_cells(Position { x: 0, y: cursor.y }, cursor);
                clear_region(&mut self.out, ClearKind::BeforeCursor)
            }
            ClearType::CurrentLine => {
                self.blank_cells(
                    Position { x: 0, y: cursor.y },
                    Position {
                        x: end.x,
                        y: cursor.y,
                    },
                );
                clear_region(&mut self.out, ClearKind::CurrentLine)
            }
            ClearType::UntilNewLine => {
                self.blank_cells(
                    cursor,
                    Position {
                        x: end.x,
                        y: cursor.y,
                    },
                );
                clear_region(&mut self.out, ClearKind::UntilNewLine)
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
    use super::{ImageFrame, ImageSlot, KatuiBackend};
    use katui_present::kitty::RgbaImage;
    use katui_scene::cell::{Attrs as SceneAttrs, Color as SceneColor, Position as ScenePosition};
    use katui_scene::placement::{CellRect, ImageId};
    use ratatui::Terminal;
    use ratatui::backend::{Backend as _, ClearType, TestBackend};
    use ratatui::buffer::Cell as RatCell;
    use ratatui::layout::{Position as RatPosition, Rect, Size};
    use ratatui::style::{Color, Modifier, Style};
    use ratatui::text::{Line, Span};
    use ratatui::widgets::{Block, Paragraph, Widget as _};

    fn render<B: ratatui::backend::Backend>(terminal: &mut Terminal<B>, headline: &str) {
        let drawn = terminal.draw(|frame| {
            Paragraph::new(vec![
                Line::from(Span::styled(headline, Style::new())),
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

    fn compare_every_cell(
        expected_terminal: &Terminal<TestBackend>,
        ours: &Terminal<KatuiBackend<Vec<u8>>>,
        columns: u16,
        rows: u16,
    ) {
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
    fn cell_parity_with_the_ratatui_testbackend_over_two_frames() {
        let (columns, rows) = (24u16, 6u16);
        let mut expected_terminal =
            Terminal::new(TestBackend::new(columns, rows)).expect("terminal constructs");
        let mut ours = Terminal::new(KatuiBackend::new(columns, rows, Vec::new()))
            .expect("terminal constructs");
        render(&mut expected_terminal, "plain text");
        render(&mut ours, "plain text");
        compare_every_cell(&expected_terminal, &ours, columns, rows);
        render(&mut expected_terminal, "changed text");
        render(&mut ours, "changed text");
        compare_every_cell(&expected_terminal, &ours, columns, rows);
    }

    #[test]
    fn the_named_color_and_modifier_tables_are_pinned() {
        let table = [
            (Color::Black, 0u8),
            (Color::Red, 1),
            (Color::Green, 2),
            (Color::Yellow, 3),
            (Color::Blue, 4),
            (Color::Magenta, 5),
            (Color::Cyan, 6),
            (Color::Gray, 7),
            (Color::DarkGray, 8),
            (Color::LightRed, 9),
            (Color::LightGreen, 10),
            (Color::LightYellow, 11),
            (Color::LightBlue, 12),
            (Color::LightMagenta, 13),
            (Color::LightCyan, 14),
            (Color::White, 15),
        ];
        for (color, index) in table {
            assert_eq!(
                super::map_color(color),
                SceneColor::Indexed(index),
                "{color:?}"
            );
        }
        assert_eq!(super::map_color(Color::Reset), SceneColor::Reset);
        assert_eq!(
            super::map_modifier(Modifier::UNDERLINED),
            SceneAttrs::UNDERLINE
        );
        assert_eq!(
            super::map_modifier(Modifier::RAPID_BLINK),
            SceneAttrs::RAPID_BLINK
        );
        assert_eq!(
            super::map_modifier(Modifier::CROSSED_OUT),
            SceneAttrs::CROSSED
        );
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
        assert_eq!(
            backend.get_cursor_position().expect("tracked position"),
            RatPosition { x: 1, y: 0 },
            "the cursor parks on the last drawn cell, like the terminal"
        );
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

    /// A 4x3 mirror full of X with the cursor at (1, 1), so each clear
    /// type can be checked against the reading-order range its escape
    /// erases.
    fn filled() -> KatuiBackend<Vec<u8>> {
        let mut backend = KatuiBackend::new(4, 3, Vec::new());
        let mut cell = RatCell::default();
        cell.set_symbol("X");
        backend
            .draw(
                (0..3u16)
                    .flat_map(|y| (0..4u16).map(move |x| (x, y)))
                    .map(|(x, y)| (x, y, &cell))
                    .collect::<Vec<_>>()
                    .into_iter(),
            )
            .expect("writing to a Vec cannot fail");
        backend
            .set_cursor_position(RatPosition { x: 1, y: 1 })
            .expect("writing to a Vec cannot fail");
        backend
    }

    fn marked(backend: &KatuiBackend<Vec<u8>>, x: u16, y: u16) -> bool {
        backend
            .screen()
            .get(ScenePosition { x, y })
            .expect("inside the mirror")
            .symbol
            == "X"
    }

    #[test]
    fn after_cursor_erases_to_the_screen_end_in_reading_order() {
        let mut backend = filled();
        backend
            .clear_region(ClearType::AfterCursor)
            .expect("writing to a Vec cannot fail");
        for x in 0..4u16 {
            assert!(marked(&backend, x, 0), "row above stays");
            assert!(!marked(&backend, x, 2), "rows below erase whole");
        }
        assert!(marked(&backend, 0, 1), "the cell left of the cursor stays");
        assert!(
            !marked(&backend, 1, 1) && !marked(&backend, 2, 1) && !marked(&backend, 3, 1),
            "the cursor row erases from the cursor right"
        );
    }

    #[test]
    fn before_cursor_erases_to_the_cursor_in_reading_order() {
        let mut backend = filled();
        backend
            .clear_region(ClearType::BeforeCursor)
            .expect("writing to a Vec cannot fail");
        for x in 0..4u16 {
            assert!(!marked(&backend, x, 0), "rows above erase whole");
            assert!(marked(&backend, x, 2), "rows below stay");
        }
        assert!(
            !marked(&backend, 0, 1) && !marked(&backend, 1, 1),
            "the cursor row erases through the cursor"
        );
        assert!(marked(&backend, 2, 1) && marked(&backend, 3, 1));
    }

    #[test]
    fn current_line_erases_only_the_cursor_row() {
        let mut backend = filled();
        backend
            .clear_region(ClearType::CurrentLine)
            .expect("writing to a Vec cannot fail");
        for x in 0..4u16 {
            assert!(marked(&backend, x, 0) && marked(&backend, x, 2));
            assert!(!marked(&backend, x, 1), "only the cursor row erases");
        }
    }

    #[test]
    fn until_new_line_erases_to_the_row_end() {
        let mut backend = filled();
        backend
            .clear_region(ClearType::UntilNewLine)
            .expect("writing to a Vec cannot fail");
        for x in 0..4u16 {
            assert!(marked(&backend, x, 0) && marked(&backend, x, 2));
        }
        assert!(marked(&backend, 0, 1));
        assert!(!marked(&backend, 1, 1) && !marked(&backend, 2, 1) && !marked(&backend, 3, 1));
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
    fn draw_image_transmits_once_and_moves_without_payload() {
        let mut backend = KatuiBackend::new(20, 6, Vec::new());
        let area = CellRect {
            x: 2,
            y: 1,
            columns: 4,
            rows: 2,
        };
        let first = backend
            .draw_image(ImageId(7), &[0xff, 0, 0, 0xff], 1, 1, area)
            .expect("writing to a Vec cannot fail");
        assert!(first > 0, "the first draw transmits and places");

        let moved = CellRect {
            x: 8,
            y: 3,
            columns: 4,
            rows: 2,
        };
        let second = backend
            .draw_image(ImageId(7), &[0xff, 0, 0, 0xff], 1, 1, moved)
            .expect("writing to a Vec cannot fail");
        assert!(second < first, "a move writes less than a first draw");
        let unchanged = backend
            .draw_image(ImageId(7), &[0xff, 0, 0, 0xff], 1, 1, moved)
            .expect("writing to a Vec cannot fail");
        assert_eq!(unchanged, 0, "an unchanged image costs nothing");

        let bytes = backend.into_inner();
        let text = String::from_utf8_lossy(&bytes);
        assert_eq!(
            text.matches("a=t,").count(),
            1,
            "one payload on the wire across all three draws"
        );
        assert!(
            text.contains("\x1b[4;9H"),
            "the move re-places at the new origin"
        );
    }

    #[test]
    fn replacing_the_image_transmits_the_new_id_and_retires_the_old() {
        let area = CellRect {
            x: 0,
            y: 0,
            columns: 4,
            rows: 2,
        };
        let mut backend = KatuiBackend::new(20, 6, Vec::new());
        backend
            .draw_image(ImageId(7), &[0xff, 0, 0, 0xff], 1, 1, area)
            .expect("writing to a Vec cannot fail");
        backend
            .draw_image(ImageId(8), &[0, 0xff, 0, 0xff], 1, 1, area)
            .expect("writing to a Vec cannot fail");
        let bytes = backend.into_inner();
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.contains("i=8,"), "the new id transmits");
        assert!(text.contains("d=i,i=7"), "the old id retires");
    }

    #[test]
    fn an_image_slot_keeps_ratatui_off_its_cells_and_two_images_share_a_frame() {
        let mut terminal =
            Terminal::new(KatuiBackend::new(10, 4, Vec::new())).expect("terminal constructs");
        terminal
            .draw(|frame| {
                Paragraph::new("xxxxxxxxxx\nxxxxxxxxxx").render(frame.area(), frame.buffer_mut());
                ImageSlot.render(Rect::new(2, 0, 3, 2), frame.buffer_mut());
            })
            .expect("drawing to a Vec cannot fail");
        let screen = terminal.backend().screen();
        let symbol = |x, y| {
            screen
                .get(ScenePosition { x, y })
                .map(|cell| cell.symbol.clone())
        };
        assert_eq!(symbol(1, 0).as_deref(), Some("x"));
        assert_eq!(
            symbol(2, 0).as_deref(),
            Some(" "),
            "the slot stays untouched"
        );
        let pixel = [0xff, 0, 0, 0xff];
        let rgba = RgbaImage::new(1, 1, &pixel).expect("one pixel");
        let frames = [
            ImageFrame {
                image: ImageId(7),
                pixels: rgba,
                area: CellRect::full(2, 2),
            },
            ImageFrame {
                image: ImageId(8),
                pixels: rgba,
                area: CellRect {
                    x: 4,
                    y: 0,
                    columns: 2,
                    rows: 2,
                },
            },
        ];
        terminal
            .backend_mut()
            .draw_images(&frames)
            .expect("writing to a Vec cannot fail");
        let bytes = terminal
            .backend_mut()
            .draw_images(&frames[..1])
            .expect("writing to a Vec cannot fail");
        assert!(bytes > 0, "the dropped image is removed");
        let text = String::from_utf8_lossy(&terminal.backend().out);
        assert!(text.contains("i=7,") && text.contains("i=8,"));
        assert!(text.contains("d=i,i=8"), "the image left out retires");
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
