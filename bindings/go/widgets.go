package katui

/*
#include "katui.h"
#include "callbacks.h"
*/
import "C"

import (
	"runtime"
	"runtime/cgo"
	"time"
)

func Label(text string) *Element {
	s := cString(text)
	defer free(s)
	return element(C.katui_label(s))
}

func Title(text string) *Element {
	s := cString(text)
	defer free(s)
	return element(C.katui_title(s))
}

func Heading(text string) *Element {
	s := cString(text)
	defer free(s)
	return element(C.katui_heading(s))
}

func Hint(text string) *Element {
	s := cString(text)
	defer free(s)
	return element(C.katui_hint(s))
}

// Button sends message when clicked.
func Button(text string, message any) *Element {
	s := cString(text)
	defer free(s)
	return element(C.katui_button(s, C.uint64_t(key(message))))
}

// FocusableButton is a button that sends message when clicked; focused
// rings it in the accent color to show where the keys act.
func FocusableButton(text string, message any, focused bool) *Element {
	s := cString(text)
	defer free(s)
	return element(C.katui_focusable_button(s, C.uint64_t(key(message)), C.bool(focused)))
}

// PrimaryButton is a button in the accent color that sends message when
// clicked.
func PrimaryButton(text string, message any) *Element {
	s := cString(text)
	defer free(s)
	return element(C.katui_primary_button(s, C.uint64_t(key(message))))
}

// Panel is a raised card around items. Nil items are left out.
func Panel(items ...*Element) *Element {
	taken := children(items)
	return element(C.katui_panel(firstOf(taken), C.size_t(len(taken))))
}

// Page is the theme's background filling the screen, around items.
func Page(items ...*Element) *Element {
	taken := children(items)
	return element(C.katui_page(firstOf(taken), C.size_t(len(taken))))
}

// TextFieldView draws field, with placeholder while it is empty.
func TextFieldView(field *TextField, placeholder string, focused bool) *Element {
	defer runtime.KeepAlive(field)
	s := cString(placeholder)
	defer free(s)
	return element(C.katui_text_field(field.p, s, C.bool(focused)))
}

// TextAreaView draws area, with placeholder while it is empty.
func TextAreaView(area *TextArea, placeholder string, focused bool) *Element {
	defer runtime.KeepAlive(area)
	s := cString(placeholder)
	defer free(s)
	return element(C.katui_text_area(area.p, s, C.bool(focused)))
}

// List draws the rows of state in view, each built by row.
func List(state *ListState, row func(index int, selected bool) *Element) *Element {
	defer runtime.KeepAlive(state)
	handle := cgo.NewHandle(row)
	defer handle.Delete()
	return element(C.katui_go_list(state.p, C.uintptr_t(handle)))
}

//export katuiGoRow
func katuiGoRow(user C.uintptr_t, index C.size_t, selected C.bool) *C.KatuiElement {
	row := cgo.Handle(user).Value().(func(int, bool) *Element)
	return row(int(index), bool(selected)).take()
}

// Checkbox sends message when clicked.
func Checkbox(text string, checked bool, message any) *Element {
	s := cString(text)
	defer free(s)
	return element(C.katui_checkbox(s, C.bool(checked), C.uint64_t(key(message))))
}

// Switch sends message when clicked.
func Switch(text string, on bool, message any) *Element {
	s := cString(text)
	defer free(s)
	return element(C.katui_switch(s, C.bool(on), C.uint64_t(key(message))))
}

// Progress is a bar filled to fraction, from 0 to 1.
func Progress(fraction float32) *Element {
	return element(C.katui_progress(C.float(fraction)))
}

// Tabs is a row of tabs; a click calls choose, a func(int) or
// func(int) any, with the tab's index.
func Tabs(titles []string, selected int, choose any) *Element {
	strings, release := cStrings(titles)
	defer release()
	return element(C.katui_tabs(firstOf(strings), C.size_t(len(strings)), C.size_t(selected), C.uint64_t(key(choose))))
}

// Radio is a column of options, one picked; a click calls choose, a
// func(int) or func(int) any, with the option's index.
func Radio(options []string, selected int, choose any) *Element {
	strings, release := cStrings(options)
	defer release()
	return element(C.katui_radio(firstOf(strings), C.size_t(len(strings)), C.size_t(selected), C.uint64_t(key(choose))))
}

// Slider is a slider at value, from 0 to 1; a drag calls change, a
// func(float32) or func(float32) any, with each new value.
func Slider(value float32, change any) *Element {
	return element(C.katui_slider(C.float(value), C.uint64_t(key(change))))
}

// Nudge is value moved by step for the arrow keys, between 0 and 1, and
// false when e is not an arrow key.
func Nudge(value float32, e *Event, step float32) (float32, bool) {
	defer runtime.KeepAlive(e)
	var moved C.float
	ok := C.katui_nudge(C.float(value), e.ptr(), C.float(step), &moved)
	return float32(moved), bool(ok)
}

// Spinner is three dots with one lit, moving every quarter second of
// elapsed.
func Spinner(elapsed time.Duration) *Element {
	return element(C.katui_spinner(C.double(elapsed.Seconds())))
}

// ToastsView draws the toasts in the corner.
func ToastsView(toasts *Toasts) *Element {
	defer runtime.KeepAlive(toasts)
	return element(C.katui_toasts(toasts.p))
}

// KeyHint is one key and what it does.
type KeyHint struct {
	Key, Action string
}

// KeyHints is a line of keys and what they do.
func KeyHints(hints ...KeyHint) *Element {
	made := make([]C.KatuiKeyHint, len(hints))
	var texts []*C.char
	defer func() {
		for _, s := range texts {
			free(s)
		}
	}()
	for i, hint := range hints {
		key, action := cString(hint.Key), cString(hint.Action)
		texts = append(texts, key, action)
		made[i] = C.KatuiKeyHint{key: key, action: action}
	}
	return element(C.katui_key_hints(firstOf(made), C.size_t(len(made))))
}

// Dialog is a card over a dimmed screen; a click outside it sends dismiss.
func Dialog(title string, body *Element, actions []*Element, dismiss any) *Element {
	s := cString(title)
	defer free(s)
	taken := children(actions)
	return element(C.katui_dialog(s, body.take(), firstOf(taken), C.size_t(len(taken)), C.uint64_t(key(dismiss))))
}

// Select is a field showing the option at selected that opens a list of
// options. A click on the field sends toggle; one on an option calls
// choose, a func(int) or func(int) any, with its index.
func Select(options []string, selected int, open bool, toggle, choose any) *Element {
	strings, release := cStrings(options)
	defer release()
	return element(C.katui_select(firstOf(strings), C.size_t(len(strings)), C.size_t(selected), C.bool(open),
		C.uint64_t(key(toggle)), C.uint64_t(key(choose))))
}

// Menu is a title that opens items. A click on the title sends toggle; one
// on an item calls choose, a func(int) or func(int) any, with its index.
func Menu(title string, items []string, open bool, toggle, choose any) *Element {
	s := cString(title)
	defer free(s)
	strings, release := cStrings(items)
	defer release()
	return element(C.katui_menu(s, firstOf(strings), C.size_t(len(strings)), C.bool(open),
		C.uint64_t(key(toggle)), C.uint64_t(key(choose))))
}

// Tooltip is anchor with text beside it while shown. The pointer entering
// sends show, leaving sends hide.
func Tooltip(anchor *Element, text string, shown bool, show, hide any) *Element {
	s := cString(text)
	defer free(s)
	return element(C.katui_tooltip(anchor.take(), s, C.bool(shown), C.uint64_t(key(show)), C.uint64_t(key(hide))))
}

// ScrollView is content in a box that scrolls; each new position calls
// moved, a func(Scroll) or func(Scroll) any.
func ScrollView(state *ScrollState, content []*Element, moved any) *Element {
	defer runtime.KeepAlive(state)
	taken := children(content)
	return element(C.katui_scroll_view(state.p, firstOf(taken), C.size_t(len(taken)), C.uint64_t(key(moved))))
}

// Orientation is how Split lays out its panes.
type Orientation uint32

const (
	Horizontal Orientation = C.KATUI_ORIENTATION_HORIZONTAL
	Vertical   Orientation = C.KATUI_ORIENTATION_VERTICAL
)

// Split is two panes and a divider, first taking fraction of the space. A
// drag on the divider calls resize, a func(float32) or func(float32) any,
// with the new fraction.
func Split(first, second *Element, fraction float32, orientation Orientation, resize any) *Element {
	return element(C.katui_split(first.take(), second.take(), C.float(fraction), C.KatuiOrientation(orientation),
		C.uint64_t(key(resize))))
}

// TableColumn is a table column: its title and its width in cells, or 0 to
// fill the rest.
type TableColumn struct {
	Title string
	Cells float32
}

// Table draws the rows of state in view under a header of columns, each
// row's cells from rows. A click on a row calls pick, a func(int) or
// func(int) any, with its index.
func Table(columns []TableColumn, rows [][]string, state *ListState, pick any) *Element {
	defer runtime.KeepAlive(state)
	titles := make([]string, len(columns))
	for i, column := range columns {
		titles[i] = column.Title
	}
	titleStrings, releaseTitles := cStrings(titles)
	defer releaseTitles()
	made := make([]C.KatuiColumn, len(columns))
	for i, column := range columns {
		made[i] = C.KatuiColumn{title: titleStrings[i], cells: C.float(column.Cells)}
	}
	var cells []string
	for _, row := range rows {
		for i := range columns {
			cell := ""
			if i < len(row) {
				cell = row[i]
			}
			cells = append(cells, cell)
		}
	}
	cellStrings, releaseCells := cStrings(cells)
	defer releaseCells()
	return element(C.katui_table(firstOf(made), C.size_t(len(made)), firstOf(cellStrings), C.size_t(len(rows)), state.p,
		C.uint64_t(key(pick))))
}

// Tree draws the visible nodes of roots, indented by depth. A click on a
// node's marker calls toggle, one on the node calls choose, each a
// func(uint64) or func(uint64) any, with the node's id.
func Tree(nodes []*TreeNode, state *TreeState, toggle, choose any) *Element {
	defer runtime.KeepAlive(state)
	defer runtime.KeepAlive(nodes)
	pointers := roots(nodes)
	return element(C.katui_tree(firstOf(pointers), C.size_t(len(pointers)), state.p, C.uint64_t(key(toggle)),
		C.uint64_t(key(choose))))
}
