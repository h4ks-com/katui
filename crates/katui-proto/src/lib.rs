//! Protocol codecs and capability probes: layer 0 of the stack in
//! ARCHITECTURE.md. Everything here is pure; the crate performs no I/O
//! and knows nothing about scenes or widgets.

pub mod kitty;
pub mod probe;
pub mod rgp;

/// The seeded RNG the parser fuzz loops share: dependency-free xorshift.
#[cfg(test)]
pub(crate) mod fuzz {
    /// Call [`XorShift::next`] for the next value; seed any odd u64.
    pub(crate) struct XorShift(u64);

    impl XorShift {
        /// A generator from `seed`.
        pub(crate) const fn new(seed: u64) -> Self {
            Self(seed)
        }

        /// The next pseudo-random value.
        pub(crate) fn next(&mut self) -> u64 {
            let mut x = self.0;
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            self.0 = x;
            x
        }

        /// A random byte.
        pub(crate) fn byte(&mut self) -> u8 {
            (self.next() % 256) as u8
        }
    }
}
