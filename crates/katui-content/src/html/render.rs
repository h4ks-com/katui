//! The walk over the document that builds elements from styled nodes.

use katui_draw::Paint;
use katui_ui::{Element, Length};
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
        let mut run = Run {
            marker,
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
        let container = container(children, &style);
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
            ("img", _) => {
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
            ("img", _) => {
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
        Some(match link {
            Some(href) => image.on_click(self.host.link(href)),
            None => image,
        })
    }
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
