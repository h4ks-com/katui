package katui

/*
#include "katui.h"
#include "callbacks.h"
*/
import "C"

import (
	"fmt"
	"runtime"
	"runtime/cgo"
	"time"
)

// Control says what to do after a message or an event.
type Control uint32

const (
	// Wait draws nothing until the next event.
	Wait Control = C.KATUI_CONTROL_WAIT
	// Redraw calls View once and draws the changes.
	Redraw Control = C.KATUI_CONTROL_REDRAW
	// Animate keeps drawing at the frame rate.
	Animate Control = C.KATUI_CONTROL_ANIMATE
	// Quit ends the app and restores the terminal.
	Quit Control = C.KATUI_CONTROL_QUIT
)

// App is a pixel UI program: state that shows itself as a view and changes
// on messages. An App can also be an Inputter, a Themer and a Starter.
type App interface {
	// View is the view of the current state.
	View() *Element
	// Update applies a message an element or a Waker sent.
	Update(message any) Control
}

// Inputter is an App that reacts to keys and pastes.
type Inputter interface {
	// Input reacts to a key or a paste. Ctrl+C quits before it is called.
	Input(event *Event) Control
}

// Themer is an App that picks its theme, asked before every frame.
type Themer interface {
	// Theme is the theme for cells of cell pixels.
	Theme(cell Cell) *Theme
}

// Starter is an App that keeps a Waker to send messages from other
// goroutines.
type Starter interface {
	Start(waker *Waker)
}

// bridge is an App as the engine sees it.
type bridge struct {
	app     App
	frames  frames
	failure error
}

// guard keeps the first panic of a callback as the bridge's failure.
func (b *bridge) guard() {
	if caught := recover(); caught != nil && b.failure == nil {
		b.failure = fmt.Errorf("katui: a callback panicked: %v", caught)
	}
}

func bridgeOf(user C.uintptr_t) *bridge {
	return cgo.Handle(user).Value().(*bridge)
}

//export katuiGoView
func katuiGoView(user C.uintptr_t) (made *C.KatuiElement) {
	b := bridgeOf(user)
	defer b.guard()
	if b.failure != nil {
		return nil
	}
	b.frames.build(func() { made = b.app.View().take() })
	return made
}

//export katuiGoUpdate
func katuiGoUpdate(user C.uintptr_t, message *C.KatuiMessage) (control C.KatuiControl) {
	b := bridgeOf(user)
	control = C.KATUI_CONTROL_QUIT
	defer b.guard()
	if b.failure != nil {
		return control
	}
	return C.KatuiControl(resolve(message, b.app.Update))
}

//export katuiGoInput
func katuiGoInput(user C.uintptr_t, e *C.KatuiEvent) (control C.KatuiControl) {
	b := bridgeOf(user)
	control = C.KATUI_CONTROL_QUIT
	defer b.guard()
	inputter, ok := b.app.(Inputter)
	if b.failure != nil || !ok {
		return C.KATUI_CONTROL_WAIT
	}
	return C.KatuiControl(inputter.Input(event(C.katui_event_clone(e))))
}

//export katuiGoTheme
func katuiGoTheme(user C.uintptr_t, width, height C.uint16_t) *C.KatuiTheme {
	b := bridgeOf(user)
	defer b.guard()
	themer, ok := b.app.(Themer)
	if b.failure != nil || !ok {
		return nil
	}
	return themer.Theme(Cell{uint16(width), uint16(height)}).take()
}

//export katuiGoStart
func katuiGoStart(user C.uintptr_t, p *C.KatuiWaker) {
	b := bridgeOf(user)
	defer b.guard()
	w := waker(p)
	if starter, ok := b.app.(Starter); ok {
		starter.Start(w)
	}
}

// Run runs app on the terminal until it quits. A panic in one of its
// methods ends it, and Run returns it as an error.
func Run(app App) error {
	b := &bridge{app: app}
	handle := cgo.NewHandle(b)
	defer handle.Delete()
	defer b.frames.close()
	status := C.katui_go_run(C.uintptr_t(handle))
	if b.failure != nil {
		return b.failure
	}
	return lastError(status)
}

// Waker sends messages to a running app from any goroutine.
type Waker struct {
	p *C.KatuiWaker
}

func waker(p *C.KatuiWaker) *Waker {
	w := &Waker{p}
	runtime.SetFinalizer(w, func(w *Waker) { C.katui_waker_free(w.p) })
	return w
}

// Send hands message to the app's Update, and reports false once the app
// has quit.
func (w *Waker) Send(message any) bool {
	defer runtime.KeepAlive(w)
	return bool(C.katui_waker_send(w.p, C.uint64_t(keep(message, true))))
}

// Every hands message to Update every interval until the app quits.
func (w *Waker) Every(interval time.Duration, message any) {
	defer runtime.KeepAlive(w)
	C.katui_waker_every(w.p, C.double(interval.Seconds()), C.uint64_t(keep(message, false)))
}
