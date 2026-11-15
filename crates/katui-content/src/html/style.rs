//! The cascade: which rules match an element, and the style they compute.

use std::collections::HashMap;

use katui_draw::{Rgba, TextSpan};
use katui_ui::{Align, Length};
use scraper::{ElementRef, Html, Selector};

use super::HtmlDefaults;
use crate::css::{self, Declaration};

/// The declarations matching each element, in cascade order.
pub(super) struct Cascade {
    pub(super) matched: HashMap<ego_tree::NodeId, Vec<Declaration>>,
}

impl Cascade {
    pub(super) fn new(document: &Html) -> Self {
        let mut ranked: HashMap<ego_tree::NodeId, Vec<(u32, usize, Declaration)>> = HashMap::new();
        let mut order = 0;
        for sheet in stylesheets(document) {
            for rule in css::rules(&sheet) {
                for selector_text in rule.selectors.split(',') {
                    let Ok(selector) = Selector::parse(selector_text.trim()) else {
                        continue;
                    };
                    let rank = specificity(selector_text);
                    for element in document.select(&selector) {
                        let entry = ranked.entry(element.id()).or_default();
                        for declaration in &rule.declarations {
                            entry.push((rank, order, declaration.clone()));
                            order += 1;
                        }
                    }
                }
            }
        }
        let matched = ranked
            .into_iter()
            .map(|(id, mut declarations)| {
                declarations.sort_by_key(|(rank, order, _)| (*rank, *order));
                (
                    id,
                    declarations
                        .into_iter()
                        .map(|(_, _, declaration)| declaration)
                        .collect(),
                )
            })
            .collect();
        Self { matched }
    }

    /// The declarations for `element`, its `style` attribute last.
    pub(super) fn declarations(&self, element: ElementRef<'_>) -> Vec<Declaration> {
        let mut declarations = self.matched.get(&element.id()).cloned().unwrap_or_default();
        if let Some(inline) = element.value().attr("style") {
            declarations.extend(css::declarations(inline));
        }
        declarations
    }
}

pub(super) fn stylesheets(document: &Html) -> Vec<String> {
    Selector::parse("style")
        .ok()
        .map_or_else(Vec::new, |selector| {
            document
                .select(&selector)
                .map(|style| style.text().collect())
                .collect()
        })
}

/// Ids weigh a hundred, classes, attributes and pseudo-classes ten, and
/// type names one, which orders every rule the subset can express.
pub(super) fn specificity(selector: &str) -> u32 {
    let mut rank = 0;
    for part in selector
        .split([' ', '>', '+', '~'])
        .filter(|part| !part.is_empty())
    {
        rank += 100 * part.matches('#').count() as u32;
        rank += 10
            * (part.matches('.').count() + part.matches('[').count() + part.matches(':').count())
                as u32;
        if part.starts_with(|first: char| first.is_ascii_alphabetic()) {
            rank += 1;
        }
    }
    rank
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Display {
    Block,
    Inline,
    Flex,
    /// A child of a flex container: its own box, sized by its content.
    Item,
    Hidden,
}

/// A computed style: the inherited text properties and the box
/// properties of one element.
#[allow(clippy::struct_excessive_bools)] // text decorations are flags
#[derive(Debug, Clone)]
pub(super) struct Style {
    pub(super) root_size: f32,
    pub(super) size: f32,
    pub(super) color: Rgba,
    pub(super) link: Rgba,
    pub(super) tint: Rgba,
    pub(super) bold: bool,
    pub(super) italic: bool,
    pub(super) underline: bool,
    pub(super) strike: bool,
    pub(super) preformatted: bool,
    pub(super) text_align: Align,
    pub(super) display: Display,
    pub(super) row: bool,
    pub(super) background: Option<Rgba>,
    pub(super) highlight: Option<Rgba>,
    pub(super) padding: [f32; 4],
    pub(super) margin: [f32; 4],
    pub(super) border: Option<(f32, Rgba)>,
    pub(super) radius: f32,
    pub(super) gap: f32,
    pub(super) width: Option<Length>,
    pub(super) height: Option<f32>,
    pub(super) justify: Align,
    pub(super) align: Align,
}

impl Style {
    pub(super) fn root(defaults: &HtmlDefaults) -> Self {
        Self {
            root_size: defaults.font_size,
            size: defaults.font_size,
            color: defaults.color,
            link: defaults.link,
            tint: defaults.tint,
            bold: false,
            italic: false,
            underline: false,
            strike: false,
            preformatted: false,
            text_align: Align::Start,
            display: Display::Block,
            row: false,
            background: None,
            highlight: None,
            padding: [0.0; 4],
            margin: [0.0; 4],
            border: None,
            radius: 0.0,
            gap: defaults.font_size * 0.6,
            width: None,
            height: None,
            justify: Align::Start,
            align: Align::Start,
        }
    }

    /// The style a child of `self` named `tag` starts from: inherited text
    /// properties kept, box properties reset.
    pub(super) fn child(&self, tag: &str) -> Self {
        let mut child = Self {
            display: Display::Block,
            row: false,
            background: None,
            highlight: None,
            padding: [0.0; 4],
            margin: [0.0; 4],
            border: None,
            radius: 0.0,
            gap: self.size * 0.6,
            width: None,
            height: None,
            justify: Align::Start,
            align: Align::Start,
            ..self.clone()
        };
        child.apply_tag(tag);
        child
    }

    pub(super) fn apply_tag(&mut self, tag: &str) {
        match tag {
            "head" | "script" | "style" | "title" | "meta" | "link" | "template" | "noscript" => {
                self.display = Display::Hidden;
            }
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => self.heading(tag),
            "a" => {
                self.display = Display::Inline;
                self.color = self.link;
                self.underline = true;
            }
            "b" | "strong" => self.inline_with(|style| style.bold = true),
            "i" | "em" | "cite" => self.inline_with(|style| style.italic = true),
            "u" | "ins" => self.inline_with(|style| style.underline = true),
            "s" | "del" | "strike" => self.inline_with(|style| style.strike = true),
            "code" | "kbd" | "samp" | "mark" => {
                let tint = self.tint;
                self.inline_with(|style| style.highlight = Some(tint));
            }
            "span" | "small" | "label" | "abbr" | "sub" | "sup" | "img" | "br" => {
                self.display = Display::Inline;
            }
            "pre" => {
                self.preformatted = true;
                self.background = Some(self.tint);
                self.padding = [self.size * 0.6; 4];
                self.radius = self.size * 0.3;
            }
            "blockquote" => {
                self.background = Some(self.tint);
                self.padding = [self.size * 0.5, self.size, self.size * 0.5, self.size];
                self.italic = true;
            }
            "ul" | "ol" => {
                self.padding = [0.0, 0.0, 0.0, self.size * 1.2];
                self.gap = self.size * 0.25;
            }
            _ => {}
        }
    }

    pub(super) fn heading(&mut self, tag: &str) {
        let scale = match tag {
            "h1" => 2.0,
            "h2" => 1.5,
            "h3" => 1.25,
            "h4" => 1.1,
            "h5" => 1.0,
            _ => 0.9,
        };
        self.size *= scale;
        self.bold = true;
    }

    pub(super) fn inline_with(&mut self, change: impl FnOnce(&mut Self)) {
        self.display = Display::Inline;
        change(self);
    }

    pub(super) fn apply(&mut self, declaration: &Declaration) {
        let property = declaration.property.as_str();
        let value = declaration.value.as_str();
        if !self.apply_text(property, value) {
            self.apply_box(property, value);
        }
    }

    /// Apply a property of the text itself; `false` when `property` is
    /// not one.
    pub(super) fn apply_text(&mut self, property: &str, value: &str) -> bool {
        match property {
            "color" => self.color = css::color(value).unwrap_or(self.color),
            "font-size" => {
                self.size = css::length(value, self.size, self.root_size).unwrap_or(self.size);
            }
            "font-weight" => {
                self.bold = matches!(value, "bold" | "bolder" | "600" | "700" | "800" | "900");
            }
            "font-style" => self.italic = value == "italic" || value == "oblique",
            "text-decoration" | "text-decoration-line" => {
                self.underline = value.contains("underline");
                self.strike = value.contains("line-through");
            }
            "text-align" => self.text_align = align(value).unwrap_or(self.text_align),
            "white-space" => self.preformatted = value.starts_with("pre"),
            _ => return false,
        }
        true
    }

    /// Apply a property of the box around the content.
    pub(super) fn apply_box(&mut self, property: &str, value: &str) {
        let (size, root) = (self.size, self.root_size);
        let length = |value: &str| css::length(value, size, root);
        match property {
            "background" | "background-color" => {
                self.background = css::color(value).or(self.background);
            }
            "padding" => self.padding = css::sides(value, size, root).unwrap_or(self.padding),
            "margin" => self.margin = css::sides(value, size, root).unwrap_or(self.margin),
            "border-radius" => self.radius = length(value).unwrap_or(self.radius),
            "gap" | "row-gap" | "column-gap" => self.gap = length(value).unwrap_or(self.gap),
            "width" => self.width = width(value, size, root),
            "height" => self.height = length(value),
            "justify-content" => self.justify = align(value).unwrap_or(self.justify),
            "align-items" => self.align = align(value).unwrap_or(self.align),
            "flex-direction" => self.row = !value.starts_with("column"),
            "border" => self.border = border(value, size, root).or(self.border),
            "display" => self.apply_display(value),
            _ => {}
        }
    }

    pub(super) fn apply_display(&mut self, value: &str) {
        self.display = match value {
            "none" => Display::Hidden,
            "inline" | "inline-block" => Display::Inline,
            "flex" | "inline-flex" => {
                self.row = true;
                Display::Flex
            }
            _ => Display::Block,
        };
    }

    pub(super) fn span(&self, text: String) -> TextSpan {
        TextSpan {
            text,
            color: Some(self.color),
            background: self.highlight,
            bold: self.bold,
            italic: self.italic,
            underline: self.underline,
            strike: self.strike,
        }
    }
}

pub(super) fn align(value: &str) -> Option<Align> {
    match value {
        "center" => Some(Align::Center),
        "flex-start" | "start" | "left" | "stretch" | "normal" => Some(Align::Start),
        "flex-end" | "end" | "right" => Some(Align::End),
        _ => None,
    }
}

pub(super) fn width(value: &str, size: f32, root_size: f32) -> Option<Length> {
    if value == "100%" {
        return Some(Length::Fill);
    }
    css::length(value, size, root_size).map(Length::Px)
}

pub(super) fn border(value: &str, size: f32, root_size: f32) -> Option<(f32, Rgba)> {
    if value == "none" || value == "0" {
        return None;
    }
    let width = value
        .split_whitespace()
        .find_map(|part| css::length(part, size, root_size))
        .unwrap_or(1.0);
    let color = value
        .split_whitespace()
        .find_map(css::color)
        .unwrap_or(Rgba::solid(128, 128, 128));
    Some((width, color))
}
