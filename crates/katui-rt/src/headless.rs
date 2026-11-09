//! A headless driver: runs an [`App`] with no terminal, so a test can type
//! bytes, deliver messages, resize the screen and read back every frame's
//! bytes.

use std::io;
use std::sync::mpsc::{self, Receiver};

use katui_proto::input::{InputEvent, InputParser};
use katui_scene::CellSize;

use crate::app::{App, AppEvent, Control};
use crate::events::{Signal, Waker};

/// Drives an [`App`] the way the event loop does, minus the terminal.
#[derive(Debug)]
pub struct Headless<A: App> {
    app: A,
    size: CellSize,
    parser: InputParser,
    messages: Receiver<Signal<A::Message>>,
    pending: Control,
}

impl<A: App> Headless<A>
where
    A::Message: Send + 'static,
{
    /// Start `app` on a screen of `size` cells.
    pub fn start(mut app: A, size: CellSize) -> Self {
        let (sender, messages) = mpsc::channel();
        app.start(Waker(sender));
        Self {
            app,
            size,
            parser: InputParser::default(),
            messages,
            pending: Control::Redraw,
        }
    }

    /// Feed `bytes` as if typed, with a lone escape counting as Esc.
    pub fn type_bytes(&mut self, bytes: &[u8]) {
        let mut events = self.parser.feed(bytes);
        events.extend(self.parser.idle());
        for event in events {
            self.input(event);
        }
    }

    /// Deliver one input event; answers to pacing queries stay with the
    /// loop, as they do on a terminal.
    pub fn input(&mut self, event: InputEvent) {
        if !crate::app::is_terminal_answer(&event) {
            self.update(AppEvent::Input(event));
        }
    }

    /// Deliver every message the app's threads queued so far.
    pub fn deliver_messages(&mut self) {
        while let Ok(signal) = self.messages.try_recv() {
            if let Signal::Message(message) = signal {
                self.update(AppEvent::Message(message));
            }
        }
    }

    /// Resize the screen and let it settle, the way a window resize ends.
    pub fn resize(&mut self, size: CellSize) {
        self.size = size;
        self.update(AppEvent::Resized(size));
        self.update(AppEvent::Repaint);
        self.pending = crate::app::merge(self.pending, Control::Redraw);
    }

    /// Draw one frame when the app asked for one; `None` when it waits or
    /// quit.
    pub fn frame(&mut self) -> io::Result<Option<Vec<u8>>> {
        if !matches!(self.pending, Control::Redraw | Control::Animate) {
            return Ok(None);
        }
        let mut bytes = Vec::new();
        self.pending = self.app.draw(&mut bytes, self.size)?;
        Ok(Some(bytes))
    }

    /// Whether the app asked to quit.
    pub fn quit(&self) -> bool {
        self.pending == Control::Quit
    }

    /// The app, for asserting on its state.
    pub fn app(&self) -> &A {
        &self.app
    }

    fn update(&mut self, event: AppEvent<A::Message>) {
        let control = self.app.update(event);
        self.pending = crate::app::merge(self.pending, control);
    }
}

#[cfg(test)]
mod tests {
    use std::io;

    use katui_proto::input::{InputEvent, Key};
    use katui_scene::CellSize;

    use super::Headless;
    use crate::app::{App, AppEvent, Control};
    use crate::events::Waker;

    #[derive(Default)]
    struct Counter {
        typed: String,
        waker: Option<Waker<u32>>,
        total: u32,
    }

    impl App for Counter {
        type Message = u32;

        fn start(&mut self, waker: Waker<u32>) {
            waker.send(5);
            self.waker = Some(waker);
        }

        fn update(&mut self, event: AppEvent<u32>) -> Control {
            match event {
                AppEvent::Input(event) if event.is_quit_key() => Control::Quit,
                AppEvent::Input(InputEvent::Key(Key::Char(letter), _)) => {
                    self.typed.push(letter);
                    Control::Redraw
                }
                AppEvent::Message(amount) => {
                    self.total += amount;
                    Control::Redraw
                }
                _ => Control::Wait,
            }
        }

        fn draw(&mut self, out: &mut Vec<u8>, size: CellSize) -> io::Result<Control> {
            out.extend(
                format!(
                    "{}x{} {} {}",
                    size.columns, size.rows, self.typed, self.total
                )
                .bytes(),
            );
            Ok(Control::Wait)
        }
    }

    fn size(columns: u16, rows: u16) -> CellSize {
        CellSize { columns, rows }
    }

    #[test]
    fn a_headless_app_types_receives_resizes_and_quits() {
        let mut driver = Headless::start(Counter::default(), size(80, 24));
        assert_eq!(
            driver.frame().expect("drawing works"),
            Some(b"80x24  0".to_vec())
        );
        assert_eq!(
            driver.frame().expect("drawing works"),
            None,
            "a waiting app draws nothing"
        );
        driver.type_bytes(b"hi");
        driver.deliver_messages();
        assert_eq!(
            driver.frame().expect("drawing works"),
            Some(b"80x24 hi 5".to_vec())
        );
        driver.type_bytes(b"\x1b[?62;4c");
        assert_eq!(
            driver.frame().expect("drawing works"),
            None,
            "an answer is no key and draws nothing"
        );
        driver.resize(size(40, 10));
        assert_eq!(
            driver.frame().expect("drawing works"),
            Some(b"40x10 hi 5".to_vec()),
            "a resize redraws"
        );
        driver.type_bytes(b"!");
        assert_eq!(
            driver.frame().expect("drawing works"),
            Some(b"40x10 hi! 5".to_vec())
        );
        driver.type_bytes(b"\x03");
        assert!(driver.quit());
        assert_eq!(driver.app().total, 5);
    }
}
