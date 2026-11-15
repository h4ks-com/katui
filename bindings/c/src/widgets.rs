//! The widget functions. Each returns an element drawn with the current
//! theme. A widget sends the key it was given, with a payload when it
//! reports what was picked.

use std::ffi::{c_char, c_void};
use std::time::Duration;

use katui_ui::Element;
use katui_widgets as w;

use crate::element::{KatuiElement, children, element, node};
use crate::ffi::{borrow, borrow_mut, slice, text, texts};
use crate::message::{Msg, Payload, plain};
use crate::state::{
    KatuiEvent, KatuiListState, KatuiScrollState, KatuiTextArea, KatuiTextField, KatuiToasts,
    KatuiTreeNode, KatuiTreeState, roots,
};

fn index(key: u64) -> impl Fn(usize) -> Msg {
    move |index| Msg::User(key, Payload::Index(index))
}

fn float(key: u64) -> impl Fn(f32) -> Msg {
    move |value| Msg::User(key, Payload::Float(value))
}

/// # Safety
/// `element` is NULL or an element from katui that is not used again.
unsafe fn or_spacer(element: *mut KatuiElement) -> crate::element::Node {
    unsafe { node(element) }.unwrap_or_else(Element::spacer)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_label(text: *const c_char) -> *mut KatuiElement {
    element(w::label(unsafe { self::text(text) }.as_ref()))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_title(text: *const c_char) -> *mut KatuiElement {
    element(w::title(unsafe { self::text(text) }.as_ref()))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_heading(text: *const c_char) -> *mut KatuiElement {
    element(w::heading(unsafe { self::text(text) }.as_ref()))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_hint(text: *const c_char) -> *mut KatuiElement {
    element(w::hint(unsafe { self::text(text) }.as_ref()))
}

/// A button that sends `key` when clicked.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_button(text: *const c_char, key: u64) -> *mut KatuiElement {
    element(w::button(&unsafe { self::text(text) }, plain(key)))
}

/// A button in the accent color that sends `key` when clicked.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_primary_button(text: *const c_char, key: u64) -> *mut KatuiElement {
    element(w::primary_button(&unsafe { self::text(text) }, plain(key)))
}

/// A raised card around `count` children. Takes over every child; NULL ones
/// are left out.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_panel(
    children: *const *mut KatuiElement,
    count: usize,
) -> *mut KatuiElement {
    element(w::panel(unsafe { self::children(children, count) }))
}

/// The theme's background filling the screen, around `count` children.
/// Takes over every child; NULL ones are left out.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_page(
    children: *const *mut KatuiElement,
    count: usize,
) -> *mut KatuiElement {
    element(w::page(unsafe { self::children(children, count) }))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_text_field(
    field: *const KatuiTextField,
    placeholder: *const c_char,
    focused: bool,
) -> *mut KatuiElement {
    let placeholder = unsafe { text(placeholder) };
    match unsafe { borrow(field) } {
        Some(field) => element(w::text_field(&field.0, &placeholder, focused)),
        None => element(Element::spacer()),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_text_area(
    area: *const KatuiTextArea,
    placeholder: *const c_char,
    focused: bool,
) -> *mut KatuiElement {
    let placeholder = unsafe { text(placeholder) };
    match unsafe { borrow(area) } {
        Some(area) => element(w::text_area(&area.0, &placeholder, focused)),
        None => element(Element::spacer()),
    }
}

/// Builds the row at `index` of a list, returning an element katui takes
/// over, or NULL for an empty row.
pub type KatuiRow = Option<
    unsafe extern "C" fn(user: *mut c_void, index: usize, selected: bool) -> *mut KatuiElement,
>;

/// The rows of `state` in view, each built by `row(user, index, selected)`
/// during this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_list(
    state: *const KatuiListState,
    row: KatuiRow,
    user: *mut c_void,
) -> *mut KatuiElement {
    let (Some(state), Some(row)) = (unsafe { borrow(state) }, row) else {
        return element(Element::spacer());
    };
    element(w::list(&state.0, |index, selected| unsafe {
        or_spacer(row(user, index, selected))
    }))
}

/// A checkbox that sends `key` when clicked.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_checkbox(
    text: *const c_char,
    checked: bool,
    key: u64,
) -> *mut KatuiElement {
    element(w::checkbox(
        &unsafe { self::text(text) },
        checked,
        plain(key),
    ))
}

/// A switch that sends `key` when clicked.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_switch(
    text: *const c_char,
    on: bool,
    key: u64,
) -> *mut KatuiElement {
    element(w::switch(&unsafe { self::text(text) }, on, plain(key)))
}

/// A bar filled to `fraction`, from 0 to 1.
#[unsafe(no_mangle)]
pub extern "C" fn katui_progress(fraction: f32) -> *mut KatuiElement {
    element(w::progress(fraction))
}

/// A row of `count` tabs; a click sends `key` with the tab's index.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_tabs(
    titles: *const *const c_char,
    count: usize,
    selected: usize,
    key: u64,
) -> *mut KatuiElement {
    let titles = unsafe { texts(titles, count) };
    let titles: Vec<&str> = titles.iter().map(String::as_str).collect();
    element(w::tabs(&titles, selected, index(key)))
}

/// `count` options, one picked; a click sends `key` with the option's index.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_radio(
    options: *const *const c_char,
    count: usize,
    selected: usize,
    key: u64,
) -> *mut KatuiElement {
    let options = unsafe { texts(options, count) };
    let options: Vec<&str> = options.iter().map(String::as_str).collect();
    element(w::radio(&options, selected, index(key)))
}

/// A slider at `value`, from 0 to 1; a drag sends `key` with each new value.
#[unsafe(no_mangle)]
pub extern "C" fn katui_slider(value: f32, key: u64) -> *mut KatuiElement {
    element(w::slider(value, float(key)))
}

/// Write `value` moved by `step` for the arrow keys into `moved`, between 0
/// and 1. False when `event` is not an arrow key.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_nudge(
    value: f32,
    event: *const KatuiEvent,
    step: f32,
    moved: *mut f32,
) -> bool {
    let Some(event) = (unsafe { borrow(event) }) else {
        return false;
    };
    let Some(next) = w::nudge(value, &event.0, step) else {
        return false;
    };
    if let Some(moved) = unsafe { borrow_mut(moved) } {
        *moved = next;
    }
    true
}

/// Three dots with one lit, moving every quarter second of `elapsed`
/// seconds.
#[unsafe(no_mangle)]
pub extern "C" fn katui_spinner(elapsed: f64) -> *mut KatuiElement {
    element(w::spinner(
        Duration::try_from_secs_f64(elapsed).unwrap_or_default(),
    ))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_toasts(toasts: *const KatuiToasts) -> *mut KatuiElement {
    match unsafe { borrow(toasts) } {
        Some(toasts) => element(w::toasts(&toasts.0)),
        None => element(Element::spacer()),
    }
}

/// One key and what it does, for `katui_key_hints`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct KatuiKeyHint {
    pub key: *const c_char,
    pub action: *const c_char,
}

/// A line of `count` keys and what they do.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_key_hints(
    hints: *const KatuiKeyHint,
    count: usize,
) -> *mut KatuiElement {
    let hints: Vec<(String, String)> = unsafe { slice(hints, count) }
        .iter()
        .map(|hint| unsafe { (text(hint.key).into_owned(), text(hint.action).into_owned()) })
        .collect();
    let hints: Vec<(&str, &str)> = hints
        .iter()
        .map(|(key, action)| (key.as_str(), action.as_str()))
        .collect();
    element(w::key_hints(&hints))
}

/// A card over a dimmed screen; a click outside it sends `dismiss_key`.
/// Takes over `body` and every action.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_dialog(
    title: *const c_char,
    body: *mut KatuiElement,
    actions: *const *mut KatuiElement,
    count: usize,
    dismiss_key: u64,
) -> *mut KatuiElement {
    let title = unsafe { text(title) };
    element(w::dialog(
        &title,
        unsafe { or_spacer(body) },
        unsafe { children(actions, count) },
        plain(dismiss_key),
    ))
}

/// A field showing the option at `selected` that opens a list of `count`
/// options. A click on the field sends `toggle_key`, one on an option
/// `choose_key` with its index.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_select(
    options: *const *const c_char,
    count: usize,
    selected: usize,
    open: bool,
    toggle_key: u64,
    choose_key: u64,
) -> *mut KatuiElement {
    let options = unsafe { texts(options, count) };
    let options: Vec<&str> = options.iter().map(String::as_str).collect();
    element(w::select(
        &options,
        selected,
        open,
        plain(toggle_key),
        index(choose_key),
    ))
}

/// A menu title that opens `count` items. A click on the title sends
/// `toggle_key`, one on an item `choose_key` with its index.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_menu(
    title: *const c_char,
    items: *const *const c_char,
    count: usize,
    open: bool,
    toggle_key: u64,
    choose_key: u64,
) -> *mut KatuiElement {
    let title = unsafe { text(title) };
    let items = unsafe { texts(items, count) };
    let items: Vec<&str> = items.iter().map(String::as_str).collect();
    element(w::menu(
        &title,
        &items,
        open,
        plain(toggle_key),
        index(choose_key),
    ))
}

/// `anchor` with `text` beside it while `shown`. The pointer entering sends
/// `show_key`, leaving `hide_key`. Takes over `anchor`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_tooltip(
    anchor: *mut KatuiElement,
    text: *const c_char,
    shown: bool,
    show_key: u64,
    hide_key: u64,
) -> *mut KatuiElement {
    let text = unsafe { self::text(text) };
    element(w::tooltip(
        unsafe { or_spacer(anchor) },
        &text,
        shown,
        plain(show_key),
        plain(hide_key),
    ))
}

/// `count` children in a box that scrolls; each new position sends `key`
/// with a scroll payload. Takes over every child.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_scroll_view(
    state: *const KatuiScrollState,
    content: *const *mut KatuiElement,
    count: usize,
    key: u64,
) -> *mut KatuiElement {
    let content = unsafe { children(content, count) };
    let Some(state) = (unsafe { borrow(state) }) else {
        return element(Element::column(content));
    };
    element(w::scroll_view(&state.0, content, move |scroll| {
        Msg::User(key, Payload::Scroll(scroll))
    }))
}

/// How `katui_split` lays out its panes.
pub type KatuiOrientation = u32;
/// Side by side.
pub const KATUI_ORIENTATION_HORIZONTAL: KatuiOrientation = 0;
/// One above the other.
pub const KATUI_ORIENTATION_VERTICAL: KatuiOrientation = 1;

/// Two panes and a divider, `first` taking `fraction` of the space. A drag
/// on the divider sends `key` with the new fraction. Takes over both panes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_split(
    first: *mut KatuiElement,
    second: *mut KatuiElement,
    fraction: f32,
    orientation: KatuiOrientation,
    key: u64,
) -> *mut KatuiElement {
    let orientation = match orientation {
        KATUI_ORIENTATION_VERTICAL => w::Orientation::Vertical,
        _ => w::Orientation::Horizontal,
    };
    element(w::split(
        unsafe { or_spacer(first) },
        unsafe { or_spacer(second) },
        fraction,
        orientation,
        float(key),
    ))
}

/// A table column: its title and its width in cells, or 0 to fill the rest.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct KatuiColumn {
    pub title: *const c_char,
    pub cells: f32,
}

/// The rows of `state` in view under a header of `column_count` columns.
/// `cells` holds `row_count` rows of `column_count` strings each, row by
/// row. A click on a row sends `key` with its index.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_table(
    columns: *const KatuiColumn,
    column_count: usize,
    cells: *const *const c_char,
    row_count: usize,
    state: *const KatuiListState,
    key: u64,
) -> *mut KatuiElement {
    let titles: Vec<(String, Option<f32>)> = unsafe { slice(columns, column_count) }
        .iter()
        .map(|column| {
            let cells = (column.cells > 0.0).then_some(column.cells);
            (unsafe { text(column.title) }.into_owned(), cells)
        })
        .collect();
    let columns: Vec<w::Column<'_>> = titles
        .iter()
        .map(|(title, cells)| w::Column {
            title,
            cells: *cells,
        })
        .collect();
    let cells = unsafe { texts(cells, column_count.saturating_mul(row_count)) };
    let rows: Vec<Vec<String>> = cells
        .chunks(column_count.max(1))
        .map(<[String]>::to_vec)
        .collect();
    match unsafe { borrow(state) } {
        Some(state) => element(w::table(&columns, &rows, &state.0, index(key))),
        None => element(Element::spacer()),
    }
}

/// The visible nodes of the `count` trees at `roots`, indented by depth. A
/// click on a node's marker sends `toggle_key` with its id, one on the node
/// `select_key` with its id. The nodes stay the caller's.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_tree(
    roots: *const *const KatuiTreeNode,
    count: usize,
    state: *const KatuiTreeState,
    toggle_key: u64,
    select_key: u64,
) -> *mut KatuiElement {
    let roots = unsafe { self::roots(roots, count) };
    match unsafe { borrow(state) } {
        Some(state) => element(w::tree(
            &roots,
            &state.0,
            move |id| Msg::User(toggle_key, Payload::Id(id)),
            move |id| Msg::User(select_key, Payload::Id(id)),
        )),
        None => element(Element::spacer()),
    }
}
