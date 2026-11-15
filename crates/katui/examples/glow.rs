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

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use katui::game::{Game, HeldKeys, Key, Light};

    use super::Ball;

    #[test]
    fn the_right_arrow_steers_the_glowing_ball() {
        let mut ball = Ball(120.0);
        let mut keys = HeldKeys::default();
        keys.press(Key::Right, Instant::now());
        for _ in 0..60 {
            ball.step(&keys, 1.0 / 60.0);
        }
        assert!(
            (ball.0 - 200.0).abs() < 0.01,
            "a second of right moves 80 pixels"
        );
        let mut light = Light::new(480, 270, 1.0);
        ball.render(&mut light);
        let surface = light.to_surface();
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../target/screenshots");
        std::fs::create_dir_all(dir).expect("the target directory is writable");
        image::save_buffer(
            format!("{dir}/glow.png"),
            surface.pixels(),
            surface.width(),
            surface.height(),
            image::ColorType::Rgba8,
        )
        .expect("the png writer saves under target");
    }
}
