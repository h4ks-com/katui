//! A ratatui app on katui's event loop: implement [`RatatuiApp`] and hand
//! it to [`run`]. The runner probes the terminal, draws your frames
//! through [`KatuiBackend`], places the images you list, rebuilds the
//! screen after a resize and passes keys, mouse and paste to your app.

use std::io;

use katui_proto::input::{InputEvent, Key, Mods};
use katui_proto::kitty::Delete;
use katui_proto::probe::{Capabilities, CellPixels};
use katui_rt::RtError;
use katui_rt::app::{App, AppEvent, run_app};
use katui_scene::CellSize;
use katui_scene::placement::CellRect;
use ratatui::layout::{Rect, Size};
use ratatui::{Frame, Terminal};

pub use katui_rt::app::Control;
pub use katui_rt::events::Waker;

use crate::{ImageFrame, KatuiBackend};

const FRAMES_PER_SECOND: u32 = 30;

/// A ratatui program: it draws frames, lists the images they show, and
/// reacts to input and messages.
pub trait RatatuiApp {
    /// What the app's own threads send it through the [`Waker`].
    type Message: Send + 'static;

    /// Keep `waker` to send messages from other threads.
    fn start(&mut self, _waker: Waker<Self::Message>) {}

    /// Draw the cells of one frame. `cell` is the pixel size of one cell,
    /// for drawing images at the full resolution of their area. Render an
    /// [`crate::ImageSlot`] over each area an image covers, so ratatui
    /// leaves those cells alone.
    fn draw(&mut self, frame: &mut Frame<'_>, cell: CellPixels);

    /// The images this frame shows, drawn after the cells. Give an image a
    /// new id when its pixels change; an unchanged image costs nothing.
    fn images(&self) -> Vec<ImageFrame<'_>> {
        Vec::new()
    }

    /// React to a key, a mouse event or a paste. Ctrl+C quits before this
    /// is called.
    fn input(&mut self, event: &InputEvent) -> Control;

    /// React to a message from one of the app's threads.
    fn update(&mut self, _message: Self::Message) -> Control {
        Control::Wait
    }
}

/// The cells a ratatui [`Rect`] covers, as the area of an [`ImageFrame`].
#[must_use]
pub fn cell_rect(area: Rect) -> CellRect {
    CellRect {
        x: area.x,
        y: area.y,
        columns: area.width,
        rows: area.height,
    }
}

/// Run `app` on the terminal until it quits.
pub fn run<A: RatatuiApp>(app: A) -> Result<(), RtError> {
    let caps = katui_rt::tty::probe_default();
    let mut runner = Runner {
        app,
        caps: caps.clone(),
        terminal: None,
    };
    run_app(&mut runner, io::stdout(), &caps, FRAMES_PER_SECOND)
}

struct Runner<A> {
    app: A,
    caps: Capabilities,
    terminal: Option<Terminal<KatuiBackend<Vec<u8>>>>,
}

impl<A: RatatuiApp> Runner<A> {
    /// A fresh terminal for a screen of `size`, cleared, with the images
    /// of the last one deleted, so the next draw sends everything.
    fn rebuild(
        &self,
        size: CellSize,
        out: &mut Vec<u8>,
    ) -> io::Result<Terminal<KatuiBackend<Vec<u8>>>> {
        let cell = katui_rt::tty::cell_size(&self.caps);
        let pixels = Size::new(
            size.columns.saturating_mul(cell.width),
            size.rows.saturating_mul(cell.height),
        );
        if self.caps.kitty {
            out.extend_from_slice(&Delete::AllVisible.encode());
        }
        let backend =
            KatuiBackend::new(size.columns, size.rows, Vec::new()).with_pixel_size(pixels);
        let mut terminal = Terminal::new(backend)?;
        terminal.clear()?;
        Ok(terminal)
    }
}

impl<A: RatatuiApp> App for Runner<A> {
    type Message = A::Message;

    fn start(&mut self, waker: Waker<A::Message>) {
        self.app.start(waker);
    }

    fn update(&mut self, event: AppEvent<A::Message>) -> Control {
        match event {
            AppEvent::Input(InputEvent::Key(Key::Char('c'), Mods { ctrl: true, .. })) => {
                Control::Quit
            }
            AppEvent::Input(event) => self.app.input(&event),
            AppEvent::Message(message) => self.app.update(message),
            AppEvent::Resized(_) | AppEvent::Repaint => {
                self.terminal = None;
                Control::Redraw
            }
        }
    }

    fn draw(&mut self, out: &mut Vec<u8>, size: CellSize) -> io::Result<Control> {
        let terminal = match self.terminal.take() {
            Some(terminal) if terminal.backend().screen().size() == size => terminal,
            _ => self.rebuild(size, out)?,
        };
        let terminal = self.terminal.insert(terminal);
        let cell = katui_rt::tty::cell_size(&self.caps);
        terminal.draw(|frame| self.app.draw(frame, cell))?;
        if self.caps.kitty {
            terminal.backend_mut().draw_images(&self.app.images())?;
        }
        out.append(terminal.backend_mut().writer_mut());
        Ok(Control::Wait)
    }
}

#[cfg(test)]
mod tests {
    use katui_proto::input::{InputEvent, Key};
    use katui_proto::probe::Capabilities;
    use katui_rt::headless::Headless;
    use katui_scene::CellSize;
    use ratatui::Frame;
    use ratatui::widgets::Paragraph;

    use super::{Control, RatatuiApp, Runner, cell_rect};
    use crate::{ImageFrame, ImageId, ImageSlot, RgbaImage};

    struct Picture {
        pixels: Vec<u8>,
        presses: u32,
    }

    impl RatatuiApp for Picture {
        type Message = ();

        fn draw(&mut self, frame: &mut Frame<'_>, _cell: katui_proto::probe::CellPixels) {
            frame.render_widget(
                Paragraph::new(format!("pressed {}", self.presses)),
                frame.area(),
            );
            frame.render_widget(ImageSlot, ratatui::layout::Rect::new(0, 1, 2, 1));
        }

        fn images(&self) -> Vec<ImageFrame<'_>> {
            RgbaImage::new(2, 1, &self.pixels)
                .map(|pixels| ImageFrame {
                    image: ImageId(7),
                    pixels,
                    area: cell_rect(ratatui::layout::Rect::new(0, 1, 2, 1)),
                })
                .into_iter()
                .collect()
        }

        fn input(&mut self, event: &InputEvent) -> Control {
            if let InputEvent::Key(Key::Char(' '), _) = event {
                self.presses += 1;
                return Control::Redraw;
            }
            Control::Wait
        }
    }

    #[test]
    fn frames_carry_the_cells_and_the_images_and_keys_reach_the_app() {
        let caps = Capabilities {
            kitty: true,
            ..Capabilities::default()
        };
        let runner = Runner {
            app: Picture {
                pixels: vec![255; 8],
                presses: 0,
            },
            caps,
            terminal: None,
        };
        let mut driver = Headless::start(
            runner,
            CellSize {
                columns: 20,
                rows: 4,
            },
        );
        let first = driver
            .frame()
            .expect("drawing works")
            .expect("a first frame");
        let text = String::from_utf8_lossy(&first);
        assert!(
            text.contains('p') && text.contains('d'),
            "the paragraph draws"
        );
        assert!(text.contains("a=t"), "the image transmits");
        driver.type_bytes(b" ");
        assert_eq!(driver.app().app.presses, 1);
        let next = driver
            .frame()
            .expect("drawing works")
            .expect("a changed frame");
        assert!(
            !String::from_utf8_lossy(&next).contains("a=t"),
            "an unchanged image is not sent again"
        );
        driver.type_bytes(b"\x03");
        assert!(driver.quit());
    }
}
