//! `Screen`: an app drawn off-terminal, for tests and screenshots.

use katui_bindings::screen::Screen as UiScreen;
use napi::bindgen_prelude::Object;
use napi::{Env, Result};

use crate::app::NodeApp;
use crate::element::Image;
use crate::event::Event;
use crate::theme::cell;

/// An app drawn into pixels, with clicks and keys sent by hand.
#[napi]
pub struct Screen {
    inner: UiScreen<NodeApp>,
}

#[napi]
impl Screen {
    /// `app` on a screen of `width` by `height` pixels in cells of `cell`.
    #[napi(
        constructor,
        ts_args_type = "app: AppLike, width?: number, height?: number, cell?: [number, number]"
    )]
    pub fn new(
        env: Env,
        app: Object<'_>,
        width: Option<u32>,
        height: Option<u32>,
        cell: Option<(u16, u16)>,
    ) -> Result<Self> {
        let size = (width.unwrap_or(960), height.unwrap_or(540));
        let inner = UiScreen::new(NodeApp::new(env, app)?, size, self::cell(cell))?;
        Ok(Self { inner })
    }

    /// Draw the app's view and return the picture.
    #[napi]
    pub fn render(&mut self) -> Result<Image> {
        self.inner.render().map(Image::new)
    }

    /// Click at `x`, `y` pixels and deliver what the click sends.
    #[napi]
    pub fn click(&mut self, x: f64, y: f64) -> Result<()> {
        self.inner.click(x as f32, y as f32)
    }

    /// Press at `x`, `y` and keep holding, for a drag.
    #[napi]
    pub fn press(&mut self, x: f64, y: f64) -> Result<()> {
        self.inner.press(x as f32, y as f32)
    }

    /// Turn the wheel once at `x`, `y`, down or up.
    #[napi]
    pub fn wheel(&mut self, x: f64, y: f64, down: Option<bool>) -> Result<()> {
        self.inner.wheel(x as f32, y as f32, down.unwrap_or(true))
    }

    /// Move the pointer to `x`, `y`: the element it left gets its leave
    /// message, the one it entered its hover message.
    #[napi]
    pub fn hover(&mut self, x: f64, y: f64) -> Result<()> {
        self.inner.hover(x as f32, y as f32)
    }

    /// Move a held press to `x`, `y`.
    #[napi]
    pub fn drag(&mut self, x: f64, y: f64) -> Result<()> {
        self.inner.drag(x as f32, y as f32)
    }

    #[napi]
    pub fn release(&mut self) {
        self.inner.release();
    }

    /// Hand `event` to the app's `input`.
    #[napi]
    pub fn send(&mut self, event: &Event) -> Result<()> {
        self.inner.send(&event.inner)
    }

    /// Type `text`, one key press per character.
    #[napi]
    pub fn type_text(&mut self, text: String) -> Result<()> {
        self.inner.type_text(&text)
    }

    /// Whether the app asked to quit.
    #[napi(getter)]
    pub fn quit(&self) -> bool {
        self.inner.quit()
    }
}
