//! What clicks, drags, links and timers carry back to the host: one of its
//! values, or one of its values and what the widget reported.

use katui_ui::{Element, Scroll};

/// A message over `H`, the host's handle for one of its own values: the
/// value itself, or a value that makes the message from the payload.
#[derive(Clone)]
pub enum Msg<H> {
    Value(H),
    Index(H, usize),
    Id(H, u64),
    Float(H, f32),
    Point(H, f32, f32),
    Scroll(H, Scroll),
    Link(H, String),
    /// A link clicked in a document that has no link handler.
    Href(String),
    /// The host failed while we drew, so the app has to end.
    Stop,
}

/// An element whose messages carry `H`.
pub type Node<H> = Element<Msg<H>>;

/// The message for the index a widget picked, made from `make`.
pub fn index<H: Clone>(make: H) -> impl Fn(usize) -> Msg<H> {
    move |index| Msg::Index(make.clone(), index)
}

/// The message for the tree node a widget picked, made from `make`.
pub fn id<H: Clone>(make: H) -> impl Fn(u64) -> Msg<H> {
    move |id| Msg::Id(make.clone(), id)
}

/// The message for a slider's or a divider's new value, made from `make`.
pub fn float<H: Clone>(make: H) -> impl Fn(f32) -> Msg<H> {
    move |value| Msg::Float(make.clone(), value)
}

/// The message for a press or a drag at `x`, `y`, made from `make`.
pub fn point<H: Clone>(make: H) -> impl Fn(f32, f32) -> Msg<H> {
    move |x, y| Msg::Point(make.clone(), x, y)
}

/// The message for a new scroll position, made from `make`.
pub fn scroll<H: Clone>(make: H) -> impl Fn(Scroll) -> Msg<H> {
    move |scroll| Msg::Scroll(make.clone(), scroll)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn makers_carry_the_handle_and_the_payload() {
        assert!(matches!(index(7)(3), Msg::Index(7, 3)));
        assert!(matches!(id(7)(9), Msg::Id(7, 9)));
        assert!(matches!(point(7)(0.5, 1.0), Msg::Point(7, ..)));
        let moved = scroll(7)(Scroll {
            offset: 2.0,
            max: 4.0,
        });
        assert!(matches!(moved, Msg::Scroll(7, _)));
    }
}
