//! Life: Conway's game of life over a slowly breathing nebula, cells born
//! white-hot cooling through gold and fire into ash that fades into the
//! background. A stagnant universe reseeds itself with a fresh bloom. Run
//! with `just demo-life`.
#![cfg(unix)]
// the runner drives /dev/tty through katui-rt's unix layer
// We wrap torus coordinates inside the grid before indexing, so every index is in bounds.
#![allow(
    clippy::indexing_slicing,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]

use std::time::{SystemTime, UNIX_EPOCH};

use katui_game::{Game, HeldKeys, Light, Rgb, Rng, play, srgb};

const GENERATION_SECONDS: f32 = 1.0 / 30.0;
const BREATH_SPEED: f32 = 0.12;
/// A universe with no births and no deaths for this many generations gets
/// a fresh nebula. Oscillators and gliders never trigger it.
const STALE_GENERATIONS: u32 = 40;
const EMBER_TOP: u8 = 14;
const EMBER_DECAY: u8 = 2;
const EMBER: [u8; 3] = [170, 55, 30];
const UNIVERSE_ROWS: u32 = 170;
const NEBULA_TOP_LEFT: [f32; 3] = [10.0, 12.0, 34.0];
const NEBULA_BOTTOM_RIGHT: [f32; 3] = [30.0, 12.0, 48.0];

const BUCKETS: [(u8, u8, u8); 7] = [
    (255, 255, 235),
    (255, 240, 170),
    (255, 210, 90),
    (255, 160, 50),
    (240, 95, 35),
    (200, 55, 40),
    (150, 40, 60),
];

fn main() -> Result<(), katui_rt::RtError> {
    play(LifeDemo::new(clock_seed()))
}

fn clock_seed() -> u32 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0x5eed_1234, |passed| passed.as_nanos() as u32)
}

/// How a universe sits on the light buffer: each life cell covers
/// `cell_width` by `cell_height` light pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Grid {
    cell_width: u32,
    cell_height: u32,
    columns: u32,
    rows: u32,
}

impl Grid {
    /// We widen each cell by the pixel aspect so life cells stay square on screen.
    fn fitting(light: &Light) -> Self {
        let cell_height = (light.height() / UNIVERSE_ROWS).max(1);
        let cell_width = ((cell_height as f32 / light.pixel_aspect()).round() as u32).max(1);
        Self {
            cell_width,
            cell_height,
            columns: (light.width() / cell_width).max(1),
            rows: (light.height() / cell_height).max(1),
        }
    }
}

struct LifeDemo {
    life: Life,
    grid: Grid,
    rng: Rng,
    clock: f32,
    banked: f32,
}

impl LifeDemo {
    fn new(seed: u32) -> Self {
        Self {
            life: Life::new(1, 1, seed),
            grid: Grid {
                cell_width: 1,
                cell_height: 1,
                columns: 1,
                rows: 1,
            },
            rng: Rng::new(seed),
            clock: 0.0,
            banked: 0.0,
        }
    }
}

impl Game for LifeDemo {
    fn resize(&mut self, light: &Light) {
        self.grid = Grid::fitting(light);
        self.life = Life::new(self.grid.columns, self.grid.rows, self.rng.next_u32());
    }

    fn step(&mut self, _keys: &HeldKeys, dt: f32) {
        self.clock += dt;
        self.banked += dt;
        while self.banked >= GENERATION_SECONDS {
            self.banked -= GENERATION_SECONDS;
            self.life.step();
            if self.life.stagnant() {
                self.life.reseed();
            }
        }
    }

    fn render(&mut self, light: &mut Light) {
        let pulse = (self.clock * BREATH_SPEED).sin() * 0.5 + 0.5;
        let colors = self.life.shade(pulse);
        let grid = self.grid;
        light.paint(|x, y| {
            let column = (x as u32 / grid.cell_width).min(grid.columns - 1);
            let row = (y as u32 / grid.cell_height).min(grid.rows - 1);
            colors[(row * grid.columns + column) as usize]
        });
    }
}

/// One universe: life, ash and the nebula under it.
struct Life {
    width: u32,
    height: u32,
    /// Alive pixels, 0 or 1, with a one-ghost border for wraparound.
    alive: Vec<u8>,
    /// Generations alive per pixel; zero is dead. Ages saturate.
    age: Vec<u8>,
    ash_heat: Vec<u8>,
    background: Vec<[u8; 3]>,
    generations_without_change: u32,
    rng: Rng,
}

impl Life {
    fn new(width: u32, height: u32, seed: u32) -> Self {
        let mut life = Self {
            width,
            height,
            alive: vec![0; ((width + 2) * (height + 2)) as usize],
            age: vec![0; (width * height) as usize],
            ash_heat: vec![0; (width * height) as usize],
            background: nebula(width, height, seed),
            generations_without_change: 0,
            rng: Rng::new(seed ^ 0x9e37_79b9),
        };
        life.reseed();
        life
    }

    fn stagnant(&self) -> bool {
        self.generations_without_change >= STALE_GENERATIONS
    }

    #[cfg(test)]
    fn index(&self, x: u32, y: u32) -> usize {
        (y * self.width + x) as usize
    }

    /// A fresh bloom: a few clouds of life at just under half density,
    /// coherent enough to grow structures that spit gliders.
    fn reseed(&mut self) {
        self.age.fill(0);
        self.ash_heat.fill(0);
        self.background = nebula(self.width, self.height, self.rng.next_u32());
        self.generations_without_change = 0;
        let clouds = (self.width * self.height / 22_000).clamp(2, 10);
        for _ in 0..clouds {
            let span = i64::from(14 + self.rng.next_u32() % 22);
            let center_x = i64::from(self.rng.next_u32() % self.width);
            let center_y = i64::from(self.rng.next_u32() % self.height);
            for dy in -span..=span {
                for dx in -span..=span {
                    if dx * dx + dy * dy > span * span || self.rng.next_u32() % 100 >= 45 {
                        continue;
                    }
                    let x = (center_x + dx).rem_euclid(i64::from(self.width)) as u32;
                    let y = (center_y + dy).rem_euclid(i64::from(self.height)) as u32;
                    self.alive[Self::ghost_index(self.width, x, y)] = 1;
                }
            }
        }
        self.absorb_ghost_border();
    }

    fn ghost_index(width: u32, x: u32, y: u32) -> usize {
        ((y + 1) * (width + 2) + x + 1) as usize
    }

    /// We copy the wrapped edges into the ghost border so every interior
    /// pixel counts neighbors without a modulo.
    fn absorb_ghost_border(&mut self) {
        let width = self.width as usize;
        let height = self.height as usize;
        let stride = width + 2;
        for x in 0..width {
            self.alive[x + 1] = self.alive[x + 1 + height * stride];
            self.alive[x + 1 + (height + 1) * stride] = self.alive[x + 1 + stride];
        }
        for y in 0..height {
            let row = (y + 1) * stride;
            self.alive[row] = self.alive[row + width];
            self.alive[row + width + 1] = self.alive[row + 1];
        }
        self.alive[0] = self.alive[height * stride + width];
        self.alive[width + 1] = self.alive[height * stride + 1];
        self.alive[(height + 1) * stride] = self.alive[stride + width];
        self.alive[(height + 1) * stride + width + 1] = self.alive[stride + 1];
    }

    /// One generation of B3/S23: the born light white-hot, survivors a
    /// generation older, the dead handed ash that then fades.
    fn step(&mut self) {
        let stride = (self.width + 2) as usize;
        self.ash_heat
            .iter_mut()
            .for_each(|heat| *heat = heat.saturating_sub(EMBER_DECAY));
        let mut next_alive = vec![0u8; self.alive.len()];
        let mut next_age = vec![0u8; self.age.len()];
        let mut churn = false;
        for y in 0..self.height as usize {
            let above = y * stride;
            let mid = above + stride;
            let below = mid + stride;
            for x in 0..self.width as usize {
                let neighbors = self.alive[above + x]
                    + self.alive[above + x + 1]
                    + self.alive[above + x + 2]
                    + self.alive[mid + x]
                    + self.alive[mid + x + 2]
                    + self.alive[below + x]
                    + self.alive[below + x + 1]
                    + self.alive[below + x + 2];
                let alive = self.alive[mid + x + 1] > 0;
                let inner = y * self.width as usize + x;
                if !alive && neighbors == 3 {
                    next_alive[mid + x + 1] = 1;
                    next_age[inner] = 1;
                    churn = true;
                } else if alive && matches!(neighbors, 2 | 3) {
                    next_alive[mid + x + 1] = 1;
                    next_age[inner] = self.age[inner].saturating_add(1);
                } else if alive {
                    self.ash_heat[inner] = EMBER_TOP;
                    churn = true;
                }
            }
        }
        self.alive = next_alive;
        self.age = next_age;
        self.absorb_ghost_border();
        self.generations_without_change = if churn {
            0
        } else {
            self.generations_without_change.saturating_add(1)
        };
    }

    /// The light of every cell: the nebula under additive heat. The glow
    /// breathes in coarse steps so unchanged cells keep their bytes and
    /// cost nothing to present.
    fn shade(&self, pulse: f32) -> Vec<Rgb> {
        let breath = 0.85 + 0.15 * ((pulse * 16.0).floor() / 16.0);
        let scale = |delta: (u8, u8, u8)| -> (u8, u8, u8) {
            let channel = |value: u8| (f32::from(value) * breath).round().min(255.0) as u8;
            (channel(delta.0), channel(delta.1), channel(delta.2))
        };
        let glow = BUCKETS.map(scale);
        let ember = scale((EMBER[0], EMBER[1], EMBER[2]));
        self.background
            .iter()
            .zip(self.age.iter().zip(&self.ash_heat))
            .map(|(&[red, green, blue], (&age, &heat))| {
                let (add_red, add_green, add_blue) = if age > 0 {
                    glow[bucket(age)]
                } else if heat > 0 {
                    ember
                } else {
                    (0, 0, 0)
                };
                srgb(
                    red.saturating_add(add_red),
                    green.saturating_add(add_green),
                    blue.saturating_add(add_blue),
                )
            })
            .collect()
    }

    #[cfg(test)]
    fn alive_cells(&self) -> Vec<(u32, u32)> {
        let mut cells = Vec::new();
        for y in 0..self.height {
            for x in 0..self.width {
                if self.age[self.index(x, y)] > 0 {
                    cells.push((x, y));
                }
            }
        }
        cells
    }
}

#[cfg(test)]
fn heat(age: u8) -> (u8, u8, u8) {
    BUCKETS[bucket(age)]
}

/// We use coarse buckets so an old cell settles to one color and stops costing bytes.
fn bucket(age: u8) -> usize {
    match age {
        1 => 0,
        2 => 1,
        3..=4 => 2,
        5..=9 => 3,
        10..=24 => 4,
        25..=60 => 5,
        _ => 6,
    }
}

/// Two hue anchors blended along the diagonal, dimmed toward the edges
/// and speckled dark enough for the heat to dominate.
fn nebula(width: u32, height: u32, seed: u32) -> Vec<[u8; 3]> {
    let mut rng = Rng::new(seed);
    let ([from_red, from_green, from_blue], [to_red, to_green, to_blue]) =
        (NEBULA_TOP_LEFT, NEBULA_BOTTOM_RIGHT);
    let mut field = Vec::with_capacity((width * height) as usize);
    for y in 0..height {
        for x in 0..width {
            let diagonal = f32::midpoint(x as f32 / width as f32, y as f32 / height as f32);
            let vx = x as f32 / width as f32 - 0.5;
            let vy = y as f32 / height as f32 - 0.5;
            let vignette = 1.0 - (vx * vx + vy * vy).sqrt() * 0.8;
            let speckle = (rng.next_u32() % 7) as f32;
            let shade =
                |from: f32, to: f32| ((from + (to - from) * diagonal) * vignette + speckle) as u8;
            field.push([
                shade(from_red, to_red),
                shade(from_green, to_green),
                shade(from_blue, to_blue),
            ]);
        }
    }
    field
}

#[cfg(test)]
#[path = "support/artifact.rs"]
mod artifact;

#[cfg(test)]
mod tests {
    use crate::artifact;

    use katui_game::{Game, HeldKeys, Light};

    use super::{EMBER, EMBER_TOP, Grid, Life, LifeDemo, STALE_GENERATIONS, heat};

    fn dish(columns: u32, rows: u32, alive: &[(u32, u32)]) -> Life {
        let mut life = Life::new(columns, rows, 7);
        life.age.fill(0);
        life.ash_heat.fill(0);
        life.alive.fill(0);
        life.generations_without_change = 0;
        for &(x, y) in alive {
            let index = life.index(x, y);
            life.age[index] = 1;
            life.alive[Life::ghost_index(columns, x, y)] = 1;
        }
        life.absorb_ghost_border();
        life
    }

    fn sorted(mut cells: Vec<(u32, u32)>) -> Vec<(u32, u32)> {
        cells.sort_unstable();
        cells
    }

    #[test]
    fn a_blinker_oscillates_and_ages() {
        let mut life = dish(9, 9, &[(3, 4), (4, 4), (5, 4)]);
        life.step();
        assert_eq!(sorted(life.alive_cells()), vec![(4, 3), (4, 4), (4, 5)]);
        life.step();
        assert_eq!(life.alive_cells().len(), 3);
        assert!(life.age[life.index(4, 4)] >= 3, "the center never dies");
    }

    #[test]
    fn a_block_is_still_and_becomes_stagnant() {
        let mut life = dish(10, 10, &[(4, 4), (5, 4), (4, 5), (5, 5)]);
        for _ in 0..=STALE_GENERATIONS {
            life.step();
        }
        assert_eq!(
            sorted(life.alive_cells()),
            vec![(4, 4), (4, 5), (5, 4), (5, 5)]
        );
        assert!(life.stagnant());
    }

    #[test]
    fn a_glider_translates_one_diagonal_step_every_four_generations() {
        let glider = [(10, 8), (11, 9), (9, 10), (10, 10), (11, 10)];
        let mut life = dish(24, 24, &glider);
        for _ in 0..4 {
            life.step();
        }
        let shifted: Vec<(u32, u32)> = glider.iter().map(|&(x, y)| (x + 1, y + 1)).collect();
        assert_eq!(sorted(life.alive_cells()), sorted(shifted));
    }

    #[test]
    fn the_dead_leave_fading_ash() {
        let mut life = dish(9, 9, &[(1, 1)]);
        life.step();
        assert_eq!(life.ash_heat[life.index(1, 1)], EMBER_TOP);
        let heats: Vec<u8> = (0..12)
            .map(|_| {
                life.step();
                life.ash_heat[life.index(1, 1)]
            })
            .collect();
        assert!(heats.windows(2).all(|pair| pair[0] >= pair[1]));
        assert_eq!(heats.last(), Some(&0), "ash burns out");
    }

    #[test]
    fn the_wraparound_feeds_the_borders() {
        let mut life = dish(6, 6, &[(4, 2), (5, 2), (0, 2)]);
        life.step();
        assert_eq!(sorted(life.alive_cells()), vec![(5, 1), (5, 2), (5, 3)]);
    }

    #[test]
    fn heat_runs_hot_to_cold_and_the_ember_leans_red() {
        assert_eq!(heat(1), (255, 255, 235));
        assert!(heat(20).0 > heat(50).0);
        assert_eq!(heat(80), heat(200), "age saturates its color");
        let [red, green, blue] = EMBER;
        assert!(red > green && green > blue);
    }

    #[test]
    fn life_cells_stay_square_on_tall_pixels() {
        let grid = Grid::fitting(&Light::new(160, 48, 0.5));
        assert_eq!((grid.cell_width, grid.cell_height), (2, 1));
        assert_eq!((grid.columns, grid.rows), (80, 48));
    }

    #[test]
    fn a_played_frame_dumps_a_png_artifact() {
        let mut demo = LifeDemo::new(99);
        let mut light = Light::new(960, 540, 1.0);
        demo.resize(&light);
        let keys = HeldKeys::default();
        for _ in 0..90 {
            demo.step(&keys, 1.0 / 60.0);
        }
        demo.render(&mut light);
        artifact::save_frame(&light, "life");
    }
}
