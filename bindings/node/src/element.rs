//! `Element`, the node of a view, plus the images and text spans it shows.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use katui_bindings::message::{point, scroll};
use katui_bindings::picture;
use katui_draw::TextSpan;
use katui_scene::surface::Surface;
use katui_ui::Element as UiElement;
use napi::bindgen_prelude::{BigInt, Buffer, Either, JsObjectValue, This, Uint8Array, Unknown};
use napi::{Env, JsValue, Result, ValueType};

use crate::convert::{
    ColorArg, LengthArg, align, color, instance, invalid, is_nothing, length, paint, raise,
};
use crate::message::{Msg, hold, value};
use crate::reclaim::{Tree, watch};

pub(crate) type Node = UiElement<Msg>;

/// One node of a view. Its methods change it and return it, so calls chain.
#[napi]
pub struct Element {
    tree: Tree,
    watched: bool,
}

impl Element {
    pub(crate) fn new(inner: Node) -> Self {
        Self {
            tree: Rc::new(RefCell::new(Some(Box::new(inner)))),
            watched: false,
        }
    }

    fn map(&mut self, change: impl FnOnce(Node) -> Node) {
        if let Some(inner) = self.tree.borrow_mut().as_mut() {
            let old = std::mem::replace(inner.as_mut(), UiElement::spacer());
            **inner = change(old);
        }
    }

    fn inner(&self) -> Node {
        self.tree
            .borrow()
            .as_deref()
            .map_or_else(UiElement::spacer, Clone::clone)
    }
}

/// The elements in `items`, with arrays opened and `null`, `undefined` and
/// `false` left out, so a view can say `column(header, rows, done && footer)`.
pub(crate) fn children(env: &Env, items: Vec<Unknown<'_>>) -> Result<Vec<Node>> {
    let mut nodes = Vec::new();
    for item in items {
        push_child(env, item, &mut nodes)?;
    }
    Ok(nodes)
}

fn push_child(env: &Env, item: Unknown<'_>, nodes: &mut Vec<Node>) -> Result<()> {
    if is_nothing(&item)? || (item.get_type()? == ValueType::Boolean && !item.coerce_to_bool()?) {
        return Ok(());
    }
    if item.is_array()? {
        let items = item.coerce_to_object()?;
        for at in 0..items.get_array_length()? {
            push_child(env, items.get_element(at)?, nodes)?;
        }
        return Ok(());
    }
    nodes.push(node(env, item)?);
    Ok(())
}

/// The tree of the element `value`, which is watched from now on so its
/// tree is freed once JavaScript drops it.
pub(crate) fn node(env: &Env, value: Unknown<'_>) -> Result<Node> {
    let mut element = instance::<Element>(env, value, "give an Element")?;
    if !element.watched {
        element.watched = true;
        watch(env, &value, Rc::clone(&element.tree))?;
    }
    Ok(element.inner())
}

#[napi]
impl Element {
    #[napi(factory, ts_args_type = "children: Child[]")]
    pub fn row(env: Env, children: Vec<Unknown<'_>>) -> Result<Self> {
        Ok(Self::new(UiElement::row(self::children(&env, children)?)))
    }

    #[napi(factory, ts_args_type = "children: Child[]")]
    pub fn column(env: Env, children: Vec<Unknown<'_>>) -> Result<Self> {
        Ok(Self::new(UiElement::column(self::children(
            &env, children,
        )?)))
    }

    #[napi(factory, ts_args_type = "children: Child[]")]
    pub fn stack(env: Env, children: Vec<Unknown<'_>>) -> Result<Self> {
        Ok(Self::new(UiElement::stack(self::children(&env, children)?)))
    }

    #[napi(factory)]
    pub fn text(text: String) -> Self {
        Self::new(UiElement::text(text))
    }

    #[napi(factory)]
    pub fn rich(spans: Vec<&Span>) -> Self {
        Self::new(UiElement::rich(
            spans.iter().map(|span| span.inner.clone()).collect(),
        ))
    }

    #[napi(factory)]
    pub fn image(image: &Image) -> Self {
        Self::new(UiElement::image(Arc::clone(&image.surface)))
    }

    #[napi(factory)]
    pub fn spacer() -> Self {
        Self::new(UiElement::spacer())
    }

    #[napi]
    pub fn layer<'a>(&mut self, this: This<'a>, key: u32) -> This<'a> {
        self.map(|e| e.layer(u64::from(key)));
        this
    }

    #[napi]
    pub fn scroll<'a>(&mut self, this: This<'a>, offset: f64) -> This<'a> {
        self.map(|e| e.scroll(offset as f32));
        this
    }

    #[napi]
    pub fn on_click<'a>(
        &mut self,
        env: Env,
        this: This<'a>,
        message: Unknown<'_>,
    ) -> Result<This<'a>> {
        let message = value(&env, message)?;
        self.map(|e| e.on_click(message));
        Ok(this)
    }

    #[napi]
    pub fn on_span_click<'a>(
        &mut self,
        env: Env,
        this: This<'a>,
        span: u32,
        message: Unknown<'_>,
    ) -> Result<This<'a>> {
        let message = value(&env, message)?;
        self.map(|e| e.on_span_click(span as usize, message));
        Ok(this)
    }

    #[napi]
    pub fn on_hover<'a>(
        &mut self,
        env: Env,
        this: This<'a>,
        message: Unknown<'_>,
    ) -> Result<This<'a>> {
        let message = value(&env, message)?;
        self.map(|e| e.on_hover(message));
        Ok(this)
    }

    #[napi]
    pub fn on_leave<'a>(
        &mut self,
        env: Env,
        this: This<'a>,
        message: Unknown<'_>,
    ) -> Result<This<'a>> {
        let message = value(&env, message)?;
        self.map(|e| e.on_leave(message));
        Ok(this)
    }

    /// `message(scroll)` makes the message for each new position.
    #[napi(ts_args_type = "message: (scroll: Scroll) => unknown")]
    pub fn on_scroll<'a>(
        &mut self,
        env: Env,
        this: This<'a>,
        message: Unknown<'_>,
    ) -> Result<This<'a>> {
        let make = hold(&env, message)?;
        self.map(|e| e.on_scroll(scroll(make)));
        Ok(this)
    }

    /// `message(x, y)` makes the message for the press and each move until
    /// the release, `x` and `y` as fractions of the element's box.
    #[napi(ts_args_type = "message: (x: number, y: number) => unknown")]
    pub fn on_press<'a>(
        &mut self,
        env: Env,
        this: This<'a>,
        message: Unknown<'_>,
    ) -> Result<This<'a>> {
        let make = hold(&env, message)?;
        self.map(|e| e.on_press(point(make)));
        Ok(this)
    }

    #[napi(ts_args_type = "message: (x: number, y: number) => unknown")]
    pub fn on_grip_press<'a>(
        &mut self,
        env: Env,
        this: This<'a>,
        message: Unknown<'_>,
    ) -> Result<This<'a>> {
        let make = hold(&env, message)?;
        self.map(|e| e.on_grip_press(point(make)));
        Ok(this)
    }

    #[napi]
    pub fn grip<'a>(&mut self, this: This<'a>) -> This<'a> {
        self.map(UiElement::grip);
        this
    }

    #[napi]
    pub fn popup<'a>(&mut self, this: This<'a>, x: f64, y: f64) -> This<'a> {
        self.map(|e| e.popup(x as f32, y as f32));
        this
    }

    #[napi]
    pub fn opaque<'a>(&mut self, this: This<'a>) -> This<'a> {
        self.map(UiElement::opaque);
        this
    }

    #[napi(ts_args_type = "color: Color")]
    pub fn scrollbar<'a>(&mut self, this: This<'a>, color: ColorArg) -> Result<This<'a>> {
        let color = self::color(color)?;
        self.map(|e| e.scrollbar(color));
        Ok(this)
    }

    #[napi]
    pub fn id<'a>(&mut self, this: This<'a>, id: u32) -> This<'a> {
        self.map(|e| e.id(u64::from(id)));
        this
    }

    /// Scroll so the descendant with `id` starts at the top; `onScroll`
    /// receives the new position. Takes an `id` number or an `anchorId`.
    #[napi(ts_args_type = "id: number | bigint")]
    pub fn reveal<'a>(&mut self, this: This<'a>, id: Either<u32, BigInt>) -> Result<This<'a>> {
        let id = match id {
            Either::A(id) => u64::from(id),
            Either::B(id) => match id.get_u64() {
                (false, id, true) => id,
                _ => return Err(invalid("reveal takes an id from 0 to 2^64 - 1")),
            },
        };
        self.map(|e| e.reveal(id));
        Ok(this)
    }

    #[napi]
    pub fn padding<'a>(&mut self, this: This<'a>, all: f64) -> This<'a> {
        self.map(|e| e.padding(all as f32));
        this
    }

    #[napi]
    pub fn padding_xy<'a>(&mut self, this: This<'a>, x: f64, y: f64) -> This<'a> {
        self.map(|e| e.padding_xy(x as f32, y as f32));
        this
    }

    #[napi]
    pub fn gap<'a>(&mut self, this: This<'a>, gap: f64) -> This<'a> {
        self.map(|e| e.gap(gap as f32));
        this
    }

    #[napi(ts_args_type = "width: Length")]
    pub fn width<'a>(&mut self, this: This<'a>, width: LengthArg) -> Result<This<'a>> {
        let width = length(width)?;
        self.map(|e| e.width(width));
        Ok(this)
    }

    #[napi(ts_args_type = "height: Length")]
    pub fn height<'a>(&mut self, this: This<'a>, height: LengthArg) -> Result<This<'a>> {
        let height = length(height)?;
        self.map(|e| e.height(height));
        Ok(this)
    }

    #[napi(ts_args_type = "width: Length, height: Length")]
    pub fn size<'a>(
        &mut self,
        this: This<'a>,
        width: LengthArg,
        height: LengthArg,
    ) -> Result<This<'a>> {
        let (width, height) = (length(width)?, length(height)?);
        self.map(|e| e.size(width, height));
        Ok(this)
    }

    #[napi]
    pub fn grow<'a>(&mut self, this: This<'a>, grow: f64) -> This<'a> {
        self.map(|e| e.grow(grow as f32));
        this
    }

    #[napi(ts_args_type = "to: Align")]
    pub fn align<'a>(&mut self, this: This<'a>, to: String) -> Result<This<'a>> {
        let to = align(&to)?;
        self.map(|e| e.align(to));
        Ok(this)
    }

    #[napi(ts_args_type = "to: Align")]
    pub fn justify<'a>(&mut self, this: This<'a>, to: String) -> Result<This<'a>> {
        let to = align(&to)?;
        self.map(|e| e.justify(to));
        Ok(this)
    }

    #[napi(ts_args_type = "color: Color")]
    pub fn background<'a>(&mut self, this: This<'a>, color: ColorArg) -> Result<This<'a>> {
        let paint = paint(color)?;
        self.map(|e| e.background(paint));
        Ok(this)
    }

    #[napi]
    pub fn radius<'a>(&mut self, this: This<'a>, radius: f64) -> This<'a> {
        self.map(|e| e.radius(radius as f32));
        this
    }

    #[napi(ts_args_type = "width: number, color: Color")]
    pub fn border<'a>(&mut self, this: This<'a>, width: f64, color: ColorArg) -> Result<This<'a>> {
        let paint = paint(color)?;
        self.map(|e| e.border(width as f32, paint));
        Ok(this)
    }

    #[napi(ts_args_type = "color: Color")]
    pub fn color<'a>(&mut self, this: This<'a>, color: ColorArg) -> Result<This<'a>> {
        let color = self::color(color)?;
        self.map(|e| e.color(color));
        Ok(this)
    }

    #[napi]
    pub fn font_size<'a>(&mut self, this: This<'a>, size: f64) -> This<'a> {
        self.map(|e| e.font_size(size as f32));
        this
    }

    #[napi]
    pub fn weight<'a>(&mut self, this: This<'a>, weight: u16) -> This<'a> {
        self.map(|e| e.weight(weight));
        this
    }
}

/// Pixels to show: decoded once and shared, so an unchanged image costs
/// nothing to draw again.
#[napi]
pub struct Image {
    pub(crate) surface: Arc<Surface>,
}

impl Image {
    pub(crate) fn new(surface: Surface) -> Self {
        Self {
            surface: Arc::new(surface),
        }
    }

    fn made(surface: std::result::Result<Surface, katui_bindings::Error>) -> Result<Self> {
        surface.map(Self::new).map_err(raise)
    }
}

#[napi]
impl Image {
    /// An image from straight-alpha RGBA bytes, four per pixel.
    #[napi(factory)]
    pub fn from_rgba(width: u32, height: u32, pixels: Uint8Array) -> Result<Self> {
        Self::made(picture::from_rgba(width, height, pixels.to_vec()))
    }

    /// An image decoded from PNG, JPEG, GIF or WebP bytes.
    #[napi(factory)]
    pub fn decode(data: Uint8Array) -> Result<Self> {
        Self::made(picture::decode(&data))
    }

    /// An image read from a PNG, JPEG, GIF or WebP file.
    #[napi(factory)]
    pub fn open(path: String) -> Result<Self> {
        Self::made(picture::open(path.as_ref()))
    }

    #[napi(getter)]
    pub fn width(&self) -> u32 {
        self.surface.width()
    }

    #[napi(getter)]
    pub fn height(&self) -> u32 {
        self.surface.height()
    }

    /// The RGBA bytes, four per pixel, row by row.
    #[napi]
    pub fn rgba(&self) -> Buffer {
        self.surface.pixels().to_vec().into()
    }

    /// The color at `x`, `y` as `[r, g, b, a]`, or `null` outside the image.
    #[napi(ts_return_type = "[number, number, number, number] | null")]
    pub fn pixel(&self, x: u32, y: u32) -> Option<Vec<u8>> {
        picture::pixel(&self.surface, x, y).map(Vec::from)
    }

    /// Write the image to `path` as a PNG.
    #[napi]
    pub fn save(&self, path: String) -> Result<()> {
        picture::save(&self.surface, path.as_ref()).map_err(raise)
    }
}

/// How a span of rich text looks.
#[napi(object)]
#[derive(Default)]
#[allow(clippy::struct_excessive_bools)]
pub struct SpanStyle {
    #[napi(ts_type = "Color")]
    pub color: Option<ColorArg>,
    #[napi(ts_type = "Color")]
    pub background: Option<ColorArg>,
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    pub underline: Option<bool>,
    pub strike: Option<bool>,
}

/// A run of text with its own look, for `Element.rich`.
#[napi]
pub struct Span {
    inner: TextSpan,
}

#[napi]
impl Span {
    #[napi(constructor)]
    pub fn new(text: String, style: Option<SpanStyle>) -> Result<Self> {
        let style = style.unwrap_or_default();
        Ok(Self {
            inner: TextSpan {
                text,
                color: style.color.map(color).transpose()?,
                background: style.background.map(color).transpose()?,
                bold: style.bold.unwrap_or(false),
                italic: style.italic.unwrap_or(false),
                underline: style.underline.unwrap_or(false),
                strike: style.strike.unwrap_or(false),
                mono: false,
            },
        })
    }
}
