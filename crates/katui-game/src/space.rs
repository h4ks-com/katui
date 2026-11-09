#![allow(clippy::cast_precision_loss)]

use std::ops::{Add, Mul, Sub};

use crate::light::{Light, P2, Rgb};

const MAX_SPARKS: usize = 1600;

/// A point or direction in world space.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct V3 {
    /// Across.
    pub x: f32,
    /// Up or down, as the game chooses.
    pub y: f32,
    /// Depth.
    pub z: f32,
}

/// A vector from its parts.
#[must_use]
pub const fn v3(x: f32, y: f32, z: f32) -> V3 {
    V3 { x, y, z }
}

impl Add for V3 {
    type Output = Self;
    fn add(self, other: Self) -> Self {
        v3(self.x + other.x, self.y + other.y, self.z + other.z)
    }
}

impl Sub for V3 {
    type Output = Self;
    fn sub(self, other: Self) -> Self {
        v3(self.x - other.x, self.y - other.y, self.z - other.z)
    }
}

impl Mul<f32> for V3 {
    type Output = Self;
    fn mul(self, scale: f32) -> Self {
        v3(self.x * scale, self.y * scale, self.z * scale)
    }
}

impl V3 {
    #[must_use]
    pub fn dot(self, other: Self) -> f32 {
        self.x * other.x + self.y * other.y + self.z * other.z
    }

    #[must_use]
    pub fn cross(self, other: Self) -> Self {
        v3(
            self.y * other.z - self.z * other.y,
            self.z * other.x - self.x * other.z,
            self.x * other.y - self.y * other.x,
        )
    }

    /// This direction at unit length.
    #[must_use]
    pub fn normalized(self) -> Self {
        self * (1.0 / self.dot(self).sqrt().max(1e-5))
    }

    /// Turned around the x axis.
    #[must_use]
    pub fn rotated_x(self, angle: f32) -> Self {
        let (sin, cos) = angle.sin_cos();
        v3(
            self.x,
            self.y * cos - self.z * sin,
            self.y * sin + self.z * cos,
        )
    }

    /// Turned around the y axis.
    #[must_use]
    pub fn rotated_y(self, angle: f32) -> Self {
        let (sin, cos) = angle.sin_cos();
        v3(
            self.x * cos + self.z * sin,
            self.y,
            self.z * cos - self.x * sin,
        )
    }

    /// Turned around the z axis.
    #[must_use]
    pub fn rotated_z(self, angle: f32) -> Self {
        let (sin, cos) = angle.sin_cos();
        v3(
            self.x * cos - self.y * sin,
            self.x * sin + self.y * cos,
            self.z,
        )
    }
}

/// A pinhole looking down +z, with y up.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Camera {
    /// Where the optical axis meets the screen.
    pub center: P2,
    /// Pixels of screen height per unit of world height at unit depth.
    pub focal: f32,
    /// How far the eye sits behind z = 0.
    pub distance: f32,
    /// One pixel's width over its height.
    pub pixel_aspect: f32,
}

impl Camera {
    /// Where `at` lands on screen and how many pixels one world unit
    /// spans there; `None` behind the eye.
    #[must_use]
    pub fn project(&self, at: V3) -> Option<(P2, f32)> {
        let depth = at.z + self.distance;
        if depth <= 0.1 {
            return None;
        }
        let scale = self.focal / depth;
        Some((
            P2::new(
                self.center.x + at.x * scale / self.pixel_aspect,
                self.center.y - at.y * scale,
            ),
            scale,
        ))
    }
}

/// A small deterministic generator, so that games stay replayable in tests.
#[derive(Debug, Clone)]
pub struct Rng(u32);

impl Rng {
    /// A generator started from `seed`.
    #[must_use]
    pub fn new(seed: u32) -> Self {
        Self(seed | 1)
    }

    /// The next raw value.
    pub fn next_u32(&mut self) -> u32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        self.0
    }

    /// A value in `low..high`.
    pub fn range(&mut self, low: f32, high: f32) -> f32 {
        let unit = (self.next_u32() >> 8) as f32 / (1 << 24) as f32;
        low + (high - low) * unit
    }

    /// A random direction of unit length.
    pub fn direction(&mut self) -> V3 {
        v3(
            self.range(-1.0, 1.0),
            self.range(-1.0, 1.0),
            self.range(-1.0, 1.0),
        )
        .normalized()
    }
}

/// One glowing particle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Spark {
    pub at: V3,
    /// In world units per second.
    pub velocity: V3,
    pub color: Rgb,
    /// Seconds lived.
    pub age: f32,
    /// Seconds it lasts.
    pub life: f32,
    /// Line width in pixels.
    pub width: f32,
}

/// Every live [`Spark`].
#[derive(Debug, Default)]
pub struct Sparks {
    live: Vec<Spark>,
}

impl Sparks {
    /// Add `spark` unless the pool is full.
    pub fn add(&mut self, spark: Spark) {
        if self.live.len() < MAX_SPARKS {
            self.live.push(spark);
        }
    }

    /// Age and move every spark, pulling each by `gravity`.
    pub fn step(&mut self, dt: f32, gravity: V3) {
        self.live.retain_mut(|spark| {
            spark.age += dt;
            spark.velocity = spark.velocity + gravity * dt;
            spark.at = spark.at + spark.velocity * dt;
            spark.age < spark.life
        });
    }

    /// Draw every spark as a fading streak, placing it with `place`.
    pub fn draw(&self, light: &mut Light, place: impl Fn(V3) -> Option<P2>) {
        for spark in &self.live {
            let fade = 1.0 - spark.age / spark.life;
            let tail = spark.at - spark.velocity * 0.02;
            if let (Some(head), Some(tail)) = (place(spark.at), place(tail)) {
                let color = spark.color.map(|channel| channel * fade * fade);
                light.line(head, tail, spark.width, color);
            }
        }
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.live.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.live.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::{Camera, Rng, Spark, Sparks, V3, v3};
    use crate::light::P2;

    #[test]
    fn nearer_points_land_farther_from_the_center() {
        let camera = Camera {
            center: P2::new(100.0, 100.0),
            focal: 100.0,
            distance: 10.0,
            pixel_aspect: 1.0,
        };
        let near = camera.project(v3(1.0, 0.0, 0.0)).expect("in front");
        let far = camera.project(v3(1.0, 0.0, 10.0)).expect("in front");
        assert!(near.0.x > far.0.x);
        assert!(camera.project(v3(0.0, 0.0, -20.0)).is_none());
    }

    #[test]
    fn a_rotation_keeps_length() {
        let turned = v3(1.0, 2.0, 3.0)
            .rotated_x(0.7)
            .rotated_y(1.1)
            .rotated_z(2.3);
        assert!((turned.dot(turned) - 14.0).abs() < 1e-3);
    }

    #[test]
    fn the_generator_stays_in_range() {
        let mut rng = Rng::new(9);
        for _ in 0..1000 {
            assert!((2.0..3.0).contains(&rng.range(2.0, 3.0)));
        }
    }

    #[test]
    fn sparks_die_of_age() {
        let mut sparks = Sparks::default();
        sparks.add(Spark {
            at: V3::default(),
            velocity: v3(1.0, 0.0, 0.0),
            color: [1.0; 3],
            age: 0.0,
            life: 0.5,
            width: 1.0,
        });
        sparks.step(0.25, V3::default());
        assert_eq!(sparks.len(), 1);
        sparks.step(0.25, V3::default());
        assert!(sparks.is_empty());
    }
}
