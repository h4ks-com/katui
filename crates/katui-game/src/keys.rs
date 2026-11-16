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
    /// The keys a game controller holds, which it reports exactly, so they
    /// need no lease.
    pad: Vec<Key>,
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

    /// Hold exactly `keys` for the game controllers, and return the ones
    /// that just went down.
    #[cfg_attr(not(feature = "gamepad"), allow(dead_code))]
    pub(crate) fn hold_for_pads(&mut self, keys: Vec<Key>) -> Vec<Key> {
        let pressed = keys
            .iter()
            .filter(|key| !self.is_held(**key))
            .copied()
            .collect();
        self.pad = keys;
        pressed
    }

    #[must_use]
    pub fn is_held(&self, key: Key) -> bool {
        self.pad.contains(&key)
            || self.down.iter().any(|(held, until)| {
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

    #[test]
    fn a_controller_holds_keys_until_it_lets_go_and_leaves_leases_alone() {
        let start = Instant::now();
        let mut keys = HeldKeys::default();
        assert_eq!(keys.hold_for_pads(vec![Key::Left]), vec![Key::Left]);
        assert_eq!(
            keys.hold_for_pads(vec![Key::Left]),
            vec![],
            "a held key goes down once"
        );
        keys.tick(start + Duration::from_secs(5));
        assert!(keys.is_held(Key::Left), "a controller hold needs no lease");
        keys.hold_for_pads(Vec::new());
        assert!(!keys.is_held(Key::Left));
        keys.press(Key::Up, start);
        keys.tick(start + Duration::from_secs(1));
        assert!(!keys.is_held(Key::Up), "keyboard presses still lapse");
    }
}
