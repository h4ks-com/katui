//! Python values to katui ones: colors, lengths and alignments, plus the
//! exception each shared error raises.

use katui_bindings::{Error, ErrorKind, names};
use katui_draw::{Paint, Rgba};
use katui_ui::{Align, Length};
use pyo3::exceptions::{PyOSError, PyRuntimeError, PyValueError};
use pyo3::prelude::*;

/// `error` as the Python exception of its kind.
pub(crate) fn raise(error: Error) -> PyErr {
    let message = error.to_string();
    match error.kind() {
        ErrorKind::Argument => PyValueError::new_err(message),
        ErrorKind::Io => PyOSError::new_err(message),
        ErrorKind::Runtime => PyRuntimeError::new_err(message),
    }
}

/// A color from `(r, g, b)`, `(r, g, b, a)` or `"#rrggbb"` / `"#rrggbbaa"`.
pub(crate) fn color(value: &Bound<'_, PyAny>) -> PyResult<Rgba> {
    if let Ok(text) = value.extract::<String>() {
        return names::hex(&text).map_err(raise);
    }
    if let Ok((r, g, b, a)) = value.extract::<(u8, u8, u8, u8)>() {
        return Ok(Rgba { r, g, b, a });
    }
    let (r, g, b) = value.extract::<(u8, u8, u8)>().map_err(|_| {
        PyValueError::new_err("give a color as (r, g, b), (r, g, b, a) or \"#rrggbb\"")
    })?;
    Ok(Rgba::solid(r, g, b))
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
    names::length(&value.extract::<String>()?).map_err(raise)
}

pub(crate) fn align(value: &str) -> PyResult<Align> {
    names::align(value).map_err(raise)
}
