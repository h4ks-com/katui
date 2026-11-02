//! The katui life demo: Conway's game of life on the cell plane, one
//! automaton cell per terminal cell, 256-color indexed SGR only. Living
//! cells glow white-hot at birth and cool through gold, fire, magenta
//! and violet into deep navy as they age; the dead leave a fading ash
//! trail. The grid wraps into a torus, a random petri-dish seeding
//! kickstarts it, and a stagnant colony (no births, no deaths) reseeds
//! itself so the show never ends. Quit with `q`, `Esc` or `Ctrl-C`.
//! Run with `just demo-life`.
#![cfg(unix)]
// the demos drive /dev/tty through katui-rt's unix layer
// every coordinate wraps inside the torus before it indexes, so the
// grid vectors are in-bounds by construction; checked indexing here
// would bury the automaton in boilerplate
#![allow(clippy::indexing_slicing)]

use std::io::{self, Write as _};
use std::time::{SystemTime, UNIX_EPOCH};

use katui_scene::CellSize;
use katui_scene::cell::{Attrs, Buffer, Cell, Color, Position};

/// Generations per second: fast enough to evolve, slow enough to read.
const FPS: u32 = 12;
/// The whole show, in generations, when nobody presses a quit key.
const GENERATIONS: u64 = 900;
/// A colony with no births and no deaths for this many generations is
/// stagnant and gets a fresh dish. Oscillators and gliders never
/// trigger it: they trade births for deaths every period.
const STALE_GENERATIONS: u32 = 24;
/// How long an ash trail glows after its cell dies.
const EMBER_TOP: u8 = 6;

/// The age ramp through the xterm 256 palette: white-hot birth, gold,
/// fire, magenta, violet, deep navy old age.
const RAMP: [u8; 22] = [
    231, 229, 228, 227, 220, 214, 208, 202, 196, 199, 201, 165, 164, 129, 93, 92, 63, 62, 27, 26,
    25, 18,
];

fn main() -> io::Result<()> {
    let size = katui_rt::tty::size().map_err(io::Error::other)?;
    let (columns, rows) = (size.columns, size.rows);
    let stage = katui_rt::session::Stage::enter(
        io::stdout(),
        katui_rt::session::Teardown {
            graphics: false,
            home: true,
            clear: true,
        },
    )
    .map_err(io::Error::other)?;

    let mut presenter = katui_present::surface::BufferSurface::new();
    let mut life = Life::new(columns, rows, seed());

    stage
        .run(FPS, GENERATIONS, |out, _| {
            life.step();
            if life.stagnant() {
                life.reseed();
            }
            let mut buffer = Buffer::blank(CellSize { columns, rows });
            life.draw(&mut buffer);
            presenter.render_frame(out, buffer)?;
            out.flush()?;
            Ok(true)
        })
        .map_err(io::Error::other)
}

/// A seed from the clock, so every run starts a different universe.
fn seed() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0x5eed_1234, |passed| passed.as_nanos() as u64)
}

/// One universe: cell ages, ash trails and the stagnation counter.
struct Life {
    columns: u16,
    rows: u16,
    /// Generations alive per cell; zero is dead.
    age: Vec<u16>,
    /// Ash heat left by a death, fading to zero.
    ember: Vec<u8>,
    /// Generations since the last birth or death.
    stale: u32,
    rng: SplitMix64,
}

impl Life {
    /// A universe seeded with a fresh petri dish.
    fn new(columns: u16, rows: u16, seed: u64) -> Self {
        let mut life = Self {
            columns,
            rows,
            age: vec![0; usize::from(columns) * usize::from(rows)],
            ember: Vec::new(),
            stale: 0,
            rng: SplitMix64(seed),
        };
        life.ember = vec![0; life.age.len()];
        life.reseed();
        life
    }

    /// Whether the colony has stopped trading births for deaths.
    fn stagnant(&self) -> bool {
        self.stale >= STALE_GENERATIONS
    }

    /// A fresh petri dish: a few random blobs at just under half
    /// density, coherent enough to grow structures that spit gliders.
    fn reseed(&mut self) {
        self.age.iter_mut().for_each(|age| *age = 0);
        self.ember.iter_mut().for_each(|heat| *heat = 0);
        self.stale = 0;
        let area = self.age.len();
        let dishes = (area / 600).clamp(1, 8);
        for _ in 0..dishes {
            let radius = 5 + self.rng.next() % 6;
            let center_x = self.rng.next() % u64::from(self.columns);
            let center_y = self.rng.next() % u64::from(self.rows);
            let span = radius.cast_signed();
            let square = (radius * radius).cast_signed();
            for dy in -span..=span {
                for dx in -span..=span {
                    if dx * dx + dy * dy > square {
                        continue;
                    }
                    if self.rng.next() % 100 >= 45 {
                        continue;
                    }
                    let x =
                        (center_x.cast_signed() + dx).rem_euclid(i64::from(self.columns)) as u16;
                    let y = (center_y.cast_signed() + dy).rem_euclid(i64::from(self.rows)) as u16;
                    let index = self.index(x, y);
                    self.age[index] = 1;
                }
            }
        }
    }

    fn index(&self, x: u16, y: u16) -> usize {
        usize::from(y) * usize::from(self.columns) + usize::from(x)
    }

    /// Living neighbors on the torus.
    fn neighbors(&self, x: u16, y: u16) -> u8 {
        let columns = i32::from(self.columns);
        let rows = i32::from(self.rows);
        let mut count = 0;
        for dy in -1i32..=1 {
            for dx in -1i32..=1 {
                if dx == 0 && dy == 0 {
                    continue;
                }
                let nx = (i32::from(x) + dx).rem_euclid(columns) as u16;
                let ny = (i32::from(y) + dy).rem_euclid(rows) as u16;
                count += u8::from(self.age[self.index(nx, ny)] > 0);
            }
        }
        count
    }

    /// One generation of B3/S23: the born light white-hot, survivors a
    /// generation older, the dead handed an ember that then fades.
    fn step(&mut self) {
        self.ember.iter_mut().for_each(|heat| {
            *heat = heat.saturating_sub(1);
        });
        let mut next = vec![0u16; self.age.len()];
        let mut churn = false;
        for y in 0..self.rows {
            for x in 0..self.columns {
                let neighbors = self.neighbors(x, y);
                let index = self.index(x, y);
                let alive = self.age[index] > 0;
                if !alive && neighbors == 3 {
                    next[index] = 1;
                    churn = true;
                } else if alive && matches!(neighbors, 2 | 3) {
                    next[index] = self.age[index].saturating_add(1);
                } else if alive {
                    self.ember[index] = EMBER_TOP;
                    churn = true;
                }
            }
        }
        self.age = next;
        self.stale = if churn {
            0
        } else {
            self.stale.saturating_add(1)
        };
    }

    /// The universe as one cell buffer: full blocks on the age ramp,
    /// ash dots fading, empty space elsewhere.
    fn draw(&self, buffer: &mut Buffer) {
        for y in 0..self.rows {
            for x in 0..self.columns {
                let index = self.index(x, y);
                let cell = if self.age[index] > 0 {
                    Cell {
                        symbol: "█".to_owned(),
                        fg: Color::Indexed(age_color(self.age[index])),
                        bg: Color::Reset,
                        attrs: Attrs::EMPTY,
                    }
                } else if self.ember[index] > 0 {
                    Cell {
                        symbol: "·".to_owned(),
                        fg: Color::Indexed(ember_color(self.ember[index])),
                        bg: Color::Reset,
                        attrs: Attrs::EMPTY,
                    }
                } else {
                    Cell::default()
                };
                buffer.set(Position { x, y }, cell);
            }
        }
    }

    /// The alive cells as coordinates, for the structure tests.
    #[cfg(test)]
    fn alive(&self) -> Vec<(u16, u16)> {
        let mut cells = Vec::new();
        for y in 0..self.rows {
            for x in 0..self.columns {
                if self.age[self.index(x, y)] > 0 {
                    cells.push((x, y));
                }
            }
        }
        cells
    }
}

/// The palette index for `age` generations of life.
fn age_color(age: u16) -> u8 {
    let step = usize::from(age.saturating_sub(1));
    RAMP[step.min(RAMP.len() - 1)]
}

/// The palette index for an ember of `heat`.
fn ember_color(heat: u8) -> u8 {
    // 237 is the brightest ash this trail uses, 232 the dimmest
    238 - heat.min(EMBER_TOP)
}

/// A tiny deterministic generator, different flavor from the snake
/// demo's xorshift: splitmix64.
struct SplitMix64(u64);

impl SplitMix64 {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }
}

#[cfg(test)]
mod tests {
    use super::{EMBER_TOP, Life, age_color, ember_color};

    fn dish(columns: u16, rows: u16, alive: &[(u16, u16)]) -> Life {
        let mut life = Life::new(columns, rows, 7);
        life.reseed_at(alive);
        life
    }

    #[test]
    fn a_blinker_oscillates_and_ages() {
        let mut life = dish(5, 5, &[(1, 2), (2, 2), (3, 2)]);
        life.step();
        let vertical = vec![(2, 1), (2, 2), (2, 3)];
        assert_eq!(life.alive(), vertical, "the blinker turns");
        life.step();
        assert_eq!(life.alive(), vec![(1, 2), (2, 2), (3, 2)], "and back");
        // survivors carry their age: the center cell never died
        assert!(life.age[life.index(2, 2)] >= 3, "age grows while alive");
    }

    #[test]
    fn a_block_is_still_and_becomes_stagnant() {
        let mut life = dish(6, 6, &[(2, 2), (3, 2), (2, 3), (3, 3)]);
        for _ in 0..=super::STALE_GENERATIONS {
            life.step();
        }
        assert_eq!(
            life.alive(),
            vec![(2, 2), (3, 2), (2, 3), (3, 3)],
            "a block never moves"
        );
        assert!(life.stagnant(), "no churn for long enough reseeds");
    }

    #[test]
    fn a_glider_translates_diagonally_every_four_steps() {
        let glider = [(5, 4), (6, 5), (4, 6), (5, 6), (6, 6)];
        let mut life = dish(14, 14, &glider);
        for _ in 0..4 {
            life.step();
        }
        let walked = life.alive();
        assert_eq!(walked.len(), glider.len(), "a glider never loses cells");
        let (dx, dy) = (
            i32::from(walked[0].0) - i32::from(glider[0].0),
            i32::from(walked[0].1) - i32::from(glider[0].1),
        );
        assert_eq!(
            (dx.abs(), dy.abs()),
            (1, 1),
            "four generations walk one diagonal step, got ({dx},{dy})"
        );
        let shifted: Vec<(u16, u16)> = glider
            .iter()
            .map(|&(x, y)| {
                (
                    (i32::from(x) + dx).rem_euclid(14) as u16,
                    (i32::from(y) + dy).rem_euclid(14) as u16,
                )
            })
            .collect();
        let mut walked_sorted = walked;
        let mut shifted_sorted = shifted;
        walked_sorted.sort_unstable();
        shifted_sorted.sort_unstable();
        assert_eq!(walked_sorted, shifted_sorted, "the shape survives whole");
    }

    #[test]
    fn the_dead_leave_fading_ash() {
        let mut life = dish(5, 5, &[(0, 0), (1, 0), (2, 0)]);
        life.step();
        let died: Vec<u8> = (0..5)
            .map(|_| {
                life.step();
                life.ember[life.index(1, 0)]
            })
            .collect();
        assert!(
            died.windows(2).all(|pair| pair[0] >= pair[1]),
            "ash only fades"
        );
        assert_eq!(*died.last().unwrap_or(&0), 0, "ash burns out");
    }

    #[test]
    fn reseeding_trades_one_dish_for_another() {
        let mut life = dish(40, 20, &[(2, 2), (3, 2), (2, 3), (3, 3)]);
        for _ in 0..30 {
            life.step();
        }
        assert!(life.stagnant());
        let population_before = life.alive().len();
        life.reseed();
        assert!(!life.stagnant(), "the stale counter resets");
        assert_ne!(life.alive().len(), population_before, "a new dish lands");
    }

    #[test]
    fn the_ramp_runs_hot_to_cold_and_holds() {
        assert_eq!(age_color(1), 231, "born white-hot");
        assert_eq!(age_color(5), 220, "gold within a week");
        assert!(age_color(30) < 30, "old cells sit in the deep blues");
        assert_eq!(age_color(500), age_color(9_000), "age saturates");
        assert_eq!(ember_color(EMBER_TOP), 232, "the freshest ash");
        assert_eq!(ember_color(1), 237, "the dimmest visible ash");
    }
}

#[cfg(test)]
impl Life {
    /// Place an exact pattern for the structure tests, clearing any dish.
    fn reseed_at(&mut self, alive: &[(u16, u16)]) {
        self.age.iter_mut().for_each(|age| *age = 0);
        self.ember.iter_mut().for_each(|heat| *heat = 0);
        self.stale = 0;
        for &(x, y) in alive {
            let index = self.index(x, y);
            self.age[index] = 1;
        }
    }
}
