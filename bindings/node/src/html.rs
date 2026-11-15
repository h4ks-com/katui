//! HTML and Markdown as elements, with links that send messages.

use std::sync::Arc;

use katui_bindings::html::{Format, Look, document, link};
use katui_content::HtmlHost;
use katui_scene::surface::Surface;
use napi::bindgen_prelude::{BigInt, JsObjectValue, Object, Unknown};
use napi::{Env, JsValue, Result, ValueType};

use crate::convert::{ColorArg, color, function, instance, is_nothing};
use crate::element::{Element, Image};
use crate::message::{Msg, Obj, hold, unknown};

/// What links and images in a document do: a link sends `onLink(href)`,
/// or the href itself without one, and `images` maps a `src` to an `Image`,
/// as an object or a function.
struct Host {
    env: Env,
    on_link: Option<Obj>,
    images: Option<Obj>,
}

impl Host {
    fn lookup(&self, images: &Obj, src: &str) -> Result<Option<Arc<Surface>>> {
        let env = &self.env;
        let images = unknown(env, images)?;
        let found: Unknown<'_> = if images.get_type()? == ValueType::Function {
            function::<String, Unknown<'_>>(images)?.call(src.to_owned())?
        } else {
            images.coerce_to_object()?.get_named_property(src)?
        };
        if is_nothing(&found)? {
            return Ok(None);
        }
        let image = instance::<Image>(env, found, "give an Image for each src")?;
        Ok(Some(Arc::clone(&image.surface)))
    }
}

impl HtmlHost<Msg> for Host {
    fn link(&self, href: &str) -> Msg {
        link(self.on_link.as_ref(), href)
    }

    fn image(&self, src: &str) -> Option<Arc<Surface>> {
        let images = self.images.as_ref()?;
        self.lookup(images, src).ok().flatten()
    }
}

fn optional<'env>(options: Option<&Object<'env>>, name: &str) -> Result<Option<Unknown<'env>>> {
    let Some(options) = options else {
        return Ok(None);
    };
    let value: Unknown<'env> = options.get_named_property(name)?;
    Ok(if is_nothing(&value)? {
        None
    } else {
        Some(value)
    })
}

fn color_option(options: Option<&Object<'_>>, name: &str) -> Result<Option<katui_draw::Rgba>> {
    let Some(options) = options else {
        return Ok(None);
    };
    let value: Option<ColorArg> = options.get_named_property(name)?;
    value.map(color).transpose()
}

/// The current theme's body size and colors, with any of them replaced.
fn look(options: Option<&Object<'_>>) -> Result<Look> {
    let font_size: Option<f64> = match options {
        Some(options) => options.get_named_property("fontSize")?,
        None => None,
    };
    Ok(Look {
        font_size: font_size.map(|size| size as f32),
        color: color_option(options, "color")?,
        link: color_option(options, "link")?,
        tint: color_option(options, "tint")?,
    })
}

fn host(env: Env, options: Option<&Object<'_>>) -> Result<Host> {
    let keep = |name| -> Result<Option<Obj>> {
        optional(options, name)?
            .map(|value| hold(&env, value))
            .transpose()
    };
    Ok(Host {
        env,
        on_link: keep("onLink")?,
        images: keep("images")?,
    })
}

fn element(
    env: Env,
    format: Format,
    source: &str,
    options: Option<&Object<'_>>,
) -> Result<Element> {
    let look = look(options)?;
    let host = host(env, options)?;
    Ok(Element::new(document(format, source, look, &host)))
}

/// The element id a link to `#fragment` reaches, for `Element.reveal`.
#[napi]
pub fn anchor_id(fragment: String) -> BigInt {
    BigInt::from(katui_content::anchor_id(&fragment))
}

/// An HTML document as an element.
#[napi(ts_args_type = "source: string, options?: DocumentOptions")]
pub fn html(env: Env, source: String, options: Option<Object<'_>>) -> Result<Element> {
    element(env, Format::Html, &source, options.as_ref())
}

/// A Markdown document as an element.
#[napi(ts_args_type = "source: string, options?: DocumentOptions")]
pub fn markdown(env: Env, source: String, options: Option<Object<'_>>) -> Result<Element> {
    element(env, Format::Markdown, &source, options.as_ref())
}
