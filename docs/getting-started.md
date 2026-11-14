# Getting started

In this guide you add katui to a project, run a small app, and see what
katui does when it starts. It takes about five minutes.

## What you need

- Rust 1.90 or newer.
- macOS or Linux.
- A terminal. katui looks best in one that speaks the kitty graphics
  protocol, like kitty, Ghostty or iTerm2 3.7, and it still runs in any
  other color terminal.

## Make a project

```sh
cargo new hello-katui
cd hello-katui
cargo add katui --git https://github.com/matheusfillipe/katui --features app
```

The `app` feature brings in the app runner and the widgets. Other features
add more later: `game` for games, `html` for documents, `ratatui` for the
ratatui backend.

## Write the app

Replace `src/main.rs` with this:

```rust,no_run
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

Then run it:

```sh
cargo run
```

You see a panel with a line of text and a button. Click the button and the
count goes up. Press <kbd>Ctrl</kbd>+<kbd>C</kbd> to quit, and your
terminal comes back the way it was.

## What just happened

The app has two parts.

- `view` describes what the screen shows for the current state. It returns
  a tree of elements: here a panel holding a label and a button.
- `update` changes the state when something happens. The button sends the
  message `()` when you click it, so `update` adds one and asks for a
  redraw.

You never draw anything yourself. katui calls `view` again after each
update, compares the result with the last frame, and sends the terminal
only the parts that changed.

When `run` starts, katui also asks the terminal what it can do. The answer
decides how pixels reach the screen:

| The terminal speaks | katui draws with |
|---|---|
| The kitty graphics protocol | Full-resolution images |
| Sixel | Sixel images |
| Neither | Quarter blocks, four pixels per cell |

Your code stays the same in every case. To see what katui found in your
terminal, run `just info` in a clone of the katui repository. To pick the
backend yourself, set `KATUI_GRAPHICS` to `kitty`, `sixel` or `blocks`:

```sh
KATUI_GRAPHICS=blocks cargo run
```

## Next

Read [Pixel UI apps](ui-apps.md) to build something bigger: a shopping
list with a text field, a list and keyboard shortcuts.
