# Python

The `katui` Python package runs the same engine as the Rust crates: apps
with widgets, HTML and Markdown documents, and games. This guide builds a
small app and a small game, and shows how to test both without a terminal.

Install it on Linux or macOS, with Python 3.10 or newer:

```sh
pip install katui
```

## An app

An app is a class with a `view` that returns elements. Here is a shopping
list you type into:

```python
import katui
from katui import Control, Event
from katui.widgets import Edit, TextField, checkbox, panel, text_field, title


class Shopping(katui.App):
    def __init__(self):
        self.items = []
        self.draft = TextField()

    def view(self):
        return panel(
            title("Shopping"),
            text_field(self.draft, "Add an item"),
            [checkbox(name, bought, lambda i=i: self.toggle(i))
             for i, (name, bought) in enumerate(self.items)],
        )

    def toggle(self, index):
        name, bought = self.items[index]
        self.items[index] = (name, not bought)

    def input(self, event: Event):
        if self.draft.handle(event) == Edit.SUBMITTED and self.draft.text.strip():
            self.items.append((self.draft.take().strip(), False))
        return Control.REDRAW


katui.run(Shopping())
```

Run it with `python shopping.py`, type an item and press <kbd>Enter</kbd>.
Click an item to tick it. <kbd>Ctrl</kbd>+<kbd>C</kbd> quits.

katui calls `view` again after every change and sends the terminal only the
parts that changed. Children can be elements, lists of elements, or `None`,
which is left out.

## Messages

A clickable element takes a message: any Python object. When it is clicked,
katui hands the message to the app's `update`. The default `update` calls
the message when it is callable, so a function or a lambda works as a click
handler:

```python
button("Save", self.save)       # calls self.save()
button("Save", "save")          # update receives "save"
```

`update` returns what to do next, and `None` redraws once:

| `Control` | What happens |
|---|---|
| `REDRAW` | katui calls `view` once and draws the changes |
| `WAIT` | nothing is drawn until the next event |
| `ANIMATE` | katui keeps drawing at 30 frames a second |
| `QUIT` | the app ends and the terminal is restored |

Widgets that report a value take a function that makes the message from it:
`slider(value, change)` calls `change(value)`, and `tabs`, `radio`,
`select`, `menu` and `table` call theirs with the index picked. Such a
function can do the work itself and return nothing.

To send messages from another thread, keep the `Waker` that `start` gets.
`waker.send(message)` delivers one, and `waker.every(seconds, message)`
delivers one on a steady beat, for a clock or a spinner.

## Keys

Keys and pastes go to `input`, which receives an `Event` and returns a
`Control`, or `None` to wait. `event.key` is the key's name: one character,
or `"enter"`, `"tab"`, `"escape"`, `"up"`, `"page_down"`, `"f1"` and so on.

The widgets that keep state have a state object you store on the app and
pass to `view`. Each one takes events with `handle`:

| State | Widget | What `handle` does |
|---|---|---|
| `TextField` | `text_field` | typing, the cursor keys and paste; returns an `Edit` |
| `TextArea` | `text_area` | the same over several lines |
| `ListState` | `list_view`, `table` | moves the selection; returns whether it moved |
| `ScrollState` | `scroll_view` | scrolls with the arrow, page, home and end keys |
| `TreeState` | `tree` | moves through nodes and opens and closes them |
| `Focus` | | moves the focus between your controls with <kbd>Tab</kbd> |

## Themes

The app's `theme` method picks the theme, and katui asks for it before every
frame, so returning another one switches the look at once:

```python
def theme(self, cell):
    theme = katui.Theme.light(cell) if self.light else katui.Theme.dark(cell)
    theme.accent = "#e0559b"
    return theme.scaled(self.zoom)
```

Inside `view`, `Theme.current()` gives the theme's colors and sizes for your
own elements, like `label("Saved").color(Theme.current().accent)`.
`theme.columns(n)` and `theme.rows(n)` give a size of `n` terminal cells.

## Layout

`katui.row`, `katui.column` and `katui.stack` place their children with
flexbox. Methods on an element change it and return it, so calls chain:

```python
katui.column(header, body).gap(8).padding(16).size("fill", "fill").align("center")
```

A size is a number of pixels, `"fill"` or `"content"`. A color is
`(r, g, b)`, `(r, g, b, a)` or `"#rrggbb"`. Alignment is `"start"`,
`"center"` or `"end"`.

## Documents

`katui.html(source)` and `katui.markdown(source)` return an element. A
click on a link calls `on_link(href)`, or sends the href itself when you
leave `on_link` out. Images come from `images`: a dict from `src` to
`katui.Image`, or a function that returns one. `Image.open(path)` reads a
PNG, JPEG, GIF or WebP file.

## Games

A game is a class with `step` and `render`. `play` runs it at 60 steps a
second:

```python
import katui.game as game
from katui.game import srgb


class Ball(game.Game):
    def __init__(self):
        self.x = 40.0

    def step(self, keys, dt):
        self.x += 60 * dt * (keys.is_held("right") - keys.is_held("left"))

    def render(self, light):
        light.dim(0.8)
        light.glow((self.x, 30), 6, srgb(255, 140, 60))


game.play(Ball())
```

`light` is a buffer of light, not of colors: shapes add to what is there,
values above one bloom, and `dim` fades the last frame into a trail.
`srgb` turns an ordinary color into light. Positions are `(x, y)` tuples
in the buffer's pixels. For 3D, `V3`, `Camera`, `Rng` and `Sparks` come
from the same module. `press` and `repeat` get key names, and `finished`
ends the game when it returns `True`.

## Tests

`katui.testing.Screen` runs an app without a terminal. `render` returns an
`Image`, and `click`, `wheel`, `hover`, `send` and `type_text` deliver
input the way the terminal would:

```python
from katui.testing import Screen

screen = Screen(Counter(), 480, 240)
screen.render().save("counter.png")
screen.click(240, 72)
```

`katui.testing.frame(game, steps=10, held=["right"])` steps a game and
returns its picture. An exception in your code reaches the caller of
`render`, `click` or `frame`, and the same happens with `katui.run`.

## Differences from Rust

The Python names follow the Rust API, with a few changes:

- `list` is `list_view`, so it does not hide Python's `list`.
- Enums the Rust API passes as values are strings: split orientation
  (`"horizontal"`), blend modes (`"add"`), key names (`"left"`).
- Points and light colors are tuples.
- `Focus.is_` is Rust's `Focus::is`, since `is` is a Python keyword.
- `Event.press("a")` builds a key event, for tests and for handing keys
  to widgets.

The [examples](../bindings/python/examples) include a to-do list, the full
widget tour, a Markdown help screen, an HTML browser and a comet game. Run
one from a clone with `just py-demo widgets`.
