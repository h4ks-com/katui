//! Small animations that play on one message: it pops into existence when
//! it lands, and throws sparks when someone reacts. Each runs in its own
//! low-resolution image with transparency around it.

use std::collections::HashMap;
use std::time::Instant;

use katui_scene::placement::{CellRect, ImageId, Placement, PlacementId};
use katui_scene::surface::{PixelPos, Surface};
use katui_ui::CellPixels;

use crate::theme::{Tint, hue_tint};

const POP_SECONDS: f32 = 0.55;
const SPARK_SECONDS: f32 = 0.85;
const SPARK_COUNT: usize = 70;
const MAX_COLUMNS: u16 = 120;
const SPARK_PIXELS_PER_COLUMN: u32 = 3;
const SPARK_PIXELS_PER_ROW: u32 = 6;
const SPARK_GRAVITY: f32 = 95.0;
const SPARK_ROWS_ABOVE: u16 = 2;
const FIRST_IMAGE: u32 = 3_000_000;
const EFFECT_Z: i32 = 30_000;
/// We render pop frames at this fraction of a cell's pixels because the
/// motion hides the lower resolution.
const POP_RESOLUTION_DIVISOR: u32 = 2;
const POP_MARGIN_COLUMNS: u16 = 5;
const POP_MARGIN_ROWS: u16 = 1;
/// Where the message scales from, in cells: about its avatar.
const POP_PIVOT_COLUMN: f32 = 5.0;
/// The share of the pop spent materializing from speckles.
const MATERIALIZE_SHARE: f32 = 0.55;
/// How much of the noise range glows at the materializing edge.
const EDGE_BAND: f32 = 0.16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Pop,
    Sparks,
}

impl Kind {
    fn seconds(self) -> f32 {
        match self {
            Self::Pop => POP_SECONDS,
            Self::Sparks => SPARK_SECONDS,
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
    source: Option<Surface>,
}

/// One finished animation frame, ready to place.
pub struct Drawn {
    pub placement: Placement,
    pub pixels: Vec<u8>,
    pub width: u32,
    pub height: u32,
    /// The cells of the message this frame stands in for while it plays.
    pub hides: Option<CellRect>,
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
            tint: hue_tint(hue, 0.95, 0.65),
            columns,
            born: now,
            last_step: now,
            width: 0,
            height: 0,
            sparks: Vec::new(),
            random: (key as u32).wrapping_mul(0x9e37_79b9) | 1,
            source: None,
        });
    }

    pub fn is_animating(&self) -> bool {
        !self.active.is_empty()
    }

    pub fn clear(&mut self) {
        self.active.clear();
    }

    /// Advance everything to `now` and draw each effect whose message is
    /// on screen. `areas` says where each layer sits and `layer` returns
    /// its pixels.
    pub fn frames(
        &mut self,
        areas: &HashMap<u64, CellRect>,
        layer: impl Fn(u64) -> Option<Surface>,
        cell: CellPixels,
        now: Instant,
    ) -> Vec<Drawn> {
        self.active
            .retain(|effect| now.duration_since(effect.born).as_secs_f32() < effect.kind.seconds());
        let mut drawn = Vec::with_capacity(self.active.len());
        for effect in &mut self.active {
            let Some(area) = areas.get(&effect.key).copied() else {
                continue;
            };
            if effect.kind == Kind::Pop && effect.source.is_none() {
                effect.source = layer(effect.key);
            }
            let region = effect.region(area);
            effect.fit(region, cell);
            let pixels = effect.step(now, area, region, cell);
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
                hides: (effect.kind == Kind::Pop).then_some(area),
            });
        }
        drawn
    }
}

impl Effect {
    fn region(&self, area: CellRect) -> CellRect {
        let columns = area.columns.min(self.columns).clamp(1, MAX_COLUMNS);
        match self.kind {
            Kind::Pop => {
                let above = POP_MARGIN_ROWS.min(area.y);
                CellRect {
                    x: area.x,
                    y: area.y - above,
                    columns: columns.saturating_add(POP_MARGIN_COLUMNS),
                    rows: area.rows + above + POP_MARGIN_ROWS,
                }
            }
            Kind::Sparks => {
                let above = SPARK_ROWS_ABOVE.min(area.y);
                CellRect {
                    x: area.x,
                    y: area.y - above,
                    columns,
                    rows: (area.rows + above).max(1),
                }
            }
        }
    }

    fn fit(&mut self, region: CellRect, cell: CellPixels) {
        let (per_column, per_row) = match self.kind {
            Kind::Pop => (
                (u32::from(cell.width) / POP_RESOLUTION_DIVISOR).max(1),
                (u32::from(cell.height) / POP_RESOLUTION_DIVISOR).max(1),
            ),
            Kind::Sparks => (SPARK_PIXELS_PER_COLUMN, SPARK_PIXELS_PER_ROW),
        };
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

    fn step(
        &mut self,
        now: Instant,
        area: CellRect,
        region: CellRect,
        cell: CellPixels,
    ) -> Vec<u8> {
        let age = now.duration_since(self.born).as_secs_f32();
        let delta = now.duration_since(self.last_step).as_secs_f32();
        self.last_step = now;
        match self.kind {
            Kind::Pop => self.pop_pixels(age / POP_SECONDS, area, region, cell),
            Kind::Sparks => {
                self.move_sparks(delta);
                self.spark_pixels(age / SPARK_SECONDS)
            }
        }
    }

    fn pop_pixels(&self, life: f32, area: CellRect, region: CellRect, cell: CellPixels) -> Vec<u8> {
        let mut pixels = vec![0; (self.width * self.height * 4) as usize];
        let Some(source) = &self.source else {
            return pixels;
        };
        let (cell_width, cell_height) = (f32::from(cell.width), f32::from(cell.height));
        let to_screen = (
            f32::from(region.columns) * cell_width / self.width as f32,
            f32::from(region.rows) * cell_height / self.height as f32,
        );
        let top_offset = f32::from(area.y - region.y) * cell_height;
        let pivot = (
            POP_PIVOT_COLUMN * cell_width,
            f32::from(area.rows) * cell_height * 0.5,
        );
        let scale = spring(life);
        let progress = (life / MATERIALIZE_SHARE).min(1.0);
        let flash = (-life * 5.5).exp();
        for (index, pixel) in pixels.as_chunks_mut::<4>().0.iter_mut().enumerate() {
            let column = index as u32 % self.width;
            let row = index as u32 / self.width;
            let screen_x = column as f32 * to_screen.0;
            let screen_y = row as f32 * to_screen.1 - top_offset;
            let layer_x = (screen_x - pivot.0) / scale + pivot.0;
            let layer_y = (screen_y - pivot.1) / scale + pivot.1;
            let speckle = lattice((column / 2) as f32, (row / 2) as f32);
            *pixel = pop_color(
                sample(source, layer_x, layer_y),
                speckle,
                progress,
                flash,
                self.tint,
            );
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
        let color = spark_color(glow, self.tint);
        if color[3] >= pixels[at + 3] {
            pixels[at..at + 4].copy_from_slice(&color);
        }
    }
}

/// The message's size through its life: it starts small, overshoots past
/// full size and settles.
fn spring(life: f32) -> f32 {
    1.0 - 0.34 * (-life * 6.5).exp() * (life * 13.0).cos()
}

/// The layer's color at (`x`, `y`), transparent outside it.
fn sample(source: &Surface, x: f32, y: f32) -> [f32; 4] {
    if x < 0.0 || y < 0.0 {
        return [0.0; 4];
    }
    source
        .get(PixelPos::new(x as u32, y as u32))
        .map_or([0.0; 4], |color| {
            [
                f32::from(color.r),
                f32::from(color.g),
                f32::from(color.b),
                f32::from(color.a),
            ]
        })
}

fn pop_color(sampled: [f32; 4], speckle: f32, progress: f32, flash: f32, tint: Tint) -> [u8; 4] {
    let [red, green, blue, alpha] = sampled;
    if alpha <= 0.0 || speckle > progress {
        return [0, 0, 0, 0];
    }
    let edge = (1.0 - (progress - speckle) / EDGE_BAND).max(0.0);
    let glow = (edge + flash * 0.65).min(1.0);
    [
        mix(red, tint.red.max(220.0), glow) as u8,
        mix(green, tint.green.max(190.0), glow) as u8,
        mix(blue, tint.blue.max(170.0), glow) as u8,
        alpha as u8,
    ]
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

fn spark_color(glow: f32, tint: Tint) -> [u8; 4] {
    if glow < 0.05 {
        return [0, 0, 0, 0];
    }
    let (red, green, blue) = if glow < 0.4 {
        (tint.red, tint.green, tint.blue)
    } else if glow < 0.72 {
        let amount = (glow - 0.4) / 0.32;
        (
            mix(tint.red, 255.0, amount),
            mix(tint.green, 140.0, amount),
            mix(tint.blue, 35.0, amount),
        )
    } else {
        let amount = (glow - 0.72) / 0.28;
        (255.0, mix(140.0, 245.0, amount), mix(35.0, 215.0, amount))
    };
    let alpha = (glow / 0.5).min(1.0).powf(1.3) * 245.0;
    [red as u8, green as u8, blue as u8, alpha as u8]
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::time::{Duration, Instant};

    use katui_scene::placement::CellRect;
    use katui_scene::surface::Surface;
    use katui_ui::CellPixels;

    use super::{Effects, Kind, spring};

    const MESSAGE: u64 = 42;
    const CELL: CellPixels = CellPixels {
        width: 10,
        height: 20,
    };

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

    fn layer(_: u64) -> Option<Surface> {
        Surface::from_rgba(1000, 80, [200, 200, 200, 255].repeat(80_000))
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
    fn the_spring_starts_small_overshoots_and_settles() {
        assert!(spring(0.0) < 0.7);
        assert!(
            (0..100).any(|step| spring(step as f32 / 100.0) > 1.02),
            "it overshoots"
        );
        assert!((spring(1.0) - 1.0).abs() < 0.01, "it settles");
    }

    #[test]
    fn a_pop_covers_only_its_message_and_stands_in_for_it() {
        let mut effects = Effects::default();
        effects.spawn(MESSAGE, Kind::Pop, 0.3, 40);
        let drawn = effects.frames(&areas(), layer, CELL, Instant::now());
        assert_eq!(
            drawn[0].placement.area,
            CellRect {
                x: 10,
                y: 19,
                columns: 45,
                rows: 6
            }
        );
        assert_eq!(drawn[0].hides, areas().get(&MESSAGE).copied());
    }

    #[test]
    fn a_pop_materializes_over_time() {
        let mut effects = Effects::default();
        effects.spawn(MESSAGE, Kind::Pop, 0.3, 40);
        let start = Instant::now();
        let early = effects
            .frames(&areas(), layer, CELL, start + Duration::from_millis(30))
            .remove(0);
        let later = effects
            .frames(&areas(), layer, CELL, start + Duration::from_millis(300))
            .remove(0);
        assert!(lit_pixels(&later.pixels) > lit_pixels(&early.pixels) * 2);
    }

    #[test]
    fn sparks_never_hide_the_message() {
        let mut effects = Effects::default();
        effects.spawn(MESSAGE, Kind::Sparks, 0.1, 40);
        let drawn = effects.frames(&areas(), layer, CELL, Instant::now());
        assert_eq!(drawn[0].hides, None);
    }

    #[test]
    fn effects_end_and_a_scrolled_away_message_draws_nothing() {
        let mut effects = Effects::default();
        effects.spawn(MESSAGE, Kind::Sparks, 0.1, 40);
        let now = Instant::now();
        assert!(effects.frames(&HashMap::new(), layer, CELL, now).is_empty());
        assert!(effects.is_animating(), "it keeps its clock while hidden");
        let _ = effects.frames(&areas(), layer, CELL, now + Duration::from_secs(5));
        assert!(!effects.is_animating());
    }
}
