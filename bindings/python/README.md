<h1 align="center">katui for Python</h1>

<p align="center">
  katui is a Python library for apps, games and documents that draw real pixels in the terminal.
</p>

<p align="center">
  <a href="#install">Install</a> ·
  <a href="https://github.com/matheusfillipe/katui/blob/main/docs/python.md">Guide</a> ·
  <a href="https://github.com/matheusfillipe/katui/tree/main/bindings/python/examples">Examples</a> ·
  <a href="https://matheusfillipe.github.io/katui/katui/">API reference</a>
</p>

<p align="center"><img src="https://raw.githubusercontent.com/matheusfillipe/katui/main/assets/demos/widgets.webp" alt="The katui widget tour: a file tree, a theme select, a switch with a spinner, a slider and a table" width="800"></p>

It draws with the kitty graphics protocol, falls back to sixel or to blocks
of colored cells in other terminals, and sends only what changed between
frames. The engine is the [katui](https://github.com/matheusfillipe/katui)
Rust library; this package runs it from Python.

```python
import katui
from katui.widgets import label, panel, primary_button


class Counter(katui.App):
    def __init__(self):
        self.count = 0

    def view(self):
        return panel(label(f"Clicked {self.count} times"), primary_button("Add one", self.add))

    def add(self):
        self.count += 1


katui.run(Counter())
```

## Install

katui runs on Linux and macOS with Python 3.10 or newer:

```sh
pip install katui
```

Save the example above as `counter.py` and run it with `python counter.py`.
A panel shows a line of text and a button; click the button and the count
goes up. Press <kbd>Ctrl</kbd>+<kbd>C</kbd> to quit.

Pixels need a terminal with graphics: kitty, Ghostty and iTerm2 3.7 show
the full picture, and terminals with sixel show it as sixel. Everywhere
else, katui draws it with colored block characters.

## What you get

- **Widgets:** labels, buttons, text fields and areas, lists, tables, trees,
  tabs, switches, sliders, selects, menus, tooltips, dialogs, toasts, scroll
  views and split panes, drawn with a theme you can switch at runtime.
- **Layout:** rows, columns and stacks laid out with flexbox, sized in pixels
  or in terminal cells so they follow the font size.
- **Documents:** `katui.html` and `katui.markdown` turn a page into elements,
  with links that send messages and images you supply.
- **Games:** `katui.game` runs a fixed-step loop over a light buffer with
  glows, bloom, sparks and a 3D camera, at up to 60 frames a second.
- **Tests:** `katui.testing.Screen` draws an app into an image and sends it
  clicks and keys without a terminal, and `frame` does the same for games.

## A game

```python
import katui.game as game
from katui.game import srgb


class Comet(game.Game):
    def __init__(self):
        self.x = 40.0

    def step(self, keys, dt):
        self.x += 60 * dt * (keys.is_held("right") - keys.is_held("left"))

    def render(self, light):
        light.dim(0.8)
        light.glow((self.x, 30), 6, srgb(255, 140, 60))


game.play(Comet())
```

The [examples](https://github.com/matheusfillipe/katui/tree/main/bindings/python/examples)
include a to-do list, the full widget tour, a Markdown help screen, an HTML
browser and this comet with sparks.

## Status

katui is young, and its API can still change between minor versions. The
Python package has the same version as the Rust crates and is released
together with them. Windows is not supported.

## License

katui is licensed under either of the
[Apache License 2.0](https://github.com/matheusfillipe/katui/blob/main/LICENSE-APACHE)
or the [MIT license](https://github.com/matheusfillipe/katui/blob/main/LICENSE-MIT),
at your option.
