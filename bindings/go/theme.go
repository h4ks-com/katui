package katui

/*
#include "katui.h"
*/
import "C"

import "runtime"

// Cell is the size of a terminal cell in pixels.
type Cell struct {
	Width, Height uint16
}

// Theme is the colors and cell-based sizes every widget draws with.
type Theme struct {
	p *C.KatuiTheme
}

func theme(p *C.KatuiTheme) *Theme {
	t := &Theme{p}
	runtime.SetFinalizer(t, func(t *Theme) { C.katui_theme_free(t.p) })
	return t
}

// take hands the theme over to the engine, as a copy.
func (t *Theme) take() *C.KatuiTheme {
	if t == nil {
		return nil
	}
	defer runtime.KeepAlive(t)
	return C.katui_theme_scaled(t.p, 1)
}

// DarkTheme is the dark theme for cells of cell pixels.
func DarkTheme(cell Cell) *Theme {
	return theme(C.katui_theme_dark(C.uint16_t(cell.Width), C.uint16_t(cell.Height)))
}

// LightTheme is the light theme for cells of cell pixels.
func LightTheme(cell Cell) *Theme {
	return theme(C.katui_theme_light(C.uint16_t(cell.Width), C.uint16_t(cell.Height)))
}

// CurrentTheme is the theme the widgets draw with right now: the app's,
// inside View.
func CurrentTheme() *Theme {
	return theme(C.katui_theme_current())
}

// Scaled is the theme with every size times factor.
func (t *Theme) Scaled(factor float32) *Theme {
	defer runtime.KeepAlive(t)
	return theme(C.katui_theme_scaled(t.p, C.float(factor)))
}

// Cell is the cell size the sizes follow.
func (t *Theme) Cell() Cell {
	defer runtime.KeepAlive(t)
	var width, height C.uint16_t
	C.katui_theme_cell(t.p, &width, &height)
	return Cell{uint16(width), uint16(height)}
}

func (t *Theme) CellWidth() float32 {
	defer runtime.KeepAlive(t)
	return float32(C.katui_theme_cell_width(t.p))
}

func (t *Theme) CellHeight() float32 {
	defer runtime.KeepAlive(t)
	return float32(C.katui_theme_cell_height(t.p))
}

// Body is the body text size, in pixels.
func (t *Theme) Body() float32 {
	defer runtime.KeepAlive(t)
	return float32(C.katui_theme_body(t.p))
}

func (t *Theme) Small() float32 {
	defer runtime.KeepAlive(t)
	return float32(C.katui_theme_small(t.p))
}

func (t *Theme) Heading() float32 {
	defer runtime.KeepAlive(t)
	return float32(C.katui_theme_heading(t.p))
}

func (t *Theme) Title() float32 {
	defer runtime.KeepAlive(t)
	return float32(C.katui_theme_title(t.p))
}

// Radius is the corner radius, in pixels.
func (t *Theme) Radius() float32 {
	defer runtime.KeepAlive(t)
	return float32(C.katui_theme_radius(t.p))
}

// Columns is count cells across, in pixels.
func (t *Theme) Columns(count float32) float32 {
	defer runtime.KeepAlive(t)
	return float32(C.katui_theme_columns(t.p, C.float(count)))
}

// Rows is count cells down, in pixels.
func (t *Theme) Rows(count float32) float32 {
	defer runtime.KeepAlive(t)
	return float32(C.katui_theme_rows(t.p, C.float(count)))
}

// Outline is the accent color while active, the border color otherwise.
func (t *Theme) Outline(active bool) Color {
	defer runtime.KeepAlive(t)
	return color(C.katui_theme_outline(t.p, C.bool(active)))
}

// Background is the page behind everything.
func (t *Theme) Background() Color {
	defer runtime.KeepAlive(t)
	return color(C.katui_theme_background(t.p))
}

func (t *Theme) SetBackground(c Color) {
	defer runtime.KeepAlive(t)
	C.katui_theme_set_background(t.p, c.c())
}

// Raised is the color of panels and fields.
func (t *Theme) Raised() Color {
	defer runtime.KeepAlive(t)
	return color(C.katui_theme_raised(t.p))
}

func (t *Theme) SetRaised(c Color) {
	defer runtime.KeepAlive(t)
	C.katui_theme_set_raised(t.p, c.c())
}

// Highlight is the color of hovered or selected rows.
func (t *Theme) Highlight() Color {
	defer runtime.KeepAlive(t)
	return color(C.katui_theme_highlight(t.p))
}

func (t *Theme) SetHighlight(c Color) {
	defer runtime.KeepAlive(t)
	C.katui_theme_set_highlight(t.p, c.c())
}

// Border is the color of hairline borders.
func (t *Theme) Border() Color {
	defer runtime.KeepAlive(t)
	return color(C.katui_theme_border(t.p))
}

func (t *Theme) SetBorder(c Color) {
	defer runtime.KeepAlive(t)
	C.katui_theme_set_border(t.p, c.c())
}

// Text is the color of body text.
func (t *Theme) Text() Color {
	defer runtime.KeepAlive(t)
	return color(C.katui_theme_text(t.p))
}

func (t *Theme) SetText(c Color) {
	defer runtime.KeepAlive(t)
	C.katui_theme_set_text(t.p, c.c())
}

// Muted is the color of hints, placeholders and secondary text.
func (t *Theme) Muted() Color {
	defer runtime.KeepAlive(t)
	return color(C.katui_theme_muted(t.p))
}

func (t *Theme) SetMuted(c Color) {
	defer runtime.KeepAlive(t)
	C.katui_theme_set_muted(t.p, c.c())
}

// Accent is the color of the action that matters most.
func (t *Theme) Accent() Color {
	defer runtime.KeepAlive(t)
	return color(C.katui_theme_accent(t.p))
}

func (t *Theme) SetAccent(c Color) {
	defer runtime.KeepAlive(t)
	C.katui_theme_set_accent(t.p, c.c())
}

// OnAccent is the color of text on the accent color.
func (t *Theme) OnAccent() Color {
	defer runtime.KeepAlive(t)
	return color(C.katui_theme_on_accent(t.p))
}

func (t *Theme) SetOnAccent(c Color) {
	defer runtime.KeepAlive(t)
	C.katui_theme_set_on_accent(t.p, c.c())
}

// Danger is the color of errors.
func (t *Theme) Danger() Color {
	defer runtime.KeepAlive(t)
	return color(C.katui_theme_danger(t.p))
}

func (t *Theme) SetDanger(c Color) {
	defer runtime.KeepAlive(t)
	C.katui_theme_set_danger(t.p, c.c())
}
