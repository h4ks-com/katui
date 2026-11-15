//! An app drawn off-terminal, with clicks and keys sent by hand, for tests
//! and screenshots.

use katui_app::Control;
use katui_draw::{TextEngine, bundled_fonts};
use katui_proto::input::InputEvent;
use katui_scene::surface::Surface;
use katui_ui::{CellPixels, Hover, Ui, UiOptions};

use crate::app::{AppHost, Bridge};
use crate::event::typed;
use crate::{Error, Msg};

pub struct Screen<H: AppHost> {
    bridge: Bridge<H>,
    ui: Ui<Msg<H::Value>>,
    size: (u32, u32),
    cell: CellPixels,
    rendered: bool,
    quit: bool,
    hover: Hover<Msg<H::Value>>,
}

impl<H: AppHost> Screen<H> {
    /// `host` on a screen of `size` pixels in cells of `cell` pixels.
    pub fn new(host: H, size: (u32, u32), cell: CellPixels) -> Result<Self, H::Error> {
        let engine = TextEngine::new(bundled_fonts())
            .map_err(|error| H::refused(Error::Fonts(error.to_string())))?;
        Ok(Self {
            bridge: Bridge::new(host),
            ui: Ui::new(engine, UiOptions::default()),
            size,
            cell,
            rendered: false,
            quit: false,
            hover: Hover::default(),
        })
    }

    /// Whether the app asked to quit.
    pub fn quit(&self) -> bool {
        self.quit
    }

    /// Draw the app's view and return the picture.
    pub fn render(&mut self) -> Result<Surface, H::Error> {
        let root = self
            .bridge
            .theme_for(self.cell)
            .scope(|_| self.bridge.render());
        self.checked(Control::Wait)?;
        let output = self.ui.render(&root, self.size, self.cell);
        self.rendered = true;
        for message in self.ui.take_clamped_scrolls() {
            self.deliver(Some(message))?;
        }
        output
            .flatten(self.cell, self.size)
            .ok_or_else(|| H::refused(Error::EmptyScreen))
    }

    /// Click at `x`, `y` pixels and deliver what the click sends.
    pub fn click(&mut self, x: f32, y: f32) -> Result<(), H::Error> {
        self.press(x, y)?;
        self.ui.release();
        Ok(())
    }

    /// Press at `x`, `y` and keep holding, for a drag.
    pub fn press(&mut self, x: f32, y: f32) -> Result<(), H::Error> {
        self.ensure_rendered()?;
        let message = self.ui.click(x, y);
        self.deliver(message)
    }

    /// Move a held press to `x`, `y`.
    pub fn drag(&mut self, x: f32, y: f32) -> Result<(), H::Error> {
        let message = self.ui.drag(x, y);
        self.deliver(message)
    }

    pub fn release(&mut self) {
        self.ui.release();
    }

    /// Turn the wheel once at `x`, `y`, down or up.
    pub fn wheel(&mut self, x: f32, y: f32, down: bool) -> Result<(), H::Error> {
        self.ensure_rendered()?;
        let message = self.ui.wheel(x, y, down);
        self.deliver(message)
    }

    /// Move the pointer to `x`, `y`: the element it left gets its leave
    /// message, the one it entered its hover message.
    pub fn hover(&mut self, x: f32, y: f32) -> Result<(), H::Error> {
        self.ensure_rendered()?;
        for message in self.hover.moved(&self.ui, x, y) {
            self.deliver(Some(message))?;
        }
        Ok(())
    }

    /// Hand `event` to the app's `input`.
    pub fn send(&mut self, event: &InputEvent) -> Result<(), H::Error> {
        let control = self.bridge.react(event);
        self.checked(control)
    }

    /// Type `text`, one key press per character.
    pub fn type_text(&mut self, text: &str) -> Result<(), H::Error> {
        typed(text).try_for_each(|event| self.send(&event))
    }

    fn checked(&mut self, control: Control) -> Result<(), H::Error> {
        if let Some(error) = self.bridge.failure().take() {
            return Err(error);
        }
        if control == Control::Quit {
            self.quit = true;
        }
        Ok(())
    }

    fn deliver(&mut self, message: Option<Msg<H::Value>>) -> Result<(), H::Error> {
        let Some(message) = message else {
            return Ok(());
        };
        let control = self.bridge.deliver(&message);
        self.checked(control)
    }

    fn ensure_rendered(&mut self) -> Result<(), H::Error> {
        if self.rendered {
            Ok(())
        } else {
            self.render().map(drop)
        }
    }
}

#[cfg(test)]
mod tests {
    use katui_app::Waker;
    use katui_widgets::Theme;

    use super::*;
    use crate::Node;

    /// Counts clicks on one button and fails on a key press.
    #[derive(Default)]
    struct Counter {
        clicks: u32,
    }

    impl AppHost for Counter {
        type Value = u32;
        type Error = String;

        fn refused(error: Error) -> String {
            error.to_string()
        }

        fn start(&mut self, _waker: Waker<Msg<u32>>) -> Result<(), String> {
            Ok(())
        }

        fn view(&self) -> Result<Node<u32>, String> {
            Ok(katui_widgets::page(vec![katui_widgets::button(
                &format!("clicked {}", self.clicks),
                Msg::Value(1),
            )]))
        }

        fn update(&mut self, message: &Msg<u32>) -> Result<Control, String> {
            if let Msg::Value(step) = message {
                self.clicks += step;
            }
            Ok(if self.clicks >= 2 {
                Control::Quit
            } else {
                Control::Redraw
            })
        }

        fn input(&mut self, _event: &InputEvent) -> Result<Control, String> {
            Err("no keys here".to_owned())
        }

        fn theme(&self, _cell: CellPixels) -> Result<Option<Theme>, String> {
            Ok(None)
        }
    }

    #[test]
    fn clicks_reach_update_and_failures_come_back() -> Result<(), String> {
        let cell = CellPixels {
            width: 10,
            height: 20,
        };
        let mut screen = Screen::new(Counter::default(), (320, 180), cell)?;
        let picture = screen.render()?;
        assert_eq!((picture.width(), picture.height()), (320, 180));
        screen.click(40.0, 30.0)?;
        assert!(!screen.quit());
        screen.click(40.0, 30.0)?;
        assert!(screen.quit());
        assert_eq!(screen.type_text("x"), Err("no keys here".to_owned()));
        Ok(())
    }
}
