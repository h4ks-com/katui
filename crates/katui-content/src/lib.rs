//! Documents as scene cells. The markdown renderer covers the subset a
//! terminal actually shows: headings, paragraphs, lists, fenced code and
//! inline code; the HTML subset follows on this crate, same shape.

use katui_scene::CellSize;
use katui_scene::cell::{Attrs, Buffer, Color, Position};
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

/// Render markdown into a cell buffer of the given width, one row per
/// source line; callers that need wrapping pre-wrap their text.
#[must_use]
pub fn markdown(markdown: &str, width: u16) -> Buffer {
    let mut lines: Vec<Vec<Span>> = Vec::new();
    let parser = Parser::new_ext(markdown, Options::empty());
    let mut context = Context::default();
    for event in parser {
        match event {
            Event::Start(tag) => context.start(&tag),
            Event::End(tag) => context.end(tag, &mut lines),
            Event::Text(text) => context.push(&text),
            Event::Code(text) => context.push_styled(&text, code_color(), Attrs::EMPTY),
            Event::SoftBreak | Event::HardBreak => context.space(),
            _ => {}
        }
    }
    context.flush_paragraph(&mut lines);
    let rows = u16::try_from(lines.len()).unwrap_or(u16::MAX);
    let mut buffer = Buffer::blank(CellSize {
        columns: width,
        rows,
    });
    for (row, line) in lines.iter().enumerate() {
        let y = u16::try_from(row).unwrap_or(u16::MAX);
        let mut x = 0u16;
        for span in line {
            let written = u16::try_from(buffer.set_string(
                Position { x, y },
                &span.text,
                span.color,
                Color::Reset,
                span.attrs,
            ))
            .unwrap_or(0);
            x = x.saturating_add(written);
        }
    }
    buffer
}

/// One styled run of text on one line.
struct Span {
    text: String,
    color: Color,
    attrs: Attrs,
}

/// The renderer's open-document state.
#[derive(Default)]
struct Context {
    paragraph: Vec<Span>,
    in_heading: bool,
    in_code_block: bool,
}

impl Context {
    fn start(&mut self, tag: &Tag<'_>) {
        match tag {
            Tag::Heading { .. } => self.in_heading = true,
            Tag::CodeBlock(_) => self.in_code_block = true,
            _ => {}
        }
    }

    fn end(&mut self, tag: TagEnd, lines: &mut Vec<Vec<Span>>) {
        match tag {
            TagEnd::Heading(_) => {
                self.in_heading = false;
                let line = std::mem::take(&mut self.paragraph);
                if !line.is_empty() {
                    push_line(lines, line, heading_color(), Attrs::BOLD);
                }
            }
            TagEnd::CodeBlock => {
                self.in_code_block = false;
                self.flush_paragraph(lines);
            }
            TagEnd::Paragraph => self.flush_paragraph(lines),
            TagEnd::Item => {
                let mut bullet = vec![Span {
                    text: "• ".to_owned(),
                    color: Color::Reset,
                    attrs: Attrs::EMPTY,
                }];
                bullet.append(&mut self.paragraph);
                lines.push(bullet);
            }
            _ => {}
        }
    }

    fn push(&mut self, text: &str) {
        let (color, attrs) = if self.in_code_block {
            (code_color(), Attrs::EMPTY)
        } else if self.in_heading {
            (heading_color(), Attrs::BOLD)
        } else {
            (Color::Reset, Attrs::EMPTY)
        };
        self.paragraph.push(Span {
            text: text.to_owned(),
            color,
            attrs,
        });
    }

    fn push_styled(&mut self, text: &str, color: Color, attrs: Attrs) {
        self.paragraph.push(Span {
            text: text.to_owned(),
            color,
            attrs,
        });
    }

    fn space(&mut self) {
        self.paragraph.push(Span {
            text: " ".to_owned(),
            color: Color::Reset,
            attrs: Attrs::EMPTY,
        });
    }

    fn flush_paragraph(&mut self, lines: &mut Vec<Vec<Span>>) {
        let paragraph = std::mem::take(&mut self.paragraph);
        if !paragraph.is_empty() {
            lines.push(paragraph);
        }
    }
}

fn heading_color() -> Color {
    Color::Indexed(12)
}

fn code_color() -> Color {
    Color::Indexed(3)
}

fn push_line(lines: &mut Vec<Vec<Span>>, mut spans: Vec<Span>, color: Color, attrs: Attrs) {
    for span in &mut spans {
        if span.color == Color::Reset {
            span.color = color;
        }
        span.attrs = span.attrs.union(attrs);
    }
    lines.push(spans);
}

#[cfg(test)]
mod tests {
    use super::markdown;
    use katui_scene::cell::{Attrs, Color, Position};

    fn cell(buffer: &katui_scene::cell::Buffer, x: u16, y: u16) -> String {
        buffer
            .get(Position { x, y })
            .map(|cell| cell.symbol.clone())
            .unwrap_or_default()
    }

    #[test]
    fn a_heading_renders_bold_and_colored() {
        let buffer = markdown("# Title", 20);
        assert_eq!(cell(&buffer, 0, 0), "T");
        let rendered = buffer.get(Position { x: 0, y: 0 }).expect("set");
        assert_eq!(rendered.attrs, Attrs::BOLD);
        assert_eq!(rendered.fg, Color::Indexed(12));
    }

    #[test]
    fn paragraphs_land_on_separate_lines() {
        let buffer = markdown("first\n\nsecond", 20);
        assert_eq!(cell(&buffer, 0, 0), "f");
        assert_eq!(cell(&buffer, 0, 1), "s");
        assert_eq!(cell(&buffer, 1, 1), "e");
    }

    #[test]
    fn list_items_carry_bullets() {
        let buffer = markdown("- one", 20);
        assert_eq!(cell(&buffer, 0, 0), "•");
        assert_eq!(cell(&buffer, 2, 0), "o");
    }

    #[test]
    fn code_blocks_render_in_their_own_color() {
        let buffer = markdown("```rust\nlet value = 1;\n```", 40);
        assert_eq!(cell(&buffer, 0, 0), "l");
        let rendered = buffer.get(Position { x: 0, y: 0 }).expect("set");
        assert_eq!(rendered.fg, Color::Indexed(3));
    }

    #[test]
    fn inline_code_keeps_its_own_color() {
        let buffer = markdown("use `katui` now", 40);
        let rendered = buffer.get(Position { x: 5, y: 0 }).expect("set");
        assert_eq!(rendered.fg, Color::Indexed(3));
        let plain = buffer.get(Position { x: 0, y: 0 }).expect("set");
        assert_eq!(plain.fg, Color::Reset);
    }

    #[test]
    fn a_code_block_color_leaks_nothing_into_the_next_paragraph() {
        let buffer = markdown("```\ncode\n```\n\nafter", 40);
        let after = buffer.get(Position { x: 0, y: 1 }).expect("set");
        assert_eq!(after.fg, Color::Reset);
    }
}
