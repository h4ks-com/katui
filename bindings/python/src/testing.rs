//! `Screen`: an app drawn off-terminal, for tests and screenshots.

use katui_bindings::screen::Screen;
use pyo3::prelude::*;

use crate::app::PyApp;
use crate::element::PyImage;
use crate::event::PyEvent;
use crate::theme::cell;

/// An app drawn into pixels, with clicks and keys sent by hand.
#[pyclass(name = "Screen", module = "katui.testing", unsendable)]
pub(crate) struct PyScreen {
    inner: Screen<PyApp>,
}

#[pymethods]
impl PyScreen {
    /// `app` on a screen of `width` by `height` pixels in cells of `cell`.
    #[new]
    #[pyo3(signature = (app, width=960, height=540, cell=(10, 20)))]
    fn new(app: Py<PyAny>, width: u32, height: u32, cell: (u16, u16)) -> PyResult<Self> {
        let inner = Screen::new(PyApp(app), (width, height), self::cell(cell))?;
        Ok(Self { inner })
    }

    /// Draw the app's view and return the picture.
    fn render(&mut self) -> PyResult<PyImage> {
        self.inner.render().map(PyImage::new)
    }

    /// Click at `x`, `y` pixels and deliver what the click sends.
    fn click(&mut self, x: f32, y: f32) -> PyResult<()> {
        self.inner.click(x, y)
    }

    /// Press at `x`, `y` and keep holding, for a drag.
    fn press(&mut self, x: f32, y: f32) -> PyResult<()> {
        self.inner.press(x, y)
    }

    /// Turn the wheel once at `x`, `y`, down or up.
    #[pyo3(signature = (x, y, down=true))]
    fn wheel(&mut self, x: f32, y: f32, down: bool) -> PyResult<()> {
        self.inner.wheel(x, y, down)
    }

    /// Move the pointer to `x`, `y`: the element it left gets its leave
    /// message, the one it entered its hover message.
    fn hover(&mut self, x: f32, y: f32) -> PyResult<()> {
        self.inner.hover(x, y)
    }

    /// Move a held press to `x`, `y`.
    fn drag(&mut self, x: f32, y: f32) -> PyResult<()> {
        self.inner.drag(x, y)
    }

    fn release(&mut self) {
        self.inner.release();
    }

    /// Hand `event` to the app's `input`.
    fn send(&mut self, event: &PyEvent) -> PyResult<()> {
        self.inner.send(&event.inner)
    }

    /// Type `text`, one key press per character.
    fn type_text(&mut self, text: &str) -> PyResult<()> {
        self.inner.type_text(text)
    }

    /// Whether the app asked to quit.
    #[getter]
    fn quit(&self) -> bool {
        self.inner.quit()
    }
}
