//! The katui life demo: Conway's game of life rendered as an image.
//! The automaton runs on a pixel surface several pixels per terminal
//! cell and lands on screen through the kitty graphics protocol, so the
//! result is a true image: a slowly breathing nebula gradient
//! underneath, cells born white-hot cooling through gold and fire into
//! violet ash that decays into the background. Fine-grained tile
//! patches carry only what changed, a stagnant universe reseeds itself
//! with a fresh nebula bloom, and `q`, `Esc` or `Ctrl-C` leave. On
//! terminals without a graphics protocol the same surface degrades to
//! quarter blocks. Run with `just demo-life`.
#![cfg(unix)]
// the demos drive /dev/tty through katui-rt's unix layer
// torus coordinates wrap inside the grid before they index, so the
// universe vectors are in-bounds by construction; the breathing pulse
// and the color math run in f32 and land in u8, where clippy's cast
// lints guard code that has no negative values to lose
#![allow(
    clippy::indexing_slicing,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::cast_possible_truncation
)]

use std::io::{self, Write as _};
use std::time::{SystemTime, UNIX_EPOCH};

use katui_scene::surface::{PixelPos, Rgba, Surface};

/// Generations per second.
const FPS: u32 = 11;
/// The whole show, in generations, when nobody presses a quit key.
const GENERATIONS: u64 = 1500;
/// A universe with no births and no deaths for this many generations
/// gets a fresh nebula. Oscillators and gliders never trigger it.
const STALE_GENERATIONS: u32 = 40;
/// How long a dead pixel's ash glows, in generations.
const EMBER_TOP: u8 = 14;
/// Ash dims this fast per generation.
const EMBER_DECAY: u8 = 2;
/// The ember's one fixed color, a dim additive red.
const EMBER: [u8; 3] = [170, 55, 30];
/// Surface pixels per terminal cell, each axis: the automaton's
/// resolution, balanced against the wire: every living pixel that
/// changes is a byte that travels.
const SUBPIXEL: u16 = 4;

fn main() -> io::Result<()> {
    let caps = katui_rt::tty::probe_default();
    let size = katui_rt::tty::size().map_err(io::Error::other)?;
    let (columns, rows) = (size.columns, size.rows);
    let kitty = caps.kitty;
    let cell = katui_rt::tty::pixel_cell(&caps);
    let width = u32::from(columns) * u32::from(SUBPIXEL);
    let height = u32::from(rows) * u32::from(SUBPIXEL);

    let stage = katui_rt::session::Stage::enter(
        io::stdout(),
        katui_rt::session::Teardown {
            graphics: kitty,
            home: true,
            clear: true,
        },
    )
    .map_err(io::Error::other)?;

    // tiles span whole actions, not single cells: the action is sparse
    // once the soup burns in, and flat neighbors compress to nothing
    let mut screen = katui_present::surface::PixelScreen::new(
        &caps,
        katui_present::surface::PatchedKitty::with_tiles(700, cell, u32::from(SUBPIXEL) * 8, 4096),
    );
    let mut life = Life::new(width, height, seed());

    stage
        .run(FPS, GENERATIONS, |out, frame| {
            life.step();
            if life.stagnant() {
                life.reseed();
            }
            let mut surface = Surface::new(width, height);
            let pulse = (frame as f32 * 0.004).sin() * 0.5 + 0.5;
            life.draw(&mut surface, pulse);
            screen.render_frame(out, &surface, columns, rows)?;
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

/// One universe: life, ash and the nebula under it.
struct Life {
    width: u32,
    height: u32,
    /// Alive pixels, 0 or 1, with a one-ghost border for wraparound.
    alive: Vec<u8>,
    /// Generations alive per pixel; zero is dead. Ages saturate.
    age: Vec<u8>,
    /// Ash heat left behind by a death, fading to zero.
    ember: Vec<u8>,
    /// The nebula underneath: one rgb triple per pixel.
    background: Vec<[u8; 3]>,
    /// Generations since the last birth or death.
    stale: u32,
    rng: SplitMix64,
}

impl Life {
    fn new(width: u32, height: u32, seed: u64) -> Self {
        let mut life = Self {
            width,
            height,
            alive: vec![0; ((width + 2) * (height + 2)) as usize],
            age: vec![0; (width * height) as usize],
            ember: vec![0; (width * height) as usize],
            background: nebula(width, height, seed),
            stale: 0,
            rng: SplitMix64(seed ^ 0x9e37_79b9),
        };
        life.reseed();
        life
    }

    /// Whether the universe has stopped trading births for deaths.
    fn stagnant(&self) -> bool {
        self.stale >= STALE_GENERATIONS
    }

    fn index(&self, x: u32, y: u32) -> usize {
        (y * self.width + x) as usize
    }

    /// A fresh bloom: a few nebula clouds of life at just under half
    /// density, coherent enough to grow structures that spit gliders.
    fn reseed(&mut self) {
        self.age.iter_mut().for_each(|age| *age = 0);
        self.ember.iter_mut().for_each(|heat| *heat = 0);
        self.background = nebula(self.width, self.height, self.rng.next());
        self.stale = 0;
        let clouds = (self.width * self.height / 22_000).clamp(2, 10);
        for _ in 0..clouds {
            let radius = 14 + self.rng.next() % 22;
            let center_x = self.rng.next() % u64::from(self.width);
            let center_y = self.rng.next() % u64::from(self.height);
            let span = radius.cast_signed();
            let square = (radius * radius).cast_signed();
            for dy in -span..=span {
                for dx in -span..=span {
                    if dx * dx + dy * dy > square || self.rng.next() % 100 >= 45 {
                        continue;
                    }
                    let width = i64::from(self.width);
                    let height = i64::from(self.height);
                    let x =
                        u32::try_from((center_x.cast_signed() + dx).rem_euclid(width)).unwrap_or(0);
                    let y = u32::try_from((center_y.cast_signed() + dy).rem_euclid(height))
                        .unwrap_or(0);
                    self.alive[Self::ghost_index(self.width, x, y)] = 1;
                }
            }
        }
        self.absorb_ghost_border();
    }

    /// Where (x, y) sits in the ghost-bordered alive grid.
    fn ghost_index(width: u32, x: u32, y: u32) -> usize {
        ((y + 1) * (width + 2) + x + 1) as usize
    }

    /// Copy the wrapped edges into the ghost border, so every interior
    /// pixel counts neighbors without a single modulo.
    fn absorb_ghost_border(&mut self) {
        let width = self.width as usize;
        let height = self.height as usize;
        let stride = width + 2;
        for x in 0..width {
            // ghost row 0 mirrors the last interior row, ghost row
            // height+1 the first
            self.alive[x + 1] = self.alive[x + 1 + height * stride];
            self.alive[x + 1 + (height + 1) * stride] = self.alive[x + 1 + stride];
        }
        for y in 0..height {
            // ghost col 0 mirrors the last interior column of the same
            // interior row, ghost col width+1 the first
            let row = (y + 1) * stride;
            self.alive[row] = self.alive[row + width];
            self.alive[row + width + 1] = self.alive[row + 1];
        }
        // the four ghost corners feed diagonal neighbors of the corners
        self.alive[0] = self.alive[height * stride + width];
        self.alive[width + 1] = self.alive[height * stride + 1];
        self.alive[(height + 1) * stride] = self.alive[stride + width];
        self.alive[(height + 1) * stride + width + 1] = self.alive[stride + 1];
    }

    /// One generation of B3/S23: the born light white-hot, survivors a
    /// generation older, the dead handed ash that then fades.
    fn step(&mut self) {
        let stride = (self.width + 2) as usize;
        self.ember.iter_mut().for_each(|heat| {
            *heat = heat.saturating_sub(EMBER_DECAY);
        });
        let mut next_alive = vec![0u8; self.alive.len()];
        let mut next_age = vec![0u8; self.age.len()];
        let mut churn = false;
        for y in 0..self.height as usize {
            let above = y * stride;
            let mid = above + stride;
            let below = mid + stride;
            for x in 0..self.width as usize {
                let left = x;
                let center = x + 1;
                let right = x + 2;
                let neighbors = u8::from(self.alive[above + left] > 0)
                    + u8::from(self.alive[above + center] > 0)
                    + u8::from(self.alive[above + right] > 0)
                    + u8::from(self.alive[mid + left] > 0)
                    + u8::from(self.alive[mid + right] > 0)
                    + u8::from(self.alive[below + left] > 0)
                    + u8::from(self.alive[below + center] > 0)
                    + u8::from(self.alive[below + right] > 0);
                let alive = self.alive[mid + center] > 0;
                let inner = y * self.width as usize + x;
                if !alive && neighbors == 3 {
                    next_alive[mid + center] = 1;
                    next_age[inner] = 1;
                    churn = true;
                } else if alive && matches!(neighbors, 2 | 3) {
                    next_alive[mid + center] = 1;
                    next_age[inner] = self.age[inner].saturating_add(1);
                } else if alive {
                    self.ember[inner] = EMBER_TOP;
                    churn = true;
                }
            }
        }
        self.alive = next_alive;
        self.age = next_age;
        self.absorb_ghost_border();
        self.stale = if churn {
            0
        } else {
            self.stale.saturating_add(1)
        };
    }

    /// The universe as one image: a static nebula under additive heat.
    /// New pixels are white-hot, cool through gold and fire as they age,
    /// and their death leaves a fixed ember that blinks out whole. Every
    /// color a pixel can hold is event-based: bytes change on birth, on
    /// a bucket transition, and twice per death, never gradually, which
    /// is what keeps the tile damage the size of the action.
    fn draw(&self, surface: &mut Surface, pulse: f32) {
        // quantized so the glow breathes in steps, not every frame
        let breath = 0.85 + 0.15 * ((pulse * 16.0).floor() / 16.0);
        for y in 0..self.height {
            for x in 0..self.width {
                let inner = self.index(x, y);
                let [br, bg, bb] = self.background[inner];
                let (dr, dg, db) = if self.age[inner] > 0 {
                    heat(self.age[inner])
                } else if self.ember[inner] > 0 {
                    (EMBER[0], EMBER[1], EMBER[2])
                } else {
                    (0, 0, 0)
                };
                let r = (f32::from(br) + f32::from(dr) * breath).round().min(255.0) as u8;
                let g = (f32::from(bg) + f32::from(dg) * breath).round().min(255.0) as u8;
                let b = (f32::from(bb) + f32::from(db) * breath).round().min(255.0) as u8;
                surface.set(PixelPos::new(x, y), Rgba { r, g, b, a: 255 });
            }
        }
    }

    /// The alive cells as coordinates, for the structure tests.
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

/// The additive glow of a pixel alive for `age` generations. Coarse
/// buckets: an old cell settles to one color and stops costing bytes.
fn heat(age: u8) -> (u8, u8, u8) {
    match u32::from(age) {
        1 => (255, 255, 235),
        2 => (255, 240, 170),
        3..=4 => (255, 210, 90),
        5..=9 => (255, 160, 50),
        10..=24 => (240, 95, 35),
        25..=60 => (200, 55, 40),
        _ => (150, 40, 60),
    }
}

/// The nebula under everything: two hue anchors blended along the
/// diagonal, dimmed toward the edges, and speckled dark enough for the
/// heat to dominate.
fn nebula(width: u32, height: u32, seed: u64) -> Vec<[u8; 3]> {
    let mut rng = SplitMix64(seed);
    let (ax, ay, az) = (10.0, 12.0, 34.0);
    let (bx, by, bz) = (30.0, 12.0, 48.0);
    let mut field = Vec::with_capacity((width * height) as usize);
    for y in 0..height {
        for x in 0..width {
            let diagonal = f32::midpoint(x as f32 / width as f32, y as f32 / height as f32);
            let vx = x as f32 / width as f32 - 0.5;
            let vy = y as f32 / height as f32 - 0.5;
            let vignette = 1.0 - (vx * vx + vy * vy).sqrt() * 0.8;
            let speckle = (rng.next() % 7) as f32;
            let base = [
                (ax + (bx - ax) * diagonal) * vignette + speckle,
                (ay + (by - ay) * diagonal) * vignette + speckle,
                (az + (bz - az) * diagonal) * vignette + speckle,
            ];
            field.push([base[0] as u8, base[1] as u8, base[2] as u8]);
        }
    }
    field
}

/// A tiny deterministic generator: splitmix64.
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
    use super::{EMBER_TOP, Life, heat};

    fn dish(columns: u32, rows: u32, alive: &[(u32, u32)]) -> Life {
        let mut life = Life::new(columns, rows, 7);
        life.age.iter_mut().for_each(|age| *age = 0);
        life.ember.iter_mut().for_each(|heat| *heat = 0);
        life.alive.iter_mut().for_each(|cell| *cell = 0);
        life.stale = 0;
        for &(x, y) in alive {
            let index = life.index(x, y);
            life.age[index] = 1;
            life.alive[super::Life::ghost_index(columns, x, y)] = 1;
        }
        life.absorb_ghost_border();
        life
    }

    #[test]
    fn a_blinker_oscillates_and_ages() {
        let mut life = dish(9, 9, &[(3, 4), (4, 4), (5, 4)]);
        life.step();
        let mut vertical = vec![(4, 3), (4, 4), (4, 5)];
        vertical.sort_unstable();
        let mut shown = life.alive_cells();
        shown.sort_unstable();
        assert_eq!(shown, vertical, "the blinker turns");
        life.step();
        assert_eq!(life.alive_cells().len(), 3, "and keeps turning");
        assert!(life.age[life.index(4, 4)] >= 3, "the center never dies");
    }

    #[test]
    fn a_block_is_still_and_becomes_stagnant() {
        let mut life = dish(10, 10, &[(4, 4), (5, 4), (4, 5), (5, 5)]);
        for _ in 0..=super::STALE_GENERATIONS {
            life.step();
        }
        let mut shown = life.alive_cells();
        shown.sort_unstable();
        assert_eq!(shown, vec![(4, 4), (4, 5), (5, 4), (5, 5)], "still life");
        assert!(life.stagnant(), "no churn for long enough reseeds");
    }

    #[test]
    fn a_glider_translates_diagonally_every_four_steps() {
        let glider = [(10, 8), (11, 9), (9, 10), (10, 10), (11, 10)];
        let mut life = dish(24, 24, &glider);
        for _ in 0..4 {
            life.step();
        }
        let walked = life.alive_cells();
        assert_eq!(walked.len(), glider.len(), "a glider never loses cells");
        let (dx, dy) = (
            walked[0].0 as i32 - glider[0].0 as i32,
            walked[0].1 as i32 - glider[0].1 as i32,
        );
        assert_eq!(
            (dx.abs(), dy.abs()),
            (1, 1),
            "four generations walk one diagonal step, got ({dx},{dy})"
        );
        let mut shifted: Vec<(u32, u32)> = glider
            .iter()
            .map(|&(x, y)| {
                (
                    (x as i32 + dx).rem_euclid(24) as u32,
                    (y as i32 + dy).rem_euclid(24) as u32,
                )
            })
            .collect();
        shifted.sort_unstable();
        let mut shown = walked;
        shown.sort_unstable();
        assert_eq!(shown, shifted, "the shape survives whole");
    }

    #[test]
    fn the_dead_leave_fading_ash() {
        // a lone cell dies exactly once, so its ash only ever fades
        let mut life = dish(9, 9, &[(1, 1)]);
        life.step();
        assert_eq!(
            life.ember[life.index(1, 1)],
            super::EMBER_TOP,
            "the death glows"
        );
        let heats: Vec<u8> = (0..12)
            .map(|_| {
                life.step();
                life.ember[life.index(1, 1)]
            })
            .collect();
        assert!(
            heats.windows(2).all(|pair| pair[0] >= pair[1]),
            "ash only fades"
        );
        assert_eq!(*heats.last().unwrap_or(&0), 0, "ash burns out");
    }

    #[test]
    fn the_wraparound_feeds_the_borders() {
        // a blinker straddling the right edge comes back around the torus
        let mut life = dish(6, 6, &[(4, 2), (5, 2), (0, 2)]);
        life.step();
        let shown = life.alive_cells();
        assert_eq!(shown.len(), 3, "edge cells keep living across the seam");
        assert!(
            shown.contains(&(5, 1)) || shown.contains(&(0, 1)) || shown.contains(&(5, 3)),
            "the reborn cells land on the seam: {shown:?}"
        );
    }

    #[test]
    #[test]
    fn heat_runs_hot_to_cold_and_the_ember_is_a_fixed_red() {
        assert_eq!(heat(1), (255, 255, 235), "born white-hot");
        assert!(heat(20).0 > heat(50).0, "cooling dims");
        let [r, g, b] = super::EMBER;
        assert!(r > g && g > b, "the ember leans red");
        // old cells settle: no bytes change past the last bucket
        assert_eq!(heat(80), heat(200), "age saturates its color");
    }

    #[test]
    fn a_headless_universe_paints_and_patches() {
        use katui_proto::probe::CellPixels;
        use katui_scene::placement::CellRect;
        use katui_scene::surface::Surface;

        let mut life = Life::new(120, 80, 99);
        let mut presenter = katui_present::surface::PatchedKitty::with_tiles(
            700,
            CellPixels {
                width: 5,
                height: 5,
            },
            40,
            4096,
        );
        let area = CellRect {
            x: 0,
            y: 0,
            columns: 24,
            rows: 16,
        };
        let mut wire = Vec::new();
        for frame in 0..60 {
            life.step();
            if life.stagnant() {
                life.reseed();
            }
            let mut surface = Surface::new(120, 80);
            let pulse = (frame as f32 * 0.004).sin() * 0.5 + 0.5;
            life.draw(&mut surface, pulse);
            presenter
                .render_frame(&mut wire, &surface, area)
                .expect("writing to a Vec cannot fail");
        }
        let text = String::from_utf8_lossy(&wire);
        assert!(
            text.matches("s=120,v=80").count() <= 1,
            "one whole transmit, the first frame"
        );
        assert!(
            text.matches("a=t").count() > 1,
            "tile patches carry the generations"
        );
    }
}
