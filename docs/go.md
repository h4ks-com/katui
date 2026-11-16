# Go

The katui Go module runs the same engine as the Rust crates: apps with
widgets, HTML and Markdown documents, and games. This guide builds a small
app and a small game, and shows how to test both without a terminal.

Add it to your module on Linux or macOS, with Go 1.24 or newer and a C
compiler for cgo:

```sh
go get github.com/h4ks-com/katui/bindings/go
```

## An app

An app is a value with a `View` that returns elements and an `Update` that
takes messages. Here is a shopping list you type into:

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

Run it with `go run .`, type an item and press <kbd>Enter</kbd>. Click an
item to tick it. <kbd>Ctrl</kbd>+<kbd>C</kbd> quits.

katui calls `View` again after every change and sends the terminal only the
parts that changed. Containers like `Panel`, `Column` and `Row` take any
number of children and leave nil ones out. An element passed to a
container belongs to it from then on.

## Messages

A clickable element takes a message: any Go value. When it is clicked, a
`func()` runs, a `func() katui.Control` runs and says what to do next, and
any other value goes to the app's `Update`:

```go
package main

import (
	"log"

	katui "github.com/h4ks-com/katui/bindings/go"
)

type editor struct{ saved int }

func (e *editor) View() *katui.Element {
	return katui.Row(
		katui.Button("Save", func() { e.saved++ }),
		katui.Button("Quit", "quit"),
	)
}

func (e *editor) Update(message any) katui.Control {
	if message == "quit" {
		return katui.Quit
	}
	return katui.Redraw
}

func main() {
	if err := katui.Run(&editor{}); err != nil {
		log.Fatal(err)
	}
}
```

`Update` returns what to do next:

| `Control` | What happens |
|---|---|
| `Redraw` | katui calls `View` once and draws the changes |
| `Wait` | nothing is drawn until the next event |
| `Animate` | katui keeps drawing at 30 frames a second |
| `Quit` | the app ends and the terminal is restored |

Widgets that report a value take a function that handles it, like
`func(int)`, or one that makes the message from it, like `func(int) any`.
`Tabs`, `Radio`, `Select`, `Menu` and `Table` pass the index picked,
`Slider` and `Split` a `float32`, `Tree` a node's `uint64` id, and
`ScrollView` a `Scroll`.

To send messages from another goroutine, make the app a `Starter` and keep
the `Waker` its `Start` gets. `waker.Send(message)` delivers one, and
`waker.Every(interval, message)` delivers one on a steady beat.

## Keys

An app that is an `Inputter` gets keys and pastes in `Input`.
`event.Key()` is the key's name: one character, or `"enter"`, `"tab"`,
`"escape"`, `"up"`, `"page_down"`, `"f1"` and so on. Keypad, media and
lone modifier keys have names like `"kp_5"`, `"media_play"` and
`"left_shift"`. Besides `Ctrl`, `Alt` and `Shift`, an event reports
`Super`, `Hyper` and `Meta`, which only terminals with the kitty keyboard
protocol send.

The widgets that keep state have a state type you keep in your app and
pass to the widget. Each one takes events with `Handle`:

| State | Widget | What `Handle` does |
|---|---|---|
| `TextField` | `TextFieldView` | typing, the cursor keys and paste; returns an `Edit` |
| `TextArea` | `TextAreaView` | the same over several lines |
| `ListState` | `List`, `Table` | moves the selection; returns whether it moved |
| `ScrollState` | `ScrollView` | scrolls with the arrow, page, home and end keys |
| `TreeState` | `Tree` | moves through nodes and opens and closes them |
| `Focus` | | moves the focus between your controls with <kbd>Tab</kbd> |

## Themes

An app that is a `Themer` picks its theme in `Theme`, which katui asks for
before every frame, so returning another one switches the look at once.
Inside `View`, `katui.CurrentTheme()` gives the theme's colors and sizes
for your own elements, and `theme.Columns(n)` and `theme.Rows(n)` give the
size of `n` terminal cells in pixels.

## Documents

`katui.HTML(source, doc)` and `katui.Markdown(source, doc)` return an
element. A click on a link calls `doc.OnLink` with the href, or sends the
href itself to `Update` when `OnLink` is nil. `doc.Images` gives the
`*katui.Image` for each `src`, and `katui.OpenImage(path)` reads a PNG,
JPEG, GIF or WebP file.

To follow a link to `#install`, call
`.Reveal(katui.AnchorID("#install"))` on the scrolling element around the
document. Its `OnScroll` handler receives the new position; store it and
stop calling `Reveal`.

## Games

A game is a value with `Step` and `Render`. `Play` runs it at 60 steps a
second:

```go
package main

import (
	"log"

	katui "github.com/h4ks-com/katui/bindings/go"
)

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

func main() {
	if err := katui.Play(&ball{x: 40}); err != nil {
		log.Fatal(err)
	}
}
```

`light` is a buffer of light, not of colors: shapes add to what is there,
values above one bloom, and `Dim` fades the last frame into a trail.
`SRGB` turns an ordinary color into light. For 3D, `V3`, `Camera`, `Rng`
and `Sparks` come from the same package. A game that is a `Presser` or a
`Repeater` gets key names, and a `Finisher` ends the game when `Finished`
returns true.

## Tests

`katui.NewScreen` runs an app without a terminal. `Render` returns an
`*Image`, and `Click`, `Wheel`, `Hover`, `Send` and `TypeText` deliver
input the way the terminal would. `katui.Frame(game, options)` steps a
game with some keys held and returns its picture. A panic in your code
comes back as an error from `Render`, `Click` or `Frame`, and the same
happens with `Run` and `Play`.

## Differences from Rust

The Go names follow the Rust API in Go's style, with a few changes:

- Widgets that share a name with their state type end in `View`:
  `TextFieldView`, `TextAreaView` and `ToastsView`, and an image element
  is `ImageView`.
- A table column is a `TableColumn`, the held keys are `Keys`, and a
  light color is an `RGB`.
- Keys are named by strings, like `"left"` and `"enter"`.
- The optional hooks of an app and a game are interfaces: `Inputter`,
  `Themer`, `Starter`, `Presser`, `Repeater`, `Resizer` and `Finisher`.

The [examples](../bindings/go/examples) include a counter, a to-do list, a
Markdown help screen and a comet game. Run one from a clone with
`just go-demo todo`.
