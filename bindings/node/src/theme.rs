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
}

/// Writes a getter and a setter for each paint field, read back as
/// `Rgba | null`, and for each color field, read back as `Rgba`.
macro_rules! theme_fields {
    (
        paints: [$($(#[doc = $paint_doc:tt])* $paint:ident, $set_paint:ident, $paint_js:tt;)*]
        colors: [$($color:ident, $set_color:ident, $color_js:tt;)*]
    ) => {
        #[napi]
        impl Theme {
            $(
                $(#[doc = $paint_doc])*
                #[napi(getter, ts_return_type = "Rgba | null")]
                pub fn $paint(&self) -> Option<Vec<u8>> {
                    paint_array(&self.inner.$paint)
                }

                #[napi(setter, js_name = $paint_js, ts_args_type = "value: Color")]
                pub fn $set_paint(&mut self, value: ColorArg) -> Result<()> {
                    self.inner.$paint = paint(value)?;
                    Ok(())
                }
            )*
            $(
                #[napi(getter, ts_return_type = "Rgba")]
                pub fn $color(&self) -> Vec<u8> {
                    rgba_array(self.inner.$color)
                }

                #[napi(setter, js_name = $color_js, ts_args_type = "value: Color")]
                pub fn $set_color(&mut self, value: ColorArg) -> Result<()> {
                    self.inner.$color = color(value)?;
                    Ok(())
                }
            )*
        }
    };
}

theme_fields! {
    paints: [
        #[doc = " A solid color, or `null` for a gradient."]
        background, set_background, "background";
        raised, set_raised, "raised";
        highlight, set_highlight, "highlight";
        border, set_border, "border";
    ]
    colors: [
        text, set_text, "text";
        muted, set_muted, "muted";
        accent, set_accent, "accent";
        on_accent, set_on_accent, "onAccent";
        danger, set_danger, "danger";
    ]
}
