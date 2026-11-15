//! The state widgets keep between frames: fields, lists, scrolling, trees,
//! focus and toasts.

use std::time::Instant;

use katui_bindings::state::{Focus, next_expiry, push_toast};
use katui_ui::Scroll;
use katui_widgets::{
    Edit, ListState, ScrollState, TextArea, TextField, Toasts, TreeNode, TreeState,
};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

use crate::event::PyEvent;

/// What an event did to a text field.
#[pyclass(name = "Edit", module = "katui", eq, eq_int, frozen)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum PyEdit {
    #[pyo3(name = "CHANGED")]
    Changed,
    #[pyo3(name = "MOVED")]
    Moved,
    #[pyo3(name = "SUBMITTED")]
    Submitted,
    #[pyo3(name = "IGNORED")]
    Ignored,
}

impl From<Edit> for PyEdit {
    fn from(edit: Edit) -> Self {
        match edit {
            Edit::Changed => Self::Changed,
            Edit::Moved => Self::Moved,
            Edit::Submitted => Self::Submitted,
            Edit::Ignored => Self::Ignored,
        }
    }
}

/// Where a scrolling element stands.
#[pyclass(name = "Scroll", module = "katui", frozen, get_all)]
#[derive(Clone, Copy)]
pub(crate) struct PyScroll {
    pub(crate) offset: f32,
    pub(crate) max: f32,
}

impl From<Scroll> for PyScroll {
    fn from(scroll: Scroll) -> Self {
        Self {
            offset: scroll.offset,
            max: scroll.max,
        }
    }
}

#[pymethods]
impl PyScroll {
    fn __repr__(&self) -> String {
        format!("Scroll(offset={}, max={})", self.offset, self.max)
    }
}

/// A one-line text input's text and cursor.
#[pyclass(name = "TextField", module = "katui")]
#[derive(Default)]
pub(crate) struct PyTextField {
    pub(crate) inner: TextField,
}

#[pymethods]
impl PyTextField {
    #[new]
    #[pyo3(signature = (text=String::new()))]
    fn new(text: String) -> Self {
        Self {
            inner: TextField::with_text(text),
        }
    }

    #[getter]
    fn text(&self) -> String {
        self.inner.text().to_owned()
    }

    #[setter]
    fn set_text(&mut self, text: String) {
        self.inner.set_text(text);
    }

    /// The text before and after the cursor.
    fn split(&self) -> (String, String) {
        let (before, after) = self.inner.split();
        (before.to_owned(), after.to_owned())
    }

    /// Empty the field and return what it held.
    fn take(&mut self) -> String {
        self.inner.take()
    }

    fn handle(&mut self, event: &PyEvent) -> PyEdit {
        self.inner.handle(&event.inner).into()
    }
}

/// A multi-line text input that scrolls to keep its cursor in view.
#[pyclass(name = "TextArea", module = "katui")]
pub(crate) struct PyTextArea {
    pub(crate) inner: TextArea,
}

#[pymethods]
impl PyTextArea {
    #[new]
    #[pyo3(signature = (rows=4))]
    fn new(rows: usize) -> Self {
        Self {
            inner: TextArea::new(rows),
        }
    }

    #[getter]
    fn text(&self) -> String {
        self.inner.text().to_owned()
    }

    #[setter]
    fn set_text(&mut self, text: String) {
        self.inner.set_text(text);
    }

    fn take(&mut self) -> String {
        self.inner.take()
    }

    #[getter]
    fn rows(&self) -> usize {
        self.inner.rows()
    }

    #[getter]
    fn cursor_line(&self) -> usize {
        self.inner.cursor_line()
    }

    fn handle(&mut self, event: &PyEvent) -> PyEdit {
        self.inner.handle(&event.inner).into()
    }
}

/// The selected row of a list and the rows in view.
#[pyclass(name = "ListState", module = "katui")]
pub(crate) struct PyListState {
    pub(crate) inner: ListState,
}

#[pymethods]
impl PyListState {
    #[new]
    #[pyo3(signature = (length=0, visible=10))]
    fn new(length: usize, visible: usize) -> Self {
        Self {
            inner: ListState::new(length, visible),
        }
    }

    fn resize(&mut self, length: usize, visible: usize) {
        self.inner.resize(length, visible);
    }

    #[getter]
    fn selected(&self) -> usize {
        self.inner.selected()
    }

    fn select(&mut self, row: usize) {
        self.inner.select(row);
    }

    /// The rows in view, as a `range`.
    fn in_view(&self) -> (usize, usize) {
        let rows = self.inner.in_view();
        (rows.start, rows.end)
    }

    fn handle(&mut self, event: &PyEvent) -> bool {
        self.inner.handle(&event.inner)
    }
}

/// How far a scroll view is scrolled.
#[pyclass(name = "ScrollState", module = "katui")]
#[derive(Default)]
pub(crate) struct PyScrollState {
    pub(crate) inner: ScrollState,
}

#[pymethods]
impl PyScrollState {
    #[new]
    fn new() -> Self {
        Self::default()
    }

    #[getter]
    fn offset(&self) -> f32 {
        self.inner.offset()
    }

    #[getter]
    fn max(&self) -> f32 {
        self.inner.max()
    }

    /// Take the position a scroll view reported.
    fn apply(&mut self, scroll: &PyScroll) {
        self.inner.apply(Scroll {
            offset: scroll.offset,
            max: scroll.max,
        });
    }

    fn scroll_by(&mut self, pixels: f32) {
        self.inner.scroll_by(pixels);
    }

    /// Move with the arrow, page, home and end keys, `line` pixels a step.
    #[pyo3(signature = (event, line=20.0))]
    fn handle(&mut self, event: &PyEvent, line: f32) -> bool {
        self.inner.handle(&event.inner, line)
    }
}

/// One node of a tree.
#[pyclass(name = "TreeNode", module = "katui", frozen, from_py_object)]
#[derive(Clone)]
pub(crate) struct PyTreeNode {
    pub(crate) inner: TreeNode,
}

#[pymethods]
impl PyTreeNode {
    /// A node with `children` under it, or a leaf without any.
    #[new]
    #[pyo3(signature = (id, text, children=Vec::new()))]
    fn new(id: u64, text: String, children: Vec<PyTreeNode>) -> Self {
        let children: Vec<TreeNode> = children.into_iter().map(|child| child.inner).collect();
        Self {
            inner: if children.is_empty() {
                TreeNode::leaf(id, text)
            } else {
                TreeNode::branch(id, text, children)
            },
        }
    }

    #[getter]
    fn id(&self) -> u64 {
        self.inner.id
    }

    #[getter]
    fn text(&self) -> String {
        self.inner.text.clone()
    }
}

pub(crate) fn roots(nodes: &[PyTreeNode]) -> Vec<TreeNode> {
    nodes.iter().map(|node| node.inner.clone()).collect()
}

/// Which tree nodes are open and which one is selected.
#[pyclass(name = "TreeState", module = "katui")]
#[derive(Default)]
pub(crate) struct PyTreeState {
    pub(crate) inner: TreeState,
}

#[pymethods]
impl PyTreeState {
    #[new]
    fn new() -> Self {
        Self::default()
    }

    #[getter]
    fn selected(&self) -> Option<u64> {
        self.inner.selected()
    }

    fn select(&mut self, id: u64) {
        self.inner.select(id);
    }

    fn is_open(&self, id: u64) -> bool {
        self.inner.is_open(id)
    }

    fn toggle(&mut self, id: u64) {
        self.inner.toggle(id);
    }

    fn handle(&mut self, event: &PyEvent, roots: Vec<PyTreeNode>) -> bool {
        self.inner.handle(&event.inner, &self::roots(&roots))
    }
}

/// Which of several controls has the keyboard; Tab and Shift+Tab move it.
#[pyclass(name = "Focus", module = "katui")]
pub(crate) struct PyFocus {
    inner: Focus<Py<PyAny>>,
}

#[pymethods]
impl PyFocus {
    /// Focus over `controls`, the first one focused.
    #[new]
    fn new(controls: Vec<Py<PyAny>>) -> PyResult<Self> {
        let inner = Focus::new(controls)
            .ok_or_else(|| PyValueError::new_err("give Focus at least one control"))?;
        Ok(Self { inner })
    }

    fn current(&self, py: Python<'_>) -> Option<Py<PyAny>> {
        self.inner.current().map(|control| control.clone_ref(py))
    }

    #[pyo3(name = "is_")]
    fn is(&self, control: &Bound<'_, PyAny>) -> PyResult<bool> {
        self.inner.is(|known| known.bind(control.py()).eq(control))
    }

    fn set(&mut self, control: &Bound<'_, PyAny>) -> PyResult<bool> {
        self.inner.set(|known| known.bind(control.py()).eq(control))
    }

    fn next(&mut self) {
        self.inner.next();
    }

    fn previous(&mut self) {
        self.inner.previous();
    }

    fn handle(&mut self, event: &PyEvent) -> bool {
        self.inner.handle(&event.inner)
    }
}

/// Short messages that go away on their own.
#[pyclass(name = "Toasts", module = "katui")]
#[derive(Default)]
pub(crate) struct PyToasts {
    pub(crate) inner: Toasts,
}

#[pymethods]
impl PyToasts {
    #[new]
    fn new() -> Self {
        Self::default()
    }

    /// Show `text`, for `seconds` or the default time.
    #[pyo3(signature = (text, seconds=None))]
    fn push(&mut self, text: String, seconds: Option<f64>) {
        push_toast(&mut self.inner, text, seconds);
    }

    /// Drop the toasts whose time is up. Returns whether any left.
    fn expire(&mut self) -> bool {
        self.inner.expire(Instant::now())
    }

    /// Seconds until the next toast leaves.
    fn next_expiry(&self) -> Option<f64> {
        next_expiry(&self.inner)
    }

    fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    fn texts(&self) -> Vec<String> {
        self.inner.texts().map(str::to_owned).collect()
    }
}
