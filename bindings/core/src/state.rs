//! Widget state over host values: focus over the host's controls, toasts
//! timed in seconds.

use std::time::{Duration, Instant};

use katui_proto::input::InputEvent;
use katui_widgets::Toasts;

/// `seconds` as a duration; a negative or unrepresentable one is zero.
#[must_use]
pub fn duration(seconds: f64) -> Duration {
    Duration::try_from_secs_f64(seconds).unwrap_or_default()
}

/// `seconds` as a timer interval of at least a millisecond.
#[must_use]
pub fn interval(seconds: f64) -> Duration {
    Duration::try_from_secs_f64(seconds.max(0.001)).unwrap_or(Duration::MAX)
}

/// Show `text`, for `seconds` or the default time.
pub fn push_toast(toasts: &mut Toasts, text: String, seconds: Option<f64>) {
    match seconds {
        Some(seconds) => toasts.push_for(text, Instant::now(), duration(seconds)),
        None => toasts.push(text, Instant::now()),
    }
}

/// Seconds until the next toast leaves.
#[must_use]
pub fn next_expiry(toasts: &Toasts) -> Option<f64> {
    toasts
        .next_expiry()
        .map(|at| at.saturating_duration_since(Instant::now()).as_secs_f64())
}

/// Which of the host's controls has the keyboard; Tab and Shift+Tab move it.
pub struct Focus<H> {
    controls: Vec<H>,
    inner: katui_widgets::Focus<usize>,
}

impl<H> Focus<H> {
    /// Focus over `controls`, the first one focused; `None` without any.
    #[must_use]
    pub fn new(controls: Vec<H>) -> Option<Self> {
        if controls.is_empty() {
            return None;
        }
        let inner = katui_widgets::Focus::new(0, 1..controls.len());
        Some(Self { controls, inner })
    }

    #[must_use]
    pub fn current(&self) -> Option<&H> {
        self.controls.get(self.inner.current())
    }

    /// Whether the focused control is the one `same` picks.
    pub fn is<E>(&self, same: impl FnMut(&H) -> Result<bool, E>) -> Result<bool, E> {
        Ok(self.position(same)? == Some(self.inner.current()))
    }

    /// Focus the control `same` picks. False when it picks none.
    pub fn set<E>(&mut self, same: impl FnMut(&H) -> Result<bool, E>) -> Result<bool, E> {
        Ok(match self.position(same)? {
            Some(index) => self.inner.set(index),
            None => false,
        })
    }

    pub fn next(&mut self) {
        self.inner.next();
    }

    pub fn previous(&mut self) {
        self.inner.previous();
    }

    /// Move the focus on Tab and Shift+Tab. True when it moved.
    pub fn handle(&mut self, event: &InputEvent) -> bool {
        self.inner.handle(event)
    }

    fn position<E>(&self, mut same: impl FnMut(&H) -> Result<bool, E>) -> Result<Option<usize>, E> {
        for (index, known) in self.controls.iter().enumerate() {
            if same(known)? {
                return Ok(Some(index));
            }
        }
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn focus_finds_controls_by_the_hosts_equality() {
        assert!(Focus::<u8>::new(Vec::new()).is_none());
        let Some(mut focus) = Focus::new(vec!["name", "mail"]) else {
            return;
        };
        assert_eq!(focus.current(), Some(&"name"));
        assert_eq!(focus.set(|known| Ok::<_, ()>(*known == "mail")), Ok(true));
        assert_eq!(focus.is(|known| Ok::<_, ()>(*known == "mail")), Ok(true));
        assert_eq!(focus.set(|known| Ok::<_, ()>(*known == "phone")), Ok(false));
        focus.next();
        assert_eq!(focus.current(), Some(&"name"));
    }

    #[test]
    fn bad_seconds_turn_into_safe_durations() {
        assert_eq!(duration(-1.0), Duration::ZERO);
        assert_eq!(duration(f64::NAN), Duration::ZERO);
        assert_eq!(interval(0.0), Duration::from_millis(1));
        assert_eq!(interval(f64::INFINITY), Duration::MAX);
    }
}
