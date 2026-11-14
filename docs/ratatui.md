# Images in ratatui apps

If you already have a [ratatui](https://ratatui.rs) app, you can keep all
of it and add pictures. katui's `KatuiBackend` is a ratatui backend: your
widgets draw the same way, and the backend can place kitty images in areas
your layout leaves for them.

Add the `ratatui` feature, and ratatui itself at the version katui uses:

```sh
cargo add katui --git https://github.com/matheusfillipe/katui --features ratatui
cargo add ratatui@0.30
```

## A picture under a title

This program shows a bordered title and the image file you pass, filling
the rest of the screen. Press <kbd>Enter</kbd> to leave.

```rust,no_run
use std::io;

use katui::ratatui::image::{decode, fit_to_area, pixel_area};
use katui::ratatui::{ImageSlot, KatuiBackend};
use katui::rt::session::Session;
use katui::scene::placement::{CellRect, ImageId};
use ratatui::Terminal;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::widgets::{Block, Paragraph};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).ok_or("pass an image file")?;
    let caps = katui::rt::tty::probe_default();
    let cell = katui::rt::tty::cell_size(&caps);
    let size = katui::rt::tty::size()?;

    let session = Session::enter(io::stdout())?;
    let backend = KatuiBackend::new(size.columns, size.rows, io::stdout());
    let mut terminal = Terminal::new(backend)?;
    let mut picture = Rect::default();
    terminal.draw(|frame| {
        let [title, rest] =
            Layout::vertical([Constraint::Length(3), Constraint::Fill(1)]).areas(frame.area());
        frame.render_widget(Paragraph::new("A picture").block(Block::bordered()), title);
        frame.render_widget(ImageSlot, rest);
        picture = rest;
    })?;

    let area = CellRect {
        x: picture.x,
        y: picture.y,
        columns: picture.width,
        rows: picture.height,
    };
    let pixels = fit_to_area(&decode(&std::fs::read(path)?)?, area, cell);
    let (width, height) = pixel_area(area, cell);
    terminal.backend_mut().draw_image(ImageId(1), &pixels, width, height, area)?;

    io::stdin().read_line(&mut String::new())?;
    session.leave()?;
    Ok(())
}
```

## How it fits together

- `probe_default` asks the terminal what it supports, and `cell_size` gives
  the pixel size of one cell, which turns a cell area into a pixel size.
- `Session` switches to the alternate screen and restores the terminal
  when you leave.
- `ImageSlot` is a ratatui widget that marks an area as taken, so ratatui
  never draws text over the picture.
- `decode` reads PNG, JPEG, GIF and WebP. `fit_to_area` scales the image to
  the area's pixel size.
- `draw_image` places the image. Call it after each `terminal.draw`.
  `draw_images` takes several at once. Both compare with the last frame, so
  an unchanged image costs nothing, a moved one is placed again without
  sending its pixels, and an image you leave out is removed.

Give an image a new `ImageId` when its pixels change, and keep the same id
while they stay the same.

Images need a terminal with kitty graphics. Elsewhere your ratatui widgets
still draw as usual and the image areas stay empty.

To decode images without blocking your app, use
`katui::ratatui::image::ImageCache`. It decodes on worker threads: you
`request` a file's bytes under a key, `poll` each frame for the keys that
finished, and take the fitted pixels with `fitted`. It decodes each key
once and scales once per size, so redraws stay cheap.

Run `just demo-gallery` to see a ratatui frame with a generated image.
