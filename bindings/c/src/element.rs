//! `KatuiElement`, the node of a view, plus the images, colors, lengths and
//! text spans it uses.

use std::ffi::c_char;
use std::path::Path;
use std::sync::Arc;

use katui_bindings::message::{point, scroll};
use katui_bindings::picture;
use katui_draw::{Paint, Rgba, TextSpan};
use katui_scene::surface::Surface;
use katui_ui::{Align, Element, Length};

use crate::ffi::{Error, KatuiStatus, owned, owned_or_null, raise, slice, status, take, text};
use crate::message::Msg;

pub(crate) type Node = Element<Msg>;

/// One node of a view. Its methods change it and return it, so calls nest.
pub struct KatuiElement(pub(crate) Node);

/// A straight-alpha color.
#[repr(C)]
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub struct KatuiColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl From<KatuiColor> for Rgba {
    fn from(color: KatuiColor) -> Self {
        Self {
            r: color.r,
            g: color.g,
            b: color.b,
            a: color.a,
        }
    }
}

impl From<Rgba> for KatuiColor {
    fn from(color: Rgba) -> Self {
        Self {
            r: color.r,
            g: color.g,
            b: color.b,
            a: color.a,
        }
    }
}

/// The color of a solid paint; a gradient reads as transparent.
pub(crate) fn paint_color(paint: &Paint) -> KatuiColor {
    match paint {
        Paint::Solid(color) => (*color).into(),
        Paint::Linear { .. } => KatuiColor::default(),
    }
}

/// `color`, or `None` when it is fully transparent.
pub(crate) fn visible(color: KatuiColor) -> Option<Rgba> {
    (color.a > 0).then(|| color.into())
}

/// How a length is measured.
pub type KatuiLengthKind = u32;
/// `pixels` pixels.
pub const KATUI_LENGTH_PX: KatuiLengthKind = 0;
/// All the space the parent leaves.
pub const KATUI_LENGTH_FILL: KatuiLengthKind = 1;
/// As much as the content needs.
pub const KATUI_LENGTH_CONTENT: KatuiLengthKind = 2;

/// A width or a height.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct KatuiLength {
    pub kind: KatuiLengthKind,
    pub pixels: f32,
}

impl From<KatuiLength> for Length {
    fn from(length: KatuiLength) -> Self {
        match length.kind {
            KATUI_LENGTH_FILL => Self::Fill,
            KATUI_LENGTH_CONTENT => Self::Content,
            _ => Self::Px(length.pixels),
        }
    }
}

/// A length of `pixels` pixels.
#[unsafe(no_mangle)]
pub extern "C" fn katui_px(pixels: f32) -> KatuiLength {
    KatuiLength {
        kind: KATUI_LENGTH_PX,
        pixels,
    }
}

/// A length that fills the space the parent leaves.
#[unsafe(no_mangle)]
pub extern "C" fn katui_fill() -> KatuiLength {
    KatuiLength {
        kind: KATUI_LENGTH_FILL,
        pixels: 0.0,
    }
}

/// A length that fits the content.
#[unsafe(no_mangle)]
pub extern "C" fn katui_content() -> KatuiLength {
    KatuiLength {
        kind: KATUI_LENGTH_CONTENT,
        pixels: 0.0,
    }
}

/// Where children sit along an axis.
pub type KatuiAlign = u32;
pub const KATUI_ALIGN_START: KatuiAlign = 0;
pub const KATUI_ALIGN_CENTER: KatuiAlign = 1;
pub const KATUI_ALIGN_END: KatuiAlign = 2;

fn align(to: KatuiAlign) -> Align {
    match to {
        KATUI_ALIGN_CENTER => Align::Center,
        KATUI_ALIGN_END => Align::End,
        _ => Align::Start,
    }
}

/// A run of text with its own look, for `katui_rich`. A color with alpha 0
/// leaves the text's color or background as it is.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct KatuiSpan {
    pub text: *const c_char,
    pub color: KatuiColor,
    pub background: KatuiColor,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
}

pub(crate) fn element(node: Node) -> *mut KatuiElement {
    owned(KatuiElement(node))
}

/// The elements of `children`, each one taken over, NULL ones left out.
///
/// # Safety
/// `children` is NULL or points at `count` elements, each NULL or from
/// katui and not used again.
pub(crate) unsafe fn children(children: *const *mut KatuiElement, count: usize) -> Vec<Node> {
    // SAFETY: our caller hands us the `count` elements at `children` to own.
    let taken = unsafe { slice(children, count) }
        .iter()
        .filter_map(|&child| unsafe { take(child) });
    taken.map(|child| child.0).collect()
}

fn change(
    mut element: Option<&mut KatuiElement>,
    edit: impl FnOnce(Node) -> Node,
) -> Option<&mut KatuiElement> {
    if let Some(element) = element.as_deref_mut() {
        let node = std::mem::replace(&mut element.0, Element::spacer());
        element.0 = edit(node);
    }
    element
}

/// Free an element katui did not take over. NULL does nothing.
#[unsafe(no_mangle)]
pub extern "C" fn katui_element_free(element: Option<Box<KatuiElement>>) {
    drop(element);
}

/// Children side by side. Takes over every child; NULL ones are left out.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_row(
    children: *const *mut KatuiElement,
    count: usize,
) -> *mut KatuiElement {
    // SAFETY: the header hands us the `count` elements at `children` to own.
    element(Element::row(unsafe { self::children(children, count) }))
}

/// Children top to bottom. Takes over every child; NULL ones are left out.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_column(
    children: *const *mut KatuiElement,
    count: usize,
) -> *mut KatuiElement {
    // SAFETY: the header hands us the `count` elements at `children` to own.
    element(Element::column(unsafe { self::children(children, count) }))
}

/// Children on top of each other. Takes over every child; NULL ones are
/// left out.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_stack(
    children: *const *mut KatuiElement,
    count: usize,
) -> *mut KatuiElement {
    // SAFETY: the header hands us the `count` elements at `children` to own.
    element(Element::stack(unsafe { self::children(children, count) }))
}

/// A line of text in the current theme's look.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_text(text: *const c_char) -> *mut KatuiElement {
    // SAFETY: the header requires `text` to be NULL or NUL-terminated.
    element(Element::text(unsafe { self::text(text) }.as_ref()))
}

/// Text made of `count` spans, each with its own look.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_rich(spans: *const KatuiSpan, count: usize) -> *mut KatuiElement {
    // SAFETY: the header requires `spans` to hold `count` values.
    let spans = unsafe { slice(spans, count) }
        .iter()
        .map(|span| TextSpan {
            // SAFETY: the header requires `span.text` to be NULL or NUL-terminated.
            text: unsafe { text(span.text) }.into_owned(),
            color: visible(span.color),
            background: visible(span.background),
            bold: span.bold,
            italic: span.italic,
            underline: span.underline,
            strike: span.strike,
            mono: false,
        })
        .collect();
    element(Element::rich(spans))
}

/// An element that shows `image`. The image stays the caller's; the
/// element keeps its own reference to the pixels.
#[unsafe(no_mangle)]
pub extern "C" fn katui_image(image: Option<&KatuiImage>) -> *mut KatuiElement {
    match image {
        Some(image) => element(Element::image(Arc::clone(&image.0))),
        None => element(Element::spacer()),
    }
}

/// Empty space that grows to fill what the others leave.
#[unsafe(no_mangle)]
pub extern "C" fn katui_spacer() -> *mut KatuiElement {
    element(Element::spacer())
}

/// Draw this subtree into its own layer, kept under `key` between frames.
#[unsafe(no_mangle)]
pub extern "C" fn katui_element_layer(
    element: Option<&mut KatuiElement>,
    key: u64,
) -> Option<&mut KatuiElement> {
    change(element, |e| e.layer(key))
}

/// Scroll the children by `offset` pixels.
#[unsafe(no_mangle)]
pub extern "C" fn katui_element_scroll(
    element: Option<&mut KatuiElement>,
    offset: f32,
) -> Option<&mut KatuiElement> {
    change(element, |e| e.scroll(offset))
}

/// A click sends `key`.
#[unsafe(no_mangle)]
pub extern "C" fn katui_element_on_click(
    element: Option<&mut KatuiElement>,
    key: u64,
) -> Option<&mut KatuiElement> {
    change(element, |e| e.on_click(Msg::Value(key)))
}

/// A click on the span at `span` of a rich text sends `key`.
#[unsafe(no_mangle)]
pub extern "C" fn katui_element_on_span_click(
    element: Option<&mut KatuiElement>,
    span: usize,
    key: u64,
) -> Option<&mut KatuiElement> {
    change(element, |e| e.on_span_click(span, Msg::Value(key)))
}

/// The pointer entering sends `key`.
#[unsafe(no_mangle)]
pub extern "C" fn katui_element_on_hover(
    element: Option<&mut KatuiElement>,
    key: u64,
) -> Option<&mut KatuiElement> {
    change(element, |e| e.on_hover(Msg::Value(key)))
}

/// The pointer leaving sends `key`.
#[unsafe(no_mangle)]
pub extern "C" fn katui_element_on_leave(
    element: Option<&mut KatuiElement>,
    key: u64,
) -> Option<&mut KatuiElement> {
    change(element, |e| e.on_leave(Msg::Value(key)))
}

/// Each new scroll position sends `key` with a scroll payload.
#[unsafe(no_mangle)]
pub extern "C" fn katui_element_on_scroll(
    element: Option<&mut KatuiElement>,
    key: u64,
) -> Option<&mut KatuiElement> {
    change(element, |e| e.on_scroll(scroll(key)))
}

/// The press and each move until the release send `key` with a point
/// payload, as fractions of the element's box.
#[unsafe(no_mangle)]
pub extern "C" fn katui_element_on_press(
    element: Option<&mut KatuiElement>,
    key: u64,
) -> Option<&mut KatuiElement> {
    change(element, |e| e.on_press(point(key)))
}

/// Like `on_press`, for presses on a child marked with `grip`.
#[unsafe(no_mangle)]
pub extern "C" fn katui_element_on_grip_press(
    element: Option<&mut KatuiElement>,
    key: u64,
) -> Option<&mut KatuiElement> {
    change(element, |e| e.on_grip_press(point(key)))
}

/// Mark this element as the handle of its parent's `on_grip_press`.
#[unsafe(no_mangle)]
pub extern "C" fn katui_element_grip(
    element: Option<&mut KatuiElement>,
) -> Option<&mut KatuiElement> {
    change(element, Element::grip)
}

/// Draw above everything, at `x`, `y` pixels from its parent's corner.
#[unsafe(no_mangle)]
pub extern "C" fn katui_element_popup(
    element: Option<&mut KatuiElement>,
    x: f32,
    y: f32,
) -> Option<&mut KatuiElement> {
    change(element, |e| e.popup(x, y))
}

/// Catch clicks over its whole box.
#[unsafe(no_mangle)]
pub extern "C" fn katui_element_opaque(
    element: Option<&mut KatuiElement>,
) -> Option<&mut KatuiElement> {
    change(element, Element::opaque)
}

/// Show a scrollbar in `color`.
#[unsafe(no_mangle)]
pub extern "C" fn katui_element_scrollbar(
    element: Option<&mut KatuiElement>,
    color: KatuiColor,
) -> Option<&mut KatuiElement> {
    change(element, |e| e.scrollbar(color.into()))
}

/// Name the element, so it keeps its state when the tree changes around it.
#[unsafe(no_mangle)]
pub extern "C" fn katui_element_id(
    element: Option<&mut KatuiElement>,
    id: u64,
) -> Option<&mut KatuiElement> {
    change(element, |e| e.id(id))
}

/// Scroll the scrolling element so its descendant with `id` starts at the
/// top; its `on_scroll` key receives the new position.
#[unsafe(no_mangle)]
pub extern "C" fn katui_element_reveal(
    element: Option<&mut KatuiElement>,
    id: u64,
) -> Option<&mut KatuiElement> {
    change(element, |e| e.reveal(id))
}

#[unsafe(no_mangle)]
pub extern "C" fn katui_element_padding(
    element: Option<&mut KatuiElement>,
    all: f32,
) -> Option<&mut KatuiElement> {
    change(element, |e| e.padding(all))
}

#[unsafe(no_mangle)]
pub extern "C" fn katui_element_padding_xy(
    element: Option<&mut KatuiElement>,
    x: f32,
    y: f32,
) -> Option<&mut KatuiElement> {
    change(element, |e| e.padding_xy(x, y))
}

/// Space between children, in pixels.
#[unsafe(no_mangle)]
pub extern "C" fn katui_element_gap(
    element: Option<&mut KatuiElement>,
    gap: f32,
) -> Option<&mut KatuiElement> {
    change(element, |e| e.gap(gap))
}

#[unsafe(no_mangle)]
pub extern "C" fn katui_element_width(
    element: Option<&mut KatuiElement>,
    width: KatuiLength,
) -> Option<&mut KatuiElement> {
    change(element, |e| e.width(width.into()))
}

#[unsafe(no_mangle)]
pub extern "C" fn katui_element_height(
    element: Option<&mut KatuiElement>,
    height: KatuiLength,
) -> Option<&mut KatuiElement> {
    change(element, |e| e.height(height.into()))
}

#[unsafe(no_mangle)]
pub extern "C" fn katui_element_size(
    element: Option<&mut KatuiElement>,
    width: KatuiLength,
    height: KatuiLength,
) -> Option<&mut KatuiElement> {
    change(element, |e| e.size(width.into(), height.into()))
}

/// How much of the free space this element takes, against its siblings.
#[unsafe(no_mangle)]
pub extern "C" fn katui_element_grow(
    element: Option<&mut KatuiElement>,
    grow: f32,
) -> Option<&mut KatuiElement> {
    change(element, |e| e.grow(grow))
}

/// Where the children sit across the main axis.
#[unsafe(no_mangle)]
pub extern "C" fn katui_element_align(
    element: Option<&mut KatuiElement>,
    to: KatuiAlign,
) -> Option<&mut KatuiElement> {
    change(element, |e| e.align(align(to)))
}

/// Where the children sit along the main axis.
#[unsafe(no_mangle)]
pub extern "C" fn katui_element_justify(
    element: Option<&mut KatuiElement>,
    to: KatuiAlign,
) -> Option<&mut KatuiElement> {
    change(element, |e| e.justify(align(to)))
}

#[unsafe(no_mangle)]
pub extern "C" fn katui_element_background(
    element: Option<&mut KatuiElement>,
    color: KatuiColor,
) -> Option<&mut KatuiElement> {
    change(element, |e| e.background(Paint::Solid(color.into())))
}

/// Round the corners by `radius` pixels.
#[unsafe(no_mangle)]
pub extern "C" fn katui_element_radius(
    element: Option<&mut KatuiElement>,
    radius: f32,
) -> Option<&mut KatuiElement> {
    change(element, |e| e.radius(radius))
}

#[unsafe(no_mangle)]
pub extern "C" fn katui_element_border(
    element: Option<&mut KatuiElement>,
    width: f32,
    color: KatuiColor,
) -> Option<&mut KatuiElement> {
    change(element, |e| e.border(width, Paint::Solid(color.into())))
}

/// The text color.
#[unsafe(no_mangle)]
pub extern "C" fn katui_element_color(
    element: Option<&mut KatuiElement>,
    color: KatuiColor,
) -> Option<&mut KatuiElement> {
    change(element, |e| e.color(color.into()))
}

#[unsafe(no_mangle)]
pub extern "C" fn katui_element_font_size(
    element: Option<&mut KatuiElement>,
    size: f32,
) -> Option<&mut KatuiElement> {
    change(element, |e| e.font_size(size))
}

/// The font weight, from 100 to 900; 400 is regular and 700 bold.
#[unsafe(no_mangle)]
pub extern "C" fn katui_element_weight(
    element: Option<&mut KatuiElement>,
    weight: u16,
) -> Option<&mut KatuiElement> {
    change(element, |e| e.weight(weight))
}

/// Pixels to show, decoded once and shared, so an unchanged image costs
/// nothing to draw again.
pub struct KatuiImage(pub(crate) Arc<Surface>);

pub(crate) fn image(surface: Surface) -> *mut KatuiImage {
    owned(KatuiImage(Arc::new(surface)))
}

fn made(surface: Result<Surface, katui_bindings::Error>) -> *mut KatuiImage {
    owned_or_null(
        surface
            .map(|surface| KatuiImage(Arc::new(surface)))
            .map_err(raise),
    )
}

/// An image from `length` bytes of straight-alpha RGBA, four per pixel,
/// copied. NULL when the length does not match.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_image_from_rgba(
    width: u32,
    height: u32,
    pixels: *const u8,
    length: usize,
) -> *mut KatuiImage {
    // SAFETY: the header requires `pixels` to hold `length` values.
    let pixels = unsafe { slice(pixels, length) }.to_vec();
    made(picture::from_rgba(width, height, pixels))
}

/// An image decoded from `length` bytes of PNG, JPEG, GIF or WebP. NULL
/// when they do not decode.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_image_decode(data: *const u8, length: usize) -> *mut KatuiImage {
    // SAFETY: the header requires `data` to hold `length` values.
    made(picture::decode(unsafe { slice(data, length) }))
}

/// An image read from a PNG, JPEG, GIF or WebP file. NULL when it cannot
/// be read or decoded.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_image_open(path: *const c_char) -> *mut KatuiImage {
    // SAFETY: the header requires `path` to be NULL or NUL-terminated.
    let path = unsafe { text(path) };
    made(picture::open(Path::new(path.as_ref())))
}

/// Free an image. Elements that show it keep their own reference. NULL
/// does nothing.
#[unsafe(no_mangle)]
pub extern "C" fn katui_image_free(image: Option<Box<KatuiImage>>) {
    drop(image);
}

#[unsafe(no_mangle)]
pub extern "C" fn katui_image_width(image: Option<&KatuiImage>) -> u32 {
    image.map_or(0, |image| image.0.width())
}

#[unsafe(no_mangle)]
pub extern "C" fn katui_image_height(image: Option<&KatuiImage>) -> u32 {
    image.map_or(0, |image| image.0.height())
}

/// The RGBA bytes, four per pixel, row by row, with their count in
/// `length`. The image owns them; they live as long as it does.
#[unsafe(no_mangle)]
pub extern "C" fn katui_image_rgba(
    image: Option<&KatuiImage>,
    length: Option<&mut usize>,
) -> *const u8 {
    let pixels = image.map_or(&[][..], |image| image.0.pixels());
    if let Some(length) = length {
        *length = pixels.len();
    }
    pixels.as_ptr()
}

/// Write the color at `x`, `y` into `color`. False outside the image.
#[unsafe(no_mangle)]
pub extern "C" fn katui_image_pixel(
    image: Option<&KatuiImage>,
    x: u32,
    y: u32,
    color: Option<&mut KatuiColor>,
) -> bool {
    let found = image.and_then(|image| picture::pixel(&image.0, x, y));
    let Some([red, green, blue, alpha]) = found else {
        return false;
    };
    if let Some(color) = color {
        *color = KatuiColor {
            r: red,
            g: green,
            b: blue,
            a: alpha,
        };
    }
    true
}

/// Write the image to `path` as a PNG.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_image_save(
    image: Option<&KatuiImage>,
    path: *const c_char,
) -> KatuiStatus {
    // SAFETY: the header requires `path` to be NULL or NUL-terminated.
    let path = unsafe { text(path) };
    let saved = image
        .ok_or(Error::Null("image"))
        .and_then(|image| picture::save(&image.0, Path::new(path.as_ref())).map_err(raise));
    status(saved)
}
