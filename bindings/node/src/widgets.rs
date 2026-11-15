//! The widget functions. Each returns an `Element` drawn with the current
//! theme. A message is any value, handed to the app's `update` as is; a
//! parameter named like `choose` takes a function that makes the message
//! from what was picked.

use katui_bindings::message::{float, id, index, scroll};
use katui_bindings::names;
use katui_bindings::state::duration;
use katui_bindings::widgets::{hints, list, strs};
use katui_widgets as w;
use napi::bindgen_prelude::{Function, Unknown};
use napi::{Env, Result};

use crate::convert::raise;
use crate::element::{Element, Node, children, node};
use crate::event::Event;
use crate::message::{hold, value};
use crate::state::{
    ListState, ScrollState, TextArea, TextField, Toasts, TreeNode, TreeState, roots,
};

fn element(inner: Node) -> Element {
    Element::new(inner)
}

#[napi]
pub fn label(text: String) -> Element {
    element(w::label(text))
}

#[napi]
pub fn title(text: String) -> Element {
    element(w::title(text))
}

#[napi]
pub fn heading(text: String) -> Element {
    element(w::heading(text))
}

#[napi]
pub fn hint(text: String) -> Element {
    element(w::hint(text))
}

#[napi]
pub fn button(env: Env, text: String, message: Unknown<'_>) -> Result<Element> {
    Ok(element(w::button(&text, value(&env, message)?)))
}

#[napi]
pub fn primary_button(env: Env, text: String, message: Unknown<'_>) -> Result<Element> {
    Ok(element(w::primary_button(&text, value(&env, message)?)))
}

#[napi(ts_args_type = "children: Child[]")]
pub fn panel(env: Env, children: Vec<Unknown<'_>>) -> Result<Element> {
    Ok(element(w::panel(self::children(&env, children)?)))
}

#[napi(ts_args_type = "children: Child[]")]
pub fn page(env: Env, children: Vec<Unknown<'_>>) -> Result<Element> {
    Ok(element(w::page(self::children(&env, children)?)))
}

#[napi]
pub fn text_field(
    field: &TextField,
    placeholder: Option<String>,
    focused: Option<bool>,
) -> Element {
    element(w::text_field(
        &field.inner,
        &placeholder.unwrap_or_default(),
        focused.unwrap_or(true),
    ))
}

#[napi]
pub fn text_area(area: &TextArea, placeholder: Option<String>, focused: Option<bool>) -> Element {
    element(w::text_area(
        &area.inner,
        &placeholder.unwrap_or_default(),
        focused.unwrap_or(true),
    ))
}

/// The rows of `state` in view, each built by `row(index, selected)`.
#[napi(ts_args_type = "state: ListState, row: (index: number, selected: boolean) => Element")]
pub fn list_view(
    env: Env,
    state: &ListState,
    row: Function<'_, napi::bindgen_prelude::FnArgs<(u32, bool)>, Unknown<'_>>,
) -> Result<Element> {
    list(&state.inner, |at, selected| {
        let made = row.call((crate::message::index(at), selected).into())?;
        node(&env, made)
    })
    .map(element)
}

#[napi]
pub fn checkbox(env: Env, text: String, checked: bool, message: Unknown<'_>) -> Result<Element> {
    Ok(element(w::checkbox(&text, checked, value(&env, message)?)))
}

#[napi(js_name = "toggleSwitch")]
pub fn switch(env: Env, text: String, on: bool, message: Unknown<'_>) -> Result<Element> {
    Ok(element(w::switch(&text, on, value(&env, message)?)))
}

#[napi]
pub fn progress(fraction: f64) -> Element {
    element(w::progress(fraction as f32))
}

#[napi(ts_args_type = "titles: string[], selected: number, choose: (index: number) => unknown")]
pub fn tabs(env: Env, titles: Vec<String>, selected: u32, choose: Unknown<'_>) -> Result<Element> {
    let make = hold(&env, choose)?;
    Ok(element(w::tabs(
        &strs(&titles),
        selected as usize,
        index(make),
    )))
}

#[napi(ts_args_type = "options: string[], selected: number, choose: (index: number) => unknown")]
pub fn radio(
    env: Env,
    options: Vec<String>,
    selected: u32,
    choose: Unknown<'_>,
) -> Result<Element> {
    let make = hold(&env, choose)?;
    Ok(element(w::radio(
        &strs(&options),
        selected as usize,
        index(make),
    )))
}

/// A slider at `value`; `change(value)` makes the message for each new one.
#[napi(ts_args_type = "value: number, change: (value: number) => unknown")]
pub fn slider(env: Env, value: f64, change: Unknown<'_>) -> Result<Element> {
    let make = hold(&env, change)?;
    Ok(element(w::slider(value as f32, float(make))))
}

#[napi]
pub fn nudge(value: f64, event: &Event, step: f64) -> Option<f64> {
    w::nudge(value as f32, &event.inner, step as f32).map(f64::from)
}

/// Three dots with one lit, moving every quarter second of `elapsed` seconds.
#[napi]
pub fn spinner(elapsed: f64) -> Element {
    element(w::spinner(duration(elapsed)))
}

#[napi]
pub fn toasts(toasts: &Toasts) -> Element {
    element(w::toasts(&toasts.inner))
}

#[napi]
pub fn key_hints(hints: Vec<(String, String)>) -> Element {
    element(w::key_hints(&self::hints(&hints)))
}

#[napi(ts_args_type = "title: string, body: Element, actions: Child[], dismiss: unknown")]
pub fn dialog(
    env: Env,
    title: String,
    body: Unknown<'_>,
    actions: Vec<Unknown<'_>>,
    dismiss: Unknown<'_>,
) -> Result<Element> {
    Ok(element(w::dialog(
        &title,
        node(&env, body)?,
        children(&env, actions)?,
        value(&env, dismiss)?,
    )))
}

#[napi(
    ts_args_type = "options: string[], selected: number, open: boolean, toggle: unknown, choose: (index: number) => unknown"
)]
pub fn select(
    env: Env,
    options: Vec<String>,
    selected: u32,
    open: bool,
    toggle: Unknown<'_>,
    choose: Unknown<'_>,
) -> Result<Element> {
    let make = hold(&env, choose)?;
    Ok(element(w::select(
        &strs(&options),
        selected as usize,
        open,
        value(&env, toggle)?,
        index(make),
    )))
}

#[napi(
    ts_args_type = "title: string, items: string[], open: boolean, toggle: unknown, choose: (index: number) => unknown"
)]
pub fn menu(
    env: Env,
    title: String,
    items: Vec<String>,
    open: bool,
    toggle: Unknown<'_>,
    choose: Unknown<'_>,
) -> Result<Element> {
    let make = hold(&env, choose)?;
    Ok(element(w::menu(
        &title,
        &strs(&items),
        open,
        value(&env, toggle)?,
        index(make),
    )))
}

#[napi(
    ts_args_type = "anchor: Element, text: string, shown: boolean, show: unknown, hide: unknown"
)]
pub fn tooltip(
    env: Env,
    anchor: Unknown<'_>,
    text: String,
    shown: bool,
    show: Unknown<'_>,
    hide: Unknown<'_>,
) -> Result<Element> {
    Ok(element(w::tooltip(
        node(&env, anchor)?,
        &text,
        shown,
        value(&env, show)?,
        value(&env, hide)?,
    )))
}

/// `content` in a box that scrolls; `moved(scroll)` makes the message for
/// each new position.
#[napi(ts_args_type = "state: ScrollState, content: Child[], moved: (scroll: Scroll) => unknown")]
pub fn scroll_view(
    env: Env,
    state: &ScrollState,
    content: Vec<Unknown<'_>>,
    moved: Unknown<'_>,
) -> Result<Element> {
    let make = hold(&env, moved)?;
    Ok(element(w::scroll_view(
        &state.inner,
        children(&env, content)?,
        scroll(make),
    )))
}

/// Two panes and a divider; `resize(fraction)` makes the message as it moves.
#[napi(
    ts_args_type = "first: Element, second: Element, fraction: number, resize: (fraction: number) => unknown, orientation?: 'horizontal' | 'vertical'"
)]
pub fn split(
    env: Env,
    first: Unknown<'_>,
    second: Unknown<'_>,
    fraction: f64,
    resize: Unknown<'_>,
    orientation: Option<String>,
) -> Result<Element> {
    let make = hold(&env, resize)?;
    let orientation =
        names::orientation(orientation.as_deref().unwrap_or("horizontal")).map_err(raise)?;
    Ok(element(w::split(
        node(&env, first)?,
        node(&env, second)?,
        fraction as f32,
        orientation,
        float(make),
    )))
}

/// A table column: its title and its width in cells, or none to fill.
#[napi]
pub struct Column {
    title: String,
    cells: Option<f64>,
}

#[napi]
impl Column {
    #[napi(constructor)]
    pub fn new(title: String, cells: Option<f64>) -> Self {
        Self { title, cells }
    }

    #[napi(getter)]
    pub fn title(&self) -> String {
        self.title.clone()
    }

    #[napi(getter)]
    pub fn cells(&self) -> Option<f64> {
        self.cells
    }
}

#[napi(
    ts_args_type = "columns: Column[], rows: string[][], state: ListState, pick: (index: number) => unknown"
)]
pub fn table(
    env: Env,
    columns: Vec<&Column>,
    rows: Vec<Vec<String>>,
    state: &ListState,
    pick: Unknown<'_>,
) -> Result<Element> {
    let columns: Vec<w::Column<'_>> = columns
        .iter()
        .map(|column| w::Column {
            title: &column.title,
            cells: column.cells.map(|cells| cells as f32),
        })
        .collect();
    let make = hold(&env, pick)?;
    Ok(element(w::table(
        &columns,
        &rows,
        &state.inner,
        index(make),
    )))
}

#[napi(
    ts_args_type = "roots: TreeNode[], state: TreeState, toggle: (id: number) => unknown, select: (id: number) => unknown"
)]
pub fn tree(
    env: Env,
    roots: Vec<&TreeNode>,
    state: &TreeState,
    toggle: Unknown<'_>,
    select: Unknown<'_>,
) -> Result<Element> {
    Ok(element(w::tree(
        &self::roots(&roots),
        &state.inner,
        id(hold(&env, toggle)?),
        id(hold(&env, select)?),
    )))
}
