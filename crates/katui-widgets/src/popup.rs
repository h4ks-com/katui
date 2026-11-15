//! Views that open above the rest of the screen: a modal dialog, a
//! select, a menu and a tooltip.

use katui_draw::{Paint, Rgba};
use katui_ui::{Align, Element, Length};

use crate::theme::Theme;
use crate::view::{heading, input_box, panel, styled};

/// The dimming behind a dialog.
const SCRIM: Rgba = Rgba {
    r: 0,
    g: 0,
    b: 0,
    a: 150,
};

/// How far an open list's click catcher reaches past its anchor, enough
/// to cover any screen.
const CATCHER: f32 = 20_000.0;

/// A dialog over the whole screen with `title`, `body` and a row of
/// `actions`, sending `dismiss` for a click outside it. Put it last in
/// your root element, and only while it is open.
pub fn dialog<M>(
    title: &str,
    body: Element<M>,
    actions: impl IntoIterator<Item = Element<M>>,
    dismiss: M,
) -> Element<M> {
    let theme = Theme::current();
    let buttons = Element::row(actions)
        .gap(theme.cell_width())
        .justify(Align::End)
        .width(Length::Fill);
    let card = panel([heading(title), body, buttons])
        .width(theme.columns(56.0))
        .opaque();
    Element::column([card])
        .align(Align::Center)
        .justify(Align::Center)
        .size(Length::Fill, Length::Fill)
        .background(Paint::Solid(SCRIM))
        .on_click(dismiss)
        .popup(0.0, 0.0)
}

/// A field showing option `selected` of `options` that sends `toggle`
/// when clicked. While `open`, a list of the options hangs below it: a
/// click on one sends `choose(index)`, and a click anywhere else sends
/// `toggle` again.
pub fn select<M: Clone>(
    options: &[&str],
    selected: usize,
    open: bool,
    toggle: M,
    choose: impl Fn(usize) -> M,
) -> Element<M> {
    let theme = Theme::current();
    let current = options.get(selected).copied().unwrap_or_default();
    let field = input_box(
        Element::row([
            styled(current, theme.body(), theme.text),
            Element::spacer(),
            down_arrow(&theme),
        ])
        .align(Align::Center)
        .gap(theme.cell_width()),
        open,
        &theme,
    )
    .on_click(toggle.clone());
    let items = open.then(|| {
        options
            .iter()
            .enumerate()
            .map(|(index, option)| item(option, index == selected, choose(index), &theme))
            .collect()
    });
    dropdown(field, items, toggle, true, &theme)
}

/// A menu titled `title` that sends `toggle` when clicked. While `open`,
/// its `items` hang below it: a click on one sends `choose(index)`, and a
/// click anywhere else sends `toggle` again. Put menus in a row for a
/// menu bar.
pub fn menu<M: Clone>(
    title: &str,
    items: &[&str],
    open: bool,
    toggle: M,
    choose: impl Fn(usize) -> M,
) -> Element<M> {
    let theme = Theme::current();
    let mut anchor = Element::column([styled(title, theme.body(), theme.text)])
        .padding_xy(theme.cell_width() * 1.5, theme.cell_height() * 0.3)
        .radius(theme.radius())
        .on_click(toggle.clone());
    if open {
        anchor = anchor.background(theme.highlight.clone());
    }
    let entries = open.then(|| {
        items
            .iter()
            .enumerate()
            .map(|(index, entry)| item(entry, false, choose(index), &theme))
            .collect()
    });
    dropdown(anchor, entries, toggle, false, &theme)
}

/// `anchor` with `text` showing below it while `shown`. Hovering it sends
/// `show`, and the pointer leaving it sends `hide`.
pub fn tooltip<M>(anchor: Element<M>, text: &str, shown: bool, show: M, hide: M) -> Element<M> {
    let theme = Theme::current();
    let tip = shown.then(|| {
        Element::column([styled(text, theme.small(), theme.text)])
            .padding_xy(theme.cell_width(), theme.cell_height() * 0.25)
            .radius(theme.cell_height() * 0.3)
            .background(theme.highlight.clone())
            .border(1.0, theme.border.clone())
            .popup(0.0, theme.cell_height() * 0.2)
    });
    Element::column([anchor, Element::column(tip).height(Length::Px(0.0))])
        .on_hover(show)
        .on_leave(hide)
}

/// `anchor` with `items` hanging below it while there are any, over a
/// screen-wide catcher that sends `close`.
fn dropdown<M>(
    anchor: Element<M>,
    items: Option<Vec<Element<M>>>,
    close: M,
    stretch: bool,
    theme: &Theme,
) -> Element<M> {
    let Some(items) = items else {
        return anchor;
    };
    let catcher = Element::row([])
        .size(Length::Px(CATCHER * 2.0), Length::Px(CATCHER * 2.0))
        .on_click(close)
        .popup(-CATCHER, -CATCHER);
    let mut list = Element::column(items)
        .padding(theme.cell_height() * 0.3)
        .radius(theme.radius())
        .background(theme.raised.clone())
        .border(1.0, theme.border.clone())
        .opaque()
        .popup(0.0, theme.cell_height() * 0.2);
    if stretch {
        list = list.width(Length::Fill);
    }
    let holder = Element::column([catcher, list])
        .height(Length::Px(0.0))
        .width(Length::Fill);
    let wrapper = Element::column([anchor, holder]);
    if stretch {
        wrapper.width(Length::Fill)
    } else {
        wrapper
    }
}

fn item<M>(text: &str, selected: bool, message: M, theme: &Theme) -> Element<M> {
    let row = Element::row([styled(text, theme.body(), theme.text)])
        .padding_xy(theme.cell_width(), theme.cell_height() * 0.25)
        .radius(theme.cell_height() * 0.3)
        .width(Length::Fill)
        .on_click(message);
    if selected {
        row.background(theme.highlight.clone())
    } else {
        row
    }
}

/// A small downward triangle, drawn from shrinking bars since the bundled
/// font has no arrow glyphs.
fn down_arrow<M>(theme: &Theme) -> Element<M> {
    let unit = (theme.cell_height() * 0.12).max(1.0);
    let bars = [4.0, 3.0, 2.0, 1.0].map(|width| {
        Element::row([])
            .size(Length::Px(width * 2.0 * unit), Length::Px(unit))
            .background(Paint::Solid(theme.muted))
    });
    Element::column(bars).align(Align::Center)
}
