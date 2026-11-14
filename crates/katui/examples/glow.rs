//! A glowing ball the arrow keys steer, the smallest katui game. Run with
//! `just demo-glow`.
#![cfg(unix)] // the runner drives /dev/tty through katui-rt's unix layer
#![allow(clippy::cast_precision_loss)]

use katui::game::{Game, HeldKeys, Key, Light, P2, play};

struct Ball(f32);

impl Game for Ball {
    fn step(&mut self, keys: &HeldKeys, dt: f32) {
        let push =
            f32::from(i8::from(keys.is_held(Key::Right)) - i8::from(keys.is_held(Key::Left)));
        self.0 += push * 80.0 * dt;
    }

    fn render(&mut self, light: &mut Light) {
        light.dim(0.0);
        let middle = P2::new(self.0, light.height() as f32 / 2.0);
        light.glow(middle, 24.0, [2.0, 0.9, 0.4]);
    }
}

fn main() -> Result<(), katui::rt::RtError> {
    play(Ball(40.0))
}
