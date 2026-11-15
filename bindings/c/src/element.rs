//! `KatuiElement`, the node of a view, plus the images, colors, lengths and
//! text spans it uses.

use std::ffi::c_char;
use std::sync::Arc;

use katui_draw::{Paint, Rgba, TextSpan};
use katui_scene::surface::Surface;
use katui_ui::{Align, Element, Length, Scroll};

use crate::ffi::{
    Error, KatuiStatus, borrow, borrow_mut, free, owned, owned_or_null, slice, status, take, text,
};
use crate::message::{Msg, Payload, plain};

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

/// # Safety
/// `element` is NULL or an element from katui that is not used again.
pub(crate) unsafe fn node(element: *mut KatuiElement) -> Option<Node> {
    unsafe { take(element) }.map(|element| element.0)
}

/// The elements of `children`, each one taken over, NULL ones left out.
///
/// # Safety
/// `children` is NULL or points at `count` elements, each NULL or from
/// katui and not used again.
pub(crate) unsafe fn children(children: *const *mut KatuiElement, count: usize) -> Vec<Node> {
    unsafe { slice(children, count) }
        .iter()
        .filter_map(|&child| unsafe { node(child) })
        .collect()
}

/// # Safety
/// `element` is NULL or a live element.
unsafe fn change(element: *mut KatuiElement, edit: impl FnOnce(Node) -> Node) -> *mut KatuiElement {
    if let Some(element) = unsafe { borrow_mut(element) } {
        let node = std::mem::replace(&mut element.0, Element::spacer());
        element.0 = edit(node);
    }
    element
}

/// Free an element katui did not take over. NULL does nothing.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_element_free(element: *mut KatuiElement) {
    unsafe { free(element) };
}

/// Children side by side. Takes over every child; NULL ones are left out.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_row(
    children: *const *mut KatuiElement,
    count: usize,
) -> *mut KatuiElement {
    element(Element::row(unsafe { self::children(children, count) }))
}

/// Children top to bottom. Takes over every child; NULL ones are left out.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_column(
    children: *const *mut KatuiElement,
    count: usize,
) -> *mut KatuiElement {
    element(Element::column(unsafe { self::children(children, count) }))
}

/// Children on top of each other. Takes over every child; NULL ones are
/// left out.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_stack(
    children: *const *mut KatuiElement,
    count: usize,
) -> *mut KatuiElement {
    element(Element::stack(unsafe { self::children(children, count) }))
}

/// A line of text in the current theme's look.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_text(text: *const c_char) -> *mut KatuiElement {
    element(Element::text(unsafe { self::text(text) }.as_ref()))
}

/// Text made of `count` spans, each with its own look.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_rich(spans: *const KatuiSpan, count: usize) -> *mut KatuiElement {
    let spans = unsafe { slice(spans, count) }
        .iter()
        .map(|span| TextSpan {
            text: unsafe { text(span.text) }.into_owned(),
            color: visible(span.color),
            background: visible(span.background),
            bold: span.bold,
            italic: span.italic,
            underline: span.underline,
            strike: span.strike,
        })
        .collect();
    element(Element::rich(spans))
}

/// An element that shows `image`. The image stays the caller's; the
/// element keeps its own reference to the pixels.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_image(image: *const KatuiImage) -> *mut KatuiElement {
    match unsafe { borrow(image) } {
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
pub unsafe extern "C" fn katui_element_layer(
    element: *mut KatuiElement,
    key: u64,
) -> *mut KatuiElement {
    unsafe { change(element, |e| e.layer(key)) }
}

/// Scroll the children by `offset` pixels.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_element_scroll(
    element: *mut KatuiElement,
    offset: f32,
) -> *mut KatuiElement {
    unsafe { change(element, |e| e.scroll(offset)) }
}

/// A click sends `key`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_element_on_click(
    element: *mut KatuiElement,
    key: u64,
) -> *mut KatuiElement {
    unsafe { change(element, |e| e.on_click(plain(key))) }
}

/// A click on the span at `span` of a rich text sends `key`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_element_on_span_click(
    element: *mut KatuiElement,
    span: usize,
    key: u64,
) -> *mut KatuiElement {
    unsafe { change(element, |e| e.on_span_click(span, plain(key))) }
}

/// The pointer entering sends `key`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_element_on_hover(
    element: *mut KatuiElement,
    key: u64,
) -> *mut KatuiElement {
    unsafe { change(element, |e| e.on_hover(plain(key))) }
}

/// The pointer leaving sends `key`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_element_on_leave(
    element: *mut KatuiElement,
    key: u64,
) -> *mut KatuiElement {
    unsafe { change(element, |e| e.on_leave(plain(key))) }
}

/// Each new scroll position sends `key` with a scroll payload.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_element_on_scroll(
    element: *mut KatuiElement,
    key: u64,
) -> *mut KatuiElement {
    unsafe {
        change(element, |e| {
            e.on_scroll(move |scroll: Scroll| Msg::User(key, Payload::Scroll(scroll)))
        })
    }
}

/// The press and each move until the release send `key` with a point
/// payload, as fractions of the element's box.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_element_on_press(
    element: *mut KatuiElement,
    key: u64,
) -> *mut KatuiElement {
    unsafe {
        change(element, |e| {
            e.on_press(move |x, y| Msg::User(key, Payload::Point(x, y)))
        })
    }
}

/// Like `on_press`, for presses on a child marked with `grip`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_element_on_grip_press(
    element: *mut KatuiElement,
    key: u64,
) -> *mut KatuiElement {
    unsafe {
        change(element, |e| {
            e.on_grip_press(move |x, y| Msg::User(key, Payload::Point(x, y)))
        })
    }
}

/// Mark this element as the handle of its parent's `on_grip_press`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_element_grip(element: *mut KatuiElement) -> *mut KatuiElement {
    unsafe { change(element, Element::grip) }
}

/// Draw above everything, at `x`, `y` pixels from its parent's corner.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_element_popup(
    element: *mut KatuiElement,
    x: f32,
    y: f32,
) -> *mut KatuiElement {
    unsafe { change(element, |e| e.popup(x, y)) }
}

/// Catch clicks over its whole box.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_element_opaque(element: *mut KatuiElement) -> *mut KatuiElement {
    unsafe { change(element, Element::opaque) }
}

/// Show a scrollbar in `color`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_element_scrollbar(
    element: *mut KatuiElement,
    color: KatuiColor,
) -> *mut KatuiElement {
    unsafe { change(element, |e| e.scrollbar(color.into())) }
}

/// Name the element, so it keeps its state when the tree changes around it.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_element_id(
    element: *mut KatuiElement,
    id: u64,
) -> *mut KatuiElement {
    unsafe { change(element, |e| e.id(id)) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_element_padding(
    element: *mut KatuiElement,
    all: f32,
) -> *mut KatuiElement {
    unsafe { change(element, |e| e.padding(all)) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_element_padding_xy(
    element: *mut KatuiElement,
    x: f32,
    y: f32,
) -> *mut KatuiElement {
    unsafe { change(element, |e| e.padding_xy(x, y)) }
}

/// Space between children, in pixels.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_element_gap(
    element: *mut KatuiElement,
    gap: f32,
) -> *mut KatuiElement {
    unsafe { change(element, |e| e.gap(gap)) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_element_width(
    element: *mut KatuiElement,
    width: KatuiLength,
) -> *mut KatuiElement {
    unsafe { change(element, |e| e.width(width.into())) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_element_height(
    element: *mut KatuiElement,
    height: KatuiLength,
) -> *mut KatuiElement {
    unsafe { change(element, |e| e.height(height.into())) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_element_size(
    element: *mut KatuiElement,
    width: KatuiLength,
    height: KatuiLength,
) -> *mut KatuiElement {
    unsafe { change(element, |e| e.size(width.into(), height.into())) }
}

/// How much of the free space this element takes, against its siblings.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_element_grow(
    element: *mut KatuiElement,
    grow: f32,
) -> *mut KatuiElement {
    unsafe { change(element, |e| e.grow(grow)) }
}

/// Where the children sit across the main axis.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_element_align(
    element: *mut KatuiElement,
    to: KatuiAlign,
) -> *mut KatuiElement {
    unsafe { change(element, |e| e.align(align(to))) }
}

/// Where the children sit along the main axis.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_element_justify(
    element: *mut KatuiElement,
    to: KatuiAlign,
) -> *mut KatuiElement {
    unsafe { change(element, |e| e.justify(align(to))) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_element_background(
    element: *mut KatuiElement,
    color: KatuiColor,
) -> *mut KatuiElement {
    unsafe { change(element, |e| e.background(Paint::Solid(color.into()))) }
}

/// Round the corners by `radius` pixels.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_element_radius(
    element: *mut KatuiElement,
    radius: f32,
) -> *mut KatuiElement {
    unsafe { change(element, |e| e.radius(radius)) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_element_border(
    element: *mut KatuiElement,
    width: f32,
    color: KatuiColor,
) -> *mut KatuiElement {
    unsafe { change(element, |e| e.border(width, Paint::Solid(color.into()))) }
}

/// The text color.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_element_color(
    element: *mut KatuiElement,
    color: KatuiColor,
) -> *mut KatuiElement {
    unsafe { change(element, |e| e.color(color.into())) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_element_font_size(
    element: *mut KatuiElement,
    size: f32,
) -> *mut KatuiElement {
    unsafe { change(element, |e| e.font_size(size)) }
}

/// The font weight, from 100 to 900; 400 is regular and 700 bold.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_element_weight(
    element: *mut KatuiElement,
    weight: u16,
) -> *mut KatuiElement {
    unsafe { change(element, |e| e.weight(weight)) }
}

/// Pixels to show, decoded once and shared, so an unchanged image costs
/// nothing to draw again.
pub struct KatuiImage(pub(crate) Arc<Surface>);

pub(crate) fn image(surface: Surface) -> *mut KatuiImage {
    owned(KatuiImage(Arc::new(surface)))
}

fn from_rgba(width: u32, height: u32, pixels: Vec<u8>) -> Result<KatuiImage, Error> {
    Surface::from_rgba(width, height, pixels)
        .map(|surface| KatuiImage(Arc::new(surface)))
        .ok_or_else(|| Error::Invalid(format!("give exactly {width} x {height} x 4 bytes of RGBA")))
}

fn decode(data: &[u8]) -> Result<KatuiImage, Error> {
    let decoded = image::load_from_memory(data)
        .map_err(|error| Error::Invalid(format!("decode the image: {error}")))?
        .into_rgba8();
    let (width, height) = decoded.dimensions();
    from_rgba(width, height, decoded.into_raw())
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
    owned_or_null(from_rgba(
        width,
        height,
        unsafe { slice(pixels, length) }.to_vec(),
    ))
}

/// An image decoded from `length` bytes of PNG, JPEG, GIF or WebP. NULL
/// when they do not decode.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_image_decode(data: *const u8, length: usize) -> *mut KatuiImage {
    owned_or_null(decode(unsafe { slice(data, length) }))
}

/// An image read from a PNG, JPEG, GIF or WebP file. NULL when it cannot
/// be read or decoded.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_image_open(path: *const c_char) -> *mut KatuiImage {
    let path = unsafe { text(path) };
    let read = std::fs::read(path.as_ref())
        .map_err(|error| Error::Io(format!("read {path}: {error}")))
        .and_then(|data| decode(&data));
    owned_or_null(read)
}

/// Free an image. Elements that show it keep their own reference. NULL
/// does nothing.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_image_free(image: *mut KatuiImage) {
    unsafe { free(image) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_image_width(image: *const KatuiImage) -> u32 {
    unsafe { borrow(image) }.map_or(0, |image| image.0.width())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_image_height(image: *const KatuiImage) -> u32 {
    unsafe { borrow(image) }.map_or(0, |image| image.0.height())
}

/// The RGBA bytes, four per pixel, row by row, with their count in
/// `length`. The image owns them; they live as long as it does.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_image_rgba(
    image: *const KatuiImage,
    length: *mut usize,
) -> *const u8 {
    let pixels = unsafe { borrow(image) }.map_or(&[][..], |image| image.0.pixels());
    if let Some(length) = unsafe { borrow_mut(length) } {
        *length = pixels.len();
    }
    pixels.as_ptr()
}

/// Write the color at `x`, `y` into `color`. False outside the image.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_image_pixel(
    image: *const KatuiImage,
    x: u32,
    y: u32,
    color: *mut KatuiColor,
) -> bool {
    let Some(image) = (unsafe { borrow(image) }) else {
        return false;
    };
    if x >= image.0.width() {
        return false;
    }
    let at = (y as usize * image.0.width() as usize + x as usize) * 4;
    let Some(&[red, green, blue, alpha]) = image.0.pixels().get(at..at + 4) else {
        return false;
    };
    if let Some(color) = unsafe { borrow_mut(color) } {
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
    image: *const KatuiImage,
    path: *const c_char,
) -> KatuiStatus {
    let path = unsafe { text(path) };
    let saved = unsafe { borrow(image) }
        .ok_or(Error::Null("image"))
        .and_then(|image| {
            image::save_buffer(
                path.as_ref(),
                image.0.pixels(),
                image.0.width(),
                image.0.height(),
                image::ExtendedColorType::Rgba8,
            )
            .map_err(|error| Error::Io(format!("write {path}: {error}")))
        });
    status(saved)
}
