# katui

katui is a terminal UI engine in Rust that draws real pixels. It renders
through the kitty graphics protocol and falls back to sixel or quarter
blocks on other terminals.

- Pixel UIs with layout, text, widgets, mouse and keyboard
- Games and video at up to 30 frames a second
- A ratatui backend that adds images to existing ratatui apps
- Works inside tmux

## Use

```
cargo add katui --git https://github.com/matheusfillipe/katui --features app
```

A click counter:

```rust
use katui::app::{Control, UiApp, run};
use katui::ui::{Align, Element};
use katui::widgets::{Emphasis, Theme, button, label, panel};

#[derive(Default)]
struct Counter(u32);

impl UiApp for Counter {
    type Message = ();

    fn view(&self, theme: &Theme) -> Element<()> {
        panel(
            [
                label(format!("Clicked {} times", self.0), theme.body(), theme.text),
                button("Add one", (), Emphasis::Primary, theme),
            ],
            theme,
        )
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

Run it with `just demo-counter`, and see `just --list` for the other
demos. The `game`, `content`, `html` and `ratatui` features add the game
runner, markdown and HTML rendering, and the ratatui backend.

## Terminals

Graphics need a terminal that speaks the kitty graphics protocol, such as
kitty. Other terminals get a quarter-block fallback.

Inside tmux, every image is anchored to one Unicode placeholder cell: plain
text that tmux moves, clears and hides with the rest of the pane, so
resizes, splits and window switches stay clean. tmux needs passthrough on
and true color for the outer terminal. A minimal config:

```
set -s escape-time 0
set -g allow-passthrough on
set -g focus-events on
set -g mouse on
set -as terminal-features ',xterm*:RGB,xterm*:sync'
```

Set `KATUI_GRAPHICS` to `kitty`, `sixel` or `blocks` to override the
backend katui detects. `just info` prints what it detected.

To try the demos without touching your own sessions, save that as
`katui.conf` and start a separate tmux server with only that config:

```
tmux -L katui -f katui.conf new -s demos
```

## Develop

Read [ARCHITECTURE.md](ARCHITECTURE.md) and [AGENTS.md](AGENTS.md) before
changing code.

```
just check   # clippy, the fast gate; run after every change
just ci      # the full local gate; must pass before any commit
just hooks   # install the prek hooks, once per clone
just --list  # everything else
```
