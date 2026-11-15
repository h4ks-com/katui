//! HTML and Markdown documents in the current theme's look, with anything
//! the caller sets replaced.

use katui_content::{HtmlDefaults, HtmlHost};
use katui_draw::Rgba;
use katui_ui::Element;
use katui_widgets::Theme;

use crate::Msg;

/// How a document's text looks; `None` takes the theme's.
#[derive(Clone, Copy, Default)]
pub struct Look {
    pub font_size: Option<f32>,
    pub color: Option<Rgba>,
    pub link: Option<Rgba>,
    pub tint: Option<Rgba>,
}

impl Look {
    fn defaults(self) -> HtmlDefaults {
        let theme = Theme::current();
        HtmlDefaults {
            font_size: self.font_size.unwrap_or_else(|| theme.body()),
            color: self.color.unwrap_or(theme.text),
            link: self.link.unwrap_or(theme.accent),
            tint: self.tint.unwrap_or(theme.muted),
        }
    }
}

#[derive(Clone, Copy)]
pub enum Format {
    Html,
    Markdown,
}

/// `source` read as `format`, links and images resolved by `host`.
pub fn document<M: Clone>(
    format: Format,
    source: &str,
    look: Look,
    host: &dyn HtmlHost<M>,
) -> Element<M> {
    let defaults = look.defaults();
    match format {
        Format::Html => katui_content::html(source, &defaults, host),
        Format::Markdown => katui_content::markdown_view(source, &defaults, host),
    }
}

/// What a click on a link to `href` sends: `on_link` made from the href,
/// or the href itself without one.
#[must_use]
pub fn link<H: Clone>(on_link: Option<&H>, href: &str) -> Msg<H> {
    match on_link {
        Some(make) => Msg::Link(make.clone(), href.to_owned()),
        None => Msg::Href(href.to_owned()),
    }
}
