//! `Screen`: an app drawn off-terminal, for tests and screenshots.

use std::rc::Rc;

use katui_app::Control as UiControl;
use katui_draw::{TextEngine, bundled_fonts};
use katui_proto::input::{InputEvent, Key, Mods};
use katui_ui::{CellPixels, Ui, UiOptions};
use napi::bindgen_prelude::Object;
use napi::{Env, Result};

use crate::app::{Bridge, Failure, take};
use crate::element::Image;
use crate::message::Msg;
use crate::state::Event;
use crate::theme::cell;

/// An app drawn into pixels, with clicks and keys sent by hand.
#[napi]
pub struct Screen {
    bridge: Bridge,
    failure: Failure,
    ui: Ui<Msg>,
    size: (u32, u32),
    cell: CellPixels,
    rendered: bool,
    quit: bool,
    hovered: Option<(usize, Option<Msg>)>,
}

impl Screen {
    fn checked(&mut self, control: UiControl) -> Result<()> {
        if let Some(error) = take(&self.failure) {
            return Err(error);
        }
        if control == UiControl::Quit {
            self.quit = true;
        }
        Ok(())
    }

    fn deliver(&mut self, message: Option<Msg>) -> Result<()> {
        let Some(message) = message else {
            return Ok(());
        };
        let control = self.bridge.deliver(&message);
        self.checked(control)
    }

    fn ensure_rendered(&mut self) -> Result<()> {
        if self.rendered {
            Ok(())
        } else {
            self.render().map(drop)
        }
    }

    fn send_event(&mut self, event: &InputEvent) -> Result<()> {
        let control = self.bridge.react(event);
        self.checked(control)
    }
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
        let engine = TextEngine::new(bundled_fonts())
            .map_err(|error| napi::Error::from_reason(format!("load the fonts: {error}")))?;
        let failure = Failure::default();
        Ok(Self {
            bridge: Bridge::new(env, app, Rc::clone(&failure))?,
            failure,
            ui: Ui::new(engine, UiOptions::default()),
            size: (width.unwrap_or(960), height.unwrap_or(540)),
            cell: self::cell(cell),
            rendered: false,
            quit: false,
            hovered: None,
        })
    }

    /// Draw the app's view and return the picture.
    #[napi]
    pub fn render(&mut self) -> Result<Image> {
        let root = self
            .bridge
            .theme_for(self.cell)
            .scope(|_| self.bridge.render());
        self.checked(UiControl::Wait)?;
        let output = self.ui.render(&root, self.size, self.cell);
        self.rendered = true;
        for message in self.ui.take_clamped_scrolls() {
            self.deliver(Some(message))?;
        }
        let picture = output
            .flatten(self.cell, self.size)
            .ok_or_else(|| napi::Error::from_reason("draw a screen of at least one pixel"))?;
        Ok(Image::new(picture))
    }

    /// Click at `x`, `y` pixels and deliver what the click sends.
    #[napi]
    pub fn click(&mut self, x: f64, y: f64) -> Result<()> {
        self.press(x, y)?;
        self.ui.release();
        Ok(())
    }

    /// Press at `x`, `y` and keep holding, for a drag.
    #[napi]
    pub fn press(&mut self, x: f64, y: f64) -> Result<()> {
        self.ensure_rendered()?;
        let (x, y) = (x as f32, y as f32);
        let message = self
            .ui
            .press(x, y)
            .or_else(|| self.ui.hit_test(x, y).and_then(|hit| hit.on_click));
        self.deliver(message)
    }

    /// Turn the wheel once at `x`, `y`, down or up.
    #[napi]
    pub fn wheel(&mut self, x: f64, y: f64, down: Option<bool>) -> Result<()> {
        self.ensure_rendered()?;
        let down = down.unwrap_or(true);
        let message = self.ui.hit_test(x as f32, y as f32).and_then(|hit| {
            if down {
                hit.on_wheel_down
            } else {
                hit.on_wheel_up
            }
        });
        self.deliver(message)
    }

    /// Move the pointer to `x`, `y`: the element it left gets its leave
    /// message, the one it entered its hover message.
    #[napi]
    pub fn hover(&mut self, x: f64, y: f64) -> Result<()> {
        self.ensure_rendered()?;
        let hit = self.ui.hit_test(x as f32, y as f32);
        let target = hit.as_ref().and_then(|hit| hit.hover_target);
        if self.hovered.as_ref().map(|(index, _)| *index) == target {
            return Ok(());
        }
        let left = self.hovered.take().and_then(|(_, leave)| leave);
        self.deliver(left)?;
        if let Some(hit) = hit {
            if let Some(index) = hit.hover_target {
                self.hovered = Some((index, hit.on_leave));
            }
            self.deliver(hit.on_hover)?;
        }
        Ok(())
    }

    /// Move a held press to `x`, `y`.
    #[napi]
    pub fn drag(&mut self, x: f64, y: f64) -> Result<()> {
        let message = self.ui.drag(x as f32, y as f32);
        self.deliver(message)
    }

    #[napi]
    pub fn release(&mut self) {
        self.ui.release();
    }

    /// Hand `event` to the app's `input`.
    #[napi]
    pub fn send(&mut self, event: &Event) -> Result<()> {
        self.send_event(&event.inner)
    }

    /// Type `text`, one key press per character.
    #[napi]
    pub fn type_text(&mut self, text: String) -> Result<()> {
        for character in text.chars() {
            self.send_event(&InputEvent::Key(Key::Char(character), Mods::NONE))?;
        }
        Ok(())
    }

    /// Whether the app asked to quit.
    #[napi(getter)]
    pub fn quit(&self) -> bool {
        self.quit
    }
}
