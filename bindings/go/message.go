package katui

/*
#include "katui.h"
*/
import "C"

import "sync"

// Point is a position: pixels in a light buffer, or fractions of an
// element's box in a press message.
type Point struct {
	X, Y float32
}

// Scroll is where a scrolling element stands, in pixels.
type Scroll struct {
	// Offset is how far the content is scrolled.
	Offset float32
	// Max is the furthest the content can scroll.
	Max float32
}

// The engine only carries a uint64 key per message. We keep the Go value
// behind each key in this registry: the keys of the last two views, since
// a hovered element sends its leave message one frame late, and the keys
// wakers hold until they are used.
var registry = struct {
	sync.Mutex
	next     uint64
	values   map[uint64]entry
	building *[]uint64
}{values: map[uint64]entry{}}

type entry struct {
	value any
	once  bool
}

// key registers value for the view being built and returns the key the
// engine carries for it.
func key(value any) uint64 {
	registry.Lock()
	defer registry.Unlock()
	registry.next++
	registry.values[registry.next] = entry{value: value}
	if registry.building != nil {
		*registry.building = append(*registry.building, registry.next)
	}
	return registry.next
}

// keep registers value for a waker; a key kept once is forgotten when it is
// looked up.
func keep(value any, once bool) uint64 {
	registry.Lock()
	defer registry.Unlock()
	registry.next++
	registry.values[registry.next] = entry{value, once}
	return registry.next
}

func lookup(key uint64) (any, bool) {
	registry.Lock()
	defer registry.Unlock()
	found, ok := registry.values[key]
	if found.once {
		delete(registry.values, key)
	}
	return found.value, ok
}

func forget(keys []uint64) {
	registry.Lock()
	defer registry.Unlock()
	for _, key := range keys {
		delete(registry.values, key)
	}
}

// frames keeps the keys of one app's last two views.
type frames struct {
	previous, current []uint64
}

// build runs view, collecting the keys it registers, and forgets the keys
// of the view before last.
func (f *frames) build(view func()) {
	var keys []uint64
	registry.Lock()
	outer := registry.building
	registry.building = &keys
	registry.Unlock()
	defer func() {
		registry.Lock()
		registry.building = outer
		registry.Unlock()
		forget(f.previous)
		f.previous, f.current = f.current, keys
	}()
	view()
}

func (f *frames) close() {
	forget(f.previous)
	forget(f.current)
}

// payload is what a message carries besides its key, as a Go value.
func payload(message *C.KatuiMessage) any {
	switch message.payload {
	case C.KATUI_PAYLOAD_INDEX:
		return int(message.index)
	case C.KATUI_PAYLOAD_ID:
		return uint64(message.id)
	case C.KATUI_PAYLOAD_FLOAT:
		return float32(message.value)
	case C.KATUI_PAYLOAD_POINT:
		return Point{float32(message.x), float32(message.y)}
	case C.KATUI_PAYLOAD_SCROLL:
		return Scroll{float32(message.scroll.offset), float32(message.scroll.max)}
	case C.KATUI_PAYLOAD_LINK:
		return C.GoString(message.link)
	}
	return nil
}

// apply turns a message maker and what was picked into the message, or
// runs a handler. It reports whether value took the payload.
func apply(value, picked any) (message any, handled bool) {
	switch picked := picked.(type) {
	case int:
		return call(value, picked)
	case uint64:
		return call(value, picked)
	case float32:
		return call(value, picked)
	case Point:
		return call(value, picked)
	case Scroll:
		return call(value, picked)
	case string:
		if value == nil {
			return picked, false
		}
		return call(value, picked)
	}
	return value, false
}

func call[T any](value any, picked T) (any, bool) {
	switch maker := value.(type) {
	case func(T):
		maker(picked)
		return nil, true
	case func(T) any:
		made := maker(picked)
		return made, made == nil
	}
	return value, false
}

// deliver hands message to update, after running it when it is a func.
func deliver(message any, update func(any) Control) Control {
	switch handler := message.(type) {
	case func():
		handler()
		return Redraw
	case func() Control:
		return handler()
	}
	return update(message)
}

// resolve delivers the message behind a C message. A key from a view long
// gone, or registered with a nil message, delivers nothing.
func resolve(message *C.KatuiMessage, update func(any) Control) Control {
	value, known := lookup(uint64(message.key))
	if !known && message.payload != C.KATUI_PAYLOAD_LINK {
		return Wait
	}
	made, handled := apply(value, payload(message))
	if made == nil && !handled {
		return Wait
	}
	if handled {
		return Redraw
	}
	return deliver(made, update)
}
