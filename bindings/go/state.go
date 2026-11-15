package katui

/*
#include "katui.h"
*/
import "C"

import (
	"errors"
	"runtime"
	"time"
)

// Edit says what an event did to a text field.
type Edit uint32

const (
	Changed   Edit = C.KATUI_EDIT_CHANGED
	Moved     Edit = C.KATUI_EDIT_MOVED
	Submitted Edit = C.KATUI_EDIT_SUBMITTED
	Ignored   Edit = C.KATUI_EDIT_IGNORED
)

// TextField is a one-line text input's text and cursor.
type TextField struct {
	p *C.KatuiTextField
}

// NewTextField is a field holding text, the cursor at its end.
func NewTextField(text string) *TextField {
	s := cString(text)
	defer free(s)
	f := &TextField{C.katui_text_field_new(s)}
	runtime.SetFinalizer(f, func(f *TextField) { C.katui_text_field_free(f.p) })
	return f
}

func (f *TextField) Text() string {
	defer runtime.KeepAlive(f)
	return goString(C.katui_text_field_text(f.p))
}

func (f *TextField) SetText(text string) {
	defer runtime.KeepAlive(f)
	s := cString(text)
	defer free(s)
	C.katui_text_field_set_text(f.p, s)
}

// Split is the text before and after the cursor.
func (f *TextField) Split() (before, after string) {
	defer runtime.KeepAlive(f)
	var head, tail *C.char
	C.katui_text_field_split(f.p, &head, &tail)
	return goString(head), goString(tail)
}

// Take empties the field and returns what it held.
func (f *TextField) Take() string {
	defer runtime.KeepAlive(f)
	return goString(C.katui_text_field_take(f.p))
}

// Handle applies typing, the cursor keys and paste.
func (f *TextField) Handle(e *Event) Edit {
	defer runtime.KeepAlive(f)
	defer runtime.KeepAlive(e)
	return Edit(C.katui_text_field_handle(f.p, e.ptr()))
}

// TextArea is a multi-line text input that scrolls to keep its cursor in
// view.
type TextArea struct {
	p *C.KatuiTextArea
}

// NewTextArea is an empty area showing rows lines.
func NewTextArea(rows int) *TextArea {
	a := &TextArea{C.katui_text_area_new(C.size_t(rows))}
	runtime.SetFinalizer(a, func(a *TextArea) { C.katui_text_area_free(a.p) })
	return a
}

func (a *TextArea) Text() string {
	defer runtime.KeepAlive(a)
	return goString(C.katui_text_area_text(a.p))
}

func (a *TextArea) SetText(text string) {
	defer runtime.KeepAlive(a)
	s := cString(text)
	defer free(s)
	C.katui_text_area_set_text(a.p, s)
}

// Take empties the area and returns what it held.
func (a *TextArea) Take() string {
	defer runtime.KeepAlive(a)
	return goString(C.katui_text_area_take(a.p))
}

func (a *TextArea) Rows() int {
	defer runtime.KeepAlive(a)
	return int(C.katui_text_area_rows(a.p))
}

func (a *TextArea) CursorLine() int {
	defer runtime.KeepAlive(a)
	return int(C.katui_text_area_cursor_line(a.p))
}

func (a *TextArea) Handle(e *Event) Edit {
	defer runtime.KeepAlive(a)
	defer runtime.KeepAlive(e)
	return Edit(C.katui_text_area_handle(a.p, e.ptr()))
}

// ListState is the selected row of a list and the rows in view.
type ListState struct {
	p *C.KatuiListState
}

// NewListState is a list of length rows showing visible at a time.
func NewListState(length, visible int) *ListState {
	l := &ListState{C.katui_list_state_new(C.size_t(length), C.size_t(visible))}
	runtime.SetFinalizer(l, func(l *ListState) { C.katui_list_state_free(l.p) })
	return l
}

func (l *ListState) Resize(length, visible int) {
	defer runtime.KeepAlive(l)
	C.katui_list_state_resize(l.p, C.size_t(length), C.size_t(visible))
}

func (l *ListState) Selected() int {
	defer runtime.KeepAlive(l)
	return int(C.katui_list_state_selected(l.p))
}

func (l *ListState) Select(row int) {
	defer runtime.KeepAlive(l)
	C.katui_list_state_select(l.p, C.size_t(row))
}

// InView is the rows in view, from start up to but not including end.
func (l *ListState) InView() (start, end int) {
	defer runtime.KeepAlive(l)
	var from, to C.size_t
	C.katui_list_state_in_view(l.p, &from, &to)
	return int(from), int(to)
}

// Handle moves the selection with the arrow, page, home and end keys, and
// reports whether it moved.
func (l *ListState) Handle(e *Event) bool {
	defer runtime.KeepAlive(l)
	defer runtime.KeepAlive(e)
	return bool(C.katui_list_state_handle(l.p, e.ptr()))
}

// ScrollState is how far a scroll view is scrolled.
type ScrollState struct {
	p *C.KatuiScrollState
}

func NewScrollState() *ScrollState {
	s := &ScrollState{C.katui_scroll_state_new()}
	runtime.SetFinalizer(s, func(s *ScrollState) { C.katui_scroll_state_free(s.p) })
	return s
}

func (s *ScrollState) Offset() float32 {
	defer runtime.KeepAlive(s)
	return float32(C.katui_scroll_state_offset(s.p))
}

func (s *ScrollState) Max() float32 {
	defer runtime.KeepAlive(s)
	return float32(C.katui_scroll_state_max(s.p))
}

// Apply takes the position a scroll view reported.
func (s *ScrollState) Apply(scroll Scroll) {
	defer runtime.KeepAlive(s)
	C.katui_scroll_state_apply(s.p, C.KatuiScroll{offset: C.float(scroll.Offset), max: C.float(scroll.Max)})
}

func (s *ScrollState) ScrollBy(pixels float32) {
	defer runtime.KeepAlive(s)
	C.katui_scroll_state_scroll_by(s.p, C.float(pixels))
}

// Handle scrolls with the arrow, page, home and end keys, line pixels a
// step, and reports whether it moved.
func (s *ScrollState) Handle(e *Event, line float32) bool {
	defer runtime.KeepAlive(s)
	defer runtime.KeepAlive(e)
	return bool(C.katui_scroll_state_handle(s.p, e.ptr(), C.float(line)))
}

// TreeNode is one node of a tree.
type TreeNode struct {
	p *C.KatuiTreeNode
}

func treeNode(p *C.KatuiTreeNode) *TreeNode {
	n := &TreeNode{p}
	runtime.SetFinalizer(n, func(n *TreeNode) { C.katui_tree_node_free(n.p) })
	return n
}

// Leaf is a node without children.
func Leaf(id uint64, text string) *TreeNode {
	s := cString(text)
	defer free(s)
	return treeNode(C.katui_tree_node_leaf(C.uint64_t(id), s))
}

// Branch is a node with children under it. The children belong to it from
// then on.
func Branch(id uint64, text string, children ...*TreeNode) *TreeNode {
	s := cString(text)
	defer free(s)
	taken := make([]*C.KatuiTreeNode, 0, len(children))
	for _, child := range children {
		if child != nil && child.p != nil {
			taken = append(taken, child.p)
			child.p = nil
		}
	}
	return treeNode(C.katui_tree_node_branch(C.uint64_t(id), s, firstOf(taken), C.size_t(len(taken))))
}

func roots(nodes []*TreeNode) []*C.KatuiTreeNode {
	pointers := make([]*C.KatuiTreeNode, 0, len(nodes))
	for _, node := range nodes {
		if node != nil {
			pointers = append(pointers, node.p)
		}
	}
	return pointers
}

// TreeState is which tree nodes are open and which one is selected.
type TreeState struct {
	p *C.KatuiTreeState
}

func NewTreeState() *TreeState {
	s := &TreeState{C.katui_tree_state_new()}
	runtime.SetFinalizer(s, func(s *TreeState) { C.katui_tree_state_free(s.p) })
	return s
}

// Selected is the selected node's id, and false when none is selected.
func (s *TreeState) Selected() (uint64, bool) {
	defer runtime.KeepAlive(s)
	var id C.uint64_t
	ok := C.katui_tree_state_selected(s.p, &id)
	return uint64(id), bool(ok)
}

func (s *TreeState) Select(id uint64) {
	defer runtime.KeepAlive(s)
	C.katui_tree_state_select(s.p, C.uint64_t(id))
}

func (s *TreeState) IsOpen(id uint64) bool {
	defer runtime.KeepAlive(s)
	return bool(C.katui_tree_state_is_open(s.p, C.uint64_t(id)))
}

func (s *TreeState) Toggle(id uint64) {
	defer runtime.KeepAlive(s)
	C.katui_tree_state_toggle(s.p, C.uint64_t(id))
}

// Handle moves through the visible nodes of roots and opens and closes
// them, and reports whether something changed.
func (s *TreeState) Handle(e *Event, nodes ...*TreeNode) bool {
	defer runtime.KeepAlive(s)
	defer runtime.KeepAlive(e)
	defer runtime.KeepAlive(nodes)
	pointers := roots(nodes)
	return bool(C.katui_tree_state_handle(s.p, e.ptr(), firstOf(pointers), C.size_t(len(pointers))))
}

// Focus is which of a number of controls, numbered from 0, has the
// keyboard; Tab and Shift+Tab move it.
type Focus struct {
	p *C.KatuiFocus
}

// NewFocus is focus over controls 0 to count-1, the first one focused.
func NewFocus(count int) (*Focus, error) {
	if count < 1 {
		return nil, errors.New("katui: give focus at least one control")
	}
	f := &Focus{C.katui_focus_new(C.size_t(count))}
	runtime.SetFinalizer(f, func(f *Focus) { C.katui_focus_free(f.p) })
	return f, nil
}

func (f *Focus) Current() int {
	defer runtime.KeepAlive(f)
	return int(C.katui_focus_current(f.p))
}

func (f *Focus) Is(control int) bool {
	defer runtime.KeepAlive(f)
	return bool(C.katui_focus_is(f.p, C.size_t(control)))
}

// Set focuses control, and reports false when it is not one of the
// controls.
func (f *Focus) Set(control int) bool {
	defer runtime.KeepAlive(f)
	return bool(C.katui_focus_set(f.p, C.size_t(control)))
}

func (f *Focus) Next() {
	defer runtime.KeepAlive(f)
	C.katui_focus_next(f.p)
}

func (f *Focus) Previous() {
	defer runtime.KeepAlive(f)
	C.katui_focus_previous(f.p)
}

// Handle moves the focus on Tab and Shift+Tab, and reports whether it
// moved.
func (f *Focus) Handle(e *Event) bool {
	defer runtime.KeepAlive(f)
	defer runtime.KeepAlive(e)
	return bool(C.katui_focus_handle(f.p, e.ptr()))
}

// Toasts are short messages that go away on their own.
type Toasts struct {
	p *C.KatuiToasts
}

func NewToasts() *Toasts {
	t := &Toasts{C.katui_toasts_new()}
	runtime.SetFinalizer(t, func(t *Toasts) { C.katui_toasts_free(t.p) })
	return t
}

// Push shows text for the default time.
func (t *Toasts) Push(text string) {
	defer runtime.KeepAlive(t)
	s := cString(text)
	defer free(s)
	C.katui_toasts_push(t.p, s)
}

// PushFor shows text for d.
func (t *Toasts) PushFor(text string, d time.Duration) {
	defer runtime.KeepAlive(t)
	s := cString(text)
	defer free(s)
	C.katui_toasts_push_for(t.p, s, C.double(d.Seconds()))
}

// Expire drops the toasts whose time is up, and reports whether any are
// left.
func (t *Toasts) Expire() bool {
	defer runtime.KeepAlive(t)
	return bool(C.katui_toasts_expire(t.p))
}

// NextExpiry is the time until the next toast leaves, and false when there
// is none.
func (t *Toasts) NextExpiry() (time.Duration, bool) {
	defer runtime.KeepAlive(t)
	var seconds C.double
	ok := C.katui_toasts_next_expiry(t.p, &seconds)
	return time.Duration(float64(seconds) * float64(time.Second)), bool(ok)
}

func (t *Toasts) IsEmpty() bool {
	defer runtime.KeepAlive(t)
	return bool(C.katui_toasts_is_empty(t.p))
}

// Texts is the text of every toast, oldest first.
func (t *Toasts) Texts() []string {
	defer runtime.KeepAlive(t)
	var texts []string
	for index := 0; ; index++ {
		text := C.katui_toasts_text(t.p, C.size_t(index))
		if text == nil {
			return texts
		}
		texts = append(texts, goString(text))
	}
}
