# Video

This guide puts a video inside a katui app. You decode it with ffmpeg,
pull frames as the clock reaches them, and show each one as an image
element.

You need `ffmpeg` on your `PATH`, and the `app` feature.

## A player in one screen

```rust,no_run
use std::sync::Arc;
use std::time::Duration;

use katui::app::{Control, UiApp, Waker, run};
use katui::rt::media::VideoStream;
use katui::scene::surface::Surface;
use katui::ui::{Align, Element};
use katui::widgets::page;

const SIZE: (u32, u32) = (640, 360);
const FRAMES_PER_SECOND: u32 = 30;

struct Player {
    stream: VideoStream,
    frame: Arc<Surface>,
}

impl UiApp for Player {
    type Message = ();

    fn start(&mut self, waker: Waker<()>) {
        std::thread::spawn(move || {
            while waker.send(()) {
                std::thread::sleep(Duration::from_secs(1) / FRAMES_PER_SECOND);
            }
        });
    }

    fn view(&self) -> Element<()> {
        page([Element::image(Arc::clone(&self.frame))])
            .align(Align::Center)
            .justify(Align::Center)
    }

    fn update(&mut self, (): ()) -> Control {
        let Some(pixels) = self.stream.latest() else {
            return Control::Wait;
        };
        if let Some(surface) = Surface::from_rgba(SIZE.0, SIZE.1, pixels) {
            self.frame = Arc::new(surface);
        }
        Control::Redraw
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).ok_or("pass a video file or URL")?;
    let stream = VideoStream::ffmpeg(&["-i", path.as_str()], SIZE, FRAMES_PER_SECOND)?;
    let frame = Arc::new(Surface::new(SIZE.0, SIZE.1));
    run(Player { stream, frame })?;
    Ok(())
}
```

Run it with a file or a stream URL:

```sh
cargo run -- movie.mp4
```

## How it works

`VideoStream::ffmpeg` starts ffmpeg, which scales the video to fit `SIZE`
with black bars where the shape differs, and writes raw RGBA frames.
A thread inside the stream reads them as they come.

`latest` returns the newest frame the clock has reached since the last
call. When the terminal falls behind, it skips the frames in between, so
the picture keeps pace with the clock and never drifts behind it. When it
returns `None`, no new frame is due yet and the app waits.

The ticker thread in `start` sends a message 30 times a second, which is
how often the app checks for a frame.

The `ffmpeg` arguments are ffmpeg's input arguments, so anything ffmpeg can
open works: `["-ss", "90", "-i", "movie.mp4"]` starts 90 seconds in, and a
URL plays a stream. `VideoStream::spawn` takes any command that writes raw
RGBA frames of the size you give it, for decoders other than ffmpeg.

`ended` tells you when the decoder has finished and every frame went out.
Dropping the stream stops the decoder.

## Sound

`VideoStream` handles pictures only. The video demo plays sound with a
second process, `ffplay -nodisp`, started at the same position, and
restarts it when you seek or unmute. Read
`crates/katui-game/examples/video.rs` for that, and for pause, seek and a
progress bar. Run it with `just demo-video movie.mp4`.

## Picking a size

Bigger frames cost more to send. 640 by 360 plays smoothly in kitty. Inside
tmux, every byte goes through tmux first, so use about half that.
