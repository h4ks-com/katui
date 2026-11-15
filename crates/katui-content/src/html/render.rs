//! The walk over the document that builds elements from styled nodes.

use katui_draw::Paint;
use katui_ui::{Align, Element, Length};
use scraper::ElementRef;
use scraper::node::Node;

use super::HtmlHost;
use super::run::{ListMarker, Run};
use super::style::{Cascade, Display, Style};

pub(super) struct Renderer<'a, M> {
    pub(super) cascade: Cascade,
    pub(super) host: &'a dyn HtmlHost<M>,
}

impl<M: Clone> Renderer<'_, M> {
    pub(super) fn style(&self, element: ElementRef<'_>, parent: &Style) -> Style {
        let mut style = parent.child(element.value().name());
        if let Some(align) = element.value().attr("align").and_then(super::style::align) {
            style.text_align = align;
        }
        for declaration in self.cascade.declarations(element) {
            style.apply(&declaration);
        }
        if parent.display == Display::Flex
            && matches!(style.display, Display::Inline | Display::Block)
        {
            style.display = Display::Item;
        }
        // we paint an inline element's background behind its glyphs,
        // because it has no box of its own
        if style.display == Display::Inline {
            style.highlight = style
                .background
                .take()
                .or(style.highlight)
                .or(parent.highlight);
        }
        style
    }

    pub(super) fn block(
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
        let task = task_checkbox(element);
        let mut run = Run {
            marker: marker.filter(|_| task.is_none()),
            ..Run::default()
        };
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
        children.extend(run.finish(&style, self.host));
        if children.is_empty() && matches!(element.value().name(), "td" | "th") {
            children.push(Element::row([]).height(Length::Px(style.size * 1.3)));
        }
        let mut container = container(children, &style);
        if let Some(checked) = task {
            container = Element::row([checkbox(checked, &style), container])
                .gap(style.size * 0.5)
                .align(Align::Center)
                .width(Length::Fill);
        }
        if let Some(anchor) = anchor_of(element) {
            container = container.id(super::anchor_id(&anchor));
        }
        Some(match href(element) {
            Some(href) => container.on_click(self.host.link(href)),
            None => container,
        })
    }

    pub(super) fn child(
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
            ("br", _) => run.push_break(&child_style),
            ("img" | "video", _) => {
                children.extend(run.finish(style, self.host));
                children.extend(self.image(child, &child_style, run.link.as_deref()));
            }
            (_, Display::Inline) => self.inline(child, &child_style, run, style, children),
            _ => {
                children.extend(run.finish(style, self.host));
                let marker = marker.or(run.marker.take());
                children.extend(self.block(child, style, marker));
            }
        }
    }

    /// Add an inline element's text to `run`. An image inside it breaks
    /// the run, because a picture sits between lines of text in a block of
    /// `block` style.
    pub(super) fn inline(
        &self,
        element: ElementRef<'_>,
        style: &Style,
        run: &mut Run,
        block: &Style,
        children: &mut Vec<Element<M>>,
    ) {
        let outer = match href(element) {
            Some(href) => run.link.replace(href.to_owned()),
            None => run.link.clone(),
        };
        for child in element.children() {
            match child.value() {
                Node::Text(text) => run.push_text(text, style),
                Node::Element(_) => {
                    if let Some(child) = ElementRef::wrap(child) {
                        self.inline_child(child, style, run, block, children);
                    }
                }
                _ => {}
            }
        }
        run.link = outer;
    }

    pub(super) fn inline_child(
        &self,
        child: ElementRef<'_>,
        style: &Style,
        run: &mut Run,
        block: &Style,
        children: &mut Vec<Element<M>>,
    ) {
        let child_style = self.style(child, style);
        match (child.value().name(), child_style.display) {
            (_, Display::Hidden) => {}
            ("br", _) => run.push_break(&child_style),
            ("img" | "video", _) => {
                children.extend(run.finish(block, self.host));
                children.extend(self.image(child, &child_style, run.link.as_deref()));
            }
            _ => self.inline(child, &child_style, run, block, children),
        }
    }

    pub(super) fn image(
        &self,
        element: ElementRef<'_>,
        style: &Style,
        link: Option<&str>,
    ) -> Option<Element<M>> {
        let surface = self.host.image(source_of(element)?)?;
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
        Some(match link {
            Some(href) => image.on_click(self.host.link(href)),
            None => image,
        })
    }
}

/// The name a link fragment reaches `element` by: its `id` attribute, or
/// for a heading the slug GitHub makes of its text.
fn anchor_of(element: ElementRef<'_>) -> Option<String> {
    if let Some(id) = element.value().attr("id") {
        return Some(id.to_owned());
    }
    let heading = matches!(
        element.value().name(),
        "h1" | "h2" | "h3" | "h4" | "h5" | "h6"
    );
    heading.then(|| slug(&element.text().collect::<String>()))
}

/// GitHub's heading slug: lowercase letters, digits, `-` and `_`, with
/// spaces turned into `-`.
fn slug(text: &str) -> String {
    text.trim()
        .chars()
        .filter_map(|letter| match letter {
            ' ' => Some('-'),
            '-' | '_' => Some(letter),
            _ if letter.is_alphanumeric() => Some(letter),
            _ => None,
        })
        .flat_map(char::to_lowercase)
        .collect()
}

/// Whether the list item `element` is a task, and whether it is done.
fn task_checkbox(element: ElementRef<'_>) -> Option<bool> {
    if element.value().name() != "li" {
        return None;
    }
    let input = element
        .children()
        .filter_map(ElementRef::wrap)
        .find(|child| child.value().name() == "input")?;
    (input.value().attr("type") == Some("checkbox"))
        .then(|| input.value().attr("checked").is_some())
}

fn checkbox<M>(checked: bool, style: &Style) -> Element<M> {
    let side = (style.size * 0.9).round();
    let mark = Element::row([])
        .size(Length::Px(side), Length::Px(side))
        .radius(side * 0.25)
        .border(1.5, Paint::Solid(style.color));
    if checked {
        mark.background(Paint::Solid(style.link))
            .border(1.5, Paint::Solid(style.link))
    } else {
        mark
    }
}

/// Where an image's or a video's pixels come from: its `src`, or for a
/// video without one, the `src` of its first `<source>`.
fn source_of(element: ElementRef<'_>) -> Option<&str> {
    element.value().attr("src").or_else(|| {
        element
            .children()
            .filter_map(ElementRef::wrap)
            .find(|child| child.value().name() == "source")
            .and_then(|source| source.value().attr("src"))
    })
}

/// The target of `element` when it is a link.
pub(super) fn href(element: ElementRef<'_>) -> Option<&str> {
    (element.value().name() == "a")
        .then(|| element.value().attr("href"))
        .flatten()
}

pub(super) fn container<M>(children: Vec<Element<M>>, style: &Style) -> Element<M> {
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

    use super::Renderer;
    use crate::html::style::{Cascade, Display, Style};
    use crate::html::{HtmlDefaults, HtmlHost};

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
    fn the_children_of_a_flex_container_become_boxes_of_their_own() {
        let document = Html::parse_document(
            "<div style=\"display: flex\"><span>one</span><p>two</p></div><span>three</span>",
        );
        let renderer = Renderer {
            cascade: Cascade::new(&document),
            host: &Host as &dyn HtmlHost<String>,
        };
        let root = Style::root(&defaults());
        let pick = |css: &str| {
            let selector = Selector::parse(css).expect("a valid selector");
            document
                .select(&selector)
                .next()
                .expect("the element exists")
        };
        let flex = renderer.style(pick("div"), &root);
        for child in ["div > span", "div > p"] {
            assert_eq!(
                renderer.style(pick(child), &flex).display,
                Display::Item,
                "{child} is a flex item"
            );
        }
        assert_eq!(
            renderer.style(pick("body > span"), &root).display,
            Display::Inline,
            "a span in normal flow stays inline"
        );
    }

    #[test]
    fn table_cells_share_their_row_and_task_items_carry_their_checkbox() {
        let mut page = String::new();
        pulldown_cmark::html::push_html(
            &mut page,
            pulldown_cmark::Parser::new_ext(
                "| a | b |\n|---|---|\n| 1 | 2 |\n\n- [x] done\n- [ ] open\n",
                crate::html::github_flavor(),
            ),
        );
        let document = Html::parse_document(&page);
        let renderer = Renderer {
            cascade: Cascade::new(&document),
            host: &Host as &dyn HtmlHost<String>,
        };
        let root = Style::root(&defaults());
        let pick = |css: &str| {
            let selector = Selector::parse(css).expect("a valid selector");
            document.select(&selector).collect::<Vec<_>>()
        };
        let row = renderer.style(pick("tbody tr")[0], &root);
        assert_eq!(row.display, Display::Flex);
        let cell = renderer.style(pick("td")[0], &row);
        assert_eq!(cell.width, Some(katui_ui::Length::Fill));
        let tasks: Vec<_> = pick("li").into_iter().map(super::task_checkbox).collect();
        assert_eq!(tasks, [Some(true), Some(false)]);
    }

    #[test]
    fn headings_take_githubs_slug_and_fragments_find_them() {
        assert_eq!(super::slug(" A first program! "), "a-first-program");
        assert_eq!(super::slug("Use katui_rt: 2.0"), "use-katui_rt-20");
        assert_eq!(
            crate::html::anchor_id("#install"),
            crate::html::anchor_id("install")
        );
        assert_ne!(
            crate::html::anchor_id("install"),
            crate::html::anchor_id("demos")
        );
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
