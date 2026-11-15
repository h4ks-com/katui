//! Inline text runs: styled spans and the line breaks between them.

use katui_draw::TextSpan;
use katui_ui::Element;

use super::HtmlHost;
use super::style::Style;

/// Which list an item sits in, for its marker.
#[derive(Debug, Clone, Copy)]
pub(super) enum ListMarker {
    Bullet,
    Number(usize),
}

/// Text gathered across inline elements until a block interrupts it,
/// each span with the link it sits in.
#[derive(Default)]
pub(super) struct Run {
    pub(super) spans: Vec<(TextSpan, Option<String>)>,
    pub(super) link: Option<String>,
    pub(super) marker: Option<ListMarker>,
    pub(super) ends_in_space: bool,
}

impl Run {
    pub(super) fn push_text(&mut self, text: &str, style: &Style) {
        let text = if style.preformatted {
            text.to_owned()
        } else {
            self.collapse(text)
        };
        if !text.is_empty() {
            self.push(style.span(text), style);
        }
    }

    /// Add `span`, after the list marker still waiting for the item's
    /// first text.
    pub(super) fn push(&mut self, span: TextSpan, style: &Style) {
        if let Some(marker) = self.marker.take() {
            let text = match marker {
                ListMarker::Bullet => "\u{2022} ".to_owned(),
                ListMarker::Number(number) => format!("{number}. "),
            };
            self.spans.push((style.span(text), None));
            self.ends_in_space = true;
        }
        self.spans.push((span, self.link.clone()));
    }

    /// A line break; whitespace right after it collapses away, the way it
    /// does at the start of a line.
    pub(super) fn push_break(&mut self, style: &Style) {
        self.push(style.span("\n".to_owned()), style);
        self.ends_in_space = true;
    }

    /// Whitespace collapsed the way a browser flows text: runs of it
    /// become one space, and none starts a line.
    pub(super) fn collapse(&mut self, text: &str) -> String {
        let mut out = String::with_capacity(text.len());
        for character in text.chars() {
            if character.is_whitespace() {
                let started = !self.spans.is_empty() || !out.is_empty();
                if !self.ends_in_space && started {
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

    pub(super) fn finish<M: Clone>(
        &mut self,
        style: &Style,
        host: &dyn HtmlHost<M>,
    ) -> Option<Element<M>> {
        let mut spans = std::mem::take(&mut self.spans);
        self.ends_in_space = false;
        if let Some((last, _)) = spans.last_mut() {
            let trimmed = last.text.trim_end().len();
            last.text.truncate(trimmed);
        }
        spans.retain(|(span, _)| !span.text.is_empty());
        if spans.is_empty() {
            return None;
        }
        let (spans, links): (Vec<TextSpan>, Vec<Option<String>>) = spans.into_iter().unzip();
        let text = Element::rich(spans)
            .font_size(style.size)
            .color(style.color);
        Some(
            links
                .into_iter()
                .enumerate()
                .filter_map(|(index, link)| Some((index, link?)))
                .fold(text, |text, (index, href)| {
                    text.on_span_click(index, host.link(&href))
                }),
        )
    }
}
