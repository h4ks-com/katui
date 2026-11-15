// Package katui draws apps, documents and games in real pixels inside the
// terminal, through the kitty graphics protocol, sixel or colored blocks.
//
// An app is a value with a View that returns elements and an Update that
// takes the messages clicks send. Run runs it on the terminal, and Screen
// runs it without one, for tests.
//
// A message is any value: a func() or func() Control runs when it is sent,
// and anything else goes to Update. A widget that reports what was picked
// takes a func(T) that handles it, or a func(T) any that makes the message
// from it.
package katui

/*
#cgo CFLAGS: -I${SRCDIR}/include
#cgo darwin,arm64 LDFLAGS: -L${SRCDIR}/lib/darwin_arm64
#cgo darwin,amd64 LDFLAGS: -L${SRCDIR}/lib/darwin_amd64
#cgo linux,arm64 LDFLAGS: -L${SRCDIR}/lib/linux_arm64
#cgo linux,amd64 LDFLAGS: -L${SRCDIR}/lib/linux_amd64
#cgo LDFLAGS: -lkatui
#cgo darwin LDFLAGS: -framework CoreFoundation -liconv -lm
#cgo linux LDFLAGS: -lm -ldl -lpthread -lrt -lutil -lgcc_s
#include <stdlib.h>
#include "katui.h"
*/
import "C"

import (
	"fmt"
	"unsafe"
)

// Version is the version of the katui engine, like "0.1.0".
func Version() string {
	return C.GoString(C.katui_version())
}

// Error is a failure katui reported, with its status code.
type Error struct {
	Status  int
	Message string
}

func (e *Error) Error() string {
	return "katui: " + e.Message
}

// lastError is the error behind status, or nil for KATUI_STATUS_OK.
func lastError(status C.KatuiStatus) error {
	if status == C.KATUI_STATUS_OK {
		return nil
	}
	return &Error{Status: int(status), Message: C.GoString(C.katui_last_error())}
}

// nullError is the error katui kept when a call returned NULL.
func nullError(what string) error {
	message := C.katui_last_error()
	if message == nil {
		return &Error{Message: fmt.Sprintf("%s failed", what)}
	}
	return &Error{Message: C.GoString(message)}
}

// cString is s as a C string the caller frees with free.
func cString(s string) *C.char {
	return C.CString(s)
}

func free(s *C.char) {
	C.free(unsafe.Pointer(s))
}

// goString is the Go copy of a string katui returned, which it frees.
func goString(s *C.char) string {
	if s == nil {
		return ""
	}
	defer C.katui_string_free(s)
	return C.GoString(s)
}

// cStrings is texts as C strings, with the function that frees them.
func cStrings(texts []string) ([]*C.char, func()) {
	strings := make([]*C.char, len(texts))
	for i, text := range texts {
		strings[i] = cString(text)
	}
	return strings, func() {
		for _, s := range strings {
			free(s)
		}
	}
}

// firstOf is the address of the first item, or nil for none.
func firstOf[T any](items []T) *T {
	if len(items) == 0 {
		return nil
	}
	return &items[0]
}
