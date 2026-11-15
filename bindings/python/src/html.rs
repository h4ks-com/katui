//! HTML and Markdown as elements, with links that send messages.

use std::sync::Arc;

use katui_content::{HtmlDefaults, HtmlHost};
use katui_scene::surface::Surface;
use katui_widgets::Theme;
use pyo3::prelude::*;
use pyo3::types::PyDict;

use crate::convert::color;
use crate::element::{PyElement, PyImage};
use crate::message::{Msg, Obj, obj};

/// What links and images in a document do: a link sends `on_link(href)`,
/// or the href itself without one, and `images` maps a `src` to an
/// `Image`, as a dict or a function.
struct Host {
    on_link: Option<Obj>,
    images: Option<Obj>,
}

impl HtmlHost<Msg> for Host {
    fn link(&self, href: &str) -> Msg {
        match &self.on_link {
            Some(make) => Msg::Link(Arc::clone(make), href.to_owned()),
            None => Python::attach(|py| {
                Msg::Value(Arc::new(
                    href.into_pyobject(py)
                        .map_or_else(|_| py.None(), |text| text.into_any().unbind()),
                ))
            }),
        }
    }

    fn image(&self, src: &str) -> Option<Arc<Surface>> {
        let images = self.images.as_ref()?;
        Python::attach(|py| {
            let images = images.bind(py);
            let found = if let Ok(table) = images.cast::<PyDict>() {
                table.get_item(src).ok().flatten()
            } else {
                images.call1((src,)).ok()
            }?;
            let image = found.cast::<PyImage>().ok()?;
            Some(Arc::clone(&image.get().surface))
        })
    }
}

/// The current theme's body size and colors, with any of them replaced.
fn defaults(
    font_size: Option<f32>,
    text: Option<&Bound<'_, PyAny>>,
    link: Option<&Bound<'_, PyAny>>,
    tint: Option<&Bound<'_, PyAny>>,
) -> PyResult<HtmlDefaults> {
    let theme = Theme::current();
    Ok(HtmlDefaults {
        font_size: font_size.unwrap_or_else(|| theme.body()),
        color: text.map(color).transpose()?.unwrap_or(theme.text),
        link: link.map(color).transpose()?.unwrap_or(theme.accent),
        tint: tint.map(color).transpose()?.unwrap_or(theme.muted),
    })
}

fn host(on_link: Option<&Bound<'_, PyAny>>, images: Option<&Bound<'_, PyAny>>) -> Host {
    Host {
        on_link: on_link.map(obj),
        images: images.map(obj),
    }
}

/// An HTML document as an element.
#[pyfunction]
#[pyo3(signature = (source, on_link=None, images=None, font_size=None, color=None, link=None, tint=None))]
pub(crate) fn html(
    source: &str,
    on_link: Option<&Bound<'_, PyAny>>,
    images: Option<&Bound<'_, PyAny>>,
    font_size: Option<f32>,
    color: Option<&Bound<'_, PyAny>>,
    link: Option<&Bound<'_, PyAny>>,
    tint: Option<&Bound<'_, PyAny>>,
) -> PyResult<PyElement> {
    let defaults = defaults(font_size, color, link, tint)?;
    Ok(PyElement::new(katui_content::html(
        source,
        &defaults,
        &host(on_link, images),
    )))
}

/// A Markdown document as an element.
#[pyfunction]
#[pyo3(signature = (source, on_link=None, images=None, font_size=None, color=None, link=None, tint=None))]
pub(crate) fn markdown(
    source: &str,
    on_link: Option<&Bound<'_, PyAny>>,
    images: Option<&Bound<'_, PyAny>>,
    font_size: Option<f32>,
    color: Option<&Bound<'_, PyAny>>,
    link: Option<&Bound<'_, PyAny>>,
    tint: Option<&Bound<'_, PyAny>>,
) -> PyResult<PyElement> {
    let defaults = defaults(font_size, color, link, tint)?;
    Ok(PyElement::new(katui_content::markdown_view(
        source,
        &defaults,
        &host(on_link, images),
    )))
}
