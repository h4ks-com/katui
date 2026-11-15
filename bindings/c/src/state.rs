//! The state widgets keep between frames: fields, lists, scrolling, trees,
//! focus and toasts, plus the input events that change them.

use std::ffi::c_char;
use std::time::{Duration, Instant};

use katui_proto::input::{Button, InputEvent, Key, Mods, MouseAction};
use katui_widgets::{
    Edit, Focus, ListState, ScrollState, TextArea, TextField, Toasts, TreeNode, TreeState,
};

use crate::ffi::{
    Error, borrow, borrow_mut, c_string, free, owned, owned_or_null, slice, take, text,
};
use crate::message::KatuiScroll;

/// A key press, a release, a mouse event, a paste or a focus change.
pub struct KatuiEvent(pub(crate) InputEvent);

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
pub(crate) fn key(name: &str) -> Result<Key, Error> {
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
                .ok_or_else(|| Error::Invalid(format!("name a key, not {other:?}")));
        }
    };
    Ok(named)
}

/// What kind of event a `KatuiEvent` is.
pub type KatuiEventKind = u32;
pub const KATUI_EVENT_OTHER: KatuiEventKind = 0;
pub const KATUI_EVENT_KEY: KatuiEventKind = 1;
pub const KATUI_EVENT_KEY_UP: KatuiEventKind = 2;
pub const KATUI_EVENT_MOUSE: KatuiEventKind = 3;
pub const KATUI_EVENT_PASTE: KatuiEventKind = 4;
pub const KATUI_EVENT_FOCUS: KatuiEventKind = 5;

/// What the mouse did.
pub type KatuiMouseAction = u32;
pub const KATUI_MOUSE_NONE: KatuiMouseAction = 0;
pub const KATUI_MOUSE_PRESS: KatuiMouseAction = 1;
pub const KATUI_MOUSE_RELEASE: KatuiMouseAction = 2;
pub const KATUI_MOUSE_DRAG: KatuiMouseAction = 3;
pub const KATUI_MOUSE_MOVE: KatuiMouseAction = 4;
pub const KATUI_MOUSE_WHEEL_UP: KatuiMouseAction = 5;
pub const KATUI_MOUSE_WHEEL_DOWN: KatuiMouseAction = 6;

/// Which mouse button.
pub type KatuiButton = u32;
pub const KATUI_BUTTON_NONE: KatuiButton = 0;
pub const KATUI_BUTTON_LEFT: KatuiButton = 1;
pub const KATUI_BUTTON_MIDDLE: KatuiButton = 2;
pub const KATUI_BUTTON_RIGHT: KatuiButton = 3;

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

/// # Safety
/// `event` is NULL or a live event.
unsafe fn event<'a>(event: *const KatuiEvent) -> Option<&'a InputEvent> {
    unsafe { borrow(event) }.map(|event| &event.0)
}

/// A press of the key `name`, for tests and for handing keys to widgets.
/// NULL when `name` names no key.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_event_press(
    name: *const c_char,
    ctrl: bool,
    alt: bool,
    shift: bool,
) -> *mut KatuiEvent {
    let mods = Mods { shift, alt, ctrl };
    owned_or_null(key(&unsafe { text(name) }).map(|key| KatuiEvent(InputEvent::Key(key, mods))))
}

/// A paste of `text`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_event_paste(text: *const c_char) -> *mut KatuiEvent {
    owned(KatuiEvent(InputEvent::Paste(
        unsafe { self::text(text) }.into_owned(),
    )))
}

/// A copy of `event` the caller owns.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_event_clone(event: *const KatuiEvent) -> *mut KatuiEvent {
    unsafe { self::event(event) }.map_or(std::ptr::null_mut(), |event| {
        owned(KatuiEvent(event.clone()))
    })
}

/// Free an event the caller owns. NULL does nothing.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_event_free(event: *mut KatuiEvent) {
    unsafe { free(event) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_event_kind(event: *const KatuiEvent) -> KatuiEventKind {
    match unsafe { self::event(event) } {
        Some(InputEvent::Key(..)) => KATUI_EVENT_KEY,
        Some(InputEvent::KeyUp(..)) => KATUI_EVENT_KEY_UP,
        Some(InputEvent::Mouse(_)) => KATUI_EVENT_MOUSE,
        Some(InputEvent::Paste(_)) => KATUI_EVENT_PASTE,
        Some(InputEvent::Focus(_)) => KATUI_EVENT_FOCUS,
        _ => KATUI_EVENT_OTHER,
    }
}

/// The key's name: one character, `"enter"`, `"up"`, `"f1"` and so on, or
/// NULL for an event without a key. The caller frees it with
/// `katui_string_free`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_event_key(event: *const KatuiEvent) -> *mut c_char {
    match unsafe { self::event(event) } {
        Some(InputEvent::Key(key, _) | InputEvent::KeyUp(key, _)) => c_string(&key_name(*key)),
        _ => std::ptr::null_mut(),
    }
}

fn mods(event: Option<&InputEvent>) -> Mods {
    match event {
        Some(InputEvent::Key(_, mods) | InputEvent::KeyUp(_, mods)) => *mods,
        Some(InputEvent::Mouse(mouse)) => mouse.mods,
        _ => Mods::NONE,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_event_ctrl(event: *const KatuiEvent) -> bool {
    mods(unsafe { self::event(event) }).ctrl
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_event_alt(event: *const KatuiEvent) -> bool {
    mods(unsafe { self::event(event) }).alt
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_event_shift(event: *const KatuiEvent) -> bool {
    mods(unsafe { self::event(event) }).shift
}

/// The pasted text, or NULL for another event. The caller frees it with
/// `katui_string_free`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_event_text(event: *const KatuiEvent) -> *mut c_char {
    match unsafe { self::event(event) } {
        Some(InputEvent::Paste(text)) => c_string(text),
        _ => std::ptr::null_mut(),
    }
}

/// Whether the window has the focus, for a focus event.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_event_focused(event: *const KatuiEvent) -> bool {
    matches!(unsafe { self::event(event) }, Some(InputEvent::Focus(true)))
}

/// Write the cell under the pointer into `column` and `row`. False for an
/// event without a pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_event_position(
    event: *const KatuiEvent,
    column: *mut u32,
    row: *mut u32,
) -> bool {
    let Some(InputEvent::Mouse(mouse)) = (unsafe { self::event(event) }) else {
        return false;
    };
    if let Some(column) = unsafe { borrow_mut(column) } {
        *column = mouse.x;
    }
    if let Some(row) = unsafe { borrow_mut(row) } {
        *row = mouse.y;
    }
    true
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_event_action(event: *const KatuiEvent) -> KatuiMouseAction {
    let Some(InputEvent::Mouse(mouse)) = (unsafe { self::event(event) }) else {
        return KATUI_MOUSE_NONE;
    };
    match mouse.action {
        MouseAction::Press(_) => KATUI_MOUSE_PRESS,
        MouseAction::Release(_) => KATUI_MOUSE_RELEASE,
        MouseAction::Drag(_) => KATUI_MOUSE_DRAG,
        MouseAction::Move => KATUI_MOUSE_MOVE,
        MouseAction::WheelUp => KATUI_MOUSE_WHEEL_UP,
        MouseAction::WheelDown => KATUI_MOUSE_WHEEL_DOWN,
    }
}

/// The button of a press, release or drag.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_event_button(event: *const KatuiEvent) -> KatuiButton {
    let Some(InputEvent::Mouse(mouse)) = (unsafe { self::event(event) }) else {
        return KATUI_BUTTON_NONE;
    };
    match mouse.action {
        MouseAction::Press(button) | MouseAction::Release(button) | MouseAction::Drag(button) => {
            match button {
                Button::Left => KATUI_BUTTON_LEFT,
                Button::Middle => KATUI_BUTTON_MIDDLE,
                Button::Right => KATUI_BUTTON_RIGHT,
            }
        }
        _ => KATUI_BUTTON_NONE,
    }
}

/// A one-line text input's text and cursor.
pub struct KatuiTextField(pub(crate) TextField);

/// A field holding `text`, the cursor at its end. NULL starts empty.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_text_field_new(text: *const c_char) -> *mut KatuiTextField {
    owned(KatuiTextField(TextField::with_text(
        unsafe { self::text(text) }.into_owned(),
    )))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_text_field_free(field: *mut KatuiTextField) {
    unsafe { free(field) };
}

/// The text, which the caller frees with `katui_string_free`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_text_field_text(field: *const KatuiTextField) -> *mut c_char {
    c_string(unsafe { borrow(field) }.map_or("", |field| field.0.text()))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_text_field_set_text(
    field: *mut KatuiTextField,
    text: *const c_char,
) {
    if let Some(field) = unsafe { borrow_mut(field) } {
        field.0.set_text(unsafe { self::text(text) }.into_owned());
    }
}

/// The text before the cursor into `before` and after it into `after`,
/// each freed by the caller with `katui_string_free`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_text_field_split(
    field: *const KatuiTextField,
    before: *mut *mut c_char,
    after: *mut *mut c_char,
) {
    let (head, tail) = unsafe { borrow(field) }.map_or(("", ""), |field| field.0.split());
    if let Some(before) = unsafe { borrow_mut(before) } {
        *before = c_string(head);
    }
    if let Some(after) = unsafe { borrow_mut(after) } {
        *after = c_string(tail);
    }
}

/// Empty the field and return what it held, freed by the caller with
/// `katui_string_free`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_text_field_take(field: *mut KatuiTextField) -> *mut c_char {
    c_string(&unsafe { borrow_mut(field) }.map_or_else(String::new, |field| field.0.take()))
}

/// Typing, the cursor keys and paste.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_text_field_handle(
    field: *mut KatuiTextField,
    event: *const KatuiEvent,
) -> KatuiEdit {
    match (unsafe { borrow_mut(field) }, unsafe { self::event(event) }) {
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
pub unsafe extern "C" fn katui_text_area_free(area: *mut KatuiTextArea) {
    unsafe { free(area) };
}

/// The text, which the caller frees with `katui_string_free`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_text_area_text(area: *const KatuiTextArea) -> *mut c_char {
    c_string(unsafe { borrow(area) }.map_or("", |area| area.0.text()))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_text_area_set_text(area: *mut KatuiTextArea, text: *const c_char) {
    if let Some(area) = unsafe { borrow_mut(area) } {
        area.0.set_text(unsafe { self::text(text) }.into_owned());
    }
}

/// Empty the area and return what it held, freed by the caller with
/// `katui_string_free`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_text_area_take(area: *mut KatuiTextArea) -> *mut c_char {
    c_string(&unsafe { borrow_mut(area) }.map_or_else(String::new, |area| area.0.take()))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_text_area_rows(area: *const KatuiTextArea) -> usize {
    unsafe { borrow(area) }.map_or(0, |area| area.0.rows())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_text_area_cursor_line(area: *const KatuiTextArea) -> usize {
    unsafe { borrow(area) }.map_or(0, |area| area.0.cursor_line())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_text_area_handle(
    area: *mut KatuiTextArea,
    event: *const KatuiEvent,
) -> KatuiEdit {
    match (unsafe { borrow_mut(area) }, unsafe { self::event(event) }) {
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
pub unsafe extern "C" fn katui_list_state_free(state: *mut KatuiListState) {
    unsafe { free(state) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_list_state_resize(
    state: *mut KatuiListState,
    length: usize,
    visible: usize,
) {
    if let Some(state) = unsafe { borrow_mut(state) } {
        state.0.resize(length, visible);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_list_state_selected(state: *const KatuiListState) -> usize {
    unsafe { borrow(state) }.map_or(0, |state| state.0.selected())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_list_state_select(state: *mut KatuiListState, row: usize) {
    if let Some(state) = unsafe { borrow_mut(state) } {
        state.0.select(row);
    }
}

/// The rows in view, from `start` up to but not including `end`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_list_state_in_view(
    state: *const KatuiListState,
    start: *mut usize,
    end: *mut usize,
) {
    let rows = unsafe { borrow(state) }.map_or(0..0, |state| state.0.in_view());
    if let Some(start) = unsafe { borrow_mut(start) } {
        *start = rows.start;
    }
    if let Some(end) = unsafe { borrow_mut(end) } {
        *end = rows.end;
    }
}

/// Move the selection with the arrow, page, home and end keys. True when
/// it moved.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_list_state_handle(
    state: *mut KatuiListState,
    event: *const KatuiEvent,
) -> bool {
    match (unsafe { borrow_mut(state) }, unsafe { self::event(event) }) {
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
pub unsafe extern "C" fn katui_scroll_state_free(state: *mut KatuiScrollState) {
    unsafe { free(state) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_scroll_state_offset(state: *const KatuiScrollState) -> f32 {
    unsafe { borrow(state) }.map_or(0.0, |state| state.0.offset())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_scroll_state_max(state: *const KatuiScrollState) -> f32 {
    unsafe { borrow(state) }.map_or(0.0, |state| state.0.max())
}

/// Take the position a scroll view reported.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_scroll_state_apply(
    state: *mut KatuiScrollState,
    scroll: KatuiScroll,
) {
    if let Some(state) = unsafe { borrow_mut(state) } {
        state.0.apply(scroll.into());
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_scroll_state_scroll_by(state: *mut KatuiScrollState, pixels: f32) {
    if let Some(state) = unsafe { borrow_mut(state) } {
        state.0.scroll_by(pixels);
    }
}

/// Move with the arrow, page, home and end keys, `line` pixels a step.
/// True when it moved.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_scroll_state_handle(
    state: *mut KatuiScrollState,
    event: *const KatuiEvent,
    line: f32,
) -> bool {
    match (unsafe { borrow_mut(state) }, unsafe { self::event(event) }) {
        (Some(state), Some(event)) => state.0.handle(event, line),
        _ => false,
    }
}

/// One node of a tree.
pub struct KatuiTreeNode(pub(crate) TreeNode);

/// A node without children.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_tree_node_leaf(id: u64, text: *const c_char) -> *mut KatuiTreeNode {
    owned(KatuiTreeNode(TreeNode::leaf(id, unsafe {
        self::text(text)
    })))
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
    let children = unsafe { slice(children, count) }
        .iter()
        .filter_map(|&child| unsafe { take(child) }.map(|child| child.0))
        .collect();
    owned(KatuiTreeNode(TreeNode::branch(
        id,
        unsafe { self::text(text) },
        children,
    )))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_tree_node_free(node: *mut KatuiTreeNode) {
    unsafe { free(node) };
}

/// The `count` roots at `roots`, cloned, NULL ones left out.
///
/// # Safety
/// `roots` is NULL or points at `count` nodes, each NULL or live.
pub(crate) unsafe fn roots(roots: *const *const KatuiTreeNode, count: usize) -> Vec<TreeNode> {
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
pub unsafe extern "C" fn katui_tree_state_free(state: *mut KatuiTreeState) {
    unsafe { free(state) };
}

/// Write the selected node's id into `id`. False when none is selected.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_tree_state_selected(
    state: *const KatuiTreeState,
    id: *mut u64,
) -> bool {
    let Some(selected) = unsafe { borrow(state) }.and_then(|state| state.0.selected()) else {
        return false;
    };
    if let Some(id) = unsafe { borrow_mut(id) } {
        *id = selected;
    }
    true
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_tree_state_select(state: *mut KatuiTreeState, id: u64) {
    if let Some(state) = unsafe { borrow_mut(state) } {
        state.0.select(id);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_tree_state_is_open(state: *const KatuiTreeState, id: u64) -> bool {
    unsafe { borrow(state) }.is_some_and(|state| state.0.is_open(id))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_tree_state_toggle(state: *mut KatuiTreeState, id: u64) {
    if let Some(state) = unsafe { borrow_mut(state) } {
        state.0.toggle(id);
    }
}

/// Move through the visible nodes of `roots` and open and close them.
/// True when something changed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_tree_state_handle(
    state: *mut KatuiTreeState,
    event: *const KatuiEvent,
    roots: *const *const KatuiTreeNode,
    count: usize,
) -> bool {
    let roots = unsafe { self::roots(roots, count) };
    match (unsafe { borrow_mut(state) }, unsafe { self::event(event) }) {
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
pub unsafe extern "C" fn katui_focus_free(focus: *mut KatuiFocus) {
    unsafe { free(focus) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_focus_current(focus: *const KatuiFocus) -> usize {
    unsafe { borrow(focus) }.map_or(0, |focus| focus.0.current())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_focus_is(focus: *const KatuiFocus, control: usize) -> bool {
    unsafe { borrow(focus) }.is_some_and(|focus| focus.0.is(control))
}

/// Focus `control`. False when it is not one of the controls.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_focus_set(focus: *mut KatuiFocus, control: usize) -> bool {
    unsafe { borrow_mut(focus) }.is_some_and(|focus| focus.0.set(control))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_focus_next(focus: *mut KatuiFocus) {
    if let Some(focus) = unsafe { borrow_mut(focus) } {
        focus.0.next();
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_focus_previous(focus: *mut KatuiFocus) {
    if let Some(focus) = unsafe { borrow_mut(focus) } {
        focus.0.previous();
    }
}

/// Move the focus on Tab and Shift+Tab. True when it moved.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_focus_handle(
    focus: *mut KatuiFocus,
    event: *const KatuiEvent,
) -> bool {
    match (unsafe { borrow_mut(focus) }, unsafe { self::event(event) }) {
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
pub unsafe extern "C" fn katui_toasts_free(toasts: *mut KatuiToasts) {
    unsafe { free(toasts) };
}

/// Show `text` for the default time.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_toasts_push(toasts: *mut KatuiToasts, text: *const c_char) {
    if let Some(toasts) = unsafe { borrow_mut(toasts) } {
        toasts
            .0
            .push(unsafe { self::text(text) }.into_owned(), Instant::now());
    }
}

/// Show `text` for `seconds`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_toasts_push_for(
    toasts: *mut KatuiToasts,
    text: *const c_char,
    seconds: f64,
) {
    if let Some(toasts) = unsafe { borrow_mut(toasts) } {
        let time = Duration::try_from_secs_f64(seconds).unwrap_or_default();
        toasts.0.push_for(
            unsafe { self::text(text) }.into_owned(),
            Instant::now(),
            time,
        );
    }
}

/// Drop the toasts whose time is up. True when any are left.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_toasts_expire(toasts: *mut KatuiToasts) -> bool {
    unsafe { borrow_mut(toasts) }.is_some_and(|toasts| toasts.0.expire(Instant::now()))
}

/// Write the seconds until the next toast leaves into `seconds`. False when
/// there is none.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_toasts_next_expiry(
    toasts: *const KatuiToasts,
    seconds: *mut f64,
) -> bool {
    let Some(at) = unsafe { borrow(toasts) }.and_then(|toasts| toasts.0.next_expiry()) else {
        return false;
    };
    if let Some(seconds) = unsafe { borrow_mut(seconds) } {
        *seconds = at.saturating_duration_since(Instant::now()).as_secs_f64();
    }
    true
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_toasts_is_empty(toasts: *const KatuiToasts) -> bool {
    unsafe { borrow(toasts) }.is_none_or(|toasts| toasts.0.is_empty())
}

/// The text of the toast at `index`, oldest first, or NULL past the last.
/// The caller frees it with `katui_string_free`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_toasts_text(
    toasts: *const KatuiToasts,
    index: usize,
) -> *mut c_char {
    unsafe { borrow(toasts) }
        .and_then(|toasts| toasts.0.texts().nth(index))
        .map_or(std::ptr::null_mut(), c_string)
}
