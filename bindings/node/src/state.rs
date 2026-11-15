//! The state widgets keep between frames: fields, lists, scrolling, trees,
//! focus and toasts.

use std::time::Instant;

use katui_bindings::state::{Focus as UiFocus, next_expiry, push_toast};
use katui_ui::Scroll;
use katui_widgets::{
    Edit as UiEdit, ListState as UiListState, ScrollState as UiScrollState, TextArea as UiTextArea,
    TextField as UiTextField, Toasts as UiToasts, TreeNode as UiTreeNode, TreeState as UiTreeState,
};
use napi::bindgen_prelude::Unknown;
use napi::{Env, Result};

use crate::convert::invalid;
use crate::event::Event;
use crate::message::{Obj, hold, unknown};

/// What an event did to a text field.
#[napi]
pub enum Edit {
    Changed,
    Moved,
    Submitted,
    Ignored,
}

impl From<UiEdit> for Edit {
    fn from(edit: UiEdit) -> Self {
        match edit {
            UiEdit::Changed => Self::Changed,
            UiEdit::Moved => Self::Moved,
            UiEdit::Submitted => Self::Submitted,
            UiEdit::Ignored => Self::Ignored,
        }
    }
}

/// Where a scrolling element stands.
#[napi(object, js_name = "Scroll")]
#[derive(Clone, Copy)]
pub struct ScrollPosition {
    pub offset: f64,
    pub max: f64,
}

impl From<Scroll> for ScrollPosition {
    fn from(scroll: Scroll) -> Self {
        Self {
            offset: f64::from(scroll.offset),
            max: f64::from(scroll.max),
        }
    }
}

/// A one-line text input's text and cursor.
#[napi]
pub struct TextField {
    pub(crate) inner: UiTextField,
}

#[napi]
impl TextField {
    #[napi(constructor)]
    pub fn new(text: Option<String>) -> Self {
        Self {
            inner: UiTextField::with_text(text.unwrap_or_default()),
        }
    }

    #[napi(getter)]
    pub fn text(&self) -> String {
        self.inner.text().to_owned()
    }

    #[napi(setter, js_name = "text")]
    pub fn set_text(&mut self, text: String) {
        self.inner.set_text(text);
    }

    /// The text before and after the cursor.
    #[napi]
    pub fn split(&self) -> (String, String) {
        let (before, after) = self.inner.split();
        (before.to_owned(), after.to_owned())
    }

    /// Empty the field and return what it held.
    #[napi]
    pub fn take(&mut self) -> String {
        self.inner.take()
    }

    #[napi]
    pub fn handle(&mut self, event: &Event) -> Edit {
        self.inner.handle(&event.inner).into()
    }
}

/// A multi-line text input that scrolls to keep its cursor in view.
#[napi]
pub struct TextArea {
    pub(crate) inner: UiTextArea,
}

#[napi]
impl TextArea {
    #[napi(constructor)]
    pub fn new(rows: Option<u32>) -> Self {
        Self {
            inner: UiTextArea::new(rows.unwrap_or(4) as usize),
        }
    }

    #[napi(getter)]
    pub fn text(&self) -> String {
        self.inner.text().to_owned()
    }

    #[napi(setter, js_name = "text")]
    pub fn set_text(&mut self, text: String) {
        self.inner.set_text(text);
    }

    #[napi]
    pub fn take(&mut self) -> String {
        self.inner.take()
    }

    #[napi(getter)]
    pub fn rows(&self) -> u32 {
        self.inner.rows() as u32
    }

    #[napi(getter)]
    pub fn cursor_line(&self) -> u32 {
        self.inner.cursor_line() as u32
    }

    #[napi]
    pub fn handle(&mut self, event: &Event) -> Edit {
        self.inner.handle(&event.inner).into()
    }
}

/// The selected row of a list and the rows in view.
#[napi]
pub struct ListState {
    pub(crate) inner: UiListState,
}

#[napi]
impl ListState {
    #[napi(constructor)]
    pub fn new(length: Option<u32>, visible: Option<u32>) -> Self {
        Self {
            inner: UiListState::new(length.unwrap_or(0) as usize, visible.unwrap_or(10) as usize),
        }
    }

    #[napi]
    pub fn resize(&mut self, length: u32, visible: u32) {
        self.inner.resize(length as usize, visible as usize);
    }

    #[napi(getter)]
    pub fn selected(&self) -> u32 {
        self.inner.selected() as u32
    }

    #[napi]
    pub fn select(&mut self, row: u32) {
        self.inner.select(row as usize);
    }

    /// The rows in view, as `[start, end)`.
    #[napi]
    pub fn in_view(&self) -> (u32, u32) {
        let rows = self.inner.in_view();
        (rows.start as u32, rows.end as u32)
    }

    #[napi]
    pub fn handle(&mut self, event: &Event) -> bool {
        self.inner.handle(&event.inner)
    }
}

/// How far a scroll view is scrolled.
#[napi]
#[derive(Default)]
pub struct ScrollState {
    pub(crate) inner: UiScrollState,
}

#[napi]
impl ScrollState {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self::default()
    }

    #[napi(getter)]
    pub fn offset(&self) -> f64 {
        f64::from(self.inner.offset())
    }

    #[napi(getter)]
    pub fn max(&self) -> f64 {
        f64::from(self.inner.max())
    }

    /// Take the position a scroll view reported.
    #[napi]
    pub fn apply(&mut self, scroll: ScrollPosition) {
        self.inner.apply(Scroll {
            offset: scroll.offset as f32,
            max: scroll.max as f32,
        });
    }

    #[napi]
    pub fn scroll_by(&mut self, pixels: f64) {
        self.inner.scroll_by(pixels as f32);
    }

    /// Move with the arrow, page, home and end keys, `line` pixels a step.
    #[napi]
    pub fn handle(&mut self, event: &Event, line: Option<f64>) -> bool {
        self.inner.handle(&event.inner, line.unwrap_or(20.0) as f32)
    }
}

/// One node of a tree.
#[napi]
#[derive(Clone)]
pub struct TreeNode {
    pub(crate) inner: UiTreeNode,
}

#[napi]
impl TreeNode {
    /// A node with `children` under it, or a leaf without any.
    #[napi(constructor)]
    pub fn new(id: u32, text: String, children: Option<Vec<&TreeNode>>) -> Self {
        let children: Vec<UiTreeNode> = children
            .unwrap_or_default()
            .into_iter()
            .map(|child| child.inner.clone())
            .collect();
        Self {
            inner: if children.is_empty() {
                UiTreeNode::leaf(u64::from(id), text)
            } else {
                UiTreeNode::branch(u64::from(id), text, children)
            },
        }
    }

    #[napi(getter)]
    pub fn id(&self) -> u32 {
        self.inner.id as u32
    }

    #[napi(getter)]
    pub fn text(&self) -> String {
        self.inner.text.clone()
    }
}

pub(crate) fn roots(nodes: &[&TreeNode]) -> Vec<UiTreeNode> {
    nodes.iter().map(|node| node.inner.clone()).collect()
}

/// Which tree nodes are open and which one is selected.
#[napi]
#[derive(Default)]
pub struct TreeState {
    pub(crate) inner: UiTreeState,
}

#[napi]
impl TreeState {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self::default()
    }

    #[napi(getter)]
    pub fn selected(&self) -> Option<u32> {
        self.inner.selected().map(|id| id as u32)
    }

    #[napi]
    pub fn select(&mut self, id: u32) {
        self.inner.select(u64::from(id));
    }

    #[napi]
    pub fn is_open(&self, id: u32) -> bool {
        self.inner.is_open(u64::from(id))
    }

    #[napi]
    pub fn toggle(&mut self, id: u32) {
        self.inner.toggle(u64::from(id));
    }

    #[napi]
    pub fn handle(&mut self, event: &Event, roots: Vec<&TreeNode>) -> bool {
        self.inner.handle(&event.inner, &self::roots(&roots))
    }
}

/// Which of several controls has the keyboard; Tab and Shift+Tab move it.
#[napi]
pub struct Focus {
    inner: UiFocus<Obj>,
}

const NO_CONTROLS: &str = "give Focus at least one control";

#[napi]
impl Focus {
    /// Focus over `controls`, the first one focused.
    #[napi(constructor)]
    pub fn new(env: Env, controls: Vec<Unknown<'_>>) -> Result<Self> {
        let controls = controls
            .into_iter()
            .map(|control| hold(&env, control))
            .collect::<Result<Vec<Obj>>>()?;
        let inner = UiFocus::new(controls).ok_or_else(|| invalid(NO_CONTROLS))?;
        Ok(Self { inner })
    }

    #[napi]
    pub fn current<'env>(&self, env: &'env Env) -> Result<Unknown<'env>> {
        match self.inner.current() {
            Some(control) => unknown(env, control),
            None => Err(invalid(NO_CONTROLS)),
        }
    }

    #[napi]
    pub fn is(&self, env: Env, control: Unknown<'_>) -> Result<bool> {
        self.inner
            .is(|known| env.strict_equals(unknown(&env, known)?, control))
    }

    #[napi]
    pub fn set(&mut self, env: Env, control: Unknown<'_>) -> Result<bool> {
        self.inner
            .set(|known| env.strict_equals(unknown(&env, known)?, control))
    }

    #[napi]
    pub fn next(&mut self) {
        self.inner.next();
    }

    #[napi]
    pub fn previous(&mut self) {
        self.inner.previous();
    }

    #[napi]
    pub fn handle(&mut self, event: &Event) -> bool {
        self.inner.handle(&event.inner)
    }
}

/// Short messages that go away on their own.
#[napi]
#[derive(Default)]
pub struct Toasts {
    pub(crate) inner: UiToasts,
}

#[napi]
impl Toasts {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self::default()
    }

    /// Show `text`, for `seconds` or the default time.
    #[napi]
    pub fn push(&mut self, text: String, seconds: Option<f64>) {
        push_toast(&mut self.inner, text, seconds);
    }

    /// Drop the toasts whose time is up. Returns whether any left.
    #[napi]
    pub fn expire(&mut self) -> bool {
        self.inner.expire(Instant::now())
    }

    /// Seconds until the next toast leaves.
    #[napi]
    pub fn next_expiry(&self) -> Option<f64> {
        next_expiry(&self.inner)
    }

    #[napi]
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    #[napi]
    pub fn texts(&self) -> Vec<String> {
        self.inner.texts().map(str::to_owned).collect()
    }
}
