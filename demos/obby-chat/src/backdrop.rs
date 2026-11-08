//! The obsidian behind the interface: a low-resolution sheen the terminal
//! scales up.

use std::f32::consts::TAU;

use crate::theme::hue_tint;

const BASALT: [f32; 3] = [27.0, 24.0, 38.0];

#[derive(Default)]
pub struct Backdrop {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
    unsent: bool,
}

impl Backdrop {
    /// Paint the sheen for this size; the same size repaints nothing.
    pub fn resize(&mut self, width: u32, height: u32) {
        if (width.max(1), height.max(1)) == (self.width, self.height) {
            return;
        }
        self.width = width.max(1);
        self.height = height.max(1);
        self.pixels = sheen(self.width, self.height);
        self.unsent = true;
    }

    /// Send the pixels again on the next frame.
    pub fn mark_unsent(&mut self) {
        self.unsent = true;
    }

    /// The pixels once per size, `None` until the size changes again.
    pub fn frame(&mut self) -> Option<(&[u8], u32, u32)> {
        std::mem::take(&mut self.unsent).then_some((
            self.pixels.as_slice(),
            self.width,
            self.height,
        ))
    }
}

/// Basalt carrying soft bands of oil-slick color.
fn sheen(width: u32, height: u32) -> Vec<u8> {
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    for row in 0..height {
        for column in 0..width {
            let x = column as f32 / width as f32;
            let y = row as f32 / height as f32;
            let band = (x * 2.3 + y * 1.1 + (y * 3.1 + x).sin() * 0.35) * TAU * 0.5;
            let glow = (band.sin() * 0.5 + 0.5).powi(3) * 0.2;
            let tint = hue_tint((x * 0.5 + y * 0.35 + 0.62).fract(), 0.7, 0.55);
            pixels.extend_from_slice(&[
                (BASALT[0] + tint.red * glow) as u8,
                (BASALT[1] + tint.green * glow) as u8,
                (BASALT[2] + tint.blue * glow) as u8,
                255,
            ]);
        }
    }
    pixels
}

#[cfg(test)]
mod tests {
    use super::Backdrop;

    #[test]
    fn the_sheen_is_sent_once_per_size() {
        let mut backdrop = Backdrop::default();
        backdrop.resize(32, 16);
        assert!(backdrop.frame().is_some());
        assert!(backdrop.frame().is_none());
        backdrop.resize(32, 16);
        assert!(backdrop.frame().is_none(), "the same size stays settled");
        backdrop.resize(40, 16);
        assert!(backdrop.frame().is_some());
    }
}
