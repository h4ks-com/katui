//! An HTML and CSS subset as katui-ui elements: block and flex layout,
//! inline text runs with their styles, colors, padding, borders, images
//! and links. Text renders in pixels with proportional fonts.
//!
//! Selectors match through Servo's `selectors` crate, with a specificity
//! that counts ids, classes and type names.

use std::sync::Arc;

use katui_draw::Rgba;
use katui_scene::surface::Surface;
use katui_ui::{Element, Length};
use scraper::{Html, Selector};

mod render;
mod run;
mod style;

use render::Renderer;
use style::{Cascade, Style};

/// What a document draws with before its own styles apply.
#[derive(Debug, Clone, PartialEq)]
pub struct HtmlDefaults {
    /// Body text size in pixels, which `em` and `rem` count from.
    pub font_size: f32,
    /// Body text color.
    pub color: Rgba,
    /// Link color.
    pub link: Rgba,
    /// The background behind code and quotes.
    pub tint: Rgba,
}

/// What the renderer asks its caller for: the message a link sends and
/// the pixels behind an image source.
pub trait HtmlHost<M> {
    /// The message a click on a link to `href` sends.
    fn link(&self, href: &str) -> M;
    /// The pixels of the image at `src`, when the caller has them.
    fn image(&self, src: &str) -> Option<Arc<Surface>>;
}

/// The colors GitHub gives its `> [!NOTE]` style alerts.
const ALERTS: &str = "<style>\
    .markdown-alert-note { border: 2px solid #4493f8 }\
    .markdown-alert-tip { border: 2px solid #3fb950 }\
    .markdown-alert-important { border: 2px solid #ab7df8 }\
    .markdown-alert-warning { border: 2px solid #d29922 }\
    .markdown-alert-caution { border: 2px solid #f85149 }\
    blockquote { font-style: normal }\
    </style>";

/// The markdown GitHub reads: tables, task lists, strikethrough,
/// footnotes and alerts on top of `CommonMark`.
pub(crate) fn github_flavor() -> pulldown_cmark::Options {
    use pulldown_cmark::Options;
    Options::ENABLE_TABLES
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_GFM
}

/// The element tree of the markdown document `source`, drawn through the
/// same HTML and CSS subset as [`html`], with GitHub's extensions.
pub fn markdown_view<M: Clone>(
    source: &str,
    defaults: &HtmlDefaults,
    host: &dyn HtmlHost<M>,
) -> Element<M> {
    let mut page = ALERTS.to_owned();
    pulldown_cmark::html::push_html(
        &mut page,
        pulldown_cmark::Parser::new_ext(source, github_flavor()),
    );
    html(&page, defaults, host)
}

/// The element tree of the HTML document `source`.
pub fn html<M: Clone>(source: &str, defaults: &HtmlDefaults, host: &dyn HtmlHost<M>) -> Element<M> {
    let document = Html::parse_document(source);
    let cascade = Cascade::new(&document);
    let renderer = Renderer { cascade, host };
    let root = Style::root(defaults);
    let body = Selector::parse("body")
        .ok()
        .and_then(|selector| document.select(&selector).next())
        .unwrap_or_else(|| document.root_element());
    renderer
        .block(body, &root, None)
        .unwrap_or_else(|| Element::column([]))
        .width(Length::Fill)
}
