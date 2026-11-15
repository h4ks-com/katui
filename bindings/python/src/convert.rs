//! Python values to katui ones: colors, lengths and alignments.

use katui_draw::{Paint, Rgba};
use katui_ui::{Align, Length};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

/// A color from `(r, g, b)`, `(r, g, b, a)` or `"#rrggbb"` / `"#rrggbbaa"`.
pub(crate) fn color(value: &Bound<'_, PyAny>) -> PyResult<Rgba> {
    if let Ok(text) = value.extract::<String>() {
        return hex(&text);
    }
    if let Ok((r, g, b, a)) = value.extract::<(u8, u8, u8, u8)>() {
        return Ok(Rgba { r, g, b, a });
    }
    let (r, g, b) = value.extract::<(u8, u8, u8)>().map_err(|_| {
        PyValueError::new_err("give a color as (r, g, b), (r, g, b, a) or \"#rrggbb\"")
    })?;
    Ok(Rgba::solid(r, g, b))
}

fn hex(text: &str) -> PyResult<Rgba> {
    Rgba::from_hex(text)
        .ok_or_else(|| PyValueError::new_err(format!("write the color {text:?} as \"#rrggbb\"")))
}

pub(crate) fn paint(value: &Bound<'_, PyAny>) -> PyResult<Paint> {
    color(value).map(Paint::Solid)
}

pub(crate) fn rgba_tuple(color: Rgba) -> (u8, u8, u8, u8) {
    (color.r, color.g, color.b, color.a)
}

/// The color of a solid paint; `None` for a gradient.
pub(crate) fn paint_tuple(paint: &Paint) -> Option<(u8, u8, u8, u8)> {
    match paint {
        Paint::Solid(color) => Some(rgba_tuple(*color)),
        Paint::Linear { .. } => None,
    }
}

/// A length from pixels, `"fill"` or `"content"`.
pub(crate) fn length(value: &Bound<'_, PyAny>) -> PyResult<Length> {
    if let Ok(pixels) = value.extract::<f32>() {
        return Ok(Length::Px(pixels));
    }
    match value.extract::<String>()?.as_str() {
        "fill" => Ok(Length::Fill),
        "content" => Ok(Length::Content),
        other => Err(PyValueError::new_err(format!(
            "give a length in pixels, \"fill\" or \"content\", not {other:?}"
        ))),
    }
}

pub(crate) fn align(value: &str) -> PyResult<Align> {
    match value {
        "start" => Ok(Align::Start),
        "center" => Ok(Align::Center),
        "end" => Ok(Align::End),
        other => Err(PyValueError::new_err(format!(
            "align to \"start\", \"center\" or \"end\", not {other:?}"
        ))),
    }
}
