package katui

/*
#include "katui.h"
#include "callbacks.h"
*/
import "C"

import (
	"runtime"
	"runtime/cgo"
)

// Document says what links and images in an HTML or Markdown document do,
// and how its text looks. The zero Document takes every default.
type Document struct {
	// OnLink handles a click on a link: a func(string) or func(string) any
	// called with the href. Without it, the href itself goes to Update.
	OnLink any
	// Images gives the image for an src, or nil to leave it out.
	Images func(src string) *Image
	// FontSize is the body text size in pixels, or 0 for the theme's.
	FontSize float32
	// Color is the text color, Link the link color and Tint the color of
	// quotes and rules; the zero Color takes the theme's.
	Color, Link, Tint Color
}

// images keeps every image Images returned alive until the engine holds
// its own reference.
type images struct {
	find  func(string) *Image
	given []*Image
}

//export katuiGoImage
func katuiGoImage(user C.uintptr_t, src *C.char) *C.KatuiImage {
	kept := cgo.Handle(user).Value().(*images)
	found := kept.find(C.GoString(src))
	kept.given = append(kept.given, found)
	return found.ptr()
}

func document(source string, markdown bool, doc Document) *Element {
	s := cString(source)
	defer free(s)
	var linkKey uint64
	if doc.OnLink != nil {
		linkKey = key(doc.OnLink)
	}
	found := &images{find: doc.Images}
	handle := cgo.NewHandle(found)
	defer handle.Delete()
	defer runtime.KeepAlive(found)
	return element(C.katui_go_document(s, C.bool(markdown), C.uintptr_t(handle), C.bool(doc.Images != nil),
		C.uint64_t(linkKey), C.float(doc.FontSize), doc.Color.c(), doc.Link.c(), doc.Tint.c()))
}

// HTML is an HTML document as an element.
func HTML(source string, doc Document) *Element {
	return document(source, false, doc)
}

// Markdown is a Markdown document as an element.
func Markdown(source string, doc Document) *Element {
	return document(source, true, doc)
}
