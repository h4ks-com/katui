//! HTML and Markdown as elements, with links that send messages.

use std::sync::Arc;

use katui_bindings::html::{Format, Look, document, link};
use katui_content::HtmlHost;
use katui_scene::surface::Surface;
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
        link(self.on_link.as_ref(), href)
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
fn look(
    font_size: Option<f32>,
    text: Option<&Bound<'_, PyAny>>,
    link: Option<&Bound<'_, PyAny>>,
    tint: Option<&Bound<'_, PyAny>>,
) -> PyResult<Look> {
    Ok(Look {
        font_size,
        color: text.map(color).transpose()?,
        link: link.map(color).transpose()?,
        tint: tint.map(color).transpose()?,
    })
}

fn element(
    format: Format,
    source: &str,
    on_link: Option<&Bound<'_, PyAny>>,
    images: Option<&Bound<'_, PyAny>>,
    look: Look,
) -> PyElement {
    let host = Host {
        on_link: on_link.map(obj),
        images: images.map(obj),
    };
    PyElement::new(document(format, source, look, &host))
}

/// Defines the function `name`, which reads its source as `format`.
macro_rules! document_function {
    ($(#[$doc:meta])* $name:ident, $format:expr) => {
        $(#[$doc])*
        #[pyfunction]
        #[pyo3(signature = (source, on_link=None, images=None, font_size=None, color=None, link=None, tint=None))]
        pub(crate) fn $name(
            source: &str,
            on_link: Option<&Bound<'_, PyAny>>,
            images: Option<&Bound<'_, PyAny>>,
            font_size: Option<f32>,
            color: Option<&Bound<'_, PyAny>>,
            link: Option<&Bound<'_, PyAny>>,
            tint: Option<&Bound<'_, PyAny>>,
        ) -> PyResult<PyElement> {
            let look = look(font_size, color, link, tint)?;
            Ok(element($format, source, on_link, images, look))
        }
    };
}

document_function!(
    /// An HTML document as an element.
    html,
    Format::Html
);

document_function!(
    /// A Markdown document as an element.
    markdown,
    Format::Markdown
);
