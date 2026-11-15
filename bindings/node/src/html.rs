//! HTML and Markdown as elements, with links that send messages.

use std::sync::Arc;

use katui_content::{HtmlDefaults, HtmlHost};
use katui_scene::surface::Surface;
use katui_widgets::Theme as UiTheme;
use napi::bindgen_prelude::{JsObjectValue, Object, Unknown};
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
        match &self.on_link {
            Some(make) => Msg::Link(Arc::clone(make), href.to_owned()),
            None => Msg::Href(href.to_owned()),
        }
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
fn defaults(options: Option<&Object<'_>>) -> Result<HtmlDefaults> {
    let theme = UiTheme::current();
    let font_size: Option<f64> = match options {
        Some(options) => options.get_named_property("fontSize")?,
        None => None,
    };
    Ok(HtmlDefaults {
        font_size: font_size.map_or_else(|| theme.body(), |size| size as f32),
        color: color_option(options, "color")?.unwrap_or(theme.text),
        link: color_option(options, "link")?.unwrap_or(theme.accent),
        tint: color_option(options, "tint")?.unwrap_or(theme.muted),
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

/// An HTML document as an element.
#[napi(ts_args_type = "source: string, options?: DocumentOptions")]
pub fn html(env: Env, source: String, options: Option<Object<'_>>) -> Result<Element> {
    let defaults = defaults(options.as_ref())?;
    let host = host(env, options.as_ref())?;
    Ok(Element::new(katui_content::html(&source, &defaults, &host)))
}

/// A Markdown document as an element.
#[napi(ts_args_type = "source: string, options?: DocumentOptions")]
pub fn markdown(env: Env, source: String, options: Option<Object<'_>>) -> Result<Element> {
    let defaults = defaults(options.as_ref())?;
    let host = host(env, options.as_ref())?;
    Ok(Element::new(katui_content::markdown_view(
        &source, &defaults, &host,
    )))
}
