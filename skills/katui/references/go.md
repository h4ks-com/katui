# katui in Go

`go get github.com/h4ks-com/katui/bindings/go` (Go 1.24+, a C compiler
for cgo, Linux or macOS). Import it as
`katui "github.com/h4ks-com/katui/bindings/go"`. Full guide:
https://github.com/h4ks-com/katui/blob/main/docs/go.md

## An app

An app is any value with `View() *katui.Element` and
`Update(message any) katui.Control`. Optional hooks are interfaces the app
can also satisfy: `Inputter` (`Input(event *katui.Event) katui.Control`),
`Themer` (`Theme(cell katui.Cell) *katui.Theme`), `Starter`
(`Start(waker *katui.Waker)`).

```go
package main

import (
	"log"
	"strings"

	katui "github.com/h4ks-com/katui/bindings/go"
)

type item struct {
	name   string
	bought bool
}

type shopping struct {
	items []item
	draft *katui.TextField
}

func (s *shopping) View() *katui.Element {
	rows := []*katui.Element{katui.Title("Shopping"), katui.TextFieldView(s.draft, "Add an item", true)}
	for i, item := range s.items {
		rows = append(rows, katui.Checkbox(item.name, item.bought, func() { s.items[i].bought = !item.bought }))
	}
	return katui.Panel(rows...)
}

func (s *shopping) Update(message any) katui.Control {
	return katui.Redraw
}

func (s *shopping) Input(event *katui.Event) katui.Control {
	if s.draft.Handle(event) == katui.Submitted && strings.TrimSpace(s.draft.Text()) != "" {
		s.items = append(s.items, item{strings.TrimSpace(s.draft.Take()), false})
	}
	return katui.Redraw
}

func main() {
	if err := katui.Run(&shopping{draft: katui.NewTextField("")}); err != nil {
		log.Fatal(err)
	}
}
```

Containers (`Panel`, `Column`, `Row`, `Stack`) take any number of children
and skip nil ones. An element passed to a container belongs to it; do not
reuse it.

## Messages

A clickable element takes any value. On click a `func()` runs, a
`func() katui.Control` runs and decides what happens, and any other value
goes to `Update`. Value widgets take `func(int)` or `func(int) any`:
`Tabs`, `Radio`, `Select`, `Menu` and `Table` pass the index, `Slider` and
`Split` a `float32`, `Tree` a node's `uint64` id, `ScrollView` a `Scroll`.
`Control` values: `katui.Redraw`, `katui.Wait`, `katui.Animate`,
`katui.Quit`. From another goroutine use the `Waker` from `Start`:
`waker.Send(message)` and `waker.Every(interval, message)`.

## Keys, layout, themes, documents

- `event.Key()` is one character or a name like `"enter"`, `"tab"`,
  `"up"`. `katui.Press("a", katui.Ctrl)` builds a key event for tests.
- State types with `Handle(event)`: `TextField` (`TextFieldView`),
  `TextArea` (`TextAreaView`), `ListState` (`List`, `Table`),
  `ScrollState` (`ScrollView`), `TreeState` (`Tree`), `Focus`.
- Layout: `katui.Column(a, b).Gap(8).Padding(16).Size(katui.Fill, katui.Fill).Align(katui.Center)`.
  Lengths are `katui.Px(n)`, `katui.Fill`, `katui.Content`; colors are
  `katui.Color{R, G, B, A}`.
- Themes: `katui.DarkTheme(cell)`, `katui.LightTheme(cell)`, `.Scaled(f)`,
  `.SetAccent(color)`; `katui.CurrentTheme()` inside `View`, and
  `theme.Columns(n)` for the width of `n` cells.
- Documents: `katui.HTML(source, katui.Document{})` and
  `katui.Markdown(source, doc)`. `doc.OnLink` is a `func(string)` or
  `func(string) any`; without it the href goes to `Update`. `doc.Images` is
  `func(src string) *katui.Image`, and `katui.OpenImage(path)` reads a file.

## Games

```go
type ball struct{ x float32 }

func (b *ball) Step(keys *katui.Keys, dt float32) {
	if keys.IsHeld("right") {
		b.x += 60 * dt
	}
	if keys.IsHeld("left") {
		b.x -= 60 * dt
	}
}

func (b *ball) Render(light *katui.Light) {
	light.Dim(0.8)
	light.Glow(katui.Point{X: b.x, Y: 30}, 6, katui.SRGB(255, 140, 60))
}

// in main: if err := katui.Play(&ball{x: 40}); err != nil { log.Fatal(err) }
```

Optional game hooks: `Presser`, `Repeater`, `Resizer`, `Finisher`. For 3D:
`V3`, `Camera`, `Rng`, `Sparks`.

## Tests

```go
func TestShopping(t *testing.T) {
	screen, err := katui.NewScreen(&shopping{draft: katui.NewTextField("")}, 480, 240)
	if err != nil {
		t.Fatal(err)
	}
	defer screen.Close()
	if err := screen.TypeText("milk"); err != nil {
		t.Fatal(err)
	}
	image, err := screen.Render()
	if err != nil {
		t.Fatal(err)
	}
	if err := image.Save("shopping.png"); err != nil {
		t.Fatal(err)
	}
	picture, err := katui.Frame(&ball{x: 40}, katui.FrameOptions{Steps: 10, Held: []string{"right"}})
	if err != nil || picture.Width() != 320 {
		t.Fatal(err)
	}
}
```

`Screen` also has `Click`, `Press`, `Drag`, `Release`, `Wheel`, `Hover`,
`Send` and `Quit`. A panic in your code comes back as an error from
`Render`, `Click` or `Frame`, and from `Run` and `Play`.

## Differences from Rust

Widgets that share a name with their state type end in `View`
(`TextFieldView`, `TextAreaView`, `ToastsView`), an image element is
`ImageView`, a table column is `TableColumn`, held keys are `Keys`, a light
color is `RGB`, and keys are strings. Examples:
https://github.com/h4ks-com/katui/tree/main/bindings/go/examples
