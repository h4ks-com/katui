use std::time::{Duration, Instant};

use katui_proto::input::{Key, Mods};

/// We keep this short so a tap stays a tap, which leaves a gap before auto-repeat starts.
const FIRST_LEASE: Duration = Duration::from_millis(160);
const REPEAT_LEASE: Duration = Duration::from_millis(120);

/// Which keys are down, and the modifiers each went down with. A terminal
/// that reports releases gives the exact answer; on any other terminal, a
/// multiplexer included, we treat a press as a lease that auto-repeat
/// keeps renewing.
#[derive(Debug, Clone, Default)]
pub struct HeldKeys {
    down: Vec<(Key, Mods, Instant)>,
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
        self.press_with(key, Mods::NONE, now);
    }

    /// `key` was pressed or auto-repeated with `mods` at `now`.
    pub fn press_with(&mut self, key: Key, mods: Mods, now: Instant) {
        self.now = Some(now);
        let lease = if self.is_held(key) {
            REPEAT_LEASE
        } else {
            FIRST_LEASE
        };
        self.down.retain(|(held, ..)| *held != key);
        self.down.push((key, mods, now + lease));
    }

    /// `key` came up. We match on the key alone, because a terminal reports
    /// the modifiers held at release time: Shift let go before an arrow
    /// would otherwise leave the arrow held.
    pub fn release(&mut self, key: Key) {
        self.reports_releases = true;
        self.down.retain(|(held, ..)| *held != key);
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
            || self
                .down
                .iter()
                .any(|(held, _, until)| *held == key && self.lasts(*until))
    }

    /// Every key held now with the modifiers it went down with; a game
    /// controller's keys come with none.
    #[must_use]
    pub fn held(&self) -> Vec<(Key, Mods)> {
        let keyboard = self
            .down
            .iter()
            .filter(|(key, _, until)| !self.pad.contains(key) && self.lasts(*until))
            .map(|(key, mods, _)| (*key, *mods));
        self.pad
            .iter()
            .map(|key| (*key, Mods::NONE))
            .chain(keyboard)
            .collect()
    }

    fn lasts(&self, until: Instant) -> bool {
        self.reports_releases || self.now.is_some_and(|now| now < until)
    }

    #[must_use]
    pub fn any_held(&self, keys: &[Key]) -> bool {
        keys.iter().any(|key| self.is_held(*key))
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use katui_proto::input::{Key, Mods};

    use super::HeldKeys;

    const SHIFT: Mods = Mods {
        shift: true,
        ..Mods::NONE
    };

    #[test]
    fn a_key_keeps_its_modifiers_and_lets_go_without_them() {
        let start = Instant::now();
        let mut keys = HeldKeys::default();
        keys.release(Key::Down);
        keys.press_with(Key::Up, SHIFT, start);
        keys.press(Key::Left, start);
        assert_eq!(keys.held(), vec![(Key::Up, SHIFT), (Key::Left, Mods::NONE)]);
        keys.release(Key::Up);
        assert_eq!(
            keys.held(),
            vec![(Key::Left, Mods::NONE)],
            "Shift went up first, so the release came without it"
        );
    }

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
