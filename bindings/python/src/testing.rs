//! `Screen`: an app drawn off-terminal, for tests and screenshots.

use std::sync::Arc;

use katui_app::Control;
use katui_draw::{TextEngine, bundled_fonts};
use katui_ui::{CellPixels, Ui, UiOptions};
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;

use crate::app::{Bridge, Failure, take};
use crate::element::PyImage;
use crate::message::Msg;
use crate::state::PyEvent;
use crate::theme::cell;

/// An app drawn into pixels, with clicks and keys sent by hand.
#[pyclass(name = "Screen", module = "katui.testing", unsendable)]
pub(crate) struct PyScreen {
    bridge: Bridge,
    failure: Failure,
    ui: Ui<Msg>,
    size: (u32, u32),
    cell: CellPixels,
    rendered: bool,
    quit: bool,
    hovered: Option<(usize, Option<Msg>)>,
}

impl PyScreen {
    fn checked(&mut self, control: Control) -> PyResult<()> {
        if let Some(error) = take(&self.failure) {
            return Err(error);
        }
        if control == Control::Quit {
            self.quit = true;
        }
        Ok(())
    }

    fn deliver(&mut self, message: Option<Msg>) -> PyResult<()> {
        let Some(message) = message else {
            return Ok(());
        };
        let control = self.bridge.deliver(&message);
        self.checked(control)
    }

    fn ensure_rendered(&mut self) -> PyResult<()> {
        if self.rendered {
            Ok(())
        } else {
            self.render().map(drop)
        }
    }
}

#[pymethods]
impl PyScreen {
    /// `app` on a screen of `width` by `height` pixels in cells of `cell`.
    #[new]
    #[pyo3(signature = (app, width=960, height=540, cell=(10, 20)))]
    fn new(app: Py<PyAny>, width: u32, height: u32, cell: (u16, u16)) -> PyResult<Self> {
        let engine = TextEngine::new(bundled_fonts())
            .map_err(|error| PyRuntimeError::new_err(format!("load the fonts: {error}")))?;
        let failure = Failure::default();
        Ok(Self {
            bridge: Bridge::new(app, Arc::clone(&failure)),
            failure,
            ui: Ui::new(engine, UiOptions::default()),
            size: (width, height),
            cell: self::cell(cell),
            rendered: false,
            quit: false,
            hovered: None,
        })
    }

    /// Draw the app's view and return the picture.
    fn render(&mut self) -> PyResult<PyImage> {
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
        let picture = output
            .flatten(self.cell, self.size)
            .ok_or_else(|| PyRuntimeError::new_err("draw a screen of at least one pixel"))?;
        Ok(PyImage::new(picture))
    }

    /// Click at `x`, `y` pixels and deliver what the click sends.
    fn click(&mut self, x: f32, y: f32) -> PyResult<()> {
        self.press(x, y)?;
        self.ui.release();
        Ok(())
    }

    /// Press at `x`, `y` and keep holding, for a drag.
    fn press(&mut self, x: f32, y: f32) -> PyResult<()> {
        self.ensure_rendered()?;
        let message = self
            .ui
            .press(x, y)
            .or_else(|| self.ui.hit_test(x, y).and_then(|hit| hit.on_click));
        self.deliver(message)
    }

    /// Turn the wheel once at `x`, `y`, down or up.
    #[pyo3(signature = (x, y, down=true))]
    fn wheel(&mut self, x: f32, y: f32, down: bool) -> PyResult<()> {
        self.ensure_rendered()?;
        let message = self.ui.hit_test(x, y).and_then(|hit| {
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
    fn hover(&mut self, x: f32, y: f32) -> PyResult<()> {
        self.ensure_rendered()?;
        let hit = self.ui.hit_test(x, y);
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
    fn drag(&mut self, x: f32, y: f32) -> PyResult<()> {
        let message = self.ui.drag(x, y);
        self.deliver(message)
    }

    fn release(&mut self) {
        self.ui.release();
    }

    /// Hand `event` to the app's `input`.
    fn send(&mut self, event: &PyEvent) -> PyResult<()> {
        let control = self.bridge.react(&event.inner);
        self.checked(control)
    }

    /// Type `text`, one key press per character.
    fn type_text(&mut self, text: &str) -> PyResult<()> {
        for character in text.chars() {
            let event = PyEvent {
                inner: katui_proto::input::InputEvent::Key(
                    katui_proto::input::Key::Char(character),
                    katui_proto::input::Mods::NONE,
                ),
            };
            self.send(&event)?;
        }
        Ok(())
    }

    /// Whether the app asked to quit.
    #[getter]
    fn quit(&self) -> bool {
        self.quit
    }
}
