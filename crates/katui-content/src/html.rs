//! An HTML and CSS subset as katui-ui elements: block and flex layout,
//! inline text runs with their styles, colors, padding, borders, images
//! and links. Text renders in pixels with proportional fonts.
//!
//! Selectors match through Servo's `selectors` crate, with a specificity
//! that counts ids, classes and type names. Floats, tables, positioning
//! and scripts are out of the subset.

use std::collections::HashMap;
use std::sync::Arc;

use katui_draw::{Paint, Rgba, TextSpan};
use katui_scene::surface::Surface;
use katui_ui::{Align, Element, Length};
use scraper::node::Node;
use scraper::{ElementRef, Html, Selector};

use crate::css::{self, Declaration};

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

/// The declarations matching each element, in cascade order.
struct Cascade {
    matched: HashMap<ego_tree::NodeId, Vec<Declaration>>,
}

impl Cascade {
    fn new(document: &Html) -> Self {
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
    fn declarations(&self, element: ElementRef<'_>) -> Vec<Declaration> {
        let mut declarations = self.matched.get(&element.id()).cloned().unwrap_or_default();
        if let Some(inline) = element.value().attr("style") {
            declarations.extend(css::declarations(inline));
        }
        declarations
    }
}

fn stylesheets(document: &Html) -> Vec<String> {
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
fn specificity(selector: &str) -> u32 {
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
enum Display {
    Block,
    Inline,
    Flex,
    Hidden,
}

/// A computed style: the inherited text properties and the box
/// properties of one element.
#[allow(clippy::struct_excessive_bools)] // text decorations are flags
#[derive(Debug, Clone)]
struct Style {
    root_size: f32,
    size: f32,
    color: Rgba,
    link: Rgba,
    tint: Rgba,
    bold: bool,
    italic: bool,
    underline: bool,
    strike: bool,
    preformatted: bool,
    text_align: Align,
    display: Display,
    row: bool,
    background: Option<Rgba>,
    highlight: Option<Rgba>,
    padding: [f32; 4],
    margin: [f32; 4],
    border: Option<(f32, Rgba)>,
    radius: f32,
    gap: f32,
    width: Option<Length>,
    height: Option<f32>,
    justify: Align,
    align: Align,
}

impl Style {
    fn root(defaults: &HtmlDefaults) -> Self {
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
    fn child(&self, tag: &str) -> Self {
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

    fn apply_tag(&mut self, tag: &str) {
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

    fn heading(&mut self, tag: &str) {
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

    fn inline_with(&mut self, change: impl FnOnce(&mut Self)) {
        self.display = Display::Inline;
        change(self);
    }

    fn apply(&mut self, declaration: &Declaration) {
        let value = declaration.value.as_str();
        let length = |value: &str| css::length(value, self.size, self.root_size);
        match declaration.property.as_str() {
            "color" => self.color = css::color(value).unwrap_or(self.color),
            "background" | "background-color" => {
                self.background = css::color(value).or(self.background);
            }
            "font-size" => self.size = length(value).unwrap_or(self.size),
            "font-weight" => {
                self.bold = matches!(value, "bold" | "bolder" | "600" | "700" | "800" | "900");
            }
            "font-style" => self.italic = value == "italic" || value == "oblique",
            "text-decoration" | "text-decoration-line" => {
                self.underline = value.contains("underline");
                self.strike = value.contains("line-through");
            }
            "text-align" => self.text_align = align(value).unwrap_or(self.text_align),
            "padding" => {
                self.padding = css::sides(value, self.size, self.root_size).unwrap_or(self.padding);
            }
            "margin" => {
                self.margin = css::sides(value, self.size, self.root_size).unwrap_or(self.margin);
            }
            "border-radius" => self.radius = length(value).unwrap_or(self.radius),
            "gap" | "row-gap" | "column-gap" => self.gap = length(value).unwrap_or(self.gap),
            "width" => self.width = width(value, self.size, self.root_size),
            "height" => self.height = length(value),
            "justify-content" => self.justify = align(value).unwrap_or(self.justify),
            "align-items" => self.align = align(value).unwrap_or(self.align),
            "flex-direction" => self.row = !value.starts_with("column"),
            "white-space" => self.preformatted = value.starts_with("pre"),
            "border" => self.border = border(value, self.size, self.root_size).or(self.border),
            "display" => self.apply_display(value),
            _ => {}
        }
    }

    fn apply_display(&mut self, value: &str) {
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

    fn span(&self, text: String) -> TextSpan {
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

fn align(value: &str) -> Option<Align> {
    match value {
        "center" => Some(Align::Center),
        "flex-start" | "start" | "left" | "stretch" | "normal" => Some(Align::Start),
        "flex-end" | "end" | "right" => Some(Align::End),
        _ => None,
    }
}

fn width(value: &str, size: f32, root_size: f32) -> Option<Length> {
    if value == "100%" {
        return Some(Length::Fill);
    }
    css::length(value, size, root_size).map(Length::Px)
}

fn border(value: &str, size: f32, root_size: f32) -> Option<(f32, Rgba)> {
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

/// Which list an item sits in, for its marker.
#[derive(Debug, Clone, Copy)]
enum ListMarker {
    Bullet,
    Number(usize),
}

/// Text and its link, gathered across inline elements until a block
/// interrupts them.
#[derive(Default)]
struct Run {
    spans: Vec<TextSpan>,
    link: Option<String>,
    ends_in_space: bool,
}

impl Run {
    fn push_text(&mut self, text: &str, style: &Style) {
        let text = if style.preformatted {
            text.to_owned()
        } else {
            self.collapse(text)
        };
        if !text.is_empty() {
            self.spans.push(style.span(text));
        }
    }

    /// Whitespace collapsed the way a browser flows text: runs of it
    /// become one space, and none starts a line.
    fn collapse(&mut self, text: &str) -> String {
        let mut out = String::with_capacity(text.len());
        for character in text.chars() {
            if character.is_whitespace() {
                if !self.ends_in_space && (!self.spans.is_empty() || !out.is_empty()) {
                    out.push(' ');
                    self.ends_in_space = true;
                }
            } else {
                out.push(character);
                self.ends_in_space = false;
            }
        }
        out
    }

    fn take<M: Clone>(&mut self, style: &Style, host: &dyn HtmlHost<M>) -> Option<Element<M>> {
        let mut spans = std::mem::take(&mut self.spans);
        self.ends_in_space = false;
        if let Some(last) = spans.last_mut() {
            let trimmed = last.text.trim_end().len();
            last.text.truncate(trimmed);
        }
        spans.retain(|span| !span.text.is_empty());
        if spans.is_empty() {
            return None;
        }
        let text = Element::rich(spans)
            .font_size(style.size)
            .color(style.color);
        Some(match self.link.take() {
            Some(href) => text.on_click(host.link(&href)),
            None => text,
        })
    }
}

struct Renderer<'a, M> {
    cascade: Cascade,
    host: &'a dyn HtmlHost<M>,
}

impl<M: Clone> Renderer<'_, M> {
    fn style(&self, element: ElementRef<'_>, parent: &Style) -> Style {
        let mut style = parent.child(element.value().name());
        for declaration in self.cascade.declarations(element) {
            style.apply(&declaration);
        }
        // we paint an inline element's background behind its glyphs,
        // because it has no box of its own
        if style.display == Display::Inline {
            style.highlight = style.background.take().or(style.highlight);
        }
        style
    }

    fn block(
        &self,
        element: ElementRef<'_>,
        parent: &Style,
        marker: Option<ListMarker>,
    ) -> Option<Element<M>> {
        let style = self.style(element, parent);
        if style.display == Display::Hidden {
            return None;
        }
        let mut children = Vec::new();
        let mut run = Run::default();
        if let Some(marker) = marker {
            let text = match marker {
                ListMarker::Bullet => "\u{2022} ".to_owned(),
                ListMarker::Number(number) => format!("{number}. "),
            };
            run.spans.push(style.span(text));
            run.ends_in_space = true;
        }
        let ordered = element.value().name() == "ol";
        let mut item = 0;
        for child in element.children() {
            match child.value() {
                Node::Text(text) => run.push_text(text, &style),
                Node::Element(_) => {
                    let Some(child) = ElementRef::wrap(child) else {
                        continue;
                    };
                    let child_marker = (child.value().name() == "li").then(|| {
                        item += 1;
                        if ordered {
                            ListMarker::Number(item)
                        } else {
                            ListMarker::Bullet
                        }
                    });
                    self.child(child, &style, child_marker, &mut run, &mut children);
                }
                _ => {}
            }
        }
        children.extend(run.take(&style, self.host));
        Some(boxed(children, &style))
    }

    fn child(
        &self,
        child: ElementRef<'_>,
        style: &Style,
        marker: Option<ListMarker>,
        run: &mut Run,
        children: &mut Vec<Element<M>>,
    ) {
        let child_style = self.style(child, style);
        match (child.value().name(), child_style.display) {
            (_, Display::Hidden) => {}
            ("br", _) => run.spans.push(child_style.span("\n".to_owned())),
            ("img", _) => {
                children.extend(run.take(style, self.host));
                children.extend(self.image(child, &child_style));
            }
            (_, Display::Inline) => self.inline(child, &child_style, run),
            _ => {
                children.extend(run.take(style, self.host));
                children.extend(self.block(child, style, marker));
            }
        }
    }

    fn inline(&self, element: ElementRef<'_>, style: &Style, run: &mut Run) {
        if element.value().name() == "a" && run.link.is_none() {
            run.link = element.value().attr("href").map(str::to_owned);
        }
        for child in element.children() {
            match child.value() {
                Node::Text(text) => run.push_text(text, style),
                Node::Element(_) => {
                    if let Some(child) = ElementRef::wrap(child) {
                        let child_style = self.style(child, style);
                        if child.value().name() == "br" {
                            run.spans.push(child_style.span("\n".to_owned()));
                        } else if child_style.display != Display::Hidden {
                            self.inline(child, &child_style, run);
                        }
                    }
                }
                _ => {}
            }
        }
    }

    fn image(&self, element: ElementRef<'_>, style: &Style) -> Option<Element<M>> {
        let surface = self.host.image(element.value().attr("src")?)?;
        let attribute = |name: &str| {
            element
                .value()
                .attr(name)
                .and_then(|value| value.parse::<f32>().ok())
        };
        let width = style.width.or_else(|| attribute("width").map(Length::Px));
        let height = style.height.or_else(|| attribute("height"));
        let mut image = Element::image(surface).radius(style.radius);
        if let Some(width) = width {
            image = image.width(width);
        }
        if let Some(height) = height {
            image = image.height(Length::Px(height));
        }
        Some(image)
    }
}

fn boxed<M>(children: Vec<Element<M>>, style: &Style) -> Element<M> {
    let container = if style.row {
        Element::row(children)
            .align(style.align)
            .justify(style.justify)
    } else {
        Element::column(children)
            .align(style.text_align)
            .justify(style.justify)
    };
    let [top, right, bottom, left] = style.padding;
    let mut container = container
        .gap(style.gap)
        .padding_xy(left.max(right), top.max(bottom))
        .radius(style.radius);
    if let Some(background) = style.background {
        container = container.background(Paint::Solid(background));
    }
    if let Some((width, color)) = style.border {
        container = container.border(width, Paint::Solid(color));
    }
    container = match style.width {
        Some(width) => container.width(width),
        None if style.row || style.display == Display::Block => container.width(Length::Fill),
        None => container,
    };
    if let Some(height) = style.height {
        container = container.height(Length::Px(height));
    }
    let [top, right, bottom, left] = style.margin;
    if top + right + bottom + left > 0.0 {
        Element::column([container])
            .padding_xy(left.max(right), top.max(bottom))
            .width(Length::Fill)
    } else {
        container
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use katui_draw::Rgba;
    use katui_scene::surface::Surface;
    use scraper::{Html, Selector};

    use super::{Cascade, HtmlDefaults, HtmlHost, Renderer, Style};

    struct Host;

    impl HtmlHost<String> for Host {
        fn link(&self, href: &str) -> String {
            href.to_owned()
        }

        fn image(&self, _src: &str) -> Option<Arc<Surface>> {
            None
        }
    }

    fn defaults() -> HtmlDefaults {
        HtmlDefaults {
            font_size: 16.0,
            color: Rgba::solid(20, 20, 20),
            link: Rgba::solid(0, 0, 200),
            tint: Rgba::solid(230, 230, 230),
        }
    }

    #[test]
    fn ids_beat_classes_beat_types_and_the_style_attribute_beats_all() {
        let document = Html::parse_document(
            "<style>#lead { color: green } .note { color: blue; font-size: 20px } p { color: red }</style>\
             <p id=lead class=note>one</p><p class=note style=\"color: #fff\">two</p><p>three</p>",
        );
        let renderer = Renderer {
            cascade: Cascade::new(&document),
            host: &Host as &dyn HtmlHost<String>,
        };
        let root = Style::root(&defaults());
        let selector = Selector::parse("p").expect("a valid selector");
        let colors: Vec<(Rgba, f32)> = document
            .select(&selector)
            .map(|paragraph| {
                let style = renderer.style(paragraph, &root);
                (style.color, style.size)
            })
            .collect();
        assert_eq!(
            colors,
            [
                (Rgba::solid(0, 128, 0), 20.0),
                (Rgba::solid(255, 255, 255), 20.0),
                (Rgba::solid(255, 0, 0), 16.0)
            ]
        );
    }
}
