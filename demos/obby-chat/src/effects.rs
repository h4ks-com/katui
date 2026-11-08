//! Small animations that play on one message: a flame front that runs
//! along it when it lands, sparks when someone reacts. Each runs in its
//! own low-resolution image with transparency around it.

use std::collections::HashMap;
use std::time::Instant;

use katui_scene::placement::{CellRect, ImageId, Placement, PlacementId};

use crate::theme::{Tint, hue_tint};

const FIRE_SECONDS: f32 = 1.5;
const SPARK_SECONDS: f32 = 0.85;
const SPARK_COUNT: usize = 70;
const MAX_COLUMNS: u16 = 120;
const FIRE_PIXELS_PER_COLUMN: u32 = 3;
const FIRE_PIXELS_PER_ROW: u32 = 6;
const SPARK_PIXELS_PER_COLUMN: u32 = 3;
const SPARK_PIXELS_PER_ROW: u32 = 6;
const FLAME_ROWS_ABOVE: u16 = 2;
const FIRST_IMAGE: u32 = 3_000_000;
const EFFECT_Z: i32 = 30_000;
/// The part of a fire's life its front spends crossing the message.
const FUSE_SHARE: f32 = 0.55;
/// Where the flames sit behind the front and how wide they spread, in
/// fractions of the run.
const FLAME_LAG: f32 = 0.1;
const FLAME_SPREAD: f32 = 0.17;
const SPARK_GRAVITY: f32 = 95.0;

/// Which animation to play.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Fire,
    Sparks,
}

impl Kind {
    fn seconds(self) -> f32 {
        match self {
            Self::Fire => FIRE_SECONDS,
            Self::Sparks => SPARK_SECONDS,
        }
    }

    fn pixels_per_cell(self) -> (u32, u32) {
        match self {
            Self::Fire => (FIRE_PIXELS_PER_COLUMN, FIRE_PIXELS_PER_ROW),
            Self::Sparks => (SPARK_PIXELS_PER_COLUMN, SPARK_PIXELS_PER_ROW),
        }
    }
}

struct Spark {
    x: f32,
    y: f32,
    speed_x: f32,
    speed_y: f32,
    heat: f32,
}

struct Effect {
    key: u64,
    kind: Kind,
    tint: Tint,
    columns: u16,
    born: Instant,
    last_step: Instant,
    width: u32,
    height: u32,
    sparks: Vec<Spark>,
    random: u32,
}

/// One finished animation frame, ready to place.
pub struct Drawn {
    pub placement: Placement,
    pub pixels: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// The animations currently playing.
#[derive(Default)]
pub struct Effects {
    active: Vec<Effect>,
    frames_drawn: u32,
}

impl Effects {
    /// Play `kind` on the layer `key` across its first `columns` cells,
    /// colored by `hue`.
    pub fn spawn(&mut self, key: u64, kind: Kind, hue: f32, columns: u16) {
        let now = Instant::now();
        self.active
            .retain(|effect| effect.key != key || effect.kind != kind);
        self.active.push(Effect {
            key,
            kind,
            tint: hue_tint(hue, 0.95, 0.6),
            columns,
            born: now,
            last_step: now,
            width: 0,
            height: 0,
            sparks: Vec::new(),
            random: (key as u32).wrapping_mul(0x9e37_79b9) | 1,
        });
    }

    pub fn is_animating(&self) -> bool {
        !self.active.is_empty()
    }

    /// Advance everything to `now` and draw each effect whose message is
    /// on screen. `areas` says where each layer sits.
    pub fn frames(&mut self, areas: &HashMap<u64, CellRect>, now: Instant) -> Vec<Drawn> {
        self.active
            .retain(|effect| now.duration_since(effect.born).as_secs_f32() < effect.kind.seconds());
        let mut drawn = Vec::with_capacity(self.active.len());
        for effect in &mut self.active {
            let Some(area) = areas.get(&effect.key).copied() else {
                continue;
            };
            let region = effect_region(area, effect.columns);
            effect.fit(region);
            let pixels = effect.step(now);
            self.frames_drawn = self.frames_drawn.wrapping_add(1);
            drawn.push(Drawn {
                placement: Placement {
                    image: ImageId(FIRST_IMAGE + self.frames_drawn % 900_000),
                    id: PlacementId(1),
                    area: region,
                    crop: None,
                    z: EFFECT_Z,
                },
                pixels,
                width: effect.width,
                height: effect.height,
            });
        }
        drawn
    }
}

/// The cells an effect covers: the message's text and headroom for flames.
fn effect_region(area: CellRect, columns: u16) -> CellRect {
    let above = FLAME_ROWS_ABOVE.min(area.y);
    CellRect {
        x: area.x,
        y: area.y - above,
        columns: area.columns.min(columns).clamp(1, MAX_COLUMNS),
        rows: (area.rows + above).max(1),
    }
}

impl Effect {
    fn fit(&mut self, region: CellRect) {
        let (per_column, per_row) = self.kind.pixels_per_cell();
        let width = u32::from(region.columns) * per_column;
        let height = u32::from(region.rows) * per_row;
        if (width, height) == (self.width, self.height) {
            return;
        }
        self.width = width;
        self.height = height;
        self.sparks.clear();
        if self.kind == Kind::Sparks {
            self.launch_sparks();
        }
    }

    fn next_random(&mut self) -> u32 {
        self.random ^= self.random << 13;
        self.random ^= self.random >> 17;
        self.random ^= self.random << 5;
        self.random
    }

    fn launch_sparks(&mut self) {
        let origin_x = (self.width as f32 * 0.2).min(90.0);
        let origin_y = self.height as f32 * 0.72;
        for _ in 0..SPARK_COUNT {
            let angle = (self.next_random() % 628) as f32 / 100.0;
            let force = 22.0 + (self.next_random() % 95) as f32;
            let heat = 0.55 + (self.next_random() % 45) as f32 / 100.0;
            self.sparks.push(Spark {
                x: origin_x,
                y: origin_y,
                speed_x: angle.cos() * force * 1.9,
                speed_y: angle.sin() * force - 42.0,
                heat,
            });
        }
    }

    fn step(&mut self, now: Instant) -> Vec<u8> {
        let age = now.duration_since(self.born).as_secs_f32();
        let delta = now.duration_since(self.last_step).as_secs_f32();
        self.last_step = now;
        match self.kind {
            Kind::Fire => self.fire_pixels(age),
            Kind::Sparks => {
                self.move_sparks(delta);
                self.spark_pixels(age / SPARK_SECONDS)
            }
        }
    }

    fn fire_pixels(&self, age: f32) -> Vec<u8> {
        let life = age / FIRE_SECONDS;
        let front = (life / FUSE_SHARE).min(1.3);
        let fade = ((1.0 - life) * 3.0).min(1.0);
        let mut pixels = Vec::with_capacity((self.width * self.height * 4) as usize);
        for row in 0..self.height {
            let rise = 1.0 - row as f32 / self.height as f32;
            for column in 0..self.width {
                let along = column as f32 / self.width as f32;
                let heat = flame_heat(along, rise, front, age, self.width as f32);
                pixels.extend_from_slice(&flame_color(heat, self.tint, fade));
            }
        }
        pixels
    }

    fn move_sparks(&mut self, delta: f32) {
        for spark in &mut self.sparks {
            spark.x += spark.speed_x * delta;
            spark.y += spark.speed_y * delta;
            spark.speed_y += SPARK_GRAVITY * delta;
            spark.speed_x *= 1.0 - 1.6 * delta;
        }
    }

    fn spark_pixels(&self, life: f32) -> Vec<u8> {
        let mut pixels = vec![0; (self.width * self.height * 4) as usize];
        let fade = ((1.0 - life) * 2.4).min(1.0);
        for spark in &self.sparks {
            let glow = spark.heat * fade;
            for (offset_x, offset_y, weight) in [
                (0, 0, 1.0),
                (1, 0, 0.75),
                (-1, 0, 0.75),
                (0, 1, 0.75),
                (0, -1, 0.75),
            ] {
                self.plot(
                    &mut pixels,
                    spark.x as i64 + offset_x,
                    spark.y as i64 + offset_y,
                    glow * weight,
                );
            }
            for step in 1..5 {
                let back = step as f32 * 0.018;
                self.plot(
                    &mut pixels,
                    (spark.x - spark.speed_x * back) as i64,
                    (spark.y - spark.speed_y * back) as i64,
                    glow * (0.55 - step as f32 * 0.1),
                );
            }
        }
        pixels
    }

    fn plot(&self, pixels: &mut [u8], x: i64, y: i64, glow: f32) {
        if x < 0 || y < 0 || x >= i64::from(self.width) || y >= i64::from(self.height) {
            return;
        }
        let at = (y as usize * self.width as usize + x as usize) * 4;
        let color = flame_color(glow, self.tint, 1.0);
        if color[3] >= pixels[at + 3] {
            pixels[at..at + 4].copy_from_slice(&color);
        }
    }
}

/// The flame's intensity at a point: `along` runs across the message and
/// `rise` up from its base, both from 0 to 1. Burning sits just behind the
/// `front`, shaped by noise that scrolls upward.
fn flame_heat(along: f32, rise: f32, front: f32, age: f32, width: f32) -> f32 {
    let distance = (along - (front - FLAME_LAG)) / FLAME_SPREAD;
    let burning = (-distance * distance).exp();
    if burning < 0.03 {
        return 0.0;
    }
    let scale = width / 14.0;
    let tongue = noise(along * scale, age * 3.2);
    let height = burning * (0.22 + tongue * tongue * 1.5);
    let body = 1.0 - rise / height.max(0.001);
    if body <= 0.0 {
        return 0.0;
    }
    let grain = noise(along * scale * 2.5, rise * 8.0 - age * 11.0);
    (body * (0.55 + grain * 0.75)).min(1.0)
}

/// Smooth value noise in about 0 to 1.
fn noise(x: f32, y: f32) -> f32 {
    let (cell_x, cell_y) = (x.floor(), y.floor());
    let (fraction_x, fraction_y) = (smooth(x - cell_x), smooth(y - cell_y));
    let corner = |offset_x: f32, offset_y: f32| lattice(cell_x + offset_x, cell_y + offset_y);
    let top = mix(corner(0.0, 0.0), corner(1.0, 0.0), fraction_x);
    let bottom = mix(corner(0.0, 1.0), corner(1.0, 1.0), fraction_x);
    mix(top, bottom, fraction_y)
}

fn smooth(value: f32) -> f32 {
    value * value * (3.0 - 2.0 * value)
}

fn mix(from: f32, to: f32, amount: f32) -> f32 {
    from + (to - from) * amount
}

fn lattice(x: f32, y: f32) -> f32 {
    let mixed = (x as i32 as u32)
        .wrapping_mul(0x9e37_79b1)
        .wrapping_add((y as i32 as u32).wrapping_mul(0x85eb_ca77));
    let scrambled = (mixed ^ (mixed >> 15)).wrapping_mul(0xc2b2_ae3d);
    ((scrambled ^ (scrambled >> 13)) % 1000) as f32 / 1000.0
}

/// Transparent where cool, then the sender's color at the tips, then fire,
/// then white at the hottest.
fn flame_color(heat: f32, tint: Tint, fade: f32) -> [u8; 4] {
    if heat < 0.05 {
        return [0, 0, 0, 0];
    }
    let (red, green, blue) = if heat < 0.4 {
        (tint.red, tint.green, tint.blue)
    } else if heat < 0.72 {
        let amount = (heat - 0.4) / 0.32;
        (
            mix(tint.red, 255.0, amount),
            mix(tint.green, 140.0, amount),
            mix(tint.blue, 35.0, amount),
        )
    } else {
        let amount = (heat - 0.72) / 0.28;
        (255.0, mix(140.0, 245.0, amount), mix(35.0, 215.0, amount))
    };
    let alpha = (heat / 0.5).min(1.0).powf(1.3) * 245.0 * fade;
    [red as u8, green as u8, blue as u8, alpha as u8]
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::time::{Duration, Instant};

    use katui_scene::placement::CellRect;

    use super::{Effects, Kind, flame_heat};

    const MESSAGE: u64 = 42;

    fn areas() -> HashMap<u64, CellRect> {
        let mut map = HashMap::new();
        map.insert(
            MESSAGE,
            CellRect {
                x: 10,
                y: 20,
                columns: 100,
                rows: 4,
            },
        );
        map
    }

    fn lit_pixels(pixels: &[u8]) -> usize {
        pixels
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|pixel| pixel[3] > 0)
            .count()
    }

    #[test]
    fn an_effect_covers_only_the_text_and_a_little_headroom() {
        let mut effects = Effects::default();
        effects.spawn(MESSAGE, Kind::Fire, 0.3, 40);
        let drawn = effects.frames(&areas(), Instant::now());
        assert_eq!(
            drawn[0].placement.area,
            CellRect {
                x: 10,
                y: 18,
                columns: 40,
                rows: 6
            }
        );
    }

    #[test]
    fn nothing_burns_ahead_of_the_front() {
        assert_eq!(flame_heat(0.9, 0.05, 0.3, 0.5, 200.0), 0.0);
    }

    #[test]
    fn flames_thin_out_toward_the_top() {
        let low: f32 = (0..40)
            .map(|x| flame_heat(x as f32 / 100.0, 0.05, 0.45, 0.5, 200.0))
            .sum();
        let high: f32 = (0..40)
            .map(|x| flame_heat(x as f32 / 100.0, 0.8, 0.45, 0.5, 200.0))
            .sum();
        assert!(low > high * 2.0, "{low} near the base, {high} at the top");
    }

    #[test]
    fn the_fire_spreads_as_the_front_advances() {
        let mut effects = Effects::default();
        effects.spawn(MESSAGE, Kind::Fire, 0.3, 80);
        let start = Instant::now() + Duration::from_millis(40);
        let early = effects.frames(&areas(), start).remove(0);
        let later = effects
            .frames(&areas(), start + Duration::from_millis(350))
            .remove(0);
        assert!(lit_pixels(&later.pixels) > lit_pixels(&early.pixels));
    }

    #[test]
    fn effects_end_and_a_scrolled_away_message_draws_nothing() {
        let mut effects = Effects::default();
        effects.spawn(MESSAGE, Kind::Sparks, 0.1, 40);
        let now = Instant::now();
        assert!(effects.frames(&HashMap::new(), now).is_empty());
        assert!(effects.is_animating(), "it keeps its clock while hidden");
        let _ = effects.frames(&areas(), now + Duration::from_secs(5));
        assert!(!effects.is_animating());
    }
}
