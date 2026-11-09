//! A one-line text field: the text, a cursor that moves by grapheme, and
//! the readline keys people expect.

use katui_proto::input::{InputEvent, Key, Mods};
use unicode_segmentation::UnicodeSegmentation as _;

/// What one input event did to a field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edit {
    /// The text changed.
    Changed,
    /// Only the cursor moved.
    Moved,
    /// Enter was pressed.
    Submitted,
    /// The event means nothing to a field.
    Ignored,
}

/// A one-line text field's state. The cursor is a byte offset that always
/// sits on a grapheme boundary.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TextField {
    text: String,
    cursor: usize,
}

impl TextField {
    /// A field holding `text`, the cursor at its end.
    #[must_use]
    pub fn with_text(text: impl Into<String>) -> Self {
        let text = text.into();
        let cursor = text.len();
        Self { text, cursor }
    }

    /// The text.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// The text before and after the cursor.
    #[must_use]
    pub fn split(&self) -> (&str, &str) {
        self.text.split_at(self.cursor)
    }

    /// Replace the text, the cursor at its end.
    pub fn set_text(&mut self, text: impl Into<String>) {
        *self = Self::with_text(text);
    }

    /// Empty the field and return what it held.
    pub fn take(&mut self) -> String {
        self.cursor = 0;
        std::mem::take(&mut self.text)
    }

    /// Apply one input event.
    pub fn handle(&mut self, event: &InputEvent) -> Edit {
        match event {
            InputEvent::Paste(text) => {
                let line: String = text
                    .chars()
                    .map(|c| if c.is_control() { ' ' } else { c })
                    .collect();
                self.insert(&line)
            }
            InputEvent::Key(key, mods) => self.key(*key, *mods),
            _ => Edit::Ignored,
        }
    }

    fn key(&mut self, key: Key, mods: Mods) -> Edit {
        let word = mods.ctrl || mods.alt;
        match key {
            Key::Enter => Edit::Submitted,
            Key::Char('a') if mods.ctrl => self.move_to(0),
            Key::Char('e') if mods.ctrl => self.move_to(self.text.len()),
            Key::Char('u') if mods.ctrl => self.remove(0, self.cursor),
            Key::Char('k') if mods.ctrl => self.remove(self.cursor, self.text.len()),
            Key::Char('w') if mods.ctrl => self.remove(self.word_start(), self.cursor),
            Key::Char(character) if !mods.ctrl && !mods.alt => {
                self.insert(character.encode_utf8(&mut [0; 4]))
            }
            Key::Backspace if word => self.remove(self.word_start(), self.cursor),
            Key::Backspace => self.remove(self.previous_boundary(), self.cursor),
            Key::Delete => self.remove(self.cursor, self.next_boundary()),
            Key::Left if word => self.move_to(self.word_start()),
            Key::Right if word => self.move_to(self.word_end()),
            Key::Left => self.move_to(self.previous_boundary()),
            Key::Right => self.move_to(self.next_boundary()),
            Key::Home => self.move_to(0),
            Key::End => self.move_to(self.text.len()),
            _ => Edit::Ignored,
        }
    }

    fn insert(&mut self, text: &str) -> Edit {
        if text.is_empty() {
            return Edit::Ignored;
        }
        self.text.insert_str(self.cursor, text);
        self.cursor += text.len();
        Edit::Changed
    }

    fn remove(&mut self, from: usize, to: usize) -> Edit {
        if from >= to {
            return Edit::Ignored;
        }
        self.text.replace_range(from..to, "");
        self.cursor = from;
        Edit::Changed
    }

    fn move_to(&mut self, at: usize) -> Edit {
        if at == self.cursor {
            return Edit::Ignored;
        }
        self.cursor = at;
        Edit::Moved
    }

    fn previous_boundary(&self) -> usize {
        self.split()
            .0
            .grapheme_indices(true)
            .next_back()
            .map_or(0, |(at, _)| at)
    }

    fn next_boundary(&self) -> usize {
        self.split()
            .1
            .graphemes(true)
            .next()
            .map_or(self.cursor, |grapheme| self.cursor + grapheme.len())
    }

    fn word_start(&self) -> usize {
        let before = self.split().0;
        let trimmed = before.trim_end();
        trimmed.rfind(char::is_whitespace).map_or(0, |at| {
            at + trimmed
                .get(at..)
                .and_then(|rest| rest.chars().next())
                .map_or(1, char::len_utf8)
        })
    }

    fn word_end(&self) -> usize {
        let after = self.split().1;
        let rest = after.trim_start();
        let leading = after.len() - rest.len();
        self.cursor + leading + rest.find(char::is_whitespace).unwrap_or(rest.len())
    }
}

#[cfg(test)]
mod tests {
    use katui_proto::input::{InputEvent, Key, Mods};

    use super::{Edit, TextField};

    fn press(field: &mut TextField, key: Key) -> Edit {
        field.handle(&InputEvent::Key(key, Mods::NONE))
    }

    fn ctrl(field: &mut TextField, letter: char) -> Edit {
        let mods = Mods {
            ctrl: true,
            ..Mods::NONE
        };
        field.handle(&InputEvent::Key(Key::Char(letter), mods))
    }

    fn typed(text: &str) -> TextField {
        let mut field = TextField::default();
        for character in text.chars() {
            press(&mut field, Key::Char(character));
        }
        field
    }

    #[test]
    fn typing_inserts_at_the_cursor_and_enter_submits() {
        let mut field = typed("ct");
        press(&mut field, Key::Left);
        assert_eq!(press(&mut field, Key::Char('a')), Edit::Changed);
        assert_eq!(field.split(), ("ca", "t"));
        assert_eq!(press(&mut field, Key::Enter), Edit::Submitted);
        assert_eq!(field.take(), "cat");
        assert_eq!(field.text(), "");
    }

    #[test]
    fn the_cursor_steps_over_whole_graphemes() {
        let mut field = TextField::with_text("ae\u{301}\u{1f469}\u{200d}\u{1f4bb}");
        press(&mut field, Key::Left);
        assert_eq!(
            field.split().1,
            "\u{1f469}\u{200d}\u{1f4bb}",
            "the joined emoji is one step"
        );
        press(&mut field, Key::Backspace);
        assert_eq!(
            field.text(),
            "a\u{1f469}\u{200d}\u{1f4bb}",
            "the accented e goes whole"
        );
        press(&mut field, Key::Delete);
        assert_eq!(field.text(), "a");
        assert_eq!(
            press(&mut field, Key::Delete),
            Edit::Ignored,
            "nothing to delete at the end"
        );
    }

    #[test]
    fn readline_keys_jump_and_cut() {
        let mut field = typed("one two  three");
        ctrl(&mut field, 'w');
        assert_eq!(field.text(), "one two  ");
        ctrl(&mut field, 'a');
        assert_eq!(field.split(), ("", "one two  "));
        field.handle(&InputEvent::Key(
            Key::Right,
            Mods {
                ctrl: true,
                ..Mods::NONE
            },
        ));
        assert_eq!(field.split(), ("one", " two  "));
        ctrl(&mut field, 'k');
        assert_eq!(field.text(), "one");
        ctrl(&mut field, 'u');
        assert_eq!(field.text(), "");
        assert_eq!(press(&mut field, Key::Home), Edit::Ignored);
    }

    #[test]
    fn a_paste_lands_as_one_line() {
        let mut field = TextField::default();
        assert_eq!(
            field.handle(&InputEvent::Paste("a\nb\tc".to_owned())),
            Edit::Changed
        );
        assert_eq!(field.text(), "a b c");
        assert_eq!(field.handle(&InputEvent::Focus(true)), Edit::Ignored);
    }
}
