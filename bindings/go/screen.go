package katui

/*
#include "katui.h"
#include "callbacks.h"
*/
import "C"

import (
	"errors"
	"runtime"
	"runtime/cgo"
)

// Screen is an app drawn into pixels without a terminal, with clicks and
// keys sent by hand, for tests and screenshots.
type Screen struct {
	p      *C.KatuiScreen
	bridge *bridge
	handle cgo.Handle
}

// NewScreen is app on a screen of width by height pixels, in cells of 10 by
// 20 pixels or of cell. Close frees it.
func NewScreen(app App, width, height int, cell ...Cell) (*Screen, error) {
	size := Cell{10, 20}
	if len(cell) > 0 {
		size = cell[0]
	}
	b := &bridge{app: app}
	handle := cgo.NewHandle(b)
	p := C.katui_go_screen_new(C.uintptr_t(handle), C.uint32_t(width), C.uint32_t(height),
		C.uint16_t(size.Width), C.uint16_t(size.Height))
	if p == nil {
		handle.Delete()
		return nil, nullError("make the screen")
	}
	return &Screen{p, b, handle}, nil
}

// Close frees the screen.
func (s *Screen) Close() {
	if s.p == nil {
		return
	}
	C.katui_screen_free(s.p)
	s.p = nil
	s.bridge.frames.close()
	s.handle.Delete()
}

// checked is the panic a callback raised, or the error behind status.
func (s *Screen) checked(status C.KatuiStatus) error {
	if failure := s.bridge.failure; failure != nil {
		s.bridge.failure = nil
		return failure
	}
	return lastError(status)
}

func (s *Screen) live() error {
	if s.p == nil {
		return errors.New("katui: use a screen before closing it")
	}
	return nil
}

// Render draws the app's view and returns the picture.
func (s *Screen) Render() (*Image, error) {
	if err := s.live(); err != nil {
		return nil, err
	}
	made := C.katui_screen_render(s.p)
	if err := s.checked(C.KATUI_STATUS_OK); err != nil {
		C.katui_image_free(made)
		return nil, err
	}
	if made == nil {
		return nil, nullError("render")
	}
	return image(made), nil
}

// Click clicks at x, y pixels and delivers what the click sends.
func (s *Screen) Click(x, y float32) error {
	if err := s.live(); err != nil {
		return err
	}
	return s.checked(C.katui_screen_click(s.p, C.float(x), C.float(y)))
}

// Press presses at x, y and keeps holding, for a drag.
func (s *Screen) Press(x, y float32) error {
	if err := s.live(); err != nil {
		return err
	}
	return s.checked(C.katui_screen_press(s.p, C.float(x), C.float(y)))
}

// Drag moves a held press to x, y.
func (s *Screen) Drag(x, y float32) error {
	if err := s.live(); err != nil {
		return err
	}
	return s.checked(C.katui_screen_drag(s.p, C.float(x), C.float(y)))
}

// Release lets go of a held press.
func (s *Screen) Release() error {
	if err := s.live(); err != nil {
		return err
	}
	return s.checked(C.katui_screen_release(s.p))
}

// Wheel turns the wheel once at x, y, down or up.
func (s *Screen) Wheel(x, y float32, down bool) error {
	if err := s.live(); err != nil {
		return err
	}
	return s.checked(C.katui_screen_wheel(s.p, C.float(x), C.float(y), C.bool(down)))
}

// Hover moves the pointer to x, y: the element it left gets its leave
// message, the one it entered its hover message.
func (s *Screen) Hover(x, y float32) error {
	if err := s.live(); err != nil {
		return err
	}
	return s.checked(C.katui_screen_hover(s.p, C.float(x), C.float(y)))
}

// Send hands e to the app's Input.
func (s *Screen) Send(e *Event) error {
	defer runtime.KeepAlive(e)
	if err := s.live(); err != nil {
		return err
	}
	return s.checked(C.katui_screen_send(s.p, e.ptr()))
}

// TypeText types text, one key press per character.
func (s *Screen) TypeText(text string) error {
	if err := s.live(); err != nil {
		return err
	}
	c := cString(text)
	defer free(c)
	return s.checked(C.katui_screen_type_text(s.p, c))
}

// Quit reports whether the app asked to quit.
func (s *Screen) Quit() bool {
	return bool(C.katui_screen_quit(s.p))
}
