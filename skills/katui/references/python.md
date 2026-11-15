# katui in Python

`pip install katui` (Python 3.14+, Linux or macOS). The package is fully
typed; keep apps typed so mypy and ty check messages. Full guide:
https://github.com/matheusfillipe/katui/blob/main/docs/python.md

Modules: `katui` (`App`, `Action`, `Control`, `Element`, `Event`, `Image`,
`Theme`, `Waker`, `run`, `row`, `column`, `stack`, `text`, `rich`, `image`,
`spacer`, `html`, `markdown`), `katui.widgets` (the widgets and their state
types), `katui.game` (`Game`, `play`, `Keys`, `Light`, `srgb`, `V3`,
`Camera`, `Rng`, `Sparks`), `katui.testing` (`Screen`, `frame`).

## An app

```python
import katui
from katui import Action, Control, Event
from katui.widgets import Edit, TextField, checkbox, panel, text_field, title


class Shopping(katui.App[Action]):
    def __init__(self) -> None:
        self.items: list[tuple[str, bool]] = []
        self.draft = TextField()

    def view(self) -> katui.Element[Action]:
        return panel(
            title("Shopping"),
            text_field(self.draft, "Add an item"),
            [
                checkbox(name, bought, lambda i=i: self.toggle(i))
                for i, (name, bought) in enumerate(self.items)
            ],
        )

    def toggle(self, index: int) -> None:
        name, bought = self.items[index]
        self.items[index] = (name, not bought)

    def input(self, event: Event) -> Control | None:
        if self.draft.handle(event) == Edit.SUBMITTED and self.draft.text.strip():
            self.items.append((self.draft.take().strip(), False))
        return Control.REDRAW


katui.run(Shopping())
```

Children can be elements, lists of elements, or `None` (left out). Hooks on
`App`: `start(waker)`, `view()`, `update(message)`, `input(event)`,
`theme(cell)`. `update` and `input` return a `Control` (`REDRAW`, `WAIT`,
`ANIMATE`, `QUIT`) or `None`: `None` from `update` redraws once, `None`
from `input` waits.

## Messages

The default `update` calls a callable message, so `button("Save", self.save)`
works in an `App[Action]`. For a typed message type:

```python
from enum import Enum

import katui
from katui.widgets import button, panel


class Message(Enum):
    SAVE = "save"
    QUIT = "quit"


class Editor(katui.App[Message]):
    def view(self) -> katui.Element[Message]:
        return panel(button("Save", Message.SAVE), button("Quit", Message.QUIT))

    def update(self, message: Message) -> katui.Control | None:
        return katui.Control.QUIT if message is Message.QUIT else None


katui.run(Editor())
```

Value widgets take a function that builds the message: `slider(value,
change)` calls `change(value)`; `tabs`, `radio`, `select`, `menu` and
`table` call theirs with the index picked. From another thread use the
`Waker` from `start`: `waker.send(message)` and `waker.every(seconds,
message)`.

## Keys and state

`event.key` is one character or a name like `"enter"`, `"tab"`,
`"escape"`, `"up"`, `"page_down"`, `"f1"`. State objects take events with
`handle`: `TextField` and `TextArea` return an `Edit` (`Edit.SUBMITTED` is
Enter), `ListState` (for `list_view` and `table`) returns whether it moved,
plus `ScrollState`, `TreeState` and `Focus`.

## Layout and themes

```python
katui.column(header, body).gap(8).padding(16).size("fill", "fill").align("center")
```

A size is pixels, `"fill"` or `"content"`. A color is `(r, g, b)`,
`(r, g, b, a)` or `"#rrggbb"`. Alignment is `"start"`, `"center"`, `"end"`.

```python
def theme(self, cell: tuple[int, int]) -> katui.Theme:
    theme = katui.Theme.light(cell) if self.light else katui.Theme.dark(cell)
    theme.accent = "#e0559b"
    return theme.scaled(self.zoom)
```

`Theme.current()` inside `view`; `theme.columns(n)` and `theme.rows(n)`
give the size of `n` cells.

## Documents

`katui.html(source)` and `katui.markdown(source)` return an element. A link
click calls `on_link(href)`, or sends the href string when `on_link` is
left out. `images` is a dict from `src` to `katui.Image` or a function
returning one; `katui.Image.open(path)` reads PNG, JPEG, GIF or WebP.

## Games

```python
from katui import game
from katui.game import Keys, Light, srgb


class Ball(game.Game):
    def __init__(self) -> None:
        self.x = 40.0

    def step(self, keys: Keys, dt: float) -> None:
        self.x += 60 * dt * (keys.is_held("right") - keys.is_held("left"))

    def render(self, light: Light) -> None:
        light.dim(0.8)
        light.glow((self.x, 30), 6, srgb(255, 140, 60))


game.play(Ball())
```

Points are `(x, y)` tuples. `press` and `repeat` get key names, `resize`
gets the new `Light`, and `finished` ends the game when it returns `True`.

## Tests

```python
from katui import Event
from katui.testing import Screen, frame

screen = Screen(Counter(), 480, 240)
screen.render().save("counter.png")
screen.click(240, 72)
screen.send(Event.press("a"))
screen.type_text("milk")
assert not screen.quit

picture = frame(Ball(), steps=10, held=["right"])
```

`Screen(app, width=960, height=540, cell=(10, 20))` also has `wheel`,
`hover`, `press`, `drag` and `release`. An exception in your code reaches
the caller of `render`, `click` or `frame`.

## Differences from Rust

`list` is `list_view`. Enums passed as values are strings (`"horizontal"`,
`"add"`, `"left"`). Points and light colors are tuples. `Focus.is_` is
Rust's `Focus::is`. Examples: https://github.com/matheusfillipe/katui/tree/main/bindings/python/examples
