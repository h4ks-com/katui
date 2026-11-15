//! HTML and Markdown as elements, with links that send messages.

use std::ffi::{CString, c_char, c_void};
use std::sync::Arc;

use katui_bindings::html::{Format, Look, document};
use katui_content::HtmlHost;
use katui_scene::surface::Surface;

use crate::element::{KatuiColor, KatuiElement, KatuiImage, element, visible};
use crate::ffi::{borrow, text};
use crate::message::Msg;

/// What links and images in a document do, and how its text looks. A zeroed
/// struct, or NULL, takes every default.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct KatuiDocument {
    /// A click on a link sends this key with a link payload.
    pub link_key: u64,
    /// The image for an `src`, which stays the caller's, or NULL to leave
    /// it out. katui calls it during `katui_html` or `katui_markdown`.
    pub image:
        Option<unsafe extern "C" fn(user: *mut c_void, src: *const c_char) -> *const KatuiImage>,
    /// Handed to `image` as is.
    pub user: *mut c_void,
    /// The body text size in pixels, or 0 for the theme's.
    pub font_size: f32,
    /// The text color, or alpha 0 for the theme's.
    pub color: KatuiColor,
    /// The link color, or alpha 0 for the theme's accent.
    pub link: KatuiColor,
    /// The color of quotes and rules, or alpha 0 for the theme's muted one.
    pub tint: KatuiColor,
}

struct Host(Option<KatuiDocument>);

impl HtmlHost<Msg> for Host {
    fn link(&self, href: &str) -> Msg {
        let key = self.0.map_or(0, |document| document.link_key);
        Msg::Link(key, href.to_owned())
    }

    fn image(&self, src: &str) -> Option<Arc<Surface>> {
        let document = self.0?;
        let image = document.image?;
        let src = CString::new(src.replace('\0', "")).ok()?;
        let found = unsafe { image(document.user, src.as_ptr()) };
        unsafe { borrow(found) }.map(|image| Arc::clone(&image.0))
    }
}

/// What `document` sets of the text's look; zeros take the theme's.
fn look(document: Option<KatuiDocument>) -> Look {
    let Some(document) = document else {
        return Look::default();
    };
    Look {
        font_size: (document.font_size > 0.0).then_some(document.font_size),
        color: visible(document.color),
        link: visible(document.link),
        tint: visible(document.tint),
    }
}

fn read(format: Format, source: &str, document: Option<KatuiDocument>) -> *mut KatuiElement {
    element(self::document(
        format,
        source,
        look(document),
        &Host(document),
    ))
}

/// An HTML document as an element.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_html(
    source: *const c_char,
    document: *const KatuiDocument,
) -> *mut KatuiElement {
    let document = unsafe { borrow(document) }.copied();
    read(Format::Html, &unsafe { text(source) }, document)
}

/// A Markdown document as an element.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_markdown(
    source: *const c_char,
    document: *const KatuiDocument,
) -> *mut KatuiElement {
    let document = unsafe { borrow(document) }.copied();
    read(Format::Markdown, &unsafe { text(source) }, document)
}
