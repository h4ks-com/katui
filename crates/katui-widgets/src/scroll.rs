//! A scrolling view: its position, the keys that move it, and the view
//! that clips its content and draws the scrollbar.

use katui_proto::input::{InputEvent, Key};
use katui_ui::{Element, Length, Scroll};

use crate::theme::Theme;

/// How many lines a page key moves.
const PAGE_LINES: f32 = 10.0;

/// Where a scroll view stands. The view clamps the offset to the content,
/// so moving it past the end is safe: the next frame sends the clamped
/// position back through the view's message.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ScrollState {
    offset: f32,
    max: f32,
}

impl ScrollState {
    /// How far the content is scrolled, in pixels.
    #[must_use]
    pub fn offset(&self) -> f32 {
        self.offset
    }

    /// The furthest the content can scroll, as of the last position the
    /// view reported.
    #[must_use]
    pub fn max(&self) -> f32 {
        self.max
    }

    /// Store a position the view reported, from the wheel, the scrollbar
    /// or a clamp.
    pub fn apply(&mut self, scroll: Scroll) {
        self.offset = scroll.offset;
        self.max = scroll.max;
    }

    /// Move by `pixels`, down for positive values.
    pub fn scroll_by(&mut self, pixels: f32) {
        self.offset = (self.offset + pixels).max(0.0);
    }

    /// Move on the arrow, page, home and end keys, `line` pixels per
    /// arrow press; `true` when `event` was one of them.
    pub fn handle(&mut self, event: &InputEvent, line: f32) -> bool {
        let InputEvent::Key(key, _) = event else {
            return false;
        };
        match key {
            Key::Up => self.scroll_by(-line),
            Key::Down => self.scroll_by(line),
            Key::PageUp => self.scroll_by(-line * PAGE_LINES),
            Key::PageDown => self.scroll_by(line * PAGE_LINES),
            Key::Home => self.offset = 0.0,
            Key::End => self.offset = f32::MAX,
            _ => return false,
        }
        true
    }
}

/// `content` stacked top to bottom in a box that fills its parent and
/// scrolls, sending `message` with each new position. The wheel and the
/// scrollbar move it; pass the position to [`ScrollState::apply`].
pub fn scroll_view<M>(
    state: &ScrollState,
    content: impl IntoIterator<Item = Element<M>>,
    theme: &Theme,
    message: impl Fn(Scroll) -> M + 'static,
) -> Element<M> {
    Element::column(content)
        .scroll(state.offset)
        .on_scroll(message)
        .scrollbar(theme.muted)
        .gap(theme.cell_height() * 0.3)
        .size(Length::Fill, Length::Fill)
}

#[cfg(test)]
mod tests {
    use katui_proto::input::{InputEvent, Key, Mods};
    use katui_ui::Scroll;

    use super::ScrollState;

    fn press(state: &mut ScrollState, key: Key) -> bool {
        state.handle(&InputEvent::Key(key, Mods::NONE), 20.0)
    }

    #[test]
    fn keys_move_the_offset_and_never_above_the_top() {
        let mut state = ScrollState::default();
        assert!(press(&mut state, Key::PageDown));
        assert_eq!(state.offset(), 200.0);
        assert!(press(&mut state, Key::Up));
        assert_eq!(state.offset(), 180.0);
        assert!(press(&mut state, Key::Home));
        assert_eq!(state.offset(), 0.0);
        assert!(press(&mut state, Key::Up));
        assert_eq!(state.offset(), 0.0, "the top stops it");
        assert!(!press(&mut state, Key::Char('x')));
    }

    #[test]
    fn a_reported_position_replaces_the_offset() {
        let mut state = ScrollState::default();
        press(&mut state, Key::End);
        state.apply(Scroll {
            offset: 300.0,
            max: 300.0,
        });
        assert_eq!((state.offset(), state.max()), (300.0, 300.0));
    }
}
