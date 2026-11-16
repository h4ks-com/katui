//! The diffed cell-buffer strategy: write a whole cell buffer once,
//! then only the cells that changed, frame over frame.

use crate::Sink;
use std::io::{self, Write as _};

use katui_scene::cell::{Buffer, Position, all_cells, diff};

/// The stateful cell-buffer presenter: writes a buffer's cells once,
/// then per frame only the cells that changed, inside synchronized
/// output. Indexed and truecolor cell colors travel through verbatim,
/// which makes this the presenter cell-plane demos drive directly and
/// the engine behind the blocks path. A fresh or resized buffer clears
/// and repaints everything; an unchanged frame writes nothing.
#[derive(Debug, Default)]
pub struct BufferSurface {
    prev: Option<Buffer>,
}

impl BufferSurface {
    /// A fresh presenter with no previous frame.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Write one frame of `next` and return the bytes written. A fresh
    /// or resized frame clears the screen and repaints every cell; a
    /// later frame repaints only the changed cells, inside synchronized
    /// output, with no clear that would erase the rest.
    pub fn render_frame<W: Sink>(&mut self, out: &mut W, next: Buffer) -> io::Result<usize> {
        self.write_frame(out, next, Position { x: 0, y: 0 }, true)
    }

    /// Write one frame of `next` with its top left cell at `origin`, for a
    /// buffer that covers part of the screen. A fresh frame repaints every
    /// cell of `next` and leaves the rest of the screen alone.
    pub fn render_frame_at<W: Sink>(
        &mut self,
        out: &mut W,
        next: Buffer,
        origin: Position,
    ) -> io::Result<usize> {
        self.write_frame(out, next, origin, false)
    }

    fn write_frame<W: Sink>(
        &mut self,
        out: &mut W,
        next: Buffer,
        origin: Position,
        clears: bool,
    ) -> io::Result<usize> {
        let full = self
            .prev
            .as_ref()
            .is_none_or(|prev| prev.size() != next.size());
        let changed = match self.prev.as_ref() {
            Some(prev) if !full => diff(prev, &next),
            _ => all_cells(&next),
        };
        let mut bytes = Vec::new();
        if !changed.is_empty() {
            bytes.write_all(b"\x1b[?2026h")?;
            if full && clears {
                bytes.write_all(b"\x1b[H\x1b[2J")?;
            }
            for (position, cell) in changed {
                let at = Position {
                    x: position.x.saturating_add(origin.x),
                    y: position.y.saturating_add(origin.y),
                };
                crate::cells::render_cell(&mut bytes, at, cell)?;
            }
            bytes.write_all(b"\x1b[0m\x1b[?2026l")?;
        }
        self.prev = Some(next);
        out.write_all(&bytes)?;
        Ok(bytes.len())
    }
}

#[cfg(test)]
mod tests {
    use super::BufferSurface;
    use katui_scene::CellSize;
    use katui_scene::cell::{Buffer, Cell, Color, Position};

    fn grid(columns: u16, rows: u16, alive: &[(u16, u16)]) -> Buffer {
        let mut buffer = Buffer::blank(CellSize { columns, rows });
        for &(x, y) in alive {
            buffer.set(
                Position { x, y },
                Cell {
                    symbol: "█".to_owned(),
                    fg: Color::Indexed(9),
                    bg: Color::Reset,
                    attrs: katui_scene::cell::Attrs::EMPTY,
                },
            );
        }
        buffer
    }

    #[test]
    fn an_identical_frame_writes_nothing() {
        let mut presenter = BufferSurface::new();
        let mut first = Vec::new();
        presenter
            .render_frame(&mut first, grid(8, 4, &[(1, 1), (2, 2)]))
            .expect("writing to a Vec cannot fail");
        let mut second = Vec::new();
        let written = presenter
            .render_frame(&mut second, grid(8, 4, &[(1, 1), (2, 2)]))
            .expect("writing to a Vec cannot fail");
        assert_eq!(written, 0, "no cell change means no bytes");
        assert!(second.is_empty());
    }

    #[test]
    fn a_fresh_frame_clears_and_a_later_change_does_not() {
        let mut presenter = BufferSurface::new();
        let mut first = Vec::new();
        presenter
            .render_frame(&mut first, grid(8, 4, &[(1, 1)]))
            .expect("writing to a Vec cannot fail");
        assert!(
            String::from_utf8_lossy(&first).contains("\x1b[H\x1b[2J"),
            "the first frame clears"
        );
        let mut second = Vec::new();
        presenter
            .render_frame(&mut second, grid(8, 4, &[(2, 2)]))
            .expect("writing to a Vec cannot fail");
        let text = String::from_utf8_lossy(&second);
        assert!(!text.contains("\x1b[2J"), "a diffed frame never erases");
        assert!(
            text.contains("\x1b[38;5;9m"),
            "indexed colors emit the 256-color SGR"
        );
    }

    #[test]
    fn a_resize_repaints_everything() {
        let mut presenter = BufferSurface::new();
        let mut first = Vec::new();
        presenter
            .render_frame(&mut first, grid(8, 4, &[(1, 1)]))
            .expect("writing to a Vec cannot fail");
        let mut second = Vec::new();
        presenter
            .render_frame(&mut second, grid(6, 4, &[(1, 1)]))
            .expect("writing to a Vec cannot fail");
        assert!(
            String::from_utf8_lossy(&second).contains("\x1b[2J"),
            "a resize clears and repaints whole"
        );
    }
}
