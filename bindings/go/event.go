package katui

/*
#include "katui.h"
*/
import "C"

import "runtime"

// Event is a key press, a release, a mouse event, a paste or a focus change.
type Event struct {
	p *C.KatuiEvent
}

func event(p *C.KatuiEvent) *Event {
	e := &Event{p}
	runtime.SetFinalizer(e, func(e *Event) { C.katui_event_free(e.p) })
	return e
}

func (e *Event) ptr() *C.KatuiEvent {
	if e == nil {
		return nil
	}
	return e.p
}

// Modifier is a key held with another: Ctrl, Alt, Shift, Super, Hyper or
// Meta.
type Modifier int

const (
	Ctrl Modifier = iota
	Alt
	Shift
	Super
	Hyper
	Meta
)

// Press is a press of the key named name: one character, or "enter",
// "tab", "escape", "up", "page_down", "f1" and so on. It is for tests and
// for handing keys to widgets.
func Press(name string, modifiers ...Modifier) (*Event, error) {
	held := [6]bool{}
	for _, modifier := range modifiers {
		held[modifier] = true
	}
	s := cString(name)
	defer free(s)
	p := C.katui_event_press(s, C.bool(held[Ctrl]), C.bool(held[Alt]), C.bool(held[Shift]),
		C.bool(held[Super]), C.bool(held[Hyper]), C.bool(held[Meta]))
	if p == nil {
		return nil, nullError("press " + name)
	}
	return event(p), nil
}

// Paste is a paste of text.
func Paste(text string) *Event {
	s := cString(text)
	defer free(s)
	return event(C.katui_event_paste(s))
}

// EventKind says what an event is.
type EventKind uint32

const (
	OtherEvent     EventKind = C.KATUI_EVENT_OTHER
	KeyEvent       EventKind = C.KATUI_EVENT_KEY
	KeyRepeatEvent EventKind = C.KATUI_EVENT_KEY_REPEAT
	KeyUpEvent     EventKind = C.KATUI_EVENT_KEY_UP
	MouseEvent     EventKind = C.KATUI_EVENT_MOUSE
	PasteEvent     EventKind = C.KATUI_EVENT_PASTE
	FocusEvent     EventKind = C.KATUI_EVENT_FOCUS
)

func (e *Event) Kind() EventKind {
	defer runtime.KeepAlive(e)
	return EventKind(C.katui_event_kind(e.ptr()))
}

// Key is the key's name: one character, "enter", "up", "f1" and so on, or
// "" for an event without a key.
func (e *Event) Key() string {
	defer runtime.KeepAlive(e)
	return goString(C.katui_event_key(e.ptr()))
}

func (e *Event) Ctrl() bool {
	defer runtime.KeepAlive(e)
	return bool(C.katui_event_ctrl(e.ptr()))
}

func (e *Event) Alt() bool {
	defer runtime.KeepAlive(e)
	return bool(C.katui_event_alt(e.ptr()))
}

func (e *Event) Shift() bool {
	defer runtime.KeepAlive(e)
	return bool(C.katui_event_shift(e.ptr()))
}

func (e *Event) Super() bool {
	defer runtime.KeepAlive(e)
	return bool(C.katui_event_super(e.ptr()))
}

func (e *Event) Hyper() bool {
	defer runtime.KeepAlive(e)
	return bool(C.katui_event_hyper(e.ptr()))
}

func (e *Event) Meta() bool {
	defer runtime.KeepAlive(e)
	return bool(C.katui_event_meta(e.ptr()))
}

// Text is the pasted text, or "" for another event.
func (e *Event) Text() string {
	defer runtime.KeepAlive(e)
	return goString(C.katui_event_text(e.ptr()))
}

// Focused is whether the window has the focus, for a focus event.
func (e *Event) Focused() bool {
	defer runtime.KeepAlive(e)
	return bool(C.katui_event_focused(e.ptr()))
}

// Position is the cell under the pointer, and false for an event without
// a pointer.
func (e *Event) Position() (column, row int, ok bool) {
	defer runtime.KeepAlive(e)
	var x, y C.uint32_t
	ok = bool(C.katui_event_position(e.ptr(), &x, &y))
	return int(x), int(y), ok
}

// MouseAction says what the mouse did.
type MouseAction uint32

const (
	MouseNone       MouseAction = C.KATUI_MOUSE_NONE
	MousePress      MouseAction = C.KATUI_MOUSE_PRESS
	MouseRelease    MouseAction = C.KATUI_MOUSE_RELEASE
	MouseDrag       MouseAction = C.KATUI_MOUSE_DRAG
	MouseMove       MouseAction = C.KATUI_MOUSE_MOVE
	MouseWheelUp    MouseAction = C.KATUI_MOUSE_WHEEL_UP
	MouseWheelDown  MouseAction = C.KATUI_MOUSE_WHEEL_DOWN
	MouseWheelLeft  MouseAction = C.KATUI_MOUSE_WHEEL_LEFT
	MouseWheelRight MouseAction = C.KATUI_MOUSE_WHEEL_RIGHT
)

func (e *Event) Action() MouseAction {
	defer runtime.KeepAlive(e)
	return MouseAction(C.katui_event_action(e.ptr()))
}

// MouseButton says which mouse button.
type MouseButton uint32

const (
	NoButton      MouseButton = C.KATUI_BUTTON_NONE
	LeftButton    MouseButton = C.KATUI_BUTTON_LEFT
	MiddleButton  MouseButton = C.KATUI_BUTTON_MIDDLE
	RightButton   MouseButton = C.KATUI_BUTTON_RIGHT
	BackButton    MouseButton = C.KATUI_BUTTON_BACK
	ForwardButton MouseButton = C.KATUI_BUTTON_FORWARD
)

// Button is the button of a press, release or drag.
func (e *Event) Button() MouseButton {
	defer runtime.KeepAlive(e)
	return MouseButton(C.katui_event_button(e.ptr()))
}
