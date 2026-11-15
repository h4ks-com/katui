//! The state widgets keep between frames: fields, lists, scrolling, trees,
//! focus and toasts.

use std::ffi::c_char;
use std::time::Instant;

use katui_bindings::state::{duration, next_expiry};
use katui_widgets::{
    Edit, Focus, ListState, ScrollState, TextArea, TextField, Toasts, TreeNode, TreeState,
};

use crate::event::{KatuiEvent, input};
use crate::ffi::{Error, borrow, c_string, owned, owned_or_null, slice, take, text};
use crate::message::KatuiScroll;

/// What an event did to a text field.
pub type KatuiEdit = u32;
pub const KATUI_EDIT_CHANGED: KatuiEdit = 0;
pub const KATUI_EDIT_MOVED: KatuiEdit = 1;
pub const KATUI_EDIT_SUBMITTED: KatuiEdit = 2;
pub const KATUI_EDIT_IGNORED: KatuiEdit = 3;

fn edit(edit: Edit) -> KatuiEdit {
    match edit {
        Edit::Changed => KATUI_EDIT_CHANGED,
        Edit::Moved => KATUI_EDIT_MOVED,
        Edit::Submitted => KATUI_EDIT_SUBMITTED,
        Edit::Ignored => KATUI_EDIT_IGNORED,
    }
}

/// A one-line text input's text and cursor.
pub struct KatuiTextField(pub(crate) TextField);

/// A field holding `text`, the cursor at its end. NULL starts empty.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_text_field_new(text: *const c_char) -> *mut KatuiTextField {
    owned(KatuiTextField(TextField::with_text(
        // SAFETY: the header requires `text` to be NULL or NUL-terminated.
        unsafe { self::text(text) }.into_owned(),
    )))
}

#[unsafe(no_mangle)]
pub extern "C" fn katui_text_field_free(field: Option<Box<KatuiTextField>>) {
    drop(field);
}

/// The text, which the caller frees with `katui_string_free`.
#[unsafe(no_mangle)]
pub extern "C" fn katui_text_field_text(field: Option<&KatuiTextField>) -> *mut c_char {
    c_string(field.map_or("", |field| field.0.text()))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_text_field_set_text(
    field: Option<&mut KatuiTextField>,
    text: *const c_char,
) {
    if let Some(field) = field {
        // SAFETY: the header requires `text` to be NULL or NUL-terminated.
        field.0.set_text(unsafe { self::text(text) }.into_owned());
    }
}

/// The text before the cursor into `before` and after it into `after`,
/// each freed by the caller with `katui_string_free`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_text_field_split(
    field: Option<&KatuiTextField>,
    before: Option<&mut *mut c_char>,
    after: Option<&mut *mut c_char>,
) {
    let (head, tail) = field.map_or(("", ""), |field| field.0.split());
    if let Some(before) = before {
        *before = c_string(head);
    }
    if let Some(after) = after {
        *after = c_string(tail);
    }
}

/// Empty the field and return what it held, freed by the caller with
/// `katui_string_free`.
#[unsafe(no_mangle)]
pub extern "C" fn katui_text_field_take(field: Option<&mut KatuiTextField>) -> *mut c_char {
    c_string(&field.map_or_else(String::new, |field| field.0.take()))
}

/// Typing, the cursor keys and paste.
#[unsafe(no_mangle)]
pub extern "C" fn katui_text_field_handle(
    field: Option<&mut KatuiTextField>,
    event: Option<&KatuiEvent>,
) -> KatuiEdit {
    match (field, input(event)) {
        (Some(field), Some(event)) => edit(field.0.handle(event)),
        _ => KATUI_EDIT_IGNORED,
    }
}

/// A multi-line text input that scrolls to keep its cursor in view.
pub struct KatuiTextArea(pub(crate) TextArea);

/// An empty area showing `rows` lines.
#[unsafe(no_mangle)]
pub extern "C" fn katui_text_area_new(rows: usize) -> *mut KatuiTextArea {
    owned(KatuiTextArea(TextArea::new(rows)))
}

#[unsafe(no_mangle)]
pub extern "C" fn katui_text_area_free(area: Option<Box<KatuiTextArea>>) {
    drop(area);
}

/// The text, which the caller frees with `katui_string_free`.
#[unsafe(no_mangle)]
pub extern "C" fn katui_text_area_text(area: Option<&KatuiTextArea>) -> *mut c_char {
    c_string(area.map_or("", |area| area.0.text()))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_text_area_set_text(
    area: Option<&mut KatuiTextArea>,
    text: *const c_char,
) {
    if let Some(area) = area {
        // SAFETY: the header requires `text` to be NULL or NUL-terminated.
        area.0.set_text(unsafe { self::text(text) }.into_owned());
    }
}

/// Empty the area and return what it held, freed by the caller with
/// `katui_string_free`.
#[unsafe(no_mangle)]
pub extern "C" fn katui_text_area_take(area: Option<&mut KatuiTextArea>) -> *mut c_char {
    c_string(&area.map_or_else(String::new, |area| area.0.take()))
}

#[unsafe(no_mangle)]
pub extern "C" fn katui_text_area_rows(area: Option<&KatuiTextArea>) -> usize {
    area.map_or(0, |area| area.0.rows())
}

#[unsafe(no_mangle)]
pub extern "C" fn katui_text_area_cursor_line(area: Option<&KatuiTextArea>) -> usize {
    area.map_or(0, |area| area.0.cursor_line())
}

#[unsafe(no_mangle)]
pub extern "C" fn katui_text_area_handle(
    area: Option<&mut KatuiTextArea>,
    event: Option<&KatuiEvent>,
) -> KatuiEdit {
    match (area, input(event)) {
        (Some(area), Some(event)) => edit(area.0.handle(event)),
        _ => KATUI_EDIT_IGNORED,
    }
}

/// The selected row of a list and the rows in view.
pub struct KatuiListState(pub(crate) ListState);

#[unsafe(no_mangle)]
pub extern "C" fn katui_list_state_new(length: usize, visible: usize) -> *mut KatuiListState {
    owned(KatuiListState(ListState::new(length, visible)))
}

#[unsafe(no_mangle)]
pub extern "C" fn katui_list_state_free(state: Option<Box<KatuiListState>>) {
    drop(state);
}

#[unsafe(no_mangle)]
pub extern "C" fn katui_list_state_resize(
    state: Option<&mut KatuiListState>,
    length: usize,
    visible: usize,
) {
    if let Some(state) = state {
        state.0.resize(length, visible);
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn katui_list_state_selected(state: Option<&KatuiListState>) -> usize {
    state.map_or(0, |state| state.0.selected())
}

#[unsafe(no_mangle)]
pub extern "C" fn katui_list_state_select(state: Option<&mut KatuiListState>, row: usize) {
    if let Some(state) = state {
        state.0.select(row);
    }
}

/// The rows in view, from `start` up to but not including `end`.
#[unsafe(no_mangle)]
pub extern "C" fn katui_list_state_in_view(
    state: Option<&KatuiListState>,
    start: Option<&mut usize>,
    end: Option<&mut usize>,
) {
    let rows = state.map_or(0..0, |state| state.0.in_view());
    if let Some(start) = start {
        *start = rows.start;
    }
    if let Some(end) = end {
        *end = rows.end;
    }
}

/// Move the selection with the arrow, page, home and end keys. True when
/// it moved.
#[unsafe(no_mangle)]
pub extern "C" fn katui_list_state_handle(
    state: Option<&mut KatuiListState>,
    event: Option<&KatuiEvent>,
) -> bool {
    match (state, input(event)) {
        (Some(state), Some(event)) => state.0.handle(event),
        _ => false,
    }
}

/// How far a scroll view is scrolled.
pub struct KatuiScrollState(pub(crate) ScrollState);

#[unsafe(no_mangle)]
pub extern "C" fn katui_scroll_state_new() -> *mut KatuiScrollState {
    owned(KatuiScrollState(ScrollState::default()))
}

#[unsafe(no_mangle)]
pub extern "C" fn katui_scroll_state_free(state: Option<Box<KatuiScrollState>>) {
    drop(state);
}

#[unsafe(no_mangle)]
pub extern "C" fn katui_scroll_state_offset(state: Option<&KatuiScrollState>) -> f32 {
    state.map_or(0.0, |state| state.0.offset())
}

#[unsafe(no_mangle)]
pub extern "C" fn katui_scroll_state_max(state: Option<&KatuiScrollState>) -> f32 {
    state.map_or(0.0, |state| state.0.max())
}

/// Take the position a scroll view reported.
#[unsafe(no_mangle)]
pub extern "C" fn katui_scroll_state_apply(
    state: Option<&mut KatuiScrollState>,
    scroll: KatuiScroll,
) {
    if let Some(state) = state {
        state.0.apply(scroll.into());
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn katui_scroll_state_scroll_by(state: Option<&mut KatuiScrollState>, pixels: f32) {
    if let Some(state) = state {
        state.0.scroll_by(pixels);
    }
}

/// Move with the arrow, page, home and end keys, `line` pixels a step.
/// True when it moved.
#[unsafe(no_mangle)]
pub extern "C" fn katui_scroll_state_handle(
    state: Option<&mut KatuiScrollState>,
    event: Option<&KatuiEvent>,
    line: f32,
) -> bool {
    match (state, input(event)) {
        (Some(state), Some(event)) => state.0.handle(event, line),
        _ => false,
    }
}

/// One node of a tree.
pub struct KatuiTreeNode(pub(crate) TreeNode);

/// A node without children.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_tree_node_leaf(id: u64, text: *const c_char) -> *mut KatuiTreeNode {
    // SAFETY: the header requires `text` to be NULL or NUL-terminated.
    let text = unsafe { self::text(text) };
    owned(KatuiTreeNode(TreeNode::leaf(id, text)))
}

/// A node with `count` children under it. Takes over every child; NULL
/// ones are left out.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_tree_node_branch(
    id: u64,
    text: *const c_char,
    children: *const *mut KatuiTreeNode,
    count: usize,
) -> *mut KatuiTreeNode {
    // SAFETY: the header hands us the `count` nodes at `children` to own.
    let children = unsafe { slice(children, count) }
        .iter()
        .filter_map(|&child| unsafe { take(child) }.map(|child| child.0))
        .collect();
    owned(KatuiTreeNode(TreeNode::branch(
        id,
        // SAFETY: the header requires `text` to be NULL or NUL-terminated.
        unsafe { self::text(text) },
        children,
    )))
}

#[unsafe(no_mangle)]
pub extern "C" fn katui_tree_node_free(node: Option<Box<KatuiTreeNode>>) {
    drop(node);
}

/// The `count` roots at `roots`, cloned, NULL ones left out.
///
/// # Safety
/// `roots` is NULL or points at `count` nodes, each NULL or live.
pub(crate) unsafe fn roots(roots: *const *const KatuiTreeNode, count: usize) -> Vec<TreeNode> {
    // SAFETY: our caller promises `count` roots at `roots`, each NULL or live.
    unsafe { slice(roots, count) }
        .iter()
        .filter_map(|&root| unsafe { borrow(root) }.map(|root| root.0.clone()))
        .collect()
}

/// Which tree nodes are open and which one is selected.
pub struct KatuiTreeState(pub(crate) TreeState);

#[unsafe(no_mangle)]
pub extern "C" fn katui_tree_state_new() -> *mut KatuiTreeState {
    owned(KatuiTreeState(TreeState::default()))
}

#[unsafe(no_mangle)]
pub extern "C" fn katui_tree_state_free(state: Option<Box<KatuiTreeState>>) {
    drop(state);
}

/// Write the selected node's id into `id`. False when none is selected.
#[unsafe(no_mangle)]
pub extern "C" fn katui_tree_state_selected(
    state: Option<&KatuiTreeState>,
    id: Option<&mut u64>,
) -> bool {
    let Some(selected) = state.and_then(|state| state.0.selected()) else {
        return false;
    };
    if let Some(id) = id {
        *id = selected;
    }
    true
}

#[unsafe(no_mangle)]
pub extern "C" fn katui_tree_state_select(state: Option<&mut KatuiTreeState>, id: u64) {
    if let Some(state) = state {
        state.0.select(id);
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn katui_tree_state_is_open(state: Option<&KatuiTreeState>, id: u64) -> bool {
    state.is_some_and(|state| state.0.is_open(id))
}

#[unsafe(no_mangle)]
pub extern "C" fn katui_tree_state_toggle(state: Option<&mut KatuiTreeState>, id: u64) {
    if let Some(state) = state {
        state.0.toggle(id);
    }
}

/// Move through the visible nodes of `roots` and open and close them.
/// True when something changed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_tree_state_handle(
    state: Option<&mut KatuiTreeState>,
    event: Option<&KatuiEvent>,
    roots: *const *const KatuiTreeNode,
    count: usize,
) -> bool {
    // SAFETY: the header requires `roots` to hold `count` nodes, each NULL or live.
    let roots = unsafe { self::roots(roots, count) };
    match (state, input(event)) {
        (Some(state), Some(event)) => state.0.handle(event, &roots),
        _ => false,
    }
}

/// Which of `count` controls, numbered from 0, has the keyboard; Tab and
/// Shift+Tab move it.
pub struct KatuiFocus(Focus<usize>);

/// Focus over controls 0 to `count - 1`, the first one focused. NULL when
/// `count` is 0.
#[unsafe(no_mangle)]
pub extern "C" fn katui_focus_new(count: usize) -> *mut KatuiFocus {
    if count == 0 {
        return owned_or_null(Err(Error::Invalid(
            "give focus at least one control".into(),
        )));
    }
    owned(KatuiFocus(Focus::new(0, 1..count)))
}

#[unsafe(no_mangle)]
pub extern "C" fn katui_focus_free(focus: Option<Box<KatuiFocus>>) {
    drop(focus);
}

#[unsafe(no_mangle)]
pub extern "C" fn katui_focus_current(focus: Option<&KatuiFocus>) -> usize {
    focus.map_or(0, |focus| focus.0.current())
}

#[unsafe(no_mangle)]
pub extern "C" fn katui_focus_is(focus: Option<&KatuiFocus>, control: usize) -> bool {
    focus.is_some_and(|focus| focus.0.is(control))
}

/// Focus `control`. False when it is not one of the controls.
#[unsafe(no_mangle)]
pub extern "C" fn katui_focus_set(focus: Option<&mut KatuiFocus>, control: usize) -> bool {
    focus.is_some_and(|focus| focus.0.set(control))
}

#[unsafe(no_mangle)]
pub extern "C" fn katui_focus_next(focus: Option<&mut KatuiFocus>) {
    if let Some(focus) = focus {
        focus.0.next();
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn katui_focus_previous(focus: Option<&mut KatuiFocus>) {
    if let Some(focus) = focus {
        focus.0.previous();
    }
}

/// Move the focus on Tab and Shift+Tab. True when it moved.
#[unsafe(no_mangle)]
pub extern "C" fn katui_focus_handle(
    focus: Option<&mut KatuiFocus>,
    event: Option<&KatuiEvent>,
) -> bool {
    match (focus, input(event)) {
        (Some(focus), Some(event)) => focus.0.handle(event),
        _ => false,
    }
}

/// Short messages that go away on their own.
pub struct KatuiToasts(pub(crate) Toasts);

#[unsafe(no_mangle)]
pub extern "C" fn katui_toasts_new() -> *mut KatuiToasts {
    owned(KatuiToasts(Toasts::default()))
}

#[unsafe(no_mangle)]
pub extern "C" fn katui_toasts_free(toasts: Option<Box<KatuiToasts>>) {
    drop(toasts);
}

/// Show `text` for the default time.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_toasts_push(toasts: Option<&mut KatuiToasts>, text: *const c_char) {
    if let Some(toasts) = toasts {
        toasts
            .0
            // SAFETY: the header requires `text` to be NULL or NUL-terminated.
            .push(unsafe { self::text(text) }.into_owned(), Instant::now());
    }
}

/// Show `text` for `seconds`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_toasts_push_for(
    toasts: Option<&mut KatuiToasts>,
    text: *const c_char,
    seconds: f64,
) {
    if let Some(toasts) = toasts {
        toasts.0.push_for(
            // SAFETY: the header requires `text` to be NULL or NUL-terminated.
            unsafe { self::text(text) }.into_owned(),
            Instant::now(),
            duration(seconds),
        );
    }
}

/// Drop the toasts whose time is up. True when any are left.
#[unsafe(no_mangle)]
pub extern "C" fn katui_toasts_expire(toasts: Option<&mut KatuiToasts>) -> bool {
    toasts.is_some_and(|toasts| toasts.0.expire(Instant::now()))
}

/// Write the seconds until the next toast leaves into `seconds`. False when
/// there is none.
#[unsafe(no_mangle)]
pub extern "C" fn katui_toasts_next_expiry(
    toasts: Option<&KatuiToasts>,
    seconds: Option<&mut f64>,
) -> bool {
    let Some(left) = toasts.and_then(|toasts| next_expiry(&toasts.0)) else {
        return false;
    };
    if let Some(seconds) = seconds {
        *seconds = left;
    }
    true
}

#[unsafe(no_mangle)]
pub extern "C" fn katui_toasts_is_empty(toasts: Option<&KatuiToasts>) -> bool {
    toasts.is_none_or(|toasts| toasts.0.is_empty())
}

/// The text of the toast at `index`, oldest first, or NULL past the last.
/// The caller frees it with `katui_string_free`.
#[unsafe(no_mangle)]
pub extern "C" fn katui_toasts_text(toasts: Option<&KatuiToasts>, index: usize) -> *mut c_char {
    toasts
        .and_then(|toasts| toasts.0.texts().nth(index))
        .map_or(std::ptr::null_mut(), c_string)
}
