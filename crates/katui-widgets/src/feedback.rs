//! Feedback: a busy spinner, toast notifications and a row of key hints.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use katui_draw::Paint;
use katui_ui::{Align, Element, Length};

use crate::theme::Theme;
use crate::view::label;

/// How long one dot of the spinner stays lit.
const SPINNER_STEP: Duration = Duration::from_millis(250);

/// How long a toast stays when nothing else is said.
const TOAST_TIME: Duration = Duration::from_secs(4);

/// Three dots with one lit, moving on every quarter second of `elapsed`.
/// Keep redrawing while it shows, for example with `Control::Animate`.
pub fn spinner<M>(elapsed: Duration, theme: &Theme) -> Element<M> {
    let lit = elapsed.as_millis() / SPINNER_STEP.as_millis() % 3;
    let side = theme.body() * 0.45;
    let dots = (0..3).map(|index| {
        let color = if index == lit {
            theme.accent
        } else {
            theme.muted
        };
        Element::row([])
            .size(Length::Px(side), Length::Px(side))
            .radius(side / 2.0)
            .background(Paint::Solid(color))
    });
    Element::row(dots)
        .gap(side * 0.6)
        .align(Align::Center)
        .height(Length::Px(theme.body()))
}

/// Short messages that go away on their own, newest last.
#[derive(Debug, Clone, Default)]
pub struct Toasts {
    shown: VecDeque<(String, Instant)>,
}

impl Toasts {
    /// Show `text` for the default toast time from `now`.
    pub fn push(&mut self, text: impl Into<String>, now: Instant) {
        self.push_for(text, now, TOAST_TIME);
    }

    /// Show `text` for `time` from `now`.
    pub fn push_for(&mut self, text: impl Into<String>, now: Instant, time: Duration) {
        self.shown.push_back((text.into(), now + time));
    }

    /// Drop the toasts whose time is up at `now`. Returns whether any toast
    /// expired.
    pub fn expire(&mut self, now: Instant) -> bool {
        let before = self.shown.len();
        self.shown.retain(|(_, until)| *until > now);
        self.shown.len() != before
    }

    /// When the next toast expires, so an app can wake up then.
    #[must_use]
    pub fn next_expiry(&self) -> Option<Instant> {
        self.shown.iter().map(|(_, until)| *until).min()
    }

    /// Whether no toast is showing.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.shown.is_empty()
    }

    /// The texts showing, oldest first.
    pub fn texts(&self) -> impl Iterator<Item = &str> {
        self.shown.iter().map(|(text, _)| text.as_str())
    }
}

/// The toasts in the bottom right corner, above everything. Put it last
/// in your root element.
pub fn toasts<M>(toasts: &Toasts, theme: &Theme) -> Element<M> {
    let cards = toasts.texts().map(|text| {
        Element::column([label(text, theme.body(), theme.text)])
            .padding_xy(theme.cell_width() * 1.5, theme.cell_height() * 0.5)
            .radius(theme.radius())
            .background(theme.raised.clone())
            .border(1.0, theme.border.clone())
    });
    Element::column(cards)
        .gap(theme.cell_height() * 0.4)
        .padding(theme.cell_height())
        .align(Align::End)
        .justify(Align::End)
        .size(Length::Fill, Length::Fill)
        .popup(0.0, 0.0)
}

/// A row of key hints: each key drawn as a key cap, followed by what it
/// does, like `("Ctrl+S", "save the file")`.
pub fn key_hints<M>(hints: &[(&str, &str)], theme: &Theme) -> Element<M> {
    let pairs = hints.iter().map(|(key, action)| {
        let cap = Element::column([label(*key, theme.small(), theme.text).weight(700)])
            .padding_xy(theme.cell_width() * 0.6, theme.cell_height() * 0.05)
            .radius(theme.cell_height() * 0.25)
            .background(theme.highlight.clone())
            .border(1.0, theme.border.clone());
        Element::row([cap, label(*action, theme.small(), theme.muted)])
            .gap(theme.cell_width() * 0.6)
            .align(Align::Center)
    });
    Element::row(pairs)
        .gap(theme.cell_width() * 2.0)
        .align(Align::Center)
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::Toasts;

    #[test]
    fn toasts_leave_when_their_time_is_up() {
        let start = Instant::now();
        let mut toasts = Toasts::default();
        toasts.push("saved", start);
        toasts.push_for("quick", start, Duration::from_secs(1));
        assert_eq!(toasts.next_expiry(), Some(start + Duration::from_secs(1)));
        assert!(!toasts.expire(start));
        assert!(toasts.expire(start + Duration::from_secs(2)));
        assert_eq!(toasts.texts().collect::<Vec<_>>(), ["saved"]);
        assert!(toasts.expire(start + Duration::from_secs(5)));
        assert!(toasts.is_empty());
    }
}
