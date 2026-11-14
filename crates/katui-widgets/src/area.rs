//! A multi-line text area: a text field whose Enter starts a new line,
//! whose arrows move between lines, and which shows a window of rows that
//! follows the cursor.

use katui_draw::Paint;
use katui_proto::input::{InputEvent, Key, Mods};
use katui_ui::{Align, Element, Length};
use unicode_segmentation::UnicodeSegmentation as _;

use crate::field::{Edit, TextField};
use crate::theme::Theme;
use crate::view::label;

/// A multi-line text area's state: the text, the cursor, and which rows
/// are in view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextArea {
    field: TextField,
    rows: usize,
    top: usize,
}

impl TextArea {
    /// An empty area showing `rows` lines at a time.
    #[must_use]
    pub fn new(rows: usize) -> Self {
        Self {
            field: TextField::default(),
            rows: rows.max(1),
            top: 0,
        }
    }

    /// The text.
    #[must_use]
    pub fn text(&self) -> &str {
        self.field.text()
    }

    /// Replace the text, the cursor at its end.
    pub fn set_text(&mut self, text: impl Into<String>) {
        self.field.set_text(text);
        self.follow_cursor();
    }

    /// Empty the area and return what it held.
    pub fn take(&mut self) -> String {
        self.top = 0;
        self.field.take()
    }

    /// How many lines the area shows at a time.
    #[must_use]
    pub fn rows(&self) -> usize {
        self.rows
    }

    /// Apply one input event. Enter starts a new line, and Ctrl+Enter or
    /// Alt+Enter reports [`Edit::Submitted`].
    pub fn handle(&mut self, event: &InputEvent) -> Edit {
        let edit = match event {
            InputEvent::Paste(text) => self.field.insert(&clean(text)),
            InputEvent::Key(key, mods) => self.key(*key, *mods),
            _ => Edit::Ignored,
        };
        self.follow_cursor();
        edit
    }

    fn key(&mut self, key: Key, mods: Mods) -> Edit {
        let (start, end) = self.line_bounds();
        match key {
            Key::Enter if mods.ctrl || mods.alt => Edit::Submitted,
            Key::Enter => self.field.insert("\n"),
            Key::Up => self.move_line(false),
            Key::Down => self.move_line(true),
            Key::Home if mods.ctrl => self.field.move_to(0),
            Key::End if mods.ctrl => self.field.move_to(self.field.text.len()),
            Key::Home => self.field.move_to(start),
            Key::End => self.field.move_to(end),
            Key::Char('a') if mods.ctrl => self.field.move_to(start),
            Key::Char('e') if mods.ctrl => self.field.move_to(end),
            Key::Char('u') if mods.ctrl => self.field.remove(start, self.field.cursor),
            Key::Char('k') if mods.ctrl => self.field.remove(self.field.cursor, end),
            _ => self.field.handle(&InputEvent::Key(key, mods)),
        }
    }

    /// The byte range of the line the cursor is on, without its newline.
    fn line_bounds(&self) -> (usize, usize) {
        let text = &self.field.text;
        let cursor = self.field.cursor;
        let start = text
            .get(..cursor)
            .and_then(|before| before.rfind('\n'))
            .map_or(0, |at| at + 1);
        let end = text
            .get(cursor..)
            .and_then(|after| after.find('\n'))
            .map_or(text.len(), |at| cursor + at);
        (start, end)
    }

    /// The line the cursor is on, counting from zero.
    #[must_use]
    pub fn cursor_line(&self) -> usize {
        self.field
            .text
            .get(..self.field.cursor)
            .map_or(0, |before| before.matches('\n').count())
    }

    fn move_line(&mut self, down: bool) -> Edit {
        let (start, end) = self.line_bounds();
        let text = &self.field.text;
        let column = text
            .get(start..self.field.cursor)
            .map_or(0, |before| before.graphemes(true).count());
        let target = if down {
            if end >= text.len() {
                return Edit::Ignored;
            }
            end + 1
        } else {
            if start == 0 {
                return Edit::Ignored;
            }
            text.get(..start - 1)
                .and_then(|before| before.rfind('\n'))
                .map_or(0, |at| at + 1)
        };
        let line = text
            .get(target..)
            .map_or("", |rest| rest.split('\n').next().unwrap_or(""));
        let offset = line
            .grapheme_indices(true)
            .nth(column)
            .map_or(line.len(), |(at, _)| at);
        self.field.move_to(target + offset)
    }

    fn follow_cursor(&mut self) {
        let line = self.cursor_line();
        if line < self.top {
            self.top = line;
        } else if line >= self.top + self.rows {
            self.top = line + 1 - self.rows;
        }
    }
}

/// Pasted text with Windows line ends folded and other control characters
/// turned into spaces.
fn clean(text: &str) -> String {
    text.replace("\r\n", "\n")
        .chars()
        .map(|c| if c.is_control() && c != '\n' { ' ' } else { c })
        .collect()
}

/// A text area showing the rows of `area` in view, its caret where the
/// cursor is while `focused`, and `placeholder` while it is empty.
pub fn text_area<M>(
    area: &TextArea,
    placeholder: &str,
    focused: bool,
    theme: &Theme,
) -> Element<M> {
    let line_height = theme.body() * 1.35;
    let cursor_line = area.cursor_line();
    let (line_start, _) = area.line_bounds();
    let rows = area
        .text()
        .split('\n')
        .enumerate()
        .skip(area.top)
        .take(area.rows)
        .map(|(index, line)| {
            let row = if index == cursor_line && focused {
                let column = area.field.cursor - line_start;
                caret_row(line, column, theme)
            } else {
                Element::row(text_part(line, theme))
            };
            row.align(Align::Center)
                .height(Length::Px(line_height))
                .width(Length::Fill)
        });
    let mut body: Vec<Element<M>> = rows.collect();
    if area.text().is_empty() && !focused {
        body = vec![
            Element::row([label(placeholder.to_owned(), theme.body(), theme.muted)])
                .height(Length::Px(line_height)),
        ];
    }
    let border = if focused {
        Paint::Solid(theme.accent)
    } else {
        theme.border.clone()
    };
    Element::column(body)
        .padding_xy(theme.cell_width() * 1.5, theme.cell_height() * 0.4)
        .height(Length::Px(
            line_height * f32::from(u16::try_from(area.rows).unwrap_or(u16::MAX))
                + theme.cell_height() * 0.8
                + 2.0,
        ))
        .radius(theme.radius())
        .background(theme.background.clone())
        .border(1.0, border)
        .width(Length::Fill)
}

fn text_part<M>(text: &str, theme: &Theme) -> Vec<Element<M>> {
    if text.is_empty() {
        Vec::new()
    } else {
        vec![label(text.to_owned(), theme.body(), theme.text)]
    }
}

fn caret_row<M>(line: &str, column: usize, theme: &Theme) -> Element<M> {
    let (before, after) = line.split_at(column.min(line.len()));
    let mut parts = text_part(before, theme);
    parts.push(
        Element::row([])
            .size(Length::Px(2.0), Length::Px(theme.body()))
            .background(Paint::Solid(theme.text)),
    );
    parts.extend(text_part(after, theme));
    Element::row(parts)
}

#[cfg(test)]
mod tests {
    use katui_proto::input::{InputEvent, Key, Mods};

    use super::{Edit, TextArea};

    fn press(area: &mut TextArea, key: Key) -> Edit {
        area.handle(&InputEvent::Key(key, Mods::NONE))
    }

    fn typed(area: &mut TextArea, text: &str) {
        for character in text.chars() {
            let key = if character == '\n' {
                Key::Enter
            } else {
                Key::Char(character)
            };
            press(area, key);
        }
    }

    #[test]
    fn enter_breaks_lines_and_ctrl_enter_submits() {
        let mut area = TextArea::new(3);
        typed(&mut area, "one\ntwo");
        assert_eq!(area.text(), "one\ntwo");
        assert_eq!(area.cursor_line(), 1);
        let submit = area.handle(&InputEvent::Key(
            Key::Enter,
            Mods {
                ctrl: true,
                ..Mods::NONE
            },
        ));
        assert_eq!(submit, Edit::Submitted);
        assert_eq!(area.text(), "one\ntwo", "submitting adds no line");
    }

    #[test]
    fn up_and_down_keep_the_column_where_the_line_allows() {
        let mut area = TextArea::new(3);
        typed(&mut area, "abcdef\nxy\nlonger line");
        press(&mut area, Key::Home);
        for _ in 0..5 {
            press(&mut area, Key::Right);
        }
        press(&mut area, Key::Up);
        assert_eq!(area.cursor_line(), 1);
        typed(&mut area, "!");
        assert_eq!(
            area.text(),
            "abcdef\nxy!\nlonger line",
            "a short line ends the column"
        );
        press(&mut area, Key::Up);
        typed(&mut area, "?");
        assert_eq!(area.text(), "abc?def\nxy!\nlonger line");
        assert_eq!(
            press(&mut area, Key::Up),
            Edit::Ignored,
            "the first line stops it"
        );
    }

    #[test]
    fn home_and_end_stay_on_the_line_and_paste_keeps_newlines() {
        let mut area = TextArea::new(2);
        typed(&mut area, "first\nsecond");
        press(&mut area, Key::Home);
        typed(&mut area, ">");
        assert_eq!(area.text(), "first\n>second");
        area.handle(&InputEvent::Paste("a\r\nb\tc".to_owned()));
        assert_eq!(area.text(), "first\n>a\nb csecond");
    }

    #[test]
    fn the_view_follows_the_cursor_down_and_back_up() {
        let mut area = TextArea::new(2);
        typed(&mut area, "1\n2\n3\n4");
        assert_eq!(area.top, 2, "the last two lines are in view");
        press(&mut area, Key::Up);
        press(&mut area, Key::Up);
        press(&mut area, Key::Up);
        assert_eq!(area.top, 0);
    }
}
