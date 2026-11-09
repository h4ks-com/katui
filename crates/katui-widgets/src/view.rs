//! Widget views: functions from state and theme to elements.

use katui_draw::Paint;
use katui_ui::{Align, Element, Length};

use crate::field::TextField;
use crate::list::ListState;
use crate::theme::Theme;

/// How much a button asks for attention.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Emphasis {
    /// The main action, in the accent color.
    Primary,
    /// Any other action.
    Plain,
}

/// Text in `size` and `color`.
pub fn label<M>(
    text: impl Into<std::sync::Arc<str>>,
    size: f32,
    color: katui_draw::Rgba,
) -> Element<M> {
    Element::text(text).font_size(size).color(color)
}

/// A button that sends `message` when clicked.
pub fn button<M>(text: &str, message: M, emphasis: Emphasis, theme: &Theme) -> Element<M> {
    let (background, color) = match emphasis {
        Emphasis::Primary => (Paint::Solid(theme.accent), theme.on_accent),
        Emphasis::Plain => (theme.raised.clone(), theme.text),
    };
    Element::column([label(text.to_owned(), theme.body(), color).weight(700)])
        .align(Align::Center)
        .padding_xy(theme.cell_width() * 2.0, theme.cell_height() * 0.4)
        .radius(theme.radius())
        .background(background)
        .border(1.0, theme.border.clone())
        .on_click(message)
}

/// A raised panel around `children`, stacked top to bottom.
pub fn panel<M>(children: impl IntoIterator<Item = Element<M>>, theme: &Theme) -> Element<M> {
    Element::column(children)
        .gap(theme.cell_height() * 0.6)
        .padding(theme.cell_height())
        .radius(theme.cell_height())
        .background(theme.raised.clone())
        .border(1.0, theme.border.clone())
}

/// A text field showing `field`, its caret where the cursor is while
/// `focused`, and `placeholder` while it is empty.
pub fn text_field<M>(
    field: &TextField,
    placeholder: &str,
    focused: bool,
    theme: &Theme,
) -> Element<M> {
    let (before, after) = field.split();
    let mut parts = Vec::new();
    if !before.is_empty() {
        parts.push(label(before.to_owned(), theme.body(), theme.text));
    }
    if focused {
        parts.push(
            Element::row([])
                .size(Length::Px(2.0), Length::Px(theme.body()))
                .background(Paint::Solid(theme.text)),
        );
    }
    if field.text().is_empty() {
        parts.push(label(placeholder.to_owned(), theme.body(), theme.muted));
    } else if !after.is_empty() {
        parts.push(label(after.to_owned(), theme.body(), theme.text));
    }
    let border = if focused {
        Paint::Solid(theme.accent)
    } else {
        theme.border.clone()
    };
    Element::row(parts)
        .align(Align::Center)
        .padding_xy(theme.cell_width() * 1.5, theme.cell_height() * 0.4)
        .radius(theme.radius())
        .background(theme.background.clone())
        .border(1.0, border)
        .width(Length::Fill)
}

/// The rows of `list` in view, each built by `row(index, selected)`, the
/// selected one highlighted.
pub fn list<M>(
    state: &ListState,
    theme: &Theme,
    mut row: impl FnMut(usize, bool) -> Element<M>,
) -> Element<M> {
    let rows = state.in_view().map(|index| {
        let selected = index == state.selected();
        let item = row(index, selected)
            .padding_xy(theme.cell_width(), theme.cell_height() * 0.2)
            .radius(theme.cell_height() * 0.3)
            .width(Length::Fill);
        if selected {
            item.background(theme.highlight.clone())
        } else {
            item
        }
    });
    Element::column(rows).width(Length::Fill)
}

/// A check box with `text`, sending `message` when clicked.
pub fn checkbox<M>(text: &str, checked: bool, message: M, theme: &Theme) -> Element<M> {
    let side = theme.body();
    let mut tick = Element::row([])
        .size(Length::Px(side), Length::Px(side))
        .radius(side * 0.25)
        .border(1.5, theme.border.clone());
    if checked {
        tick = tick.background(Paint::Solid(theme.accent));
    }
    Element::row([tick, label(text.to_owned(), theme.body(), theme.text)])
        .gap(theme.cell_width())
        .align(Align::Center)
        .on_click(message)
}

/// A bar filled to `fraction`, clamped into zero through one.
pub fn progress<M>(fraction: f32, theme: &Theme) -> Element<M> {
    let height = theme.cell_height() * 0.4;
    let filled = Element::row([])
        .height(Length::Px(height))
        .grow(fraction.clamp(0.0, 1.0))
        .radius(height / 2.0)
        .background(Paint::Solid(theme.accent));
    let rest = Element::row([]).grow(1.0 - fraction.clamp(0.0, 1.0));
    Element::row([filled, rest])
        .height(Length::Px(height))
        .radius(height / 2.0)
        .background(theme.highlight.clone())
        .width(Length::Fill)
}

/// A row of tabs titled `titles`, `selected` highlighted, each sending
/// `message(index)` when clicked.
pub fn tabs<M>(
    titles: &[&str],
    selected: usize,
    theme: &Theme,
    message: impl Fn(usize) -> M,
) -> Element<M> {
    let tabs = titles.iter().enumerate().map(|(index, title)| {
        let active = index == selected;
        let color = if active { theme.text } else { theme.muted };
        let mut tab = Element::column([
            label((*title).to_owned(), theme.body(), color).weight(if active { 700 } else { 400 })
        ])
        .padding_xy(theme.cell_width() * 1.5, theme.cell_height() * 0.3)
        .radius(theme.radius())
        .on_click(message(index));
        if active {
            tab = tab.background(theme.highlight.clone());
        }
        tab
    });
    Element::row(tabs).gap(theme.cell_width() * 0.5)
}
