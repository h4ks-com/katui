//! Plasma: a classic interference plasma drifting from deep blue through
//! orange to hot pink. Run with `just demo-plasma`.
#![cfg(unix)] // the runner drives /dev/tty through katui-rt's unix layer
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]

use katui_game::{Game, HeldKeys, Light, Rgb, play, srgb};

const SPEED: f32 = 3.6;
const SHADES: usize = 256;

fn main() -> Result<(), katui_rt::RtError> {
    play(Plasma::new())
}

struct Plasma {
    time: f32,
    palette: Vec<Rgb>,
}

impl Plasma {
    fn new() -> Self {
        Self {
            time: 0.0,
            palette: (0..SHADES)
                .map(|shade| palette(shade as f32 / (SHADES - 1) as f32))
                .collect(),
        }
    }

    fn shade(&self, value: f32) -> Rgb {
        let index = (value.clamp(0.0, 1.0) * (SHADES - 1) as f32).round() as usize;
        self.palette.get(index).copied().unwrap_or_default()
    }
}

impl Game for Plasma {
    fn step(&mut self, _keys: &HeldKeys, dt: f32) {
        self.time += dt * SPEED;
    }

    /// We sample the horizontal and vertical waves once per column and row,
    /// since only the diagonal waves vary with both.
    fn render(&mut self, light: &mut Light) {
        let time = self.time;
        let (width, height) = (light.width() as f32, light.height() as f32);
        let across: Vec<(f32, f32)> = (0..light.width())
            .map(|x| x as f32 / width)
            .map(|x| (x, (x * 6.0 + time).sin()))
            .collect();
        let down: Vec<(f32, f32)> = (0..light.height())
            .map(|y| y as f32 / height)
            .map(|y| (y, (y * 6.0 - time * 0.7).sin()))
            .collect();
        light.paint(|x, y| {
            let (Some(&(x, horizontal)), Some(&(y, vertical))) = (across.get(x), down.get(y))
            else {
                return [0.0; 3];
            };
            self.shade(plasma(x, y, time, horizontal, vertical))
        });
    }
}

/// Four waves summed and kept in zero to one.
fn plasma(x: f32, y: f32, time: f32, horizontal: f32, vertical: f32) -> f32 {
    let diagonal = ((x + y) * 5.0 + time * 1.3).sin();
    let cross = (((x - y) * 4.0 - time).sin() + horizontal).sin();
    f32::midpoint((horizontal + vertical + diagonal + cross) / 4.0, 1.0)
}

fn palette(value: f32) -> Rgb {
    let mix = |from: f32, to: f32| (from + (to - from) * value).round() as u8;
    srgb(mix(20.0, 255.0), mix(30.0, 90.0), mix(120.0, 60.0))
}

#[cfg(test)]
#[path = "support/artifact.rs"]
mod artifact;

#[cfg(test)]
mod tests {
    use crate::artifact;

    use katui_game::{Game, HeldKeys, Light};
    use katui_scene::surface::{PixelPos, Rgba};

    use super::{Plasma, plasma};

    fn frame(steps: u32) -> Light {
        let mut demo = Plasma::new();
        let mut light = Light::new(480, 270, 1.0);
        demo.resize(&light);
        let keys = HeldKeys::default();
        for _ in 0..steps {
            demo.step(&keys, 1.0 / 60.0);
        }
        demo.render(&mut light);
        light
    }

    #[test]
    fn the_waves_stay_between_zero_and_one() {
        for step in 0..40 {
            let (x, y, time) = (step as f32 * 0.07, step as f32 * 0.03, step as f32 * 0.5);
            let value = plasma(x, y, time, (x * 6.0 + time).sin(), (y * 6.0).sin());
            assert!((0.0..=1.0).contains(&value), "{value} at step {step}");
        }
    }

    #[test]
    fn the_palette_runs_from_deep_blue_to_hot_orange() {
        let mut light = Light::new(2, 1, 1.0);
        let demo = Plasma::new();
        light.paint(|x, _| demo.shade(x as f32));
        let surface = light.to_surface();
        let at = |x| surface.get(PixelPos::new(x, 0)).expect("inside");
        let blue = Rgba {
            r: 20,
            g: 30,
            b: 120,
            a: 255,
        };
        let orange = Rgba {
            r: 255,
            g: 90,
            b: 60,
            a: 255,
        };
        assert_eq!((at(0), at(1)), (blue, orange));
    }

    #[test]
    fn the_plasma_moves() {
        assert_ne!(
            frame(0).to_surface().pixels(),
            frame(30).to_surface().pixels()
        );
    }

    #[test]
    fn a_played_frame_dumps_a_png_artifact() {
        artifact::save_frame(&frame(90), "plasma");
    }
}
