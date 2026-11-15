//! A flat-shaded cube spinning in software 3D. Hold the arrow keys to
//! steer its spin. Run with `just demo-cube`.
#![cfg(unix)] // the runner drives /dev/tty through katui-rt's unix layer
#![allow(clippy::cast_precision_loss)]

use katui_game::{
    Blend, Camera, Game, HeldKeys, Key, Light, P2, Rgb, V3, lerp3, play, scaled, srgb, v3,
};

/// Each face as four corner indices, wound so the second edge crossed with
/// the first points out of the cube, and its base color.
const FACES: [([usize; 4], [u8; 3]); 6] = [
    ([0, 1, 2, 3], [235, 80, 60]),
    ([5, 4, 7, 6], [60, 170, 235]),
    ([4, 0, 3, 7], [250, 200, 70]),
    ([1, 5, 6, 2], [90, 210, 120]),
    ([3, 2, 6, 7], [200, 110, 235]),
    ([4, 5, 1, 0], [240, 240, 240]),
];
const SUN: V3 = V3 {
    x: -0.4,
    y: 0.7,
    z: -0.6,
};
const AMBIENT: f32 = 0.18;
const BASE_SPIN: P2 = P2 { x: 0.7, y: 0.45 };
const STEER: f32 = 1.6;

fn main() -> Result<(), katui_rt::RtError> {
    play(Cube::default())
}

#[derive(Default)]
struct Cube {
    yaw: f32,
    pitch: f32,
    spin: P2,
}

/// The near square counterclockwise from the bottom left, then the far.
fn corners() -> [V3; 8] {
    [
        v3(-1.0, -1.0, -1.0),
        v3(1.0, -1.0, -1.0),
        v3(1.0, 1.0, -1.0),
        v3(-1.0, 1.0, -1.0),
        v3(-1.0, -1.0, 1.0),
        v3(1.0, -1.0, 1.0),
        v3(1.0, 1.0, 1.0),
        v3(-1.0, 1.0, 1.0),
    ]
}

fn backdrop(share: f32) -> Rgb {
    lerp3(srgb(18, 12, 40), srgb(2, 3, 10), share)
}

impl Cube {
    fn turned(&self, corner: V3) -> V3 {
        corner.rotated_x(self.pitch).rotated_y(self.yaw)
    }

    fn camera(light: &Light) -> Camera {
        Camera {
            center: P2::new(light.width() as f32 / 2.0, light.height() as f32 / 2.0),
            focal: light.height() as f32 * 1.4,
            distance: 6.0,
            pixel_aspect: light.pixel_aspect(),
        }
    }
}

impl Game for Cube {
    fn step(&mut self, keys: &HeldKeys, dt: f32) {
        let steer = |minus: Key, plus: Key| {
            f32::from(i8::from(keys.is_held(plus)) - i8::from(keys.is_held(minus))) * STEER
        };
        self.spin = P2::new(
            BASE_SPIN.x + steer(Key::Left, Key::Right),
            BASE_SPIN.y + steer(Key::Up, Key::Down),
        );
        self.yaw += self.spin.x * dt;
        self.pitch += self.spin.y * dt;
    }

    fn render(&mut self, light: &mut Light) {
        let height = light.height() as f32;
        light.paint(|_, y| backdrop(y as f32 / height));
        let camera = Self::camera(light);
        let corners = corners().map(|corner| self.turned(corner));
        let sun = SUN.normalized();
        let mut visible = Vec::new();
        for (indices, [red, green, blue]) in FACES {
            let face =
                indices.map(|index| corners.get(index).copied().unwrap_or(v3(0.0, 0.0, 0.0)));
            let [a, b, _, d] = face;
            let normal = (d - a).cross(b - a).normalized();
            let center = face
                .iter()
                .fold(v3(0.0, 0.0, 0.0), |sum, corner| sum + *corner * 0.25);
            let to_eye = v3(0.0, 0.0, -camera.distance) - center;
            if normal.dot(to_eye) <= 0.0 {
                continue;
            }
            let lit = AMBIENT + (1.0 - AMBIENT) * normal.dot(sun).max(0.0);
            let points: Option<Vec<P2>> = face
                .iter()
                .map(|corner| camera.project(*corner).map(|(point, _)| point))
                .collect();
            if let Some(points) = points {
                visible.push((center.z, points, scaled(srgb(red, green, blue), lit)));
            }
        }
        visible.sort_by(|a, b| b.0.total_cmp(&a.0));
        for (_, points, color) in &visible {
            light.polygon(points, *color, Blend::Over);
            for (from, to) in points.iter().zip(points.iter().cycle().skip(1)) {
                light.line(*from, *to, 1.5, scaled(*color, 1.8));
            }
        }
        light.bloom(0.8, 0.6);
    }
}

#[cfg(test)]
#[path = "support/artifact.rs"]
mod artifact;

#[cfg(test)]
mod tests {
    use crate::artifact;

    use katui_game::{Game, HeldKeys, Key, Light};
    use std::time::Instant;

    use super::{Cube, corners};

    #[test]
    fn the_corners_span_a_unit_cube_once_each() {
        let corners = corners();
        for (index, corner) in corners.iter().enumerate() {
            for other in &corners[index + 1..] {
                assert!(
                    (*corner - *other).dot(*corner - *other) > 0.5,
                    "corners differ"
                );
            }
            assert!(
                [corner.x, corner.y, corner.z]
                    .iter()
                    .all(|axis| axis.abs() == 1.0)
            );
        }
    }

    #[test]
    fn the_cube_turns_shows_three_faces_and_steers_with_arrows() {
        let mut cube = Cube::default();
        let mut light = Light::new(640, 360, 1.0);
        let idle = HeldKeys::default();
        for _ in 0..40 {
            cube.step(&idle, 1.0 / 60.0);
        }
        cube.render(&mut light);
        artifact::save_frame(&light, "cube");
        let surface = light.to_surface();
        let center = surface
            .get(katui_scene::surface::PixelPos::new(320, 180))
            .expect("inside the screen");
        let corner = surface
            .get(katui_scene::surface::PixelPos::new(2, 2))
            .expect("inside the screen");
        assert_ne!(
            center, corner,
            "the cube covers the middle, the sky the corner"
        );
        let yaw = cube.yaw;
        let mut steering = HeldKeys::default();
        steering.press(Key::Right, Instant::now());
        cube.step(&steering, 1.0 / 60.0);
        assert!(
            cube.yaw - yaw > super::BASE_SPIN.x / 60.0,
            "holding right spins faster"
        );
    }
}
