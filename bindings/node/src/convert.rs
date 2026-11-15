//! JavaScript values to katui ones: colors, lengths, alignments and
//! elements, plus the errors a wrong one raises.

use katui_draw::{Paint, Rgba};
use katui_ui::{Align, Length};
use napi::bindgen_prelude::{
    ClassInstance, Either, FromNapiValue, Function, JavaScriptClassExt, JsValuesTupleIntoVec,
    Unknown,
};
use napi::{Env, Error, Result, Status, ValueType};

/// A color: `"#rrggbb"`, `"#rrggbbaa"`, `[r, g, b]` or `[r, g, b, a]`.
pub(crate) type ColorArg = Either<String, Vec<u8>>;

/// A length: pixels, `"fill"` or `"content"`.
pub(crate) type LengthArg = Either<f64, String>;

pub(crate) fn invalid(message: impl Into<String>) -> Error {
    Error::new(Status::InvalidArg, message.into())
}

pub(crate) fn color(value: ColorArg) -> Result<Rgba> {
    match value {
        Either::A(text) => hex(&text),
        Either::B(channels) => match *channels.as_slice() {
            [r, g, b] => Ok(Rgba::solid(r, g, b)),
            [r, g, b, a] => Ok(Rgba { r, g, b, a }),
            _ => Err(invalid(
                "give a color as [r, g, b], [r, g, b, a] or \"#rrggbb\"",
            )),
        },
    }
}

fn hex(text: &str) -> Result<Rgba> {
    Rgba::from_hex(text).ok_or_else(|| invalid(format!("write the color {text:?} as \"#rrggbb\"")))
}

pub(crate) fn paint(value: ColorArg) -> Result<Paint> {
    color(value).map(Paint::Solid)
}

pub(crate) fn rgba_array(color: Rgba) -> Vec<u8> {
    vec![color.r, color.g, color.b, color.a]
}

/// The color of a solid paint; `None` for a gradient.
pub(crate) fn paint_array(paint: &Paint) -> Option<Vec<u8>> {
    match paint {
        Paint::Solid(color) => Some(rgba_array(*color)),
        Paint::Linear { .. } => None,
    }
}

pub(crate) fn length(value: LengthArg) -> Result<Length> {
    match value {
        Either::A(pixels) => Ok(Length::Px(pixels as f32)),
        Either::B(name) => match name.as_str() {
            "fill" => Ok(Length::Fill),
            "content" => Ok(Length::Content),
            other => Err(invalid(format!(
                "give a length in pixels, \"fill\" or \"content\", not {other:?}"
            ))),
        },
    }
}

pub(crate) fn align(value: &str) -> Result<Align> {
    match value {
        "start" => Ok(Align::Start),
        "center" => Ok(Align::Center),
        "end" => Ok(Align::End),
        other => Err(invalid(format!(
            "align to \"start\", \"center\" or \"end\", not {other:?}"
        ))),
    }
}

/// Whether `value` is `null` or `undefined`.
pub(crate) fn is_nothing(value: &Unknown<'_>) -> Result<bool> {
    Ok(matches!(
        value.get_type()?,
        ValueType::Null | ValueType::Undefined
    ))
}

/// `value` as a function, or an error when it is something else.
pub(crate) fn function<'env, Args, Return>(
    value: Unknown<'env>,
) -> Result<Function<'env, Args, Return>>
where
    Args: JsValuesTupleIntoVec,
    Return: FromNapiValue,
{
    if value.get_type()? != ValueType::Function {
        return Err(invalid("pass a function"));
    }
    // SAFETY: the type check above confirms a function.
    unsafe { value.cast::<Function<'env, Args, Return>>() }
}

/// `value` as an instance of the class `T`, or the error `what` names.
pub(crate) fn instance<'env, T>(
    env: &Env,
    value: Unknown<'env>,
    what: &str,
) -> Result<ClassInstance<'env, T>>
where
    T: JavaScriptClassExt + napi::bindgen_prelude::MaybeTypeTag + 'env,
{
    if !T::instance_of(env, &value)? {
        return Err(invalid(what));
    }
    // SAFETY: instance_of above and the type tag check in the cast both confirm a `T`.
    unsafe { value.cast::<ClassInstance<'env, T>>() }
}
