---
name: katui
description: Build terminal programs with katui, the library that draws real pixels in the terminal through the kitty graphics protocol (with sixel and quarter-block fallbacks), from Rust, Python, Node.js/TypeScript, Go or C. Use this skill whenever the user wants a pixel-drawn terminal UI, a terminal game, images, video, HTML or markdown rendered inside a terminal app, or pictures inside a ratatui app with katui, and whenever they mention katui, kitty graphics in their own app, or KATUI_GRAPHICS. Also use it for any change in a project that already depends on katui, such as adding a screen, a widget, a theme or a headless test, even when the request does not name katui.
---

# katui

katui draws apps, games and video as images in the terminal. You describe a
view, and katui probes the terminal, lays the view out with flexbox, draws it
with real fonts, sends only what changed, and turns clicks, keys and pastes
back into your messages. The same engine ships as Rust crates and as Python,
Node.js, Go and C packages with the same shapes.

## Pick the entry point

Choose by what the program is, then read the matching section or reference.

| The program is | Rust | Python / Node.js / Go / C |
|---|---|---|
| An app with widgets, forms, lists | `katui::app::run` with a `UiApp` (feature `app`) | `katui.run(app)` / `run(app)` / `katui.Run(app)` / `katui_run(&app)` |
| A game or animation at a fixed step | `katui::game::play` with a `Game` (feature `game`) | `game.play` / `play` / `katui.Play` / `katui_play` |
| A document (help page, notes) in a view | `katui::content::html` or `markdown_view` (features `app,html`) | `html`, `markdown` / `katui.HTML`, `katui.Markdown` / `katui_html`, `katui_markdown` |
| An existing ratatui app that needs pictures | `katui::ratatui::run::run` with a `RatatuiApp` (feature `ratatui`) | Rust only |
| Video from a file or stream | `katui::rt::media::VideoStream` inside a `UiApp` (needs ffmpeg) | Rust only |

Then load the reference for the language:

- Rust: [references/rust.md](references/rust.md) (apps, games, documents, video, ratatui, tests)
- Python: [references/python.md](references/python.md)
- Node.js and TypeScript: [references/node.md](references/node.md)
- Go: [references/go.md](references/go.md)
- C: [references/c.md](references/c.md)

## Install

| Language | Command |
|---|---|
| Rust (1.90+) | `cargo add katui --features app` (or `game`, `html`, `ratatui`) |
| Python (3.14+) | `pip install katui` |
| Node.js (22.18+) | `npm install @mattfillipe/katui` |
| Go (1.24+, cgo) | `go get github.com/h4ks-com/katui/bindings/go` |
| C | the release archive with `katui.h` and the libraries, see the C README |

katui runs on macOS and Linux.

## The app model

Every binding uses the same loop, so learn it once:

1. State lives in one object (your app struct or class).
2. `view` returns an element tree for the current state. It never draws or
   mutates; katui calls it again after each change and diffs the result.
3. Clickable elements carry a message. A click sends it to `update`, which
   changes the state and returns a `Control`.
4. Keys and pastes go to `input`, which hands the event to the focused state
   object (`TextField::handle`, `ListState::handle`, ...) and returns a
   `Control`. <kbd>Ctrl</kbd>+<kbd>C</kbd> always quits before `input` runs.
5. Work off the app's thread sends messages back through the `Waker` that
   `start` receives: `send(message)` once, `every(interval, message)` on a
   beat.

| `Control` | Effect |
|---|---|
| `Redraw` | call `view` once and draw the changes |
| `Wait` | draw nothing, sleep until the next event |
| `Animate` | keep drawing at 30 frames a second |
| `Quit` | end the app and restore the terminal |

The smallest Rust app, from the README:

```rust
use katui::app::{Control, UiApp, run};
use katui::ui::{Align, Element};
use katui::widgets::{label, panel, primary_button};

#[derive(Default)]
struct Counter(u32);

impl UiApp for Counter {
    type Message = ();

    fn view(&self) -> Element<()> {
        panel([
            label(format!("Clicked {} times", self.0)),
            primary_button("Add one", ()),
        ])
        .align(Align::Center)
    }

    fn update(&mut self, (): ()) -> Control {
        self.0 += 1;
        Control::Redraw
    }
}

fn main() -> Result<(), katui::rt::RtError> {
    run(Counter::default())
}
```

Widgets are plain functions that return elements: `label`, `title`,
`heading`, `hint`, `button`, `primary_button`, `panel`, `page`,
`text_field`, `text_area`, `list`, `table`, `tree`, `scroll_view`,
`checkbox`, `switch`, `radio`, `slider`, `tabs`, `progress`, `spinner`,
`select`, `menu`, `tooltip`, `dialog`, `toasts`, `key_hints`, `split`.
Widgets that keep state between frames take a state object you store in the
app: `TextField`, `TextArea`, `ListState`, `ScrollState`, `TreeState`,
`Toasts`, and `Focus` to move focus with <kbd>Tab</kbd>. Each binding
renames a few of them to fit the language; the references list the changes.

Layout is flexbox: `row`, `column`, `stack`, then chained methods such as
`.gap`, `.padding`, `.size`, `.grow`, `.align`, `.justify`, `.background`,
`.border`, `.radius`, `.color`, `.font_size`, `.weight`. Sizes are pixels,
fill or content. Use `Theme::current().columns(n)` and `.rows(n)` for sizes
in terminal cells so the layout follows the font.

## Typed messages

Keep messages typed so the compiler catches a wrong button:

- Rust: `type Message = YourEnum;` on `UiApp`, and every `Element<Message>`
  carries it.
- Python: `class Editor(katui.App[Message])` and `katui.Element[Message]`;
  mypy and ty reject a mismatched message. Use `katui.App[katui.Action]`
  when buttons take callables like `self.save`.
- Node.js: `class Editor extends App<Message>` and `Element<Message>`; tsc
  rejects a mismatch. Use `App<Action>` for arrow-function handlers.
- Go: messages are `any`; a `func()` runs on click, a `func() katui.Control`
  runs and decides what happens, other values go to `Update`.
- C: a message is a `uint64_t` key you choose plus a payload for widgets
  that report a value.

## Themes

The app's `theme` hook returns the theme before every frame, so returning a
different one switches the look at once. Start from `Theme::dark(cell)` or
`Theme::light(cell)` (dark is the default), change fields such as `accent`,
and call `.scaled(zoom)`. Theme fields: `cell`, `background`, `raised`,
`highlight`, `border`, `text`, `muted`, `accent`, `on_accent`, `danger`.
Inside `view`, `Theme::current()` gives the active theme for your own
elements, like `label("Saved").color(Theme::current().accent)`.

## Games

A game implements `step(keys, dt)`, which katui calls 60 times a second with
a fixed `dt`, and `render(light)`, called once per frame. Optional hooks:
`press` and `repeat` for one-off key actions, `resize` to redo prebaked art,
`finished` to end the game. `Light` holds light, not paint: shapes add up,
values above 1.0 glow, and `light.dim(0.85)` each frame leaves a fading
trail. Convert ordinary colors with `srgb(r, g, b)`. Draw with `glow`,
`ring`, `line`, `rect`, `polygon` (with a `Blend` of `Over` or `Add`),
`text`, `centered_text`, `bloom` and `paint`. For 3D there are `V3`,
`Camera`, `Rng` and `Sparks`. Use `HeldKeys::is_held` for movement and
`press` for firing or pausing.

## Test without a terminal

Write tests that render to pixels instead of launching a terminal:

- Python: `katui.testing.Screen(app, 480, 240)`, then `render().save(path)`,
  `click(x, y)`, `wheel`, `hover`, `send(Event.press("a"))`, `type_text`;
  `katui.testing.frame(game, steps=10, held=["right"])` for games.
- Node.js: `new Screen(app, 480, 240)` with `render`, `click`, `wheel`,
  `hover`, `send`, `typeText`; `frame(game, { steps: 10, held: ['right'] })`.
- Go: `katui.NewScreen(app, 480, 240)` returns `(*Screen, error)`; close it
  with `Close`. `katui.Frame(game, katui.FrameOptions{Steps: 10, Held: []string{"right"}})`.
- C: `katui_screen_new(&app, 480, 240, 10, 20)`, `katui_screen_render`,
  `katui_screen_click`, `katui_frame`, and `katui_image_save` for a PNG.
- Rust: call `update` and `input` on the app struct directly for logic. For
  pixels, see the test section of [references/rust.md](references/rust.md).

An exception or panic inside your callbacks reaches the caller of `render`,
`click` or `frame` (an error in Go and C), so a test fails loudly. Save a PNG
and look at it when layout matters.

## Terminals

| Terminal | What katui draws |
|---|---|
| kitty, Ghostty, iTerm2 3.7 | full pixels through the kitty graphics protocol |
| terminals with sixel | sixel images |
| any other color terminal | quarter blocks, four pixels per cell |
| tmux and rmux | kitty graphics in kitty and Ghostty outside the multiplexer; iTerm2 shows no images there |

katui picks the backend when it starts. Override it with
`KATUI_GRAPHICS=kitty`, `sixel` or `blocks`, for example
`KATUI_GRAPHICS=blocks cargo run`. Inside tmux, the user needs this in their
tmux config for images, true color, and keys and the mouse without delay:

```
set -s escape-time 0
set -g allow-passthrough on
set -g focus-events on
set -g mouse on
set -as terminal-features ',xterm*:RGB,xterm*:sync'
```

To try something without touching the user's tmux sessions, save that as
`katui.conf` and run `tmux -L katui -f katui.conf new -s demos`. Inside tmux
every byte passes through tmux, so keep images and video frames smaller
(about half of 640 by 360 for video). In a clone of the katui repository,
`just info` prints what katui detected in the current terminal.

## Where to look

- Guides, one per kind of program: https://github.com/h4ks-com/katui/blob/main/docs/README.md
- API reference (rustdoc): https://h4ks-com.github.io/katui/katui/
- Every guide and the public API as one text file: https://h4ks-com.github.io/katui/llms-full.txt, indexed by https://h4ks-com.github.io/katui/llms.txt
- Examples: `crates/katui-app/examples` (widget tour, to-do list, HTML browser),
  `crates/katui-game/examples` (games, video player), and `examples` in each
  of `bindings/python`, `bindings/node`, `bindings/go`, `bindings/c`.

When a name here and the API reference disagree, trust the reference and the
examples: katui is young and its API still changes between commits.
