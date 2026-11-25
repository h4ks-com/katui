//! The widget functions. Each returns an `Element` drawn with the current
//! theme. A message is any object, handed to the app's `update` as is; a
//! parameter named like `choose` takes a function that makes the message
//! from what was picked.

use katui_bindings::message::{float, id, index, scroll};
use katui_bindings::names;
use katui_bindings::state::duration;
use katui_bindings::widgets::{hints, list, strs};
use katui_widgets as w;
use pyo3::prelude::*;
use pyo3::types::PyTuple;

use crate::convert::raise;
use crate::element::{Node, PyElement, children, children_of, node};
use crate::event::PyEvent;
use crate::message::{obj, value};
use crate::state::{
    PyListState, PyScrollState, PyTextArea, PyTextField, PyToasts, PyTreeNode, PyTreeState, roots,
};

fn element(inner: Node) -> PyElement {
    PyElement::new(inner)
}

#[pyfunction]
pub(crate) fn label(text: &str) -> PyElement {
    element(w::label(text))
}

#[pyfunction]
pub(crate) fn title(text: &str) -> PyElement {
    element(w::title(text))
}

#[pyfunction]
pub(crate) fn heading(text: &str) -> PyElement {
    element(w::heading(text))
}

#[pyfunction]
pub(crate) fn hint(text: &str) -> PyElement {
    element(w::hint(text))
}

#[pyfunction]
pub(crate) fn button(text: &str, message: &Bound<'_, PyAny>) -> PyElement {
    element(w::button(text, value(message)))
}

#[pyfunction]
pub(crate) fn focusable_button(text: &str, message: &Bound<'_, PyAny>, focused: bool) -> PyElement {
    element(w::focusable_button(text, value(message), focused))
}

#[pyfunction]
pub(crate) fn primary_button(text: &str, message: &Bound<'_, PyAny>) -> PyElement {
    element(w::primary_button(text, value(message)))
}

#[pyfunction]
#[pyo3(signature = (*children))]
pub(crate) fn panel(children: &Bound<'_, PyTuple>) -> PyResult<PyElement> {
    Ok(element(w::panel(self::children(children)?)))
}

#[pyfunction]
#[pyo3(signature = (*children))]
pub(crate) fn page(children: &Bound<'_, PyTuple>) -> PyResult<PyElement> {
    Ok(element(w::page(self::children(children)?)))
}

#[pyfunction]
#[pyo3(signature = (field, placeholder="", focused=true))]
pub(crate) fn text_field(field: &PyTextField, placeholder: &str, focused: bool) -> PyElement {
    element(w::text_field(&field.inner, placeholder, focused))
}

#[pyfunction]
#[pyo3(signature = (area, placeholder="", focused=true))]
pub(crate) fn text_area(area: &PyTextArea, placeholder: &str, focused: bool) -> PyElement {
    element(w::text_area(&area.inner, placeholder, focused))
}

/// The rows of `state` in view, each built by `row(index, selected)`.
#[pyfunction]
pub(crate) fn list_view(state: &PyListState, row: &Bound<'_, PyAny>) -> PyResult<PyElement> {
    list(&state.inner, |index, selected| {
        node(&row.call1((index, selected))?)
    })
    .map(element)
}

#[pyfunction]
pub(crate) fn checkbox(text: &str, checked: bool, message: &Bound<'_, PyAny>) -> PyElement {
    element(w::checkbox(text, checked, value(message)))
}

#[pyfunction]
pub(crate) fn switch(text: &str, on: bool, message: &Bound<'_, PyAny>) -> PyElement {
    element(w::switch(text, on, value(message)))
}

#[pyfunction]
pub(crate) fn progress(fraction: f32) -> PyElement {
    element(w::progress(fraction))
}

#[pyfunction]
pub(crate) fn tabs(titles: Vec<String>, selected: usize, choose: &Bound<'_, PyAny>) -> PyElement {
    element(w::tabs(&strs(&titles), selected, index(obj(choose))))
}

#[pyfunction]
pub(crate) fn radio(options: Vec<String>, selected: usize, choose: &Bound<'_, PyAny>) -> PyElement {
    element(w::radio(&strs(&options), selected, index(obj(choose))))
}

/// A slider at `value`; `change(value)` makes the message for each new one.
#[pyfunction]
pub(crate) fn slider(value: f32, change: &Bound<'_, PyAny>) -> PyElement {
    element(w::slider(value, float(obj(change))))
}

#[pyfunction]
pub(crate) fn nudge(value: f32, event: &PyEvent, step: f32) -> Option<f32> {
    w::nudge(value, &event.inner, step)
}

/// Three dots with one lit, moving every quarter second of `elapsed` seconds.
#[pyfunction]
pub(crate) fn spinner(elapsed: f64) -> PyElement {
    element(w::spinner(duration(elapsed)))
}

#[pyfunction]
pub(crate) fn toasts(toasts: &PyToasts) -> PyElement {
    element(w::toasts(&toasts.inner))
}

#[pyfunction]
pub(crate) fn key_hints(hints: Vec<(String, String)>) -> PyElement {
    element(w::key_hints(&self::hints(&hints)))
}

#[pyfunction]
pub(crate) fn dialog(
    title: &str,
    body: &Bound<'_, PyAny>,
    actions: &Bound<'_, PyAny>,
    dismiss: &Bound<'_, PyAny>,
) -> PyResult<PyElement> {
    Ok(element(w::dialog(
        title,
        node(body)?,
        children_of(actions)?,
        value(dismiss),
    )))
}

#[pyfunction]
pub(crate) fn select(
    options: Vec<String>,
    selected: usize,
    open: bool,
    toggle: &Bound<'_, PyAny>,
    choose: &Bound<'_, PyAny>,
) -> PyElement {
    element(w::select(
        &strs(&options),
        selected,
        open,
        value(toggle),
        index(obj(choose)),
    ))
}

#[pyfunction]
pub(crate) fn menu(
    title: &str,
    items: Vec<String>,
    open: bool,
    toggle: &Bound<'_, PyAny>,
    choose: &Bound<'_, PyAny>,
) -> PyElement {
    element(w::menu(
        title,
        &strs(&items),
        open,
        value(toggle),
        index(obj(choose)),
    ))
}

#[pyfunction]
pub(crate) fn tooltip(
    anchor: &Bound<'_, PyAny>,
    text: &str,
    shown: bool,
    show: &Bound<'_, PyAny>,
    hide: &Bound<'_, PyAny>,
) -> PyResult<PyElement> {
    Ok(element(w::tooltip(
        node(anchor)?,
        text,
        shown,
        value(show),
        value(hide),
    )))
}

/// `content` in a box that scrolls; `moved(scroll)` makes the message for
/// each new position.
#[pyfunction]
pub(crate) fn scroll_view(
    state: &PyScrollState,
    content: &Bound<'_, PyAny>,
    moved: &Bound<'_, PyAny>,
) -> PyResult<PyElement> {
    Ok(element(w::scroll_view(
        &state.inner,
        children_of(content)?,
        scroll(obj(moved)),
    )))
}

/// Two panes and a divider; `resize(fraction)` makes the message as it moves.
#[pyfunction]
#[pyo3(signature = (first, second, fraction, resize, orientation="horizontal"))]
pub(crate) fn split(
    first: &Bound<'_, PyAny>,
    second: &Bound<'_, PyAny>,
    fraction: f32,
    resize: &Bound<'_, PyAny>,
    orientation: &str,
) -> PyResult<PyElement> {
    let make = obj(resize);
    Ok(element(w::split(
        node(first)?,
        node(second)?,
        fraction,
        names::orientation(orientation).map_err(raise)?,
        float(make),
    )))
}

/// A table column: its title and its width in cells, or `None` to fill.
#[pyclass(name = "Column", module = "katui", frozen, get_all, from_py_object)]
#[derive(Clone)]
pub(crate) struct PyColumn {
    title: String,
    cells: Option<f32>,
}

#[pymethods]
impl PyColumn {
    #[new]
    #[pyo3(signature = (title, cells=None))]
    fn new(title: String, cells: Option<f32>) -> Self {
        Self { title, cells }
    }
}

#[pyfunction]
pub(crate) fn table(
    columns: Vec<PyColumn>,
    rows: Vec<Vec<String>>,
    state: &PyListState,
    pick: &Bound<'_, PyAny>,
) -> PyElement {
    let columns: Vec<w::Column<'_>> = columns
        .iter()
        .map(|column| w::Column {
            title: &column.title,
            cells: column.cells,
        })
        .collect();
    element(w::table(&columns, &rows, &state.inner, index(obj(pick))))
}

#[pyfunction]
pub(crate) fn tree(
    roots: Vec<PyTreeNode>,
    state: &PyTreeState,
    toggle: &Bound<'_, PyAny>,
    select: &Bound<'_, PyAny>,
) -> PyElement {
    element(w::tree(
        &self::roots(&roots),
        &state.inner,
        id(obj(toggle)),
        id(obj(select)),
    ))
}
