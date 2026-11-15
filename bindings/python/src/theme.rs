//! `Theme`: the colors and cell-based sizes every widget draws with.

use katui_draw::Paint;
use katui_ui::CellPixels;
use katui_widgets::Theme;
use pyo3::prelude::*;

use crate::convert::{color, paint, paint_tuple, rgba_tuple};

type Color = (u8, u8, u8, u8);

/// The colors and sizes every widget draws with.
#[pyclass(name = "Theme", module = "katui", from_py_object)]
#[derive(Clone)]
pub(crate) struct PyTheme {
    pub(crate) inner: Theme,
}

pub(crate) fn cell((width, height): (u16, u16)) -> CellPixels {
    CellPixels { width, height }
}

#[pymethods]
impl PyTheme {
    /// The dark theme for cells of `cell` pixels, `(width, height)`.
    #[staticmethod]
    #[pyo3(signature = (cell=(10, 20)))]
    fn dark(cell: (u16, u16)) -> Self {
        Self {
            inner: Theme::dark(self::cell(cell)),
        }
    }

    #[staticmethod]
    #[pyo3(signature = (cell=(10, 20)))]
    fn light(cell: (u16, u16)) -> Self {
        Self {
            inner: Theme::light(self::cell(cell)),
        }
    }

    /// The theme the widgets draw with right now: the app's, inside a view.
    #[staticmethod]
    fn current() -> Self {
        Self {
            inner: Theme::current().as_ref().clone(),
        }
    }

    /// The same theme with every size times `factor`.
    fn scaled(&self, factor: f32) -> Self {
        Self {
            inner: self.inner.clone().scaled(factor),
        }
    }

    #[getter]
    fn cell(&self) -> (u16, u16) {
        (self.inner.cell.width, self.inner.cell.height)
    }

    fn cell_width(&self) -> f32 {
        self.inner.cell_width()
    }

    fn cell_height(&self) -> f32 {
        self.inner.cell_height()
    }

    fn body(&self) -> f32 {
        self.inner.body()
    }

    fn small(&self) -> f32 {
        self.inner.small()
    }

    fn heading(&self) -> f32 {
        self.inner.heading()
    }

    fn title(&self) -> f32 {
        self.inner.title()
    }

    fn radius(&self) -> f32 {
        self.inner.radius()
    }

    /// The accent color while `active`, the border color otherwise.
    fn outline(&self, active: bool) -> Option<Color> {
        paint_tuple(&self.inner.outline(active))
    }

    /// `count` cells across, in pixels.
    fn columns(&self, count: f32) -> f32 {
        self.inner.cell_width() * count
    }

    /// `count` cells down, in pixels.
    fn rows(&self, count: f32) -> f32 {
        self.inner.cell_height() * count
    }

    #[getter]
    fn background(&self) -> Option<Color> {
        paint_tuple(&self.inner.background)
    }

    #[setter]
    fn set_background(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.background = paint(value)?;
        Ok(())
    }

    #[getter]
    fn raised(&self) -> Option<Color> {
        paint_tuple(&self.inner.raised)
    }

    #[setter]
    fn set_raised(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.raised = paint(value)?;
        Ok(())
    }

    #[getter]
    fn highlight(&self) -> Option<Color> {
        paint_tuple(&self.inner.highlight)
    }

    #[setter]
    fn set_highlight(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.highlight = paint(value)?;
        Ok(())
    }

    #[getter]
    fn border(&self) -> Option<Color> {
        paint_tuple(&self.inner.border)
    }

    #[setter]
    fn set_border(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.border = paint(value)?;
        Ok(())
    }

    #[getter]
    fn text(&self) -> Color {
        rgba_tuple(self.inner.text)
    }

    #[setter]
    fn set_text(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.text = color(value)?;
        Ok(())
    }

    #[getter]
    fn muted(&self) -> Color {
        rgba_tuple(self.inner.muted)
    }

    #[setter]
    fn set_muted(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.muted = color(value)?;
        Ok(())
    }

    #[getter]
    fn accent(&self) -> Color {
        rgba_tuple(self.inner.accent)
    }

    #[setter]
    fn set_accent(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.accent = color(value)?;
        Ok(())
    }

    #[getter]
    fn on_accent(&self) -> Color {
        rgba_tuple(self.inner.on_accent)
    }

    #[setter]
    fn set_on_accent(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.on_accent = color(value)?;
        Ok(())
    }

    #[getter]
    fn danger(&self) -> Color {
        rgba_tuple(self.inner.danger)
    }

    #[setter]
    fn set_danger(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.danger = color(value)?;
        Ok(())
    }

    fn __repr__(&self) -> String {
        let solid = |paint: &Paint| {
            paint_tuple(paint)
                .map(|c| format!("{c:?}"))
                .unwrap_or_default()
        };
        format!(
            "Theme(cell={:?}, background={}, accent={:?})",
            self.cell(),
            solid(&self.inner.background),
            rgba_tuple(self.inner.accent)
        )
    }
}
