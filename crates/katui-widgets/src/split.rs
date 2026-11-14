//! Two panes side by side or one above the other, with a divider the
//! pointer drags.

use katui_ui::{Element, Length};

use crate::theme::Theme;

/// Which way a [`split`] lays its panes out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Orientation {
    /// Side by side, the divider upright.
    Horizontal,
    /// One above the other, the divider lying down.
    Vertical,
}

/// How much of a split the smaller pane keeps at least.
const LEAST: f32 = 0.05;

/// `first` and `second` filling the parent, `first` taking `fraction` of
/// the space. Dragging the divider sends `resize` with the new fraction.
pub fn split<M>(
    first: Element<M>,
    second: Element<M>,
    fraction: f32,
    orientation: Orientation,
    theme: &Theme,
    resize: impl Fn(f32) -> M + 'static,
) -> Element<M> {
    let fraction = fraction.clamp(LEAST, 1.0 - LEAST);
    let thickness = (theme.cell_width() * 0.5).max(3.0);
    // we wrap each pane in a box without padding so its share of the space
    // follows the fraction exactly, whatever padding the pane brings
    let pane = |element: Element<M>, share: f32| {
        Element::column([element.size(Length::Fill, Length::Fill)])
            .size(Length::Fill, Length::Fill)
            .grow(share * 1000.0)
    };
    let divider = Element::row([]).background(theme.border.clone()).grip();
    let container = match orientation {
        Orientation::Horizontal => Element::row([
            pane(first, fraction),
            divider.size(Length::Px(thickness), Length::Fill),
            pane(second, 1.0 - fraction),
        ]),
        Orientation::Vertical => Element::column([
            pane(first, fraction),
            divider.size(Length::Fill, Length::Px(thickness)),
            pane(second, 1.0 - fraction),
        ]),
    };
    container
        .size(Length::Fill, Length::Fill)
        .on_grip_press(move |x, y| {
            let along = match orientation {
                Orientation::Horizontal => x,
                Orientation::Vertical => y,
            };
            resize(along.clamp(LEAST, 1.0 - LEAST))
        })
}
