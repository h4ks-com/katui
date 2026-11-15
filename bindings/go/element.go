package katui

/*
#include <stdlib.h>
#include "katui.h"
*/
import "C"

import (
	"runtime"
	"unsafe"
)

// Element is one node of a view. Its methods change it and return it, so
// calls chain. An element passed to another one as a child belongs to that
// one from then on.
type Element struct {
	p *C.KatuiElement
}

func element(p *C.KatuiElement) *Element {
	e := &Element{p}
	runtime.SetFinalizer(e, (*Element).free)
	return e
}

func (e *Element) free() {
	if e.p != nil {
		C.katui_element_free(e.p)
		e.p = nil
	}
}

// take hands the element over to the engine.
func (e *Element) take() *C.KatuiElement {
	if e == nil {
		return nil
	}
	p := e.p
	e.p = nil
	return p
}

func (e *Element) change(p *C.KatuiElement) *Element {
	if e != nil {
		e.p = p
	}
	return e
}

func (e *Element) ptr() *C.KatuiElement {
	if e == nil {
		return nil
	}
	return e.p
}

// children hands each element over, leaving nil ones out.
func children(elements []*Element) []*C.KatuiElement {
	taken := make([]*C.KatuiElement, 0, len(elements))
	for _, child := range elements {
		if p := child.take(); p != nil {
			taken = append(taken, p)
		}
	}
	return taken
}

// Color is a straight-alpha color.
type Color struct {
	R, G, B, A uint8
}

func (c Color) c() C.KatuiColor {
	return C.KatuiColor{r: C.uint8_t(c.R), g: C.uint8_t(c.G), b: C.uint8_t(c.B), a: C.uint8_t(c.A)}
}

func color(c C.KatuiColor) Color {
	return Color{uint8(c.r), uint8(c.g), uint8(c.b), uint8(c.a)}
}

// Length is a width or a height: Px(n), Fill or Content.
type Length struct {
	kind   C.KatuiLengthKind
	pixels float32
}

// Px is a length of n pixels.
func Px(n float32) Length {
	return Length{C.KATUI_LENGTH_PX, n}
}

var (
	// Fill takes the space the parent leaves.
	Fill = Length{kind: C.KATUI_LENGTH_FILL}
	// Content fits the content.
	Content = Length{kind: C.KATUI_LENGTH_CONTENT}
)

func (l Length) c() C.KatuiLength {
	return C.KatuiLength{kind: l.kind, pixels: C.float(l.pixels)}
}

// Align says where children sit along an axis.
type Align uint32

const (
	Start  Align = C.KATUI_ALIGN_START
	Center Align = C.KATUI_ALIGN_CENTER
	End    Align = C.KATUI_ALIGN_END
)

// Row places items side by side. Nil items are left out.
func Row(items ...*Element) *Element {
	taken := children(items)
	return element(C.katui_row(firstOf(taken), C.size_t(len(taken))))
}

// Column places items top to bottom. Nil items are left out.
func Column(items ...*Element) *Element {
	taken := children(items)
	return element(C.katui_column(firstOf(taken), C.size_t(len(taken))))
}

// Stack places items on top of each other. Nil items are left out.
func Stack(items ...*Element) *Element {
	taken := children(items)
	return element(C.katui_stack(firstOf(taken), C.size_t(len(taken))))
}

// Text is a line of text in the current theme's look.
func Text(text string) *Element {
	s := cString(text)
	defer free(s)
	return element(C.katui_text(s))
}

// Span is a run of text with its own look. A zero color leaves the text's
// color or background as it is.
type Span struct {
	Text                            string
	Color, Background               Color
	Bold, Italic, Underline, Strike bool
}

// Rich is text made of spans, each with its own look.
func Rich(spans ...Span) *Element {
	made := make([]C.KatuiSpan, len(spans))
	texts := make([]*C.char, len(spans))
	defer func() {
		for _, s := range texts {
			free(s)
		}
	}()
	for i, span := range spans {
		texts[i] = cString(span.Text)
		made[i] = C.KatuiSpan{
			text:       texts[i],
			color:      span.Color.c(),
			background: span.Background.c(),
			bold:       C.bool(span.Bold),
			italic:     C.bool(span.Italic),
			underline:  C.bool(span.Underline),
			strike:     C.bool(span.Strike),
		}
	}
	return element(C.katui_rich(firstOf(made), C.size_t(len(made))))
}

// ImageView shows img. The element keeps its own reference to the pixels.
func ImageView(img *Image) *Element {
	defer runtime.KeepAlive(img)
	return element(C.katui_image(img.ptr()))
}

// Spacer is empty space that grows to fill what the others leave.
func Spacer() *Element {
	return element(C.katui_spacer())
}

// Layer draws this subtree into its own layer, kept under key between frames.
func (e *Element) Layer(key uint64) *Element {
	return e.change(C.katui_element_layer(e.ptr(), C.uint64_t(key)))
}

// Scroll scrolls the children by offset pixels.
func (e *Element) Scroll(offset float32) *Element {
	return e.change(C.katui_element_scroll(e.ptr(), C.float(offset)))
}

// OnClick sends message when the element is clicked.
func (e *Element) OnClick(message any) *Element {
	return e.change(C.katui_element_on_click(e.ptr(), C.uint64_t(key(message))))
}

// OnSpanClick sends message when the span at index span of a Rich text is
// clicked.
func (e *Element) OnSpanClick(span int, message any) *Element {
	return e.change(C.katui_element_on_span_click(e.ptr(), C.size_t(span), C.uint64_t(key(message))))
}

// OnHover sends message when the pointer enters.
func (e *Element) OnHover(message any) *Element {
	return e.change(C.katui_element_on_hover(e.ptr(), C.uint64_t(key(message))))
}

// OnLeave sends message when the pointer leaves.
func (e *Element) OnLeave(message any) *Element {
	return e.change(C.katui_element_on_leave(e.ptr(), C.uint64_t(key(message))))
}

// OnScroll calls moved, a func(Scroll) or func(Scroll) any, with each new
// scroll position.
func (e *Element) OnScroll(moved any) *Element {
	return e.change(C.katui_element_on_scroll(e.ptr(), C.uint64_t(key(moved))))
}

// OnPress calls pressed, a func(Point) or func(Point) any, on the press and
// each move until the release, with the pointer as fractions of the box.
func (e *Element) OnPress(pressed any) *Element {
	return e.change(C.katui_element_on_press(e.ptr(), C.uint64_t(key(pressed))))
}

// OnGripPress is OnPress for presses on a child marked with Grip.
func (e *Element) OnGripPress(pressed any) *Element {
	return e.change(C.katui_element_on_grip_press(e.ptr(), C.uint64_t(key(pressed))))
}

// Grip marks the element as the handle of its parent's OnGripPress.
func (e *Element) Grip() *Element {
	return e.change(C.katui_element_grip(e.ptr()))
}

// Popup draws the element above everything, at x, y pixels from its
// parent's corner.
func (e *Element) Popup(x, y float32) *Element {
	return e.change(C.katui_element_popup(e.ptr(), C.float(x), C.float(y)))
}

// Opaque makes the element catch clicks over its whole box.
func (e *Element) Opaque() *Element {
	return e.change(C.katui_element_opaque(e.ptr()))
}

// Scrollbar shows a scrollbar in c.
func (e *Element) Scrollbar(c Color) *Element {
	return e.change(C.katui_element_scrollbar(e.ptr(), c.c()))
}

// ID names the element, so it keeps its state when the tree changes
// around it.
func (e *Element) ID(id uint64) *Element {
	return e.change(C.katui_element_id(e.ptr(), C.uint64_t(id)))
}

// Padding pads every side by all pixels.
func (e *Element) Padding(all float32) *Element {
	return e.change(C.katui_element_padding(e.ptr(), C.float(all)))
}

// PaddingXY pads the sides by x pixels and the top and bottom by y.
func (e *Element) PaddingXY(x, y float32) *Element {
	return e.change(C.katui_element_padding_xy(e.ptr(), C.float(x), C.float(y)))
}

// Gap puts gap pixels between the children.
func (e *Element) Gap(gap float32) *Element {
	return e.change(C.katui_element_gap(e.ptr(), C.float(gap)))
}

func (e *Element) Width(width Length) *Element {
	return e.change(C.katui_element_width(e.ptr(), width.c()))
}

func (e *Element) Height(height Length) *Element {
	return e.change(C.katui_element_height(e.ptr(), height.c()))
}

func (e *Element) Size(width, height Length) *Element {
	return e.change(C.katui_element_size(e.ptr(), width.c(), height.c()))
}

// Grow is how much of the free space the element takes, against its
// siblings.
func (e *Element) Grow(grow float32) *Element {
	return e.change(C.katui_element_grow(e.ptr(), C.float(grow)))
}

// Align places the children across the main axis.
func (e *Element) Align(to Align) *Element {
	return e.change(C.katui_element_align(e.ptr(), C.KatuiAlign(to)))
}

// Justify places the children along the main axis.
func (e *Element) Justify(to Align) *Element {
	return e.change(C.katui_element_justify(e.ptr(), C.KatuiAlign(to)))
}

func (e *Element) Background(c Color) *Element {
	return e.change(C.katui_element_background(e.ptr(), c.c()))
}

// Radius rounds the corners by radius pixels.
func (e *Element) Radius(radius float32) *Element {
	return e.change(C.katui_element_radius(e.ptr(), C.float(radius)))
}

func (e *Element) Border(width float32, c Color) *Element {
	return e.change(C.katui_element_border(e.ptr(), C.float(width), c.c()))
}

// Color sets the text color.
func (e *Element) Color(c Color) *Element {
	return e.change(C.katui_element_color(e.ptr(), c.c()))
}

func (e *Element) FontSize(size float32) *Element {
	return e.change(C.katui_element_font_size(e.ptr(), C.float(size)))
}

// Weight sets the font weight, from 100 to 900; 400 is regular and 700 bold.
func (e *Element) Weight(weight uint16) *Element {
	return e.change(C.katui_element_weight(e.ptr(), C.uint16_t(weight)))
}

// Image is pixels to show, decoded once and shared, so an unchanged image
// costs nothing to draw again.
type Image struct {
	p *C.KatuiImage
}

func image(p *C.KatuiImage) *Image {
	img := &Image{p}
	runtime.SetFinalizer(img, func(img *Image) { C.katui_image_free(img.p) })
	return img
}

func (img *Image) ptr() *C.KatuiImage {
	if img == nil {
		return nil
	}
	return img.p
}

// NewImage is an image from straight-alpha RGBA bytes, four per pixel.
func NewImage(width, height int, rgba []byte) (*Image, error) {
	p := C.katui_image_from_rgba(C.uint32_t(width), C.uint32_t(height), (*C.uint8_t)(firstOf(rgba)), C.size_t(len(rgba)))
	if p == nil {
		return nil, nullError("make the image")
	}
	return image(p), nil
}

// DecodeImage decodes PNG, JPEG, GIF or WebP bytes.
func DecodeImage(data []byte) (*Image, error) {
	p := C.katui_image_decode((*C.uint8_t)(firstOf(data)), C.size_t(len(data)))
	if p == nil {
		return nil, nullError("decode the image")
	}
	return image(p), nil
}

// OpenImage reads a PNG, JPEG, GIF or WebP file.
func OpenImage(path string) (*Image, error) {
	s := cString(path)
	defer free(s)
	p := C.katui_image_open(s)
	if p == nil {
		return nil, nullError("open the image")
	}
	return image(p), nil
}

func (img *Image) Width() int {
	defer runtime.KeepAlive(img)
	return int(C.katui_image_width(img.ptr()))
}

func (img *Image) Height() int {
	defer runtime.KeepAlive(img)
	return int(C.katui_image_height(img.ptr()))
}

// RGBA is a copy of the pixels, four bytes per pixel, row by row.
func (img *Image) RGBA() []byte {
	defer runtime.KeepAlive(img)
	var length C.size_t
	pixels := C.katui_image_rgba(img.ptr(), &length)
	return C.GoBytes(unsafe.Pointer(pixels), C.int(length))
}

// Pixel is the color at x, y, and false outside the image.
func (img *Image) Pixel(x, y int) (Color, bool) {
	defer runtime.KeepAlive(img)
	var c C.KatuiColor
	ok := C.katui_image_pixel(img.ptr(), C.uint32_t(x), C.uint32_t(y), &c)
	return color(c), bool(ok)
}

// Save writes the image to path as a PNG.
func (img *Image) Save(path string) error {
	defer runtime.KeepAlive(img)
	s := cString(path)
	defer free(s)
	return lastError(C.katui_image_save(img.ptr(), s))
}
