//! Widget views: functions from state to elements.

use std::sync::Arc;

use katui_draw::{Paint, Rgba};
use katui_ui::{Align, Element, Length};

use crate::field::TextField;
use crate::list::ListState;
use crate::theme::Theme;

/// Body text.
pub fn label<M>(text: impl Into<Arc<str>>) -> Element<M> {
    let theme = Theme::current();
    styled(text, theme.body(), theme.text)
}

/// A bold heading.
pub fn title<M>(text: impl Into<Arc<str>>) -> Element<M> {
    let theme = Theme::current();
    styled(text, theme.title(), theme.text).weight(800)
}

/// A bold section heading, smaller than a [`title`].
pub fn heading<M>(text: impl Into<Arc<str>>) -> Element<M> {
    let theme = Theme::current();
    styled(text, theme.heading(), theme.text).weight(800)
}

/// Small, muted text for hints and captions.
pub fn hint<M>(text: impl Into<Arc<str>>) -> Element<M> {
    let theme = Theme::current();
    styled(text, theme.small(), theme.muted)
}

/// Text in `size` and `color`.
pub(crate) fn styled<M>(text: impl Into<Arc<str>>, size: f32, color: Rgba) -> Element<M> {
    Element::text(text).font_size(size).color(color)
}

/// A button that sends `message` when clicked.
pub fn button<M>(text: &str, message: M) -> Element<M> {
    let theme = Theme::current();
    button_in(text, message, theme.raised.clone(), theme.text, &theme)
}

/// A button that keyboard or controller focus can rest on; `focused` rings
/// it in the accent color so the reader sees where the keys act.
pub fn focusable_button<M>(text: &str, message: M, focused: bool) -> Element<M> {
    let theme = Theme::current();
    button_in(text, message, theme.raised.clone(), theme.text, &theme)
        .border(1.0, theme.outline(focused))
}

/// A button in the accent color, for the action that matters most.
pub fn primary_button<M>(text: &str, message: M) -> Element<M> {
    let theme = Theme::current();
    button_in(
        text,
        message,
        Paint::Solid(theme.accent),
        theme.on_accent,
        &theme,
    )
}

fn button_in<M>(
    text: &str,
    message: M,
    background: Paint,
    color: Rgba,
    theme: &Theme,
) -> Element<M> {
    Element::column([styled(text, theme.body(), color).weight(700)])
        .align(Align::Center)
        .padding_xy(theme.cell_width() * 2.0, theme.cell_height() * 0.4)
        .radius(theme.radius())
        .background(background)
        .border(1.0, theme.border.clone())
        .on_click(message)
}

/// A raised panel around `children`, stacked top to bottom.
pub fn panel<M>(children: impl IntoIterator<Item = Element<M>>) -> Element<M> {
    let theme = Theme::current();
    Element::column(children)
        .gap(theme.cell_height() * 0.6)
        .padding(theme.cell_height())
        .radius(theme.cell_height())
        .background(theme.raised.clone())
        .border(1.0, theme.border.clone())
}

/// The whole screen in the theme's background, `children` stacked top to
/// bottom with a cell of padding. Use it as the root of a view.
pub fn page<M>(children: impl IntoIterator<Item = Element<M>>) -> Element<M> {
    let theme = Theme::current();
    Element::column(children)
        .gap(theme.cell_height() * 0.6)
        .padding(theme.cell_height())
        .size(Length::Fill, Length::Fill)
        .background(theme.background.clone())
}

/// A text field showing `field`, its caret where the cursor is while
/// `focused`, and `placeholder` while it is empty.
#[must_use]
pub fn text_field<M>(field: &TextField, placeholder: &str, focused: bool) -> Element<M> {
    let theme = Theme::current();
    let (before, after) = field.split();
    let mut parts = Vec::new();
    if !before.is_empty() {
        parts.push(styled(before, theme.body(), theme.text));
    }
    if focused {
        parts.push(cursor_bar(&theme));
    }
    if field.text().is_empty() {
        parts.push(styled(placeholder, theme.body(), theme.muted));
    } else if !after.is_empty() {
        parts.push(styled(after, theme.body(), theme.text));
    }
    input_box(Element::row(parts).align(Align::Center), focused, &theme)
}

/// The text cursor: a thin bar one line of body text tall.
pub(crate) fn cursor_bar<M>(theme: &Theme) -> Element<M> {
    Element::row([])
        .size(Length::Px(2.0), Length::Px(theme.body()))
        .background(Paint::Solid(theme.text))
}

/// `content` framed as an input, outlined in the accent color while
/// `active`.
pub(crate) fn input_box<M>(content: Element<M>, active: bool, theme: &Theme) -> Element<M> {
    content
        .padding_xy(theme.cell_width() * 1.5, theme.cell_height() * 0.4)
        .radius(theme.radius())
        .background(theme.background.clone())
        .border(1.0, theme.outline(active))
        .width(Length::Fill)
}

/// The rows of `list` in view, each built by `row(index, selected)`, the
/// selected one highlighted.
pub fn list<M>(state: &ListState, mut row: impl FnMut(usize, bool) -> Element<M>) -> Element<M> {
    let theme = Theme::current();
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
pub fn checkbox<M>(text: &str, checked: bool, message: M) -> Element<M> {
    let theme = Theme::current();
    let side = theme.body();
    let mut tick = Element::row([])
        .size(Length::Px(side), Length::Px(side))
        .radius(side * 0.25)
        .border(1.5, theme.border.clone());
    if checked {
        tick = tick.background(Paint::Solid(theme.accent));
    }
    Element::row([tick, styled(text, theme.body(), theme.text)])
        .gap(theme.cell_width())
        .align(Align::Center)
        .on_click(message)
}

/// A bar filled to `fraction`, clamped into zero through one.
#[must_use]
pub fn progress<M>(fraction: f32) -> Element<M> {
    let theme = Theme::current();
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
pub fn tabs<M>(titles: &[&str], selected: usize, message: impl Fn(usize) -> M) -> Element<M> {
    let theme = Theme::current();
    let tabs = titles.iter().enumerate().map(|(index, title)| {
        let active = index == selected;
        let color = if active { theme.text } else { theme.muted };
        let mut tab = Element::column([styled(*title, theme.body(), color).weight(if active {
            700
        } else {
            400
        })])
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
