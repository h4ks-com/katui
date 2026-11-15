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
)

const (
	// BaseImage is the first kitty image id games use.
	BaseImage = C.KATUI_BASE_IMAGE
	// MultiplexedShrink is how much smaller games draw under a terminal
	// multiplexer.
	MultiplexedShrink = C.KATUI_MULTIPLEXED_SHRINK
)

// RGB is light, in linear channels; values above 1 bloom.
type RGB struct {
	R, G, B float32
}

func (c RGB) c() C.KatuiRgb {
	return C.KatuiRgb{r: C.float(c.R), g: C.float(c.G), b: C.float(c.B)}
}

func rgb(c C.KatuiRgb) RGB {
	return RGB{float32(c.r), float32(c.g), float32(c.b)}
}

// SRGB is the light that shows as the sRGB color red, green, blue.
func SRGB(red, green, blue uint8) RGB {
	return rgb(C.katui_srgb(C.uint8_t(red), C.uint8_t(green), C.uint8_t(blue)))
}

// Scaled is c with every channel times factor.
func Scaled(c RGB, factor float32) RGB {
	return rgb(C.katui_scaled(c.c(), C.float(factor)))
}

// Lerp3 is the color amount of the way from start to end.
func Lerp3(start, end RGB, amount float32) RGB {
	return rgb(C.katui_lerp3(start.c(), end.c(), C.float(amount)))
}

func (p Point) c() C.KatuiPoint {
	return C.KatuiPoint{x: C.float(p.X), y: C.float(p.Y)}
}

// Blend says how a shape meets the light under it.
type Blend uint32

const (
	// BlendOver covers what is there.
	BlendOver Blend = C.KATUI_BLEND_OVER
	// BlendAdd adds to what is there.
	BlendAdd Blend = C.KATUI_BLEND_ADD
)

// Light is a floating-point framebuffer games add light to. The Light a
// game gets in Render and Resize lives during that call only.
type Light struct {
	p *C.KatuiLight
}

// NewLight is a black light buffer of width by height pixels.
func NewLight(width, height int, pixelAspect float32) *Light {
	l := &Light{C.katui_light_new(C.uint32_t(width), C.uint32_t(height), C.float(pixelAspect))}
	runtime.SetFinalizer(l, func(l *Light) { C.katui_light_free(l.p) })
	return l
}

func (l *Light) Width() int {
	defer runtime.KeepAlive(l)
	return int(C.katui_light_width(l.p))
}

func (l *Light) Height() int {
	defer runtime.KeepAlive(l)
	return int(C.katui_light_height(l.p))
}

// PixelAspect is one pixel's width over its height.
func (l *Light) PixelAspect() float32 {
	defer runtime.KeepAlive(l)
	return float32(C.katui_light_pixel_aspect(l.p))
}

// CopyFrom copies the pixels of other, a light of the same size.
func (l *Light) CopyFrom(other *Light) {
	defer runtime.KeepAlive(l)
	defer runtime.KeepAlive(other)
	C.katui_light_copy_from(l.p, other.p)
}

// Paint fills every pixel with shade(x, y), for backdrops baked once.
func (l *Light) Paint(shade func(x, y int) RGB) {
	defer runtime.KeepAlive(l)
	handle := cgo.NewHandle(shade)
	defer handle.Delete()
	C.katui_go_light_paint(l.p, C.uintptr_t(handle))
}

//export katuiGoShade
func katuiGoShade(user C.uintptr_t, x, y C.size_t) C.KatuiRgb {
	shade := cgo.Handle(user).Value().(func(int, int) RGB)
	return shade(int(x), int(y)).c()
}

// Dim fades the last frame by factor, 0 for black and 1 to keep it.
func (l *Light) Dim(factor float32) {
	defer runtime.KeepAlive(l)
	C.katui_light_dim(l.p, C.float(factor))
}

func (l *Light) Rect(at Point, width, height float32, c RGB, blend Blend) {
	defer runtime.KeepAlive(l)
	C.katui_light_rect(l.p, at.c(), C.float(width), C.float(height), c.c(), C.KatuiBlend(blend))
}

// Polygon fills the polygon through points.
func (l *Light) Polygon(points []Point, c RGB, blend Blend) {
	defer runtime.KeepAlive(l)
	made := make([]C.KatuiPoint, len(points))
	for i, point := range points {
		made[i] = point.c()
	}
	C.katui_light_polygon(l.p, firstOf(made), C.size_t(len(made)), c.c(), C.KatuiBlend(blend))
}

func (l *Light) Line(from, to Point, width float32, c RGB) {
	defer runtime.KeepAlive(l)
	C.katui_light_line(l.p, from.c(), to.c(), C.float(width), c.c())
}

// Glow is a soft disc of light that fades out at radius.
func (l *Light) Glow(center Point, radius float32, c RGB) {
	defer runtime.KeepAlive(l)
	C.katui_light_glow(l.p, center.c(), C.float(radius), c.c())
}

func (l *Light) Ring(center Point, radius, width float32, c RGB) {
	defer runtime.KeepAlive(l)
	C.katui_light_ring(l.p, center.c(), C.float(radius), C.float(width), c.c())
}

// Text draws pixel-font text with its top left at at, each font pixel
// scale pixels wide.
func (l *Light) Text(at Point, text string, scale int, c RGB) {
	defer runtime.KeepAlive(l)
	s := cString(text)
	defer free(s)
	C.katui_light_text(l.p, at.c(), s, C.uint32_t(scale), c.c())
}

func (l *Light) BlendedText(at Point, text string, scale int, c RGB, blend Blend) {
	defer runtime.KeepAlive(l)
	s := cString(text)
	defer free(s)
	C.katui_light_blended_text(l.p, at.c(), s, C.uint32_t(scale), c.c(), C.KatuiBlend(blend))
}

// CenteredText draws pixel-font text centered on centerX, its top at y.
func (l *Light) CenteredText(centerX, y float32, text string, scale int, c RGB) {
	defer runtime.KeepAlive(l)
	s := cString(text)
	defer free(s)
	C.katui_light_centered_text(l.p, C.float(centerX), C.float(y), s, C.uint32_t(scale), c.c())
}

func (l *Light) TextWidth(text string, scale int) float32 {
	defer runtime.KeepAlive(l)
	s := cString(text)
	defer free(s)
	return float32(C.katui_light_text_width(l.p, s, C.uint32_t(scale)))
}

// LightTextHeight is the height of a line of pixel-font text at scale.
func LightTextHeight(scale int) float32 {
	return float32(C.katui_light_text_height(C.uint32_t(scale)))
}

// Bloom spreads light above threshold into a glow of strength.
func (l *Light) Bloom(threshold, strength float32) {
	defer runtime.KeepAlive(l)
	C.katui_light_bloom(l.p, C.float(threshold), C.float(strength))
}

// ToImage is the buffer tone-mapped into an image.
func (l *Light) ToImage() *Image {
	defer runtime.KeepAlive(l)
	return image(C.katui_light_to_image(l.p))
}

// Keys is the keys held down right now. The Keys Step gets live during that
// call only.
type Keys struct {
	p *C.KatuiKeys
}

// IsHeld reports whether the key named name, like "left" or "a", is held.
func (k *Keys) IsHeld(name string) bool {
	s := cString(name)
	defer free(s)
	return bool(C.katui_keys_is_held(k.p, s))
}

// AnyHeld reports whether any of the keys named names is held.
func (k *Keys) AnyHeld(names ...string) bool {
	strings, release := cStrings(names)
	defer release()
	return bool(C.katui_keys_any_held(k.p, firstOf(strings), C.size_t(len(strings))))
}

// V3 is a point or direction in world space.
type V3 struct {
	X, Y, Z float32
}

func (v V3) c() C.KatuiV3 {
	return C.KatuiV3{x: C.float(v.X), y: C.float(v.Y), z: C.float(v.Z)}
}

func v3(v C.KatuiV3) V3 {
	return V3{float32(v.x), float32(v.y), float32(v.z)}
}

func (v V3) Add(other V3) V3 { return v3(C.katui_v3_add(v.c(), other.c())) }

func (v V3) Sub(other V3) V3 { return v3(C.katui_v3_sub(v.c(), other.c())) }

func (v V3) Mul(scale float32) V3 { return v3(C.katui_v3_mul(v.c(), C.float(scale))) }

func (v V3) Dot(other V3) float32 { return float32(C.katui_v3_dot(v.c(), other.c())) }

func (v V3) Cross(other V3) V3 { return v3(C.katui_v3_cross(v.c(), other.c())) }

func (v V3) Normalized() V3 { return v3(C.katui_v3_normalized(v.c())) }

func (v V3) RotatedX(angle float32) V3 { return v3(C.katui_v3_rotated_x(v.c(), C.float(angle))) }

func (v V3) RotatedY(angle float32) V3 { return v3(C.katui_v3_rotated_y(v.c(), C.float(angle))) }

func (v V3) RotatedZ(angle float32) V3 { return v3(C.katui_v3_rotated_z(v.c(), C.float(angle))) }

// Camera is a pinhole camera looking down +z, with y up.
type Camera struct {
	// Center is where the optical axis meets the screen.
	Center Point
	// Focal is pixels of screen height per unit of world height at unit
	// depth.
	Focal float32
	// Distance is how far the eye sits behind z = 0.
	Distance float32
	// PixelAspect is one pixel's width over its height.
	PixelAspect float32
}

// Project is where at lands on screen and how many pixels one world unit
// spans there, and false behind the eye.
func (c Camera) Project(at V3) (Point, float32, bool) {
	camera := C.KatuiCamera{center: c.Center.c(), focal: C.float(c.Focal), distance: C.float(c.Distance),
		pixel_aspect: C.float(c.PixelAspect)}
	var onScreen C.KatuiPoint
	var scale C.float
	ok := C.katui_camera_project(&camera, at.c(), &onScreen, &scale)
	return Point{float32(onScreen.x), float32(onScreen.y)}, float32(scale), bool(ok)
}

// Rng is a small deterministic generator, so games replay the same in tests.
type Rng struct {
	p *C.KatuiRng
}

func NewRng(seed uint32) *Rng {
	r := &Rng{C.katui_rng_new(C.uint32_t(seed))}
	runtime.SetFinalizer(r, func(r *Rng) { C.katui_rng_free(r.p) })
	return r
}

func (r *Rng) NextU32() uint32 {
	defer runtime.KeepAlive(r)
	return uint32(C.katui_rng_next_u32(r.p))
}

// Range is a value from low up to high.
func (r *Rng) Range(low, high float32) float32 {
	defer runtime.KeepAlive(r)
	return float32(C.katui_rng_range(r.p, C.float(low), C.float(high)))
}

// Direction is a random direction of unit length.
func (r *Rng) Direction() V3 {
	defer runtime.KeepAlive(r)
	return v3(C.katui_rng_direction(r.p))
}

// Spark is one glowing particle.
type Spark struct {
	At V3
	// Velocity is in world units per second.
	Velocity V3
	Color    RGB
	// Age is the seconds lived, and Life the seconds lived when it dies.
	Age, Life float32
	// Width is the line width in pixels.
	Width float32
}

// Sparks is every live spark, aged, moved and drawn together.
type Sparks struct {
	p *C.KatuiSparks
}

func NewSparks() *Sparks {
	s := &Sparks{C.katui_sparks_new()}
	runtime.SetFinalizer(s, func(s *Sparks) { C.katui_sparks_free(s.p) })
	return s
}

func (s *Sparks) Add(spark Spark) {
	defer runtime.KeepAlive(s)
	C.katui_sparks_add(s.p, C.KatuiSpark{at: spark.At.c(), velocity: spark.Velocity.c(), color: spark.Color.c(),
		age: C.float(spark.Age), life: C.float(spark.Life), width: C.float(spark.Width)})
}

// Step ages and moves every spark by dt seconds under gravity.
func (s *Sparks) Step(dt float32, gravity V3) {
	defer runtime.KeepAlive(s)
	C.katui_sparks_step(s.p, C.float(dt), gravity.c())
}

// Draw draws every spark as a fading streak; place gives its spot on
// screen, or false to skip it.
func (s *Sparks) Draw(light *Light, place func(at V3) (Point, bool)) {
	defer runtime.KeepAlive(s)
	handle := cgo.NewHandle(place)
	defer handle.Delete()
	C.katui_go_sparks_draw(s.p, light.p, C.uintptr_t(handle))
}

//export katuiGoPlace
func katuiGoPlace(user C.uintptr_t, at C.KatuiV3, onScreen *C.KatuiPoint) C.bool {
	place := cgo.Handle(user).Value().(func(V3) (Point, bool))
	spot, ok := place(v3(at))
	*onScreen = spot.c()
	return C.bool(ok)
}

func (s *Sparks) Len() int {
	defer runtime.KeepAlive(s)
	return int(C.katui_sparks_len(s.p))
}

func (s *Sparks) IsEmpty() bool {
	defer runtime.KeepAlive(s)
	return bool(C.katui_sparks_is_empty(s.p))
}

// Game is a game Play can run: it moves the world in Step and draws it in
// Render. A Game can also be a Presser, a Repeater, a Resizer and a
// Finisher.
type Game interface {
	// Step advances the world by dt seconds, a fixed step.
	Step(keys *Keys, dt float32)
	// Render draws the world into light.
	Render(light *Light)
}

// Presser is a Game told when a key goes down.
type Presser interface {
	Press(key string)
}

// Repeater is a Game told when a held key auto-repeats.
type Repeater interface {
	Repeat(key string)
}

// Resizer is a Game told when the screen becomes the size of light, so
// baked art needs redoing.
type Resizer interface {
	Resize(light *Light)
}

// Finisher is a Game that can end itself: Play ends after the frame where
// Finished reports true.
type Finisher interface {
	Finished() bool
}

type player struct {
	game    Game
	failure error
}

func (p *player) guard() {
	if caught := recover(); caught != nil && p.failure == nil {
		p.failure = fmt.Errorf("katui: a game callback panicked: %v", caught)
	}
}

func playerOf(user C.uintptr_t) *player {
	return cgo.Handle(user).Value().(*player)
}

//export katuiGoPress
func katuiGoPress(user C.uintptr_t, key *C.char, repeat C.bool) {
	p := playerOf(user)
	defer p.guard()
	name := C.GoString(key)
	if presser, ok := p.game.(Presser); ok && !bool(repeat) {
		presser.Press(name)
	}
	if repeater, ok := p.game.(Repeater); ok && bool(repeat) {
		repeater.Repeat(name)
	}
}

//export katuiGoResize
func katuiGoResize(user C.uintptr_t, light *C.KatuiLight) {
	p := playerOf(user)
	defer p.guard()
	if resizer, ok := p.game.(Resizer); ok {
		resizer.Resize(&Light{light})
	}
}

//export katuiGoStep
func katuiGoStep(user C.uintptr_t, keys *C.KatuiKeys, dt C.float) {
	p := playerOf(user)
	defer p.guard()
	if p.failure == nil {
		p.game.Step(&Keys{keys}, float32(dt))
	}
}

//export katuiGoRender
func katuiGoRender(user C.uintptr_t, light *C.KatuiLight) {
	p := playerOf(user)
	defer p.guard()
	if p.failure == nil {
		p.game.Render(&Light{light})
	}
}

//export katuiGoFinished
func katuiGoFinished(user C.uintptr_t) (over C.bool) {
	p := playerOf(user)
	over = true
	defer p.guard()
	finisher, ok := p.game.(Finisher)
	return C.bool(p.failure != nil || (ok && finisher.Finished()))
}

// Play runs game on the terminal at 60 steps a second until the player
// quits or the game finishes. A panic in one of its methods ends it, and
// Play returns it as an error.
func Play(game Game) error {
	p := &player{game: game}
	handle := cgo.NewHandle(p)
	defer handle.Delete()
	status := C.katui_go_play(C.uintptr_t(handle))
	if p.failure != nil {
		return p.failure
	}
	return lastError(status)
}

// FrameOptions says how Frame draws a game. The zero value draws 320 by 180
// pixels after one step of a sixtieth of a second with no keys held.
type FrameOptions struct {
	Width, Height int
	Steps         int
	Held          []string
	DT            float32
}

// Frame is game drawn off-terminal after some steps with some keys held,
// for tests and screenshots.
func Frame(game Game, options FrameOptions) (*Image, error) {
	width, height, steps, dt := options.Width, options.Height, options.Steps, options.DT
	if width == 0 {
		width = 320
	}
	if height == 0 {
		height = 180
	}
	if steps == 0 {
		steps = 1
	}
	if dt == 0 {
		dt = 1.0 / 60
	}
	held, release := cStrings(options.Held)
	defer release()
	p := &player{game: game}
	handle := cgo.NewHandle(p)
	defer handle.Delete()
	made := C.katui_go_frame(C.uintptr_t(handle), C.uint32_t(width), C.uint32_t(height), C.uint32_t(steps),
		firstOf(held), C.size_t(len(held)), C.float(dt))
	if p.failure != nil {
		C.katui_image_free(made)
		return nil, p.failure
	}
	if made == nil {
		return nil, nullError("draw the frame")
	}
	return image(made), nil
}
