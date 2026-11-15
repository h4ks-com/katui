//! `Theme`: the colors and cell-based sizes every widget draws with.

use katui_ui::CellPixels;
use katui_widgets::Theme as UiTheme;
use napi::Result;

use crate::convert::{ColorArg, color, paint, paint_array, rgba_array};

/// The colors and sizes every widget draws with.
#[napi]
#[derive(Clone)]
#[must_use]
pub struct Theme {
    pub(crate) inner: UiTheme,
}

pub(crate) fn cell(cell: Option<(u16, u16)>) -> CellPixels {
    let (width, height) = cell.unwrap_or((10, 20));
    CellPixels { width, height }
}

#[napi]
impl Theme {
    /// The dark theme for cells of `cell` pixels, `[width, height]`.
    #[napi(factory)]
    pub fn dark(cell: Option<(u16, u16)>) -> Self {
        Self {
            inner: UiTheme::dark(self::cell(cell)),
        }
    }

    #[napi(factory)]
    pub fn light(cell: Option<(u16, u16)>) -> Self {
        Self {
            inner: UiTheme::light(self::cell(cell)),
        }
    }

    /// The theme the widgets draw with right now: the app's, inside a view.
    #[napi(factory)]
    pub fn current() -> Self {
        Self {
            inner: UiTheme::current().as_ref().clone(),
        }
    }

    /// The same theme with every size times `factor`.
    #[napi]
    pub fn scaled(&self, factor: f64) -> Self {
        Self {
            inner: self.inner.clone().scaled(factor as f32),
        }
    }

    #[napi(getter)]
    pub fn cell(&self) -> (u16, u16) {
        (self.inner.cell.width, self.inner.cell.height)
    }

    #[napi]
    pub fn cell_width(&self) -> f64 {
        f64::from(self.inner.cell_width())
    }

    #[napi]
    pub fn cell_height(&self) -> f64 {
        f64::from(self.inner.cell_height())
    }

    #[napi]
    pub fn body(&self) -> f64 {
        f64::from(self.inner.body())
    }

    #[napi]
    pub fn small(&self) -> f64 {
        f64::from(self.inner.small())
    }

    #[napi]
    pub fn heading(&self) -> f64 {
        f64::from(self.inner.heading())
    }

    #[napi]
    pub fn title(&self) -> f64 {
        f64::from(self.inner.title())
    }

    #[napi]
    pub fn radius(&self) -> f64 {
        f64::from(self.inner.radius())
    }

    /// The accent color while `active`, the border color otherwise.
    #[napi(ts_return_type = "Rgba | null")]
    pub fn outline(&self, active: bool) -> Option<Vec<u8>> {
        paint_array(&self.inner.outline(active))
    }

    /// `count` cells across, in pixels.
    #[napi]
    pub fn columns(&self, count: f64) -> f64 {
        f64::from(self.inner.cell_width()) * count
    }

    /// `count` cells down, in pixels.
    #[napi]
    pub fn rows(&self, count: f64) -> f64 {
        f64::from(self.inner.cell_height()) * count
    }

    /// A solid color, or `null` for a gradient.
    #[napi(getter, ts_return_type = "Rgba | null")]
    pub fn background(&self) -> Option<Vec<u8>> {
        paint_array(&self.inner.background)
    }

    #[napi(setter, js_name = "background", ts_args_type = "value: Color")]
    pub fn set_background(&mut self, value: ColorArg) -> Result<()> {
        self.inner.background = paint(value)?;
        Ok(())
    }

    #[napi(getter, ts_return_type = "Rgba | null")]
    pub fn raised(&self) -> Option<Vec<u8>> {
        paint_array(&self.inner.raised)
    }

    #[napi(setter, js_name = "raised", ts_args_type = "value: Color")]
    pub fn set_raised(&mut self, value: ColorArg) -> Result<()> {
        self.inner.raised = paint(value)?;
        Ok(())
    }

    #[napi(getter, ts_return_type = "Rgba | null")]
    pub fn highlight(&self) -> Option<Vec<u8>> {
        paint_array(&self.inner.highlight)
    }

    #[napi(setter, js_name = "highlight", ts_args_type = "value: Color")]
    pub fn set_highlight(&mut self, value: ColorArg) -> Result<()> {
        self.inner.highlight = paint(value)?;
        Ok(())
    }

    #[napi(getter, ts_return_type = "Rgba | null")]
    pub fn border(&self) -> Option<Vec<u8>> {
        paint_array(&self.inner.border)
    }

    #[napi(setter, js_name = "border", ts_args_type = "value: Color")]
    pub fn set_border(&mut self, value: ColorArg) -> Result<()> {
        self.inner.border = paint(value)?;
        Ok(())
    }

    #[napi(getter, ts_return_type = "Rgba")]
    pub fn text(&self) -> Vec<u8> {
        rgba_array(self.inner.text)
    }

    #[napi(setter, js_name = "text", ts_args_type = "value: Color")]
    pub fn set_text(&mut self, value: ColorArg) -> Result<()> {
        self.inner.text = color(value)?;
        Ok(())
    }

    #[napi(getter, ts_return_type = "Rgba")]
    pub fn muted(&self) -> Vec<u8> {
        rgba_array(self.inner.muted)
    }

    #[napi(setter, js_name = "muted", ts_args_type = "value: Color")]
    pub fn set_muted(&mut self, value: ColorArg) -> Result<()> {
        self.inner.muted = color(value)?;
        Ok(())
    }

    #[napi(getter, ts_return_type = "Rgba")]
    pub fn accent(&self) -> Vec<u8> {
        rgba_array(self.inner.accent)
    }

    #[napi(setter, js_name = "accent", ts_args_type = "value: Color")]
    pub fn set_accent(&mut self, value: ColorArg) -> Result<()> {
        self.inner.accent = color(value)?;
        Ok(())
    }

    #[napi(getter, ts_return_type = "Rgba")]
    pub fn on_accent(&self) -> Vec<u8> {
        rgba_array(self.inner.on_accent)
    }

    #[napi(setter, js_name = "onAccent", ts_args_type = "value: Color")]
    pub fn set_on_accent(&mut self, value: ColorArg) -> Result<()> {
        self.inner.on_accent = color(value)?;
        Ok(())
    }

    #[napi(getter, ts_return_type = "Rgba")]
    pub fn danger(&self) -> Vec<u8> {
        rgba_array(self.inner.danger)
    }

    #[napi(setter, js_name = "danger", ts_args_type = "value: Color")]
    pub fn set_danger(&mut self, value: ColorArg) -> Result<()> {
        self.inner.danger = color(value)?;
        Ok(())
    }
}
