use std::time::{Duration, Instant};

use katui_proto::input::Key;

/// We keep this short so a tap stays a tap, which leaves a gap before auto-repeat starts.
const FIRST_LEASE: Duration = Duration::from_millis(160);
const REPEAT_LEASE: Duration = Duration::from_millis(120);

/// Which keys are down. A terminal that reports releases gives the exact
/// answer; on any other terminal, a multiplexer included, we treat a press
/// as a lease that auto-repeat keeps renewing.
#[derive(Debug, Clone, Default)]
pub struct HeldKeys {
    down: Vec<(Key, Instant)>,
    now: Option<Instant>,
    reports_releases: bool,
}

impl HeldKeys {
    /// Move the clock [`is_held`](Self::is_held) reads.
    pub fn tick(&mut self, now: Instant) {
        self.now = Some(now);
    }

    /// `key` was pressed or auto-repeated at `now`.
    pub fn press(&mut self, key: Key, now: Instant) {
        self.now = Some(now);
        let lease = if self.is_held(key) {
            REPEAT_LEASE
        } else {
            FIRST_LEASE
        };
        self.down.retain(|(held, _)| *held != key);
        self.down.push((key, now + lease));
    }

    pub fn release(&mut self, key: Key) {
        self.reports_releases = true;
        self.down.retain(|(held, _)| *held != key);
    }

    /// Forget every key, for when releases can no longer reach us.
    pub fn clear(&mut self) {
        self.down.clear();
    }

    #[must_use]
    pub fn is_held(&self, key: Key) -> bool {
        self.down.iter().any(|(held, until)| {
            *held == key && (self.reports_releases || self.now.is_some_and(|now| now < *until))
        })
    }

    #[must_use]
    pub fn any_held(&self, keys: &[Key]) -> bool {
        keys.iter().any(|key| self.is_held(*key))
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use katui_proto::input::Key;

    use super::HeldKeys;

    #[test]
    fn without_releases_a_press_lapses_unless_repeats_renew_it() {
        let start = Instant::now();
        let mut keys = HeldKeys::default();
        keys.press(Key::Up, start);
        keys.tick(start + Duration::from_millis(100));
        assert!(keys.is_held(Key::Up));
        keys.press(Key::Up, start + Duration::from_millis(110));
        keys.tick(start + Duration::from_millis(200));
        assert!(keys.is_held(Key::Up), "a repeat renews the lease");
        keys.tick(start + Duration::from_millis(300));
        assert!(!keys.is_held(Key::Up), "repeats stopped");
    }

    #[test]
    fn a_reported_release_makes_holds_exact() {
        let start = Instant::now();
        let mut keys = HeldKeys::default();
        keys.release(Key::Down);
        keys.press(Key::Up, start);
        keys.tick(start + Duration::from_secs(30));
        assert!(keys.is_held(Key::Up), "a hold outlasts any lease");
        keys.release(Key::Up);
        assert!(!keys.is_held(Key::Up));
    }
}
