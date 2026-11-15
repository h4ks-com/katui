# katui in Rust

The `katui` crate re-exports every layer under one name. Enable features by
what you build: `app` (runner and widgets), `game`, `html` (with `app` for
documents in a view), `ratatui`.

```sh
cargo add katui --git https://github.com/matheusfillipe/katui --features app
```

Main paths: `katui::app::{run, UiApp, Control, Waker}`,
`katui::ui::{Element, Align, Length, CellPixels}`,
`katui::widgets::{Theme, label, panel, ...}`,
`katui::proto::input::{InputEvent, Key}`, `katui::game::*`,
`katui::content::{html, markdown_view, HtmlHost, HtmlDefaults}`,
`katui::rt::media::VideoStream`, `katui::scene::surface::Surface`.
Errors come back as `katui::rt::RtError`.

## Contents

- An app with keys and state
- Layout, themes, scrolling, popups, background work
- Games
- Documents
- Video
- ratatui
- Tests

## An app with keys and state

`UiApp` has `type Message: Clone + Send + 'static`, the required `view` and
`update`, and the optional `start`, `input` and `theme`. This shopping list
is the one in the Pixel UI apps guide:

```rust
use katui::app::{Control, UiApp, run};
use katui::proto::input::{InputEvent, Key};
use katui::ui::Element;
use katui::widgets::{Edit, ListState, TextField, Theme, checkbox, list, panel, text_field, title};

const VISIBLE_ROWS: usize = 10;

#[derive(Default)]
struct Shopping {
    items: Vec<(String, bool)>,
    draft: TextField,
    selection: ListState,
}

#[derive(Debug, Clone)]
enum Message {
    Toggle(usize),
}

impl UiApp for Shopping {
    type Message = Message;

    fn view(&self) -> Element<Message> {
        let rows = list(&self.selection, |index, _selected| {
            let (name, bought) = self.items.get(index).cloned().unwrap_or_default();
            checkbox(&name, bought, Message::Toggle(index))
        });
        panel([
            title("Shopping"),
            text_field(&self.draft, "Add an item", true),
            rows,
        ])
        .width(Theme::current().columns(40.0))
    }

    fn update(&mut self, message: Message) -> Control {
        let Message::Toggle(index) = message;
        if let Some((_, bought)) = self.items.get_mut(index) {
            *bought = !*bought;
        }
        Control::Redraw
    }

    fn input(&mut self, event: &InputEvent) -> Control {
        if self.selection.handle(event) {
            return Control::Redraw;
        }
        if let InputEvent::Key(Key::Tab, _) = event {
            return self.update(Message::Toggle(self.selection.selected()));
        }
        match self.draft.handle(event) {
            Edit::Submitted => {
                let item = self.draft.take();
                if !item.trim().is_empty() {
                    self.items.push((item.trim().to_owned(), false));
                    self.selection.resize(self.items.len(), VISIBLE_ROWS);
                }
                Control::Redraw
            }
            Edit::Changed | Edit::Moved => Control::Redraw,
            Edit::Ignored => Control::Wait,
        }
    }
}

fn main() -> Result<(), katui::rt::RtError> {
    run(Shopping::default())
}
```

Hand each key event to one control after another and stop at the first one
that takes it. `ListState::resize(len, visible_rows)` must follow every change
to the list length. Any element sends messages with `.on_click(message)`,
`.on_hover(message)`, `.on_leave(message)`, and `.on_press(|x, y| ...)` for
drags (fractions of the element's box).

## Layout, themes, scrolling, popups, background work

```rust
Element::column([header, body, footer])
    .gap(8.0)
    .padding(16.0)
    .size(Length::Fill, Length::Fill)
    .align(Align::Center)
```

`Length::Px(n)`, `Length::Fill`, `Length::Content`; `.grow(1.0)` and
`Element::spacer()` take leftover space; `.align` is across the main axis and
`.justify` along it.

Theme hook:

```rust
fn theme(&self, cell: CellPixels) -> Theme {
    let theme = if self.light { Theme::light(cell) } else { Theme::dark(cell) };
    theme.scaled(self.zoom)
}
```

Scrolling: keep a `ScrollState`, pass it to `scroll_view`, and store the
`Scroll` message it sends with `self.page.apply(scroll)`.
`ScrollState::handle` takes the arrow, page, home and end keys. katui clamps
an offset that runs past the end on the next frame.

Popups: `select`, `menu`, `tooltip`, `dialog` and `toasts` draw above the
layout. Your state holds the `open` flag. Put `dialog` (only while open) and
`toasts` last in the root element. `.popup(x, y)` lifts any element out of
the layout.

Background work: `view`, `update` and `input` run on the app thread.

```rust
fn start(&mut self, waker: Waker<Message>) {
    std::thread::spawn(move || {
        let page = download();
        waker.send(Message::Loaded(page));
    });
}
```

`waker.send` returns `false` once the app quit.
`waker.every(Duration::from_millis(250), || Message::Tick)` sends on a beat.

## Games

Feature `game`. `play` owns a fixed-step loop: `step` at 60 Hz with the same
`dt`, `render` once per frame.

```rust
use katui::game::{Game, HeldKeys, Key, Light, P2, play, srgb};

struct Comet {
    at: P2,
}

impl Game for Comet {
    fn step(&mut self, keys: &HeldKeys, dt: f32) {
        let speed = 90.0 * dt;
        if keys.is_held(Key::Left) {
            self.at.x -= speed;
        }
        if keys.is_held(Key::Right) {
            self.at.x += speed;
        }
    }

    fn render(&mut self, light: &mut Light) {
        light.dim(0.85);
        light.glow(self.at, 20.0, srgb(255, 140, 60));
        light.text(P2::new(4.0, 4.0), "arrows to steer", 1, srgb(120, 120, 140));
    }
}

fn main() -> Result<(), katui::rt::RtError> {
    play(Comet { at: P2::new(60.0, 40.0) })
}
```

Coordinates are game pixels from the top left; `light.width()`,
`light.height()` and `light.pixel_aspect()` depend on the terminal, so read
them in `render` or `resize` and never hard-code the screen size. The pixel
font draws capital letters.

## Documents

Features `app,html`. The renderer asks a host for link messages and images,
and never reads files or the network itself.

```rust
use std::sync::Arc;

use katui::app::{Control, UiApp, run};
use katui::content::{HtmlDefaults, HtmlHost, markdown_view};
use katui::scene::surface::Surface;
use katui::ui::Element;
use katui::widgets::{Theme, page};

struct Help;

impl HtmlHost<String> for Help {
    fn link(&self, href: &str) -> String {
        href.to_owned()
    }

    fn image(&self, _src: &str) -> Option<Arc<Surface>> {
        None
    }
}

impl UiApp for Help {
    type Message = String;

    fn view(&self) -> Element<String> {
        let theme = Theme::current();
        let defaults = HtmlDefaults {
            font_size: theme.body(),
            color: theme.text,
            link: theme.accent,
            tint: theme.muted,
        };
        page([markdown_view("# Help\n\nRead [the keys](keys).", &defaults, self)])
    }

    fn update(&mut self, _href: String) -> Control {
        Control::Redraw
    }
}

fn main() -> Result<(), katui::rt::RtError> {
    run(Help)
}
```

`katui::content::html(source, &defaults, host)` takes HTML with a CSS subset:
block and flex layout, text styles, `background`, `border`,
`border-radius`, `<style>` blocks with type, class and id selectors, and
`display: none`. It runs no JavaScript and has no tables or floats. Keep the
same `Arc<Surface>` for an unchanged image so katui skips resending it.

## Video

Needs `ffmpeg` on `PATH` and the `app` feature.

```rust
let stream = VideoStream::ffmpeg(&["-i", path.as_str()], (640, 360), 30)?;
```

Tick with `waker.every(Duration::from_secs(1) / 30, || ())` in `start`. In
`update`, `stream.latest()` returns the newest due RGBA frame or `None`;
wrap it with `Surface::from_rgba(640, 360, pixels)` into an `Arc<Surface>`
and show it with `Element::image(frame)`. `latest` skips frames when the
terminal falls behind. `ended()` reports the end, and dropping the stream
stops ffmpeg. `VideoStream::spawn` takes any command that writes raw RGBA.
Sound is a separate process (`ffplay -nodisp`), as the video demo shows.
Full example: https://github.com/matheusfillipe/katui/blob/main/docs/video.md

## ratatui

Feature `ratatui`, plus `ratatui@0.30`. Implement
`katui::ratatui::run::RatatuiApp`: `draw(&mut self, frame, cell)` renders
widgets as usual and places `katui::ratatui::ImageSlot` where a picture
goes, `images(&self) -> Vec<ImageFrame<'_>>` lists the pictures (an
`ImageId`, `RgbaImage` pixels and the `cell_rect` area), and `input` returns
a `Control`. Give an image a new `ImageId` when its pixels change.
`katui::ratatui::image` has `decode`, `contain_in_area`, `fit_to_area` and
`ImageCache` for decoding on worker threads. With your own loop, use
`KatuiBackend` in a `ratatui::Terminal` and call
`terminal.backend_mut().draw_images(&frames)` after each `draw`. Images need
kitty graphics; elsewhere the image areas stay empty.
Full example: https://github.com/matheusfillipe/katui/blob/main/docs/ratatui.md

## Tests

Test logic by calling `update` and `input` on the app value. For pixels,
render the view with `katui::ui::Ui` and flatten the layers into one
`Surface`. Wrap `view` in the app's theme with `Theme::scope`, which is what
the runner does every frame:

```rust
use katui::app::UiApp;
use katui::draw::{TextEngine, bundled_fonts};
use katui::ui::{CellPixels, Ui, UiOptions};

#[test]
fn the_counter_renders() -> Result<(), Box<dyn std::error::Error>> {
    let app = Counter::default();
    let cell = CellPixels { width: 10, height: 20 };
    let size = (480, 240);
    let mut ui = Ui::new(TextEngine::new(bundled_fonts())?, UiOptions::for_terminal(1, false));
    let view = app.theme(cell).scope(|_| app.view());
    let picture = ui.render(&view, size, cell).flatten(cell, size);
    assert!(picture.is_some_and(|surface| surface.width() == 480));
    Ok(())
}
```

After a `render`, `ui.click(x, y)` returns the message a click at that pixel
sends, so a test can pass it to `update`. For games, build a `Light` with
`Light::new(width, height, 1.0)`, call `step` with a `HeldKeys` (press keys
with `keys.press(Key::Right, Instant::now())`), then `render` and
`light.to_surface()`.
