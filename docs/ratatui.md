# Images in ratatui apps

If you already have a [ratatui](https://ratatui.rs) app, you can keep all
of it and add pictures. katui runs your app on its own event loop, draws
your widgets as text, and places kitty images in the areas your layout
leaves for them.

Add the `ratatui` feature, and ratatui itself at the version katui uses:

```sh
cargo add katui --git https://github.com/matheusfillipe/katui --features ratatui
cargo add ratatui@0.30
```

## A picture under a title

This program shows a bordered title and a gradient below it. Press
<kbd>q</kbd> to quit.

```rust,no_run
use katui::proto::input::{InputEvent, Key};
use katui::proto::probe::CellPixels;
use katui::ratatui::image::pixel_area;
use katui::ratatui::run::{Control, RatatuiApp, cell_rect, run};
use katui::ratatui::{ImageFrame, ImageId, ImageSlot, RgbaImage};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::widgets::{Block, Paragraph};

#[derive(Default)]
struct Picture {
    area: Rect,
    size: (u32, u32),
    pixels: Vec<u8>,
}

impl RatatuiApp for Picture {
    type Message = ();

    fn draw(&mut self, frame: &mut Frame<'_>, cell: CellPixels) {
        let [title, rest] =
            Layout::vertical([Constraint::Length(3), Constraint::Fill(1)]).areas(frame.area());
        frame.render_widget(Paragraph::new("A picture").block(Block::bordered()), title);
        frame.render_widget(ImageSlot, rest);
        if rest != self.area {
            self.area = rest;
            self.size = pixel_area(cell_rect(rest), cell);
            self.pixels = gradient(self.size);
        }
    }

    fn images(&self) -> Vec<ImageFrame<'_>> {
        RgbaImage::new(self.size.0, self.size.1, &self.pixels)
            .map(|pixels| ImageFrame {
                image: ImageId(self.size.0 * 10_000 + self.size.1),
                pixels,
                area: cell_rect(self.area),
            })
            .into_iter()
            .collect()
    }

    fn input(&mut self, event: &InputEvent) -> Control {
        match event {
            InputEvent::Key(Key::Char('q'), _) => Control::Quit,
            _ => Control::Wait,
        }
    }
}

fn gradient((width, height): (u32, u32)) -> Vec<u8> {
    (0..width * height)
        .flat_map(|at| {
            let red = u8::try_from(at % width.max(1) * 255 / width.max(1)).unwrap_or(255);
            [red, 90, 200, 255]
        })
        .collect()
}

fn main() -> Result<(), katui::rt::RtError> {
    run(Picture::default())
}
```

## How it fits together

`run` probes the terminal, switches to the alternate screen, and calls
your app:

- `draw` renders a frame with ratatui, as you always have. `cell` is the
  pixel size of one terminal cell, so you can draw an image at the full
  resolution of its area.
- `ImageSlot` is a ratatui widget that marks an area as taken, so ratatui
  never draws text over the picture.
- `images` lists the pictures for this frame, each with an id, its RGBA
  pixels and the cells it covers. katui compares them with the last frame:
  an unchanged image costs nothing, a moved one is placed again without
  sending its pixels, and one you leave out is removed. Give an image a
  new id when its pixels change.
- `input` gets keys, mouse events and pastes, and returns what to do next.
  <kbd>Ctrl</kbd>+<kbd>C</kbd> always quits.

After a resize, katui clears the screen and calls `draw` again with the
new size.

## Real images

`katui::ratatui::image` decodes PNG, JPEG, GIF and WebP with `decode`.
`contain_in_area` scales a picture to fit an area with its shape kept and
transparent bars on the sides, and `fit_to_area` stretches it to fill the
area. `ImageCache` decodes on worker threads: you `request` a file's bytes
under a key, `poll` each frame for the keys that finished, and take the
scaled pixels with `fitted` or `contained`. Pass `on_ready` a function
that sends your app a message, so it draws again when a decode finishes.

Images need a terminal with kitty graphics. Elsewhere your widgets still
draw as usual and the image areas stay empty.

Run `just demo-ratatui` to see an image viewer built this way, with lists,
tabs, a gauge and a sparkline beside the picture. Pass image files to add
them to its list: `just demo-ratatui photo.jpg`.

## Your own loop

If your app already owns its loop, use `KatuiBackend` directly as the
backend of a `ratatui::Terminal`, and call
`terminal.backend_mut().draw_images(&frames)` after each `terminal.draw`.
