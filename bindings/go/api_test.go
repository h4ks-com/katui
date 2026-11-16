package katui

import (
	"strings"
	"testing"
)

// picker shows tabs and a link, and records what reached Update.
type picker struct {
	tab      int
	received []any
	light    bool
	broken   bool
}

func (p *picker) View() *Element {
	if p.broken {
		panic("the view broke")
	}
	return Page(
		Tabs([]string{"One", "Two", "Three"}, p.tab, func(index int) { p.tab = index }),
		Markdown("[open the keys page](keys)", Document{FontSize: 20}),
		Button("Plain", "plain"),
		nil,
	)
}

func (p *picker) Update(message any) Control {
	p.received = append(p.received, message)
	return Redraw
}

func (p *picker) Theme(cell Cell) *Theme {
	if !p.light {
		return nil
	}
	theme := LightTheme(cell)
	theme.SetAccent(Color{224, 85, 155, 255})
	return theme
}

func screen(t *testing.T, app App) *Screen {
	t.Helper()
	s, err := NewScreen(app, 600, 300)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(s.Close)
	if _, err := s.Render(); err != nil {
		t.Fatal(err)
	}
	return s
}

// clickUntil clicks over the top of the screen until done reports true.
func clickUntil(t *testing.T, s *Screen, done func() bool) {
	t.Helper()
	for y := float32(2); y < 140 && !done(); y += 4 {
		for x := float32(2); x < 400 && !done(); x += 8 {
			if err := s.Click(x, y); err != nil {
				t.Fatal(err)
			}
		}
	}
	if !done() {
		t.Fatal("no click did it")
	}
}

func TestAPickerFuncTakesTheIndexAndAPlainMessageGoesToUpdate(t *testing.T) {
	app := &picker{}
	s := screen(t, app)
	clickUntil(t, s, func() bool { return app.tab == 2 })
	clickUntil(t, s, func() bool { return len(app.received) > 0 })
	if got, ok := app.received[0].(string); !ok || got != "keys" && got != "plain" {
		t.Fatalf("Update got %#v, want the href or the plain message", app.received[0])
	}
}

func TestALinkWithoutOnLinkSendsItsHref(t *testing.T) {
	app := &picker{}
	s := screen(t, app)
	clickUntil(t, s, func() bool { return len(app.received) > 0 })
	if app.received[0] != "keys" {
		t.Fatalf("Update got %#v, want the href", app.received[0])
	}
}

// revealer shows a long document scrolled to its target heading.
type revealer struct{ offset float32 }

func (r *revealer) View() *Element {
	source := strings.Repeat("filler\n\n", 20) + "## The target\n\nend"
	return Column(Markdown(source, Document{})).
		Scroll(0).
		Reveal(AnchorID("#the-target")).
		OnScroll(func(scroll Scroll) { r.offset = scroll.Offset }).
		Size(Fill, Fill)
}

func (r *revealer) Update(any) Control { return Redraw }

func TestRevealScrollsToAnAnchorAndReportsIt(t *testing.T) {
	app := &revealer{}
	screen(t, app)
	if app.offset <= 0 {
		t.Fatalf("OnScroll got %v, want the heading's offset", app.offset)
	}
	if AnchorID("#the-target") != AnchorID("the-target") {
		t.Fatal("AnchorID keeps the hash")
	}
}

func TestThemeSwitchesTheLook(t *testing.T) {
	app := &picker{}
	s := screen(t, app)
	dark, _ := s.Render()
	app.light = true
	light, err := s.Render()
	if err != nil {
		t.Fatal(err)
	}
	before, _ := dark.Pixel(599, 299)
	after, ok := light.Pixel(599, 299)
	if !ok || before == after {
		t.Fatalf("the corner stays %v", before)
	}
	if _, ok := light.Pixel(600, 0); ok {
		t.Fatal("a pixel outside the image")
	}
}

func TestAPanicInViewComesBackAsAnError(t *testing.T) {
	app := &picker{}
	s := screen(t, app)
	app.broken = true
	if _, err := s.Render(); err == nil || !strings.Contains(err.Error(), "the view broke") {
		t.Fatalf("Render returned %v", err)
	}
	app.broken = false
	if _, err := s.Render(); err != nil {
		t.Fatalf("the screen renders again after the panic: %v", err)
	}
}

func TestOldViewsAreForgotten(t *testing.T) {
	s := screen(t, &picker{})
	for range 20 {
		if _, err := s.Render(); err != nil {
			t.Fatal(err)
		}
	}
	registry.Lock()
	kept := len(registry.values)
	registry.Unlock()
	if kept > 20 {
		t.Fatalf("the registry keeps %d messages after 20 views", kept)
	}
}

func TestAnElementBelongsToItsFirstParent(t *testing.T) {
	child := Label("once")
	Column(child)
	if child.p != nil {
		t.Fatal("the column took the label")
	}
	Row(child, nil).Padding(4)
}

func TestFailuresComeBackAsErrors(t *testing.T) {
	if _, err := NewImage(2, 2, []byte{1, 2, 3}); err == nil || !strings.Contains(err.Error(), "RGBA") {
		t.Fatalf("NewImage returned %v", err)
	}
	if _, err := OpenImage("no/such/file.png"); err == nil {
		t.Fatal("OpenImage of a missing file succeeds")
	}
	if _, err := Press("nonsense"); err == nil {
		t.Fatal("Press of an unknown key succeeds")
	}
	if _, err := NewFocus(0); err == nil {
		t.Fatal("NewFocus(0) succeeds")
	}
}

func TestPressHoldsEveryModifier(t *testing.T) {
	chord, err := Press("kp_5", Super, Hyper, Meta)
	if err != nil || chord.Key() != "kp_5" || chord.Kind() != KeyEvent {
		t.Fatalf("a keypad press reads back, got %v", err)
	}
	if !chord.Super() || !chord.Hyper() || !chord.Meta() || chord.Ctrl() {
		t.Fatal("super, hyper and meta are held and ctrl is not")
	}
}

func TestALightTakesPixelsImagesAndUIText(t *testing.T) {
	light := NewLight(60, 30, 1)
	light.SetPixel(1, 2, RGB{0.5, 0.25, 1})
	if c, ok := light.Pixel(1, 2); !ok || c != (RGB{0.5, 0.25, 1}) {
		t.Fatalf("the pixel reads back %v", c)
	}
	if _, ok := light.Pixel(60, 0); ok {
		t.Fatal("a pixel outside the buffer is missing")
	}
	sprite, err := NewImage(2, 2, []byte{255, 0, 0, 255, 255, 0, 0, 255, 255, 0, 0, 255, 255, 0, 0, 255})
	if err != nil {
		t.Fatal(err)
	}
	light.Image(Point{10, 10}, sprite)
	if width := light.UITextWidth("Hi", 16); width <= 0 || width >= 40 {
		t.Fatalf("Hi is %v wide", width)
	}
	light.UIText(Point{30, 4}, "Hi", 16, RGB{1, 1, 1})
	if c, _ := light.ToImage().Pixel(11, 11); c != (Color{255, 0, 0, 255}) {
		t.Fatalf("the sprite shows as %v", c)
	}
}

func TestStateTypesAnswer(t *testing.T) {
	down, _ := Press("down")
	list := NewListState(5, 2)
	if !list.Handle(down) || list.Selected() != 1 {
		t.Fatal("down moves the selection")
	}
	tab, _ := Press("tab")
	focus, _ := NewFocus(3)
	if !focus.Handle(tab) || !focus.Is(1) {
		t.Fatal("tab moves the focus")
	}
	field := NewTextField("hello")
	if before, after := field.Split(); before != "hello" || after != "" {
		t.Fatalf("the cursor splits %q and %q", before, after)
	}
	toasts := NewToasts()
	toasts.Push("saved")
	if texts := toasts.Texts(); len(texts) != 1 || texts[0] != "saved" {
		t.Fatalf("toasts hold %v", texts)
	}
	if paste := Paste("text"); paste.Kind() != PasteEvent || paste.Text() != "text" {
		t.Fatal("a paste carries its text")
	}
	camera := Camera{Center: Point{50, 50}, Focal: 100, Distance: 5, PixelAspect: 1}
	if at, scale, ok := camera.Project(V3{1, 0, 0}); !ok || at.X <= 50 || scale <= 0 {
		t.Fatalf("the camera projects to %v at %v", at, scale)
	}
}
