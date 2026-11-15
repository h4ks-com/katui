//! `Element`, the node of a view, plus the images and text spans it shows.

use std::path::PathBuf;
use std::sync::Arc;

use katui_draw::TextSpan;
use katui_scene::surface::Surface;
use katui_ui::{Element, Scroll};
use pyo3::exceptions::{PyOSError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyList, PyTuple};

use crate::convert::{align, color, length, paint};
use crate::message::{Msg, obj};

pub(crate) type Node = Element<Msg>;

/// One node of a view. Its methods change it and return it, so calls chain.
#[pyclass(name = "Element", module = "katui", unsendable)]
pub(crate) struct PyElement {
    pub(crate) inner: Node,
}

impl PyElement {
    pub(crate) fn new(inner: Node) -> Self {
        Self { inner }
    }

    fn map(mut slf: PyRefMut<'_, Self>, change: impl FnOnce(Node) -> Node) -> PyRefMut<'_, Self> {
        let inner = std::mem::replace(&mut slf.inner, Element::spacer());
        slf.inner = change(inner);
        slf
    }
}

/// The elements in `args`, with lists and tuples opened one level and
/// `None` left out, so a view can say `column(header, rows, footer if x else None)`.
pub(crate) fn children(args: &Bound<'_, PyTuple>) -> PyResult<Vec<Node>> {
    let mut nodes = Vec::new();
    for arg in args.iter() {
        push_child(&arg, &mut nodes)?;
    }
    Ok(nodes)
}

pub(crate) fn children_of(items: &Bound<'_, PyAny>) -> PyResult<Vec<Node>> {
    let mut nodes = Vec::new();
    for item in items.try_iter()? {
        push_child(&item?, &mut nodes)?;
    }
    Ok(nodes)
}

fn push_child(arg: &Bound<'_, PyAny>, nodes: &mut Vec<Node>) -> PyResult<()> {
    if arg.is_none() {
        return Ok(());
    }
    if arg.is_instance_of::<PyList>() || arg.is_instance_of::<PyTuple>() {
        for item in arg.try_iter()? {
            push_child(&item?, nodes)?;
        }
        return Ok(());
    }
    nodes.push(node(arg)?);
    Ok(())
}

pub(crate) fn node(value: &Bound<'_, PyAny>) -> PyResult<Node> {
    let element = value.cast::<PyElement>()?;
    Ok(element.borrow().inner.clone())
}

#[pymethods]
impl PyElement {
    #[staticmethod]
    #[pyo3(signature = (*children))]
    fn row(children: &Bound<'_, PyTuple>) -> PyResult<Self> {
        Ok(Self::new(Element::row(self::children(children)?)))
    }

    #[staticmethod]
    #[pyo3(signature = (*children))]
    fn column(children: &Bound<'_, PyTuple>) -> PyResult<Self> {
        Ok(Self::new(Element::column(self::children(children)?)))
    }

    #[staticmethod]
    #[pyo3(signature = (*children))]
    fn stack(children: &Bound<'_, PyTuple>) -> PyResult<Self> {
        Ok(Self::new(Element::stack(self::children(children)?)))
    }

    #[staticmethod]
    fn text(text: &str) -> Self {
        Self::new(Element::text(text))
    }

    #[staticmethod]
    fn rich(spans: Vec<PyRef<'_, PySpan>>) -> Self {
        Self::new(Element::rich(
            spans.iter().map(|span| span.inner.clone()).collect(),
        ))
    }

    #[staticmethod]
    fn image(image: PyRef<'_, PyImage>) -> Self {
        Self::new(Element::image(Arc::clone(&image.surface)))
    }

    #[staticmethod]
    fn spacer() -> Self {
        Self::new(Element::spacer())
    }

    fn layer(slf: PyRefMut<'_, Self>, key: u64) -> PyRefMut<'_, Self> {
        Self::map(slf, |e| e.layer(key))
    }

    fn scroll(slf: PyRefMut<'_, Self>, offset: f32) -> PyRefMut<'_, Self> {
        Self::map(slf, |e| e.scroll(offset))
    }

    fn on_click<'py>(slf: PyRefMut<'py, Self>, message: &Bound<'py, PyAny>) -> PyRefMut<'py, Self> {
        let message = Msg::Value(obj(message));
        Self::map(slf, |e| e.on_click(message))
    }

    fn on_span_click<'py>(
        slf: PyRefMut<'py, Self>,
        span: usize,
        message: &Bound<'py, PyAny>,
    ) -> PyRefMut<'py, Self> {
        let message = Msg::Value(obj(message));
        Self::map(slf, |e| e.on_span_click(span, message))
    }

    fn on_hover<'py>(slf: PyRefMut<'py, Self>, message: &Bound<'py, PyAny>) -> PyRefMut<'py, Self> {
        let message = Msg::Value(obj(message));
        Self::map(slf, |e| e.on_hover(message))
    }

    fn on_leave<'py>(slf: PyRefMut<'py, Self>, message: &Bound<'py, PyAny>) -> PyRefMut<'py, Self> {
        let message = Msg::Value(obj(message));
        Self::map(slf, |e| e.on_leave(message))
    }

    /// `message(scroll)` makes the message for each new position.
    fn on_scroll<'py>(
        slf: PyRefMut<'py, Self>,
        message: &Bound<'py, PyAny>,
    ) -> PyRefMut<'py, Self> {
        let make = obj(message);
        Self::map(slf, |e| {
            e.on_scroll(move |scroll: Scroll| Msg::Scroll(Arc::clone(&make), scroll))
        })
    }

    /// `message(x, y)` makes the message for the press and each move until
    /// the release, `x` and `y` as fractions of the element's box.
    fn on_press<'py>(slf: PyRefMut<'py, Self>, message: &Bound<'py, PyAny>) -> PyRefMut<'py, Self> {
        let make = obj(message);
        Self::map(slf, |e| {
            e.on_press(move |x, y| Msg::Point(Arc::clone(&make), x, y))
        })
    }

    fn on_grip_press<'py>(
        slf: PyRefMut<'py, Self>,
        message: &Bound<'py, PyAny>,
    ) -> PyRefMut<'py, Self> {
        let make = obj(message);
        Self::map(slf, |e| {
            e.on_grip_press(move |x, y| Msg::Point(Arc::clone(&make), x, y))
        })
    }

    fn grip(slf: PyRefMut<'_, Self>) -> PyRefMut<'_, Self> {
        Self::map(slf, Element::grip)
    }

    fn popup(slf: PyRefMut<'_, Self>, x: f32, y: f32) -> PyRefMut<'_, Self> {
        Self::map(slf, |e| e.popup(x, y))
    }

    fn opaque(slf: PyRefMut<'_, Self>) -> PyRefMut<'_, Self> {
        Self::map(slf, Element::opaque)
    }

    fn scrollbar<'py>(
        slf: PyRefMut<'py, Self>,
        color: &Bound<'py, PyAny>,
    ) -> PyResult<PyRefMut<'py, Self>> {
        let color = self::color(color)?;
        Ok(Self::map(slf, |e| e.scrollbar(color)))
    }

    fn id(slf: PyRefMut<'_, Self>, id: u64) -> PyRefMut<'_, Self> {
        Self::map(slf, |e| e.id(id))
    }

    fn padding(slf: PyRefMut<'_, Self>, all: f32) -> PyRefMut<'_, Self> {
        Self::map(slf, |e| e.padding(all))
    }

    fn padding_xy(slf: PyRefMut<'_, Self>, x: f32, y: f32) -> PyRefMut<'_, Self> {
        Self::map(slf, |e| e.padding_xy(x, y))
    }

    fn gap(slf: PyRefMut<'_, Self>, gap: f32) -> PyRefMut<'_, Self> {
        Self::map(slf, |e| e.gap(gap))
    }

    fn width<'py>(
        slf: PyRefMut<'py, Self>,
        width: &Bound<'py, PyAny>,
    ) -> PyResult<PyRefMut<'py, Self>> {
        let width = length(width)?;
        Ok(Self::map(slf, |e| e.width(width)))
    }

    fn height<'py>(
        slf: PyRefMut<'py, Self>,
        height: &Bound<'py, PyAny>,
    ) -> PyResult<PyRefMut<'py, Self>> {
        let height = length(height)?;
        Ok(Self::map(slf, |e| e.height(height)))
    }

    fn size<'py>(
        slf: PyRefMut<'py, Self>,
        width: &Bound<'py, PyAny>,
        height: &Bound<'py, PyAny>,
    ) -> PyResult<PyRefMut<'py, Self>> {
        let (width, height) = (length(width)?, length(height)?);
        Ok(Self::map(slf, |e| e.size(width, height)))
    }

    fn grow(slf: PyRefMut<'_, Self>, grow: f32) -> PyRefMut<'_, Self> {
        Self::map(slf, |e| e.grow(grow))
    }

    fn align<'py>(slf: PyRefMut<'py, Self>, to: &str) -> PyResult<PyRefMut<'py, Self>> {
        let to = align(to)?;
        Ok(Self::map(slf, |e| e.align(to)))
    }

    fn justify<'py>(slf: PyRefMut<'py, Self>, to: &str) -> PyResult<PyRefMut<'py, Self>> {
        let to = align(to)?;
        Ok(Self::map(slf, |e| e.justify(to)))
    }

    fn background<'py>(
        slf: PyRefMut<'py, Self>,
        color: &Bound<'py, PyAny>,
    ) -> PyResult<PyRefMut<'py, Self>> {
        let paint = paint(color)?;
        Ok(Self::map(slf, |e| e.background(paint)))
    }

    fn radius(slf: PyRefMut<'_, Self>, radius: f32) -> PyRefMut<'_, Self> {
        Self::map(slf, |e| e.radius(radius))
    }

    fn border<'py>(
        slf: PyRefMut<'py, Self>,
        width: f32,
        color: &Bound<'py, PyAny>,
    ) -> PyResult<PyRefMut<'py, Self>> {
        let paint = paint(color)?;
        Ok(Self::map(slf, |e| e.border(width, paint)))
    }

    fn color<'py>(
        slf: PyRefMut<'py, Self>,
        color: &Bound<'py, PyAny>,
    ) -> PyResult<PyRefMut<'py, Self>> {
        let color = self::color(color)?;
        Ok(Self::map(slf, |e| e.color(color)))
    }

    fn font_size(slf: PyRefMut<'_, Self>, size: f32) -> PyRefMut<'_, Self> {
        Self::map(slf, |e| e.font_size(size))
    }

    fn weight(slf: PyRefMut<'_, Self>, weight: u16) -> PyRefMut<'_, Self> {
        Self::map(slf, |e| e.weight(weight))
    }
}

/// Pixels to show: decoded once and shared, so an unchanged image costs
/// nothing to draw again.
#[pyclass(name = "Image", module = "katui", frozen)]
pub(crate) struct PyImage {
    pub(crate) surface: Arc<Surface>,
}

impl PyImage {
    pub(crate) fn new(surface: Surface) -> Self {
        Self {
            surface: Arc::new(surface),
        }
    }
}

#[pymethods]
impl PyImage {
    /// An image from straight-alpha RGBA bytes, four per pixel.
    #[staticmethod]
    fn from_rgba(width: u32, height: u32, pixels: Vec<u8>) -> PyResult<Self> {
        Surface::from_rgba(width, height, pixels)
            .map(Self::new)
            .ok_or_else(|| {
                PyValueError::new_err(format!("give exactly {width} x {height} x 4 bytes of RGBA"))
            })
    }

    /// An image decoded from PNG, JPEG, GIF or WebP bytes.
    #[staticmethod]
    fn decode(data: &[u8]) -> PyResult<Self> {
        let decoded = image::load_from_memory(data)
            .map_err(|error| PyValueError::new_err(format!("decode the image: {error}")))?
            .into_rgba8();
        let (width, height) = decoded.dimensions();
        Self::from_rgba(width, height, decoded.into_raw())
    }

    /// An image read from a PNG, JPEG, GIF or WebP file.
    #[staticmethod]
    fn open(path: PathBuf) -> PyResult<Self> {
        let data = std::fs::read(&path)
            .map_err(|error| PyOSError::new_err(format!("read {}: {error}", path.display())))?;
        Self::decode(&data)
    }

    #[getter]
    fn width(&self) -> u32 {
        self.surface.width()
    }

    #[getter]
    fn height(&self) -> u32 {
        self.surface.height()
    }

    /// The RGBA bytes, four per pixel, row by row.
    fn rgba(&self) -> Vec<u8> {
        self.surface.pixels().to_vec()
    }

    /// The color at `x`, `y` as `(r, g, b, a)`.
    fn pixel(&self, x: u32, y: u32) -> Option<(u8, u8, u8, u8)> {
        let at = (y as usize * self.surface.width() as usize + x as usize) * 4;
        let pixel = self.surface.pixels().get(at..at + 4)?;
        match pixel {
            [red, green, blue, alpha] if x < self.surface.width() => {
                Some((*red, *green, *blue, *alpha))
            }
            _ => None,
        }
    }

    /// Write the image to `path` as a PNG.
    fn save(&self, path: PathBuf) -> PyResult<()> {
        image::save_buffer(
            &path,
            self.surface.pixels(),
            self.surface.width(),
            self.surface.height(),
            image::ExtendedColorType::Rgba8,
        )
        .map_err(|error| PyOSError::new_err(format!("write {}: {error}", path.display())))
    }
}

/// A run of text with its own look, for [`PyElement::rich`].
#[pyclass(name = "Span", module = "katui", frozen)]
pub(crate) struct PySpan {
    inner: TextSpan,
}

#[pymethods]
impl PySpan {
    #[new]
    #[pyo3(signature = (text, color=None, background=None, bold=false, italic=false, underline=false, strike=false))]
    #[allow(clippy::fn_params_excessive_bools)]
    fn new(
        text: String,
        color: Option<&Bound<'_, PyAny>>,
        background: Option<&Bound<'_, PyAny>>,
        bold: bool,
        italic: bool,
        underline: bool,
        strike: bool,
    ) -> PyResult<Self> {
        Ok(Self {
            inner: TextSpan {
                text,
                color: color.map(self::color).transpose()?,
                background: background.map(self::color).transpose()?,
                bold,
                italic,
                underline,
                strike,
            },
        })
    }
}
