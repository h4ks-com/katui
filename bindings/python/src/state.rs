//! The state widgets keep between frames: fields, lists, scrolling, trees,
//! focus and toasts, plus the input events that change them.

use std::time::{Duration, Instant};

use katui_proto::input::{Button, InputEvent, Key, Mods, MouseAction};
use katui_ui::Scroll;
use katui_widgets::{
    Edit, Focus, ListState, ScrollState, TextArea, TextField, Toasts, TreeNode, TreeState,
};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

/// A key press, a release, a mouse event, a paste or a focus change.
#[pyclass(name = "Event", module = "katui", frozen)]
pub(crate) struct PyEvent {
    pub(crate) inner: InputEvent,
}

pub(crate) fn key_name(key: Key) -> String {
    match key {
        Key::Char(c) => c.to_string(),
        Key::Enter => "enter".into(),
        Key::Tab => "tab".into(),
        Key::Backspace => "backspace".into(),
        Key::Delete => "delete".into(),
        Key::Insert => "insert".into(),
        Key::Escape => "escape".into(),
        Key::Up => "up".into(),
        Key::Down => "down".into(),
        Key::Left => "left".into(),
        Key::Right => "right".into(),
        Key::Home => "home".into(),
        Key::End => "end".into(),
        Key::PageUp => "page_up".into(),
        Key::PageDown => "page_down".into(),
        Key::Function(n) => format!("f{n}"),
    }
}

/// The key named `name`: one character, `"enter"`, `"up"`, `"f1"` and so on.
pub(crate) fn key(name: &str) -> PyResult<Key> {
    let mut chars = name.chars();
    if let (Some(c), None) = (chars.next(), chars.next()) {
        return Ok(Key::Char(c));
    }
    let named = match name {
        "enter" => Key::Enter,
        "tab" => Key::Tab,
        "backspace" => Key::Backspace,
        "delete" => Key::Delete,
        "insert" => Key::Insert,
        "escape" => Key::Escape,
        "space" => Key::Char(' '),
        "up" => Key::Up,
        "down" => Key::Down,
        "left" => Key::Left,
        "right" => Key::Right,
        "home" => Key::Home,
        "end" => Key::End,
        "page_up" => Key::PageUp,
        "page_down" => Key::PageDown,
        other => {
            return other
                .strip_prefix('f')
                .and_then(|n| n.parse().ok())
                .map(Key::Function)
                .ok_or_else(|| PyValueError::new_err(format!("name a key, not {other:?}")));
        }
    };
    Ok(named)
}

const fn mods(ctrl: bool, alt: bool, shift: bool) -> Mods {
    Mods { shift, alt, ctrl }
}

fn button_name(button: Button) -> &'static str {
    match button {
        Button::Left => "left",
        Button::Middle => "middle",
        Button::Right => "right",
    }
}

#[pymethods]
impl PyEvent {
    /// A press of the key `name`, for tests and for handing keys to widgets.
    #[staticmethod]
    #[pyo3(signature = (name, ctrl=false, alt=false, shift=false))]
    #[allow(clippy::fn_params_excessive_bools)]
    fn press(name: &str, ctrl: bool, alt: bool, shift: bool) -> PyResult<Self> {
        Ok(Self {
            inner: InputEvent::Key(key(name)?, mods(ctrl, alt, shift)),
        })
    }

    #[staticmethod]
    fn paste(text: String) -> Self {
        Self {
            inner: InputEvent::Paste(text),
        }
    }

    /// `"key"`, `"key_up"`, `"mouse"`, `"paste"`, `"focus"` or `"other"`.
    #[getter]
    fn kind(&self) -> &'static str {
        match self.inner {
            InputEvent::Key(..) => "key",
            InputEvent::KeyUp(..) => "key_up",
            InputEvent::Mouse(_) => "mouse",
            InputEvent::Paste(_) => "paste",
            InputEvent::Focus(_) => "focus",
            _ => "other",
        }
    }

    /// The key's name: one character, `"enter"`, `"up"`, `"f1"` and so on.
    #[getter(key)]
    fn key_of(&self) -> Option<String> {
        match self.inner {
            InputEvent::Key(key, _) | InputEvent::KeyUp(key, _) => Some(key_name(key)),
            _ => None,
        }
    }

    #[getter]
    fn ctrl(&self) -> bool {
        self.mods().ctrl
    }

    #[getter]
    fn alt(&self) -> bool {
        self.mods().alt
    }

    #[getter]
    fn shift(&self) -> bool {
        self.mods().shift
    }

    /// The pasted text.
    #[getter]
    fn text(&self) -> Option<String> {
        match &self.inner {
            InputEvent::Paste(text) => Some(text.clone()),
            _ => None,
        }
    }

    /// Whether the window has the focus, for a focus event.
    #[getter]
    fn focused(&self) -> Option<bool> {
        match self.inner {
            InputEvent::Focus(focused) => Some(focused),
            _ => None,
        }
    }

    /// The cell under the pointer, `(column, row)`, for a mouse event.
    #[getter]
    fn position(&self) -> Option<(u32, u32)> {
        match &self.inner {
            InputEvent::Mouse(mouse) => Some((mouse.x, mouse.y)),
            _ => None,
        }
    }

    /// `"press"`, `"release"`, `"drag"`, `"move"`, `"wheel_up"` or
    /// `"wheel_down"`, for a mouse event.
    #[getter]
    fn action(&self) -> Option<&'static str> {
        match &self.inner {
            InputEvent::Mouse(mouse) => Some(match mouse.action {
                MouseAction::Press(_) => "press",
                MouseAction::Release(_) => "release",
                MouseAction::Drag(_) => "drag",
                MouseAction::Move => "move",
                MouseAction::WheelUp => "wheel_up",
                MouseAction::WheelDown => "wheel_down",
            }),
            _ => None,
        }
    }

    /// `"left"`, `"middle"` or `"right"`, for a press, release or drag.
    #[getter]
    fn button(&self) -> Option<&'static str> {
        match &self.inner {
            InputEvent::Mouse(mouse) => match mouse.action {
                MouseAction::Press(button)
                | MouseAction::Release(button)
                | MouseAction::Drag(button) => Some(button_name(button)),
                _ => None,
            },
            _ => None,
        }
    }

    fn __repr__(&self) -> String {
        format!("Event({:?})", self.inner)
    }
}

impl PyEvent {
    fn mods(&self) -> Mods {
        match &self.inner {
            InputEvent::Key(_, mods) | InputEvent::KeyUp(_, mods) => *mods,
            InputEvent::Mouse(mouse) => mouse.mods,
            _ => Mods::NONE,
        }
    }
}

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
    controls: Vec<Py<PyAny>>,
    inner: Focus<usize>,
}

impl PyFocus {
    fn position(&self, control: &Bound<'_, PyAny>) -> PyResult<Option<usize>> {
        for (index, known) in self.controls.iter().enumerate() {
            if known.bind(control.py()).eq(control)? {
                return Ok(Some(index));
            }
        }
        Ok(None)
    }
}

#[pymethods]
impl PyFocus {
    /// Focus over `controls`, the first one focused.
    #[new]
    fn new(controls: Vec<Py<PyAny>>) -> PyResult<Self> {
        if controls.is_empty() {
            return Err(PyValueError::new_err("give Focus at least one control"));
        }
        let inner = Focus::new(0, 1..controls.len());
        Ok(Self { controls, inner })
    }

    fn current(&self, py: Python<'_>) -> Option<Py<PyAny>> {
        self.controls
            .get(self.inner.current())
            .map(|control| control.clone_ref(py))
    }

    #[pyo3(name = "is_")]
    fn is(&self, control: &Bound<'_, PyAny>) -> PyResult<bool> {
        Ok(self.position(control)? == Some(self.inner.current()))
    }

    fn set(&mut self, control: &Bound<'_, PyAny>) -> PyResult<bool> {
        Ok(match self.position(control)? {
            Some(index) => self.inner.set(index),
            None => false,
        })
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
        match seconds {
            Some(seconds) => {
                self.inner.push_for(
                    text,
                    Instant::now(),
                    Duration::from_secs_f64(seconds.max(0.0)),
                );
            }
            None => self.inner.push(text, Instant::now()),
        }
    }

    /// Drop the toasts whose time is up. Returns whether any left.
    fn expire(&mut self) -> bool {
        self.inner.expire(Instant::now())
    }

    /// Seconds until the next toast leaves.
    fn next_expiry(&self) -> Option<f64> {
        self.inner
            .next_expiry()
            .map(|at| at.saturating_duration_since(Instant::now()).as_secs_f64())
    }

    fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    fn texts(&self) -> Vec<String> {
        self.inner.texts().map(str::to_owned).collect()
    }
}
