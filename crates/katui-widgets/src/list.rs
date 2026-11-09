//! A selectable list: which row is selected and which rows are in view,
//! moved by the arrow, page and home keys.

use std::ops::Range;

use katui_proto::input::{InputEvent, Key};

/// A list's selection and scroll over `len` rows, `visible` at a time.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ListState {
    len: usize,
    visible: usize,
    selected: usize,
    offset: usize,
}

impl ListState {
    /// A list of `len` rows showing `visible` at once.
    #[must_use]
    pub fn new(len: usize, visible: usize) -> Self {
        let mut state = Self::default();
        state.resize(len, visible);
        state
    }

    /// Follow a list that grew, shrank or got a new height, keeping the
    /// selection inside it and in view.
    pub fn resize(&mut self, len: usize, visible: usize) {
        self.len = len;
        self.visible = visible.max(1);
        self.select(self.selected);
    }

    /// The selected row; zero in an empty list.
    #[must_use]
    pub fn selected(&self) -> usize {
        self.selected
    }

    /// The rows in view.
    #[must_use]
    pub fn in_view(&self) -> Range<usize> {
        self.offset..(self.offset + self.visible).min(self.len)
    }

    /// Select `row`, clamped into the list, and scroll it into view.
    pub fn select(&mut self, row: usize) {
        self.selected = row.min(self.len.saturating_sub(1));
        if self.selected < self.offset {
            self.offset = self.selected;
        } else if self.selected >= self.offset + self.visible {
            self.offset = self.selected + 1 - self.visible;
        }
        self.offset = self.offset.min(self.len.saturating_sub(self.visible));
    }

    /// Move the selection on the arrow, page, home and end keys; `true`
    /// when `event` was one of them.
    pub fn handle(&mut self, event: &InputEvent) -> bool {
        let InputEvent::Key(key, _) = event else {
            return false;
        };
        let page = self.visible.saturating_sub(1).max(1);
        let row = match key {
            Key::Up => self.selected.saturating_sub(1),
            Key::Down => self.selected + 1,
            Key::PageUp => self.selected.saturating_sub(page),
            Key::PageDown => self.selected + page,
            Key::Home => 0,
            Key::End => self.len,
            _ => return false,
        };
        self.select(row);
        true
    }
}

#[cfg(test)]
mod tests {
    use katui_proto::input::{InputEvent, Key, Mods};

    use super::ListState;

    fn press(list: &mut ListState, key: Key) {
        assert!(list.handle(&InputEvent::Key(key, Mods::NONE)));
    }

    #[test]
    fn moving_the_selection_scrolls_just_enough_to_keep_it_in_view() {
        let mut list = ListState::new(20, 5);
        assert_eq!(list.in_view(), 0..5);
        for _ in 0..6 {
            press(&mut list, Key::Down);
        }
        assert_eq!((list.selected(), list.in_view()), (6, 2..7));
        press(&mut list, Key::PageDown);
        assert_eq!((list.selected(), list.in_view()), (10, 6..11));
        press(&mut list, Key::End);
        assert_eq!((list.selected(), list.in_view()), (19, 15..20));
        press(&mut list, Key::Home);
        assert_eq!((list.selected(), list.in_view()), (0, 0..5));
        press(&mut list, Key::Up);
        assert_eq!(list.selected(), 0, "the top stays the top");
    }

    #[test]
    fn a_shrinking_list_pulls_the_selection_and_view_back_in() {
        let mut list = ListState::new(20, 5);
        list.select(18);
        list.resize(8, 5);
        assert_eq!((list.selected(), list.in_view()), (7, 3..8));
        list.resize(0, 5);
        assert_eq!((list.selected(), list.in_view()), (0, 0..0));
        assert!(!list.handle(&InputEvent::Key(Key::Enter, Mods::NONE)));
    }
}
