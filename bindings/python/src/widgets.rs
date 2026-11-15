//! The widget functions. Each returns an `Element` drawn with the current
//! theme. A message is any object, handed to the app's `update` as is; a
//! parameter named like `choose` takes a function that makes the message
//! from what was picked.

use std::cell::RefCell;
use std::sync::Arc;
use std::time::Duration;

use katui_widgets as w;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyTuple;

use crate::element::{Node, PyElement, children, children_of, node};
use crate::message::{Msg, obj};
use crate::state::{
    PyEvent, PyListState, PyScrollState, PyTextArea, PyTextField, PyToasts, PyTreeNode,
    PyTreeState, roots,
};

fn element(inner: Node) -> PyElement {
    PyElement::new(inner)
}

fn value(message: &Bound<'_, PyAny>) -> Msg {
    Msg::Value(obj(message))
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
    let failed = RefCell::new(None);
    let built = w::list(&state.inner, |index, selected| {
        match row.call1((index, selected)).and_then(|made| node(&made)) {
            Ok(made) => made,
            Err(error) => {
                failed.borrow_mut().get_or_insert(error);
                katui_ui::Element::spacer()
            }
        }
    });
    match failed.into_inner() {
        Some(error) => Err(error),
        None => Ok(element(built)),
    }
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
    let titles: Vec<&str> = titles.iter().map(String::as_str).collect();
    let make = obj(choose);
    element(w::tabs(&titles, selected, |index| {
        Msg::Index(Arc::clone(&make), index)
    }))
}

#[pyfunction]
pub(crate) fn radio(options: Vec<String>, selected: usize, choose: &Bound<'_, PyAny>) -> PyElement {
    let options: Vec<&str> = options.iter().map(String::as_str).collect();
    let make = obj(choose);
    element(w::radio(&options, selected, |index| {
        Msg::Index(Arc::clone(&make), index)
    }))
}

/// A slider at `value`; `change(value)` makes the message for each new one.
#[pyfunction]
pub(crate) fn slider(value: f32, change: &Bound<'_, PyAny>) -> PyElement {
    let make = obj(change);
    element(w::slider(value, move |at| {
        Msg::Float(Arc::clone(&make), at)
    }))
}

#[pyfunction]
pub(crate) fn nudge(value: f32, event: &PyEvent, step: f32) -> Option<f32> {
    w::nudge(value, &event.inner, step)
}

/// Three dots with one lit, moving every quarter second of `elapsed` seconds.
#[pyfunction]
pub(crate) fn spinner(elapsed: f64) -> PyElement {
    element(w::spinner(Duration::from_secs_f64(elapsed.max(0.0))))
}

#[pyfunction]
pub(crate) fn toasts(toasts: &PyToasts) -> PyElement {
    element(w::toasts(&toasts.inner))
}

#[pyfunction]
pub(crate) fn key_hints(hints: Vec<(String, String)>) -> PyElement {
    let hints: Vec<(&str, &str)> = hints
        .iter()
        .map(|(key, action)| (key.as_str(), action.as_str()))
        .collect();
    element(w::key_hints(&hints))
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
    let options: Vec<&str> = options.iter().map(String::as_str).collect();
    let make = obj(choose);
    element(w::select(
        &options,
        selected,
        open,
        value(toggle),
        |index| Msg::Index(Arc::clone(&make), index),
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
    let items: Vec<&str> = items.iter().map(String::as_str).collect();
    let make = obj(choose);
    element(w::menu(title, &items, open, value(toggle), |index| {
        Msg::Index(Arc::clone(&make), index)
    }))
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
    let make = obj(moved);
    Ok(element(w::scroll_view(
        &state.inner,
        children_of(content)?,
        move |scroll| Msg::Scroll(Arc::clone(&make), scroll),
    )))
}

fn orientation(name: &str) -> PyResult<w::Orientation> {
    match name {
        "horizontal" => Ok(w::Orientation::Horizontal),
        "vertical" => Ok(w::Orientation::Vertical),
        other => Err(PyValueError::new_err(format!(
            "split \"horizontal\" or \"vertical\", not {other:?}"
        ))),
    }
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
        self::orientation(orientation)?,
        move |at| Msg::Float(Arc::clone(&make), at),
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
    let make = obj(pick);
    element(w::table(&columns, &rows, &state.inner, |index| {
        Msg::Index(Arc::clone(&make), index)
    }))
}

#[pyfunction]
pub(crate) fn tree(
    roots: Vec<PyTreeNode>,
    state: &PyTreeState,
    toggle: &Bound<'_, PyAny>,
    select: &Bound<'_, PyAny>,
) -> PyElement {
    let (toggle, select) = (obj(toggle), obj(select));
    element(w::tree(
        &self::roots(&roots),
        &state.inner,
        |id| Msg::Id(Arc::clone(&toggle), id),
        |id| Msg::Id(Arc::clone(&select), id),
    ))
}
