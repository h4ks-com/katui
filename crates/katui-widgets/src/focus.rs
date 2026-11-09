//! Focus order: which of a screen's controls takes the keyboard, moved by
//! Tab and Shift+Tab or set by a click.

use katui_proto::input::{InputEvent, Key};

/// The controls that can take focus, in Tab order, and the one that has it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Focus<F> {
    order: Vec<F>,
    current: F,
}

impl<F: Copy + PartialEq> Focus<F> {
    /// Focus over `first` and then `rest`, `first` focused.
    pub fn new(first: F, rest: impl IntoIterator<Item = F>) -> Self {
        let order = std::iter::once(first).chain(rest).collect();
        Self {
            order,
            current: first,
        }
    }

    /// The focused control.
    pub fn current(&self) -> F {
        self.current
    }

    /// Whether `control` has focus.
    pub fn is(&self, control: F) -> bool {
        self.current == control
    }

    /// Focus `control`; `false` when it is not in the order.
    pub fn set(&mut self, control: F) -> bool {
        let known = self.order.contains(&control);
        if known {
            self.current = control;
        }
        known
    }

    /// Move to the next control, wrapping around.
    pub fn next(&mut self) {
        self.step(1);
    }

    /// Move to the previous control, wrapping around.
    pub fn previous(&mut self) {
        self.step(self.order.len() - 1);
    }

    fn step(&mut self, by: usize) {
        let index = self
            .order
            .iter()
            .position(|control| *control == self.current)
            .unwrap_or(0);
        if let Some(next) = self.order.get((index + by) % self.order.len()) {
            self.current = *next;
        }
    }

    /// Move on Tab and Shift+Tab; `true` when `event` was one of them.
    pub fn handle(&mut self, event: &InputEvent) -> bool {
        match event {
            InputEvent::Key(Key::Tab, mods) if mods.shift => self.previous(),
            InputEvent::Key(Key::Tab, _) => self.next(),
            _ => return false,
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use katui_proto::input::{InputEvent, Key, Mods};

    use super::Focus;

    #[test]
    fn tab_cycles_forward_and_shift_tab_back() {
        let mut focus = Focus::new('a', ['b', 'c']);
        assert!(focus.is('a'));
        assert!(focus.handle(&InputEvent::Key(Key::Tab, Mods::NONE)));
        assert!(focus.is('b'));
        let back = Mods {
            shift: true,
            ..Mods::NONE
        };
        focus.handle(&InputEvent::Key(Key::Tab, back));
        focus.handle(&InputEvent::Key(Key::Tab, back));
        assert!(focus.is('c'), "shift tab wraps to the end");
        assert!(!focus.handle(&InputEvent::Key(Key::Enter, Mods::NONE)));
        assert!(focus.set('a'));
        assert!(
            !focus.set('z'),
            "an unknown control keeps focus where it was"
        );
        assert!(focus.is('a'));
    }
}
