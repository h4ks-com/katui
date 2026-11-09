//! Lumenshaft: stack glowing glass blocks down a shaft that recedes into
//! the screen, with pieces of three to five cells. Run with
//! `just demo-lumenshaft`.
#![cfg(unix)] // the runner drives /dev/tty through katui-rt's unix layer
#![allow(
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]

use std::collections::HashMap;

use katui_game::{
    Blend, Camera, Game, HeldKeys, Key, Light, P2, Rgb, Rng, Spark, Sparks, V3, play, scaled, v3,
};

const COLUMNS: i32 = 9;
const ROWS: i32 = 18;
const HALF_COLUMNS: f32 = COLUMNS as f32 / 2.0;
const HALF_ROWS: f32 = ROWS as f32 / 2.0;

const CUBE_DEPTH: f32 = 1.4;
const SHAFT_DEPTH: f32 = 7.0;
const EYE_DISTANCE: f32 = 14.0;
const SHAFT_SHARE_OF_HEIGHT: f32 = 0.9;

const LOCK_DELAY: f32 = 0.45;
const SOFT_DROP_SPEED: f32 = 18.0;
const LINES_PER_LEVEL: u32 = 8;
const KICKS: [(i32, i32); 7] = [(0, 0), (-1, 0), (1, 0), (-2, 0), (2, 0), (0, -1), (0, -2)];
const FALLING_GLOW: f32 = 0.25;
const BEAM_LIFE: f32 = 0.4;
const STREAK_LIFE: f32 = 0.28;

const SHAPES: [&[(i32, i32)]; 8] = [
    &[(0, 0), (1, 0), (2, 0)],
    &[(0, 0), (1, 0), (0, 1)],
    &[(0, 0), (1, 0), (0, 1), (1, 1)],
    &[(0, 0), (1, 0), (2, 0), (2, 1)],
    &[(0, 0), (1, 0), (2, 0), (1, 1)],
    &[(0, 0), (1, 0), (1, 1), (2, 1)],
    &[(1, 0), (0, 1), (1, 1), (2, 1), (1, 2)],
    &[(0, 0), (1, 0), (2, 0), (0, 1), (2, 1)],
];
const HUES: [Rgb; 8] = [
    [0.15, 0.85, 1.0],
    [1.0, 0.35, 0.55],
    [1.0, 0.75, 0.2],
    [0.45, 1.0, 0.45],
    [0.7, 0.4, 1.0],
    [1.0, 0.5, 0.15],
    [0.3, 0.55, 1.0],
    [0.95, 0.3, 0.95],
];
const SHAFT_TINTS: [Rgb; 5] = [
    [0.1, 0.4, 0.9],
    [0.55, 0.2, 0.9],
    [0.05, 0.7, 0.6],
    [0.9, 0.3, 0.35],
    [0.9, 0.6, 0.1],
];

fn main() -> Result<(), katui_rt::RtError> {
    play(Lumenshaft::new(0x5eed_1234))
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Piece {
    hue: usize,
    cells: Vec<(i32, i32)>,
    x: i32,
    y: i32,
}

impl Piece {
    fn squares(&self) -> impl Iterator<Item = (i32, i32)> + '_ {
        self.cells
            .iter()
            .map(|(column, row)| (self.x + column, self.y + row))
    }
}

fn normalized(cells: &[(i32, i32)]) -> Vec<(i32, i32)> {
    let left = cells.iter().map(|cell| cell.0).min().unwrap_or(0);
    let bottom = cells.iter().map(|cell| cell.1).min().unwrap_or(0);
    cells
        .iter()
        .map(|(column, row)| (column - left, row - bottom))
        .collect()
}

fn shape_of(hue: usize) -> Vec<(i32, i32)> {
    SHAPES
        .get(hue)
        .map_or_else(Vec::new, |shape| shape.to_vec())
}

/// We square the palette because the light buffer is linear.
fn hue_of(index: usize) -> Rgb {
    HUES.get(index)
        .copied()
        .unwrap_or([1.0; 3])
        .map(|channel| channel * channel * 1.3)
}

struct Clear {
    rows: Vec<i32>,
    points: u32,
}

#[derive(Debug, Clone)]
struct Well {
    filled: HashMap<(i32, i32), usize>,
    falling: Piece,
    next: usize,
    bag: Vec<usize>,
    rng: Rng,
    score: u32,
    lines: u32,
    level: u32,
    over: bool,
}

impl Well {
    fn new(seed: u32) -> Self {
        let mut well = Self {
            filled: HashMap::new(),
            falling: Piece {
                hue: 0,
                cells: Vec::new(),
                x: 0,
                y: 0,
            },
            next: 0,
            bag: Vec::new(),
            rng: Rng::new(seed),
            score: 0,
            lines: 0,
            level: 1,
            over: false,
        };
        well.next = well.draw_from_bag();
        well.spawn();
        well
    }

    fn draw_from_bag(&mut self) -> usize {
        if self.bag.is_empty() {
            self.bag = (0..SHAPES.len()).collect();
        }
        let at = self.rng.next_u32() as usize % self.bag.len();
        self.bag.swap_remove(at)
    }

    fn spawn(&mut self) {
        let hue = self.next;
        self.next = self.draw_from_bag();
        let cells = shape_of(hue);
        let width = cells.iter().map(|cell| cell.0).max().unwrap_or(0) + 1;
        let height = cells.iter().map(|cell| cell.1).max().unwrap_or(0) + 1;
        self.falling = Piece {
            hue,
            cells,
            x: (COLUMNS - width) / 2,
            y: ROWS - height,
        };
        self.over = !self.fits(&self.falling);
    }

    fn fits(&self, piece: &Piece) -> bool {
        piece.squares().all(|(column, row)| {
            (0..COLUMNS).contains(&column)
                && (0..ROWS).contains(&row)
                && !self.filled.contains_key(&(column, row))
        })
    }

    fn shift(&mut self, dx: i32, dy: i32) -> bool {
        let moved = Piece {
            x: self.falling.x + dx,
            y: self.falling.y + dy,
            ..self.falling.clone()
        };
        let fits = self.fits(&moved);
        if fits {
            self.falling = moved;
        }
        fits
    }

    fn rotate(&mut self) -> bool {
        let turned: Vec<(i32, i32)> = self
            .falling
            .cells
            .iter()
            .map(|(column, row)| (*row, -column))
            .collect();
        let cells = normalized(&turned);
        let kicked = KICKS.iter().find_map(|(right, up)| {
            let piece = Piece {
                cells: cells.clone(),
                x: self.falling.x + right,
                y: self.falling.y + up,
                ..self.falling.clone()
            };
            self.fits(&piece).then_some(piece)
        });
        let rotated = kicked.is_some();
        if let Some(piece) = kicked {
            self.falling = piece;
        }
        rotated
    }

    fn is_grounded(&self) -> bool {
        let below = Piece {
            y: self.falling.y - 1,
            ..self.falling.clone()
        };
        !self.fits(&below)
    }

    fn ghost(&self) -> Piece {
        let mut ghost = self.falling.clone();
        loop {
            let below = Piece {
                y: ghost.y - 1,
                ..ghost.clone()
            };
            if !self.fits(&below) {
                return ghost;
            }
            ghost = below;
        }
    }

    fn full_rows(&self) -> Vec<i32> {
        (0..ROWS)
            .filter(|row| (0..COLUMNS).all(|column| self.filled.contains_key(&(column, *row))))
            .collect()
    }

    fn lock(&mut self) -> Clear {
        let hue = self.falling.hue;
        let squares: Vec<(i32, i32)> = self.falling.squares().collect();
        self.filled
            .extend(squares.into_iter().map(|cell| (cell, hue)));
        let rows = self.full_rows();
        self.remove_rows(&rows);
        let count = rows.len() as u32;
        let points = count * count * 100 * self.level;
        self.score += points;
        self.lines += count;
        self.level = 1 + self.lines / LINES_PER_LEVEL;
        self.spawn();
        Clear { rows, points }
    }

    fn remove_rows(&mut self, cleared: &[i32]) {
        self.filled = self
            .filled
            .iter()
            .filter(|((_, row), _)| !cleared.contains(row))
            .map(|((column, row), hue)| {
                let below = cleared.iter().filter(|gone| **gone < *row).count() as i32;
                ((*column, row - below), *hue)
            })
            .collect();
    }

    fn drop_interval(&self) -> f32 {
        (0.75 * 0.86_f32.powi(self.level as i32 - 1)).max(0.05)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Title,
    Playing,
    Paused,
    Over,
}

struct Beam {
    row: i32,
    age: f32,
}

struct Streak {
    column: i32,
    top: i32,
    bottom: i32,
    hue: usize,
    age: f32,
}

struct Popup {
    text: String,
    row: f32,
    age: f32,
}

struct Lumenshaft {
    well: Well,
    phase: Phase,
    seed: u32,
    rng: Rng,
    sparks: Sparks,
    camera: Camera,
    unit: f32,
    backdrop: Light,
    backdrop_level: u32,
    fall: f32,
    resting: f32,
    shake: f32,
    lock_glow: HashMap<(i32, i32), f32>,
    sink: HashMap<i32, f32>,
    beams: Vec<Beam>,
    streaks: Vec<Streak>,
    popups: Vec<Popup>,
}

impl Lumenshaft {
    fn new(seed: u32) -> Self {
        Self {
            well: Well::new(seed),
            phase: Phase::Title,
            seed,
            rng: Rng::new(seed ^ 0xa5a5),
            sparks: Sparks::default(),
            camera: Camera {
                center: P2::default(),
                focal: 1.0,
                distance: EYE_DISTANCE,
                pixel_aspect: 1.0,
            },
            unit: 1.0,
            backdrop: Light::new(1, 1, 1.0),
            backdrop_level: 0,
            fall: 0.0,
            resting: 0.0,
            shake: 0.0,
            lock_glow: HashMap::new(),
            sink: HashMap::new(),
            beams: Vec::new(),
            streaks: Vec::new(),
            popups: Vec::new(),
        }
    }

    fn start(&mut self) {
        self.seed = self.seed.wrapping_mul(31).wrapping_add(7);
        self.well = Well::new(self.seed);
        self.phase = Phase::Playing;
        self.fall = 0.0;
        self.resting = 0.0;
        self.lock_glow.clear();
        self.sink.clear();
    }

    fn slam(&mut self) {
        let from = self.well.falling.clone();
        let ghost = self.well.ghost();
        let distance = from.y - ghost.y;
        self.well.falling = ghost;
        self.well.score += distance as u32 * 2;
        let squares: Vec<(i32, i32)> = self.well.falling.squares().collect();
        for (column, row) in &squares {
            let top = row + distance;
            self.streaks.push(Streak {
                column: *column,
                top,
                bottom: *row,
                hue: from.hue,
                age: 0.0,
            });
            self.dust(*column, *row, from.hue);
        }
        self.shake = self.shake.max(0.22);
        self.settle();
    }

    fn dust(&mut self, column: i32, row: i32, hue: usize) {
        for _ in 0..7 {
            let spread = self.rng.direction();
            self.sparks.add(Spark {
                at: cell_center(column, row),
                velocity: v3(spread.x * 4.0, spread.y.abs() * 3.0, -spread.z.abs() * 3.0),
                color: scaled(hue_of(hue), 2.2),
                age: 0.0,
                life: self.rng.range(0.25, 0.6),
                width: 1.5,
            });
        }
    }

    fn settle(&mut self) {
        let squares: Vec<(i32, i32)> = self.well.falling.squares().collect();
        let hues = self.well.filled.clone();
        let hue = self.well.falling.hue;
        let clear = self.well.lock();
        for (column, row) in squares {
            if !clear.rows.contains(&row) {
                let below = clear.rows.iter().filter(|gone| **gone < row).count() as i32;
                self.lock_glow.insert((column, row - below), 1.0);
            }
        }
        self.on_cleared(&clear, &hues, hue);
        self.fall = 0.0;
        self.resting = 0.0;
        if self.well.over {
            self.phase = Phase::Over;
            self.shatter_everything();
        }
    }

    fn on_cleared(&mut self, clear: &Clear, before: &HashMap<(i32, i32), usize>, fallback: usize) {
        for row in &clear.rows {
            self.beams.push(Beam {
                row: *row,
                age: 0.0,
            });
            for column in 0..COLUMNS {
                let hue = before.get(&(column, *row)).copied().unwrap_or(fallback);
                self.shards(column, *row, hue);
            }
        }
        if let Some(lowest) = clear.rows.first() {
            self.sink_rows_above(&clear.rows);
            self.shake = self.shake.max(0.25 + 0.2 * clear.rows.len() as f32);
            self.popups.push(Popup {
                text: format!("+{}", clear.points),
                row: *lowest as f32,
                age: 0.0,
            });
        }
    }

    fn sink_rows_above(&mut self, cleared: &[i32]) {
        for row in 0..ROWS {
            let fell = rows_fallen(cleared, row);
            if fell > 0 {
                *self.sink.entry(row).or_insert(0.0) += fell as f32;
            }
        }
    }

    fn shards(&mut self, column: i32, row: i32, hue: usize) {
        for _ in 0..10 {
            let direction = self.rng.direction();
            let speed = self.rng.range(5.0, 15.0);
            self.sparks.add(Spark {
                at: cell_center(column, row),
                velocity: v3(
                    direction.x * speed,
                    direction.y * speed,
                    -direction.z.abs() * speed * 1.3,
                ),
                color: scaled(hue_of(hue), 3.0),
                age: 0.0,
                life: self.rng.range(0.45, 1.0),
                width: 2.0,
            });
        }
    }

    fn shatter_everything(&mut self) {
        let cells: Vec<((i32, i32), usize)> = self
            .well
            .filled
            .iter()
            .map(|(at, hue)| (*at, *hue))
            .collect();
        for ((column, row), hue) in cells {
            self.shards(column, row, hue);
        }
        self.shake = 0.8;
    }

    fn decay_effects(&mut self, dt: f32) {
        self.shake = (self.shake - dt * 1.4).max(0.0);
        self.sparks.step(dt, v3(0.0, -11.0, 3.0));
        self.lock_glow.retain(|_, glow| {
            *glow -= dt * 3.0;
            *glow > 0.0
        });
        let settle = (-14.0 * dt).exp();
        self.sink.retain(|_, sink| {
            *sink *= settle;
            *sink > 0.01
        });
        self.beams.retain_mut(|beam| {
            beam.age += dt;
            beam.age < BEAM_LIFE
        });
        self.streaks.retain_mut(|streak| {
            streak.age += dt;
            streak.age < STREAK_LIFE
        });
        self.popups.retain_mut(|popup| {
            popup.age += dt;
            popup.row += dt * 2.0;
            popup.age < 1.0
        });
    }

    fn fall_step(&mut self, keys: &HeldKeys, dt: f32) {
        let speed = if keys.is_held(Key::Down) {
            SOFT_DROP_SPEED
        } else {
            1.0
        };
        self.fall += dt * speed / self.well.drop_interval();
        while self.fall >= 1.0 {
            self.fall -= 1.0;
            if self.well.shift(0, -1) {
                self.resting = 0.0;
            }
        }
        if self.well.is_grounded() {
            self.resting += dt;
            if self.resting >= LOCK_DELAY {
                self.settle();
            }
        }
    }

    fn bake_backdrop(&mut self, source: &Light) {
        let mut backdrop = Light::new(source.width(), source.height(), source.pixel_aspect());
        let tint = shaft_tint(self.well.level);
        let (width, height) = (source.width() as f32, source.height() as f32);
        backdrop.paint(|x, y| {
            let dx = (x as f32 / width - 0.5) * 1.4;
            let dy = y as f32 / height - 0.5;
            let vignette = (1.0 - (dx * dx + dy * dy) * 2.2).max(0.0);
            scaled(tint, 0.03 + vignette * vignette * 0.09)
        });
        draw_shaft(&mut backdrop, &self.camera, tint);
        backdrop.bloom(0.5, 0.5);
        self.backdrop = backdrop;
        self.backdrop_level = self.well.level;
    }

    fn shaken_camera(&mut self) -> Camera {
        let strength = self.shake * self.shake * self.unit;
        let mut camera = self.camera;
        camera.center.x += self.rng.range(-strength, strength);
        camera.center.y += self.rng.range(-strength, strength);
        camera
    }

    fn is_piece_in_play(&self) -> bool {
        self.phase == Phase::Playing || self.phase == Phase::Paused
    }

    fn draw_cubes(&self, light: &mut Light, camera: &Camera) {
        let settled = self.well.filled.iter().map(|(at, hue)| {
            let glow = self.lock_glow.get(at).copied().unwrap_or(0.0);
            let sink = self.sink.get(&at.1).copied().unwrap_or(0.0);
            (*at, sink, *hue, glow)
        });
        let falling = self
            .well
            .falling
            .squares()
            .filter(|_| self.is_piece_in_play())
            .map(|at| (at, 0.0, self.well.falling.hue, FALLING_GLOW));
        let mut cubes: Vec<((i32, i32), f32, usize, f32)> = settled.chain(falling).collect();
        cubes.sort_by(|a, b| distance_from_axis(b.0).total_cmp(&distance_from_axis(a.0)));
        for (at, sink, hue, glow) in cubes {
            draw_cube(light, camera, (at.0, at.1 as f32 + sink), hue_of(hue), glow);
        }
    }

    fn draw_ghost(&self, light: &mut Light, camera: &Camera) {
        if !self.is_piece_in_play() {
            return;
        }
        let color = scaled(hue_of(self.well.falling.hue), 0.55);
        for (column, row) in self.well.ghost().squares() {
            draw_outline(light, camera, (column, row as f32), color);
        }
    }

    fn draw_effects(&self, light: &mut Light, camera: &Camera) {
        for streak in &self.streaks {
            draw_streak(light, camera, streak);
        }
        self.sparks
            .draw(light, |at| camera.project(at).map(|spot| spot.0));
        for beam in &self.beams {
            draw_beam(light, camera, beam);
        }
    }

    fn draw_popups(&self, light: &mut Light, camera: &Camera, scale: u32) {
        for popup in &self.popups {
            let at = v3(0.0, popup.row + 2.0 - HALF_ROWS, 0.0);
            if let Some((spot, _)) = camera.project(at) {
                let fade = (1.0 - popup.age).max(0.0) * 3.0;
                light.centered_text(
                    spot.x,
                    spot.y,
                    &popup.text,
                    scale * 2,
                    [fade, fade, fade * 0.8],
                );
            }
        }
    }

    fn draw_panel(&self, light: &mut Light, camera: &Camera, scale: u32) {
        let reach = HALF_COLUMNS * camera.focal / EYE_DISTANCE / camera.pixel_aspect;
        let left = camera.center.x + reach + 8.0 * scale as f32;
        let line = Light::text_height(scale) * 1.9;
        let top = camera.center.y - HALF_ROWS * camera.focal / EYE_DISTANCE;
        let label = [0.45, 0.55, 0.8];
        let value = [1.5, 1.5, 1.7];
        let rows = [
            ("score", self.well.score),
            ("lines", self.well.lines),
            ("level", self.well.level),
        ];
        let mut y = top;
        for (name, number) in rows {
            light.text(P2::new(left, y), name, scale, label);
            light.text(
                P2::new(left, y + line),
                &number.to_string(),
                scale * 2,
                value,
            );
            y += line * 3.2;
        }
        light.text(P2::new(left, y), "next", scale, label);
        self.draw_next(light, P2::new(left, y + line * 1.4), scale);
    }

    fn draw_next(&self, light: &mut Light, at: P2, scale: u32) {
        let cube = 6.0 * scale as f32;
        let wide = cube / light.pixel_aspect();
        let color = hue_of(self.well.next);
        for (column, row) in shape_of(self.well.next) {
            let spot = P2::new(at.x + column as f32 * wide, at.y + (2 - row) as f32 * cube);
            light.rect(
                spot,
                wide - 1.0,
                cube - 1.0,
                scaled(color, 0.5),
                Blend::Over,
            );
            light.rect(
                P2::new(spot.x + wide * 0.2, spot.y + cube * 0.2),
                wide * 0.5,
                cube * 0.5,
                scaled(color, 1.4),
                Blend::Add,
            );
        }
    }

    fn draw_hints(light: &mut Light, camera: &Camera, scale: u32) {
        let hints = [
            "arrows move",
            "up turn",
            "down fall faster",
            "space drop",
            "p pause",
            "q quit",
        ];
        let reach = HALF_COLUMNS * camera.focal / EYE_DISTANCE / camera.pixel_aspect;
        let right = camera.center.x - reach - 8.0 * scale as f32;
        let line = Light::text_height(scale) * 1.9;
        let mut y = camera.center.y + HALF_ROWS * camera.focal / EYE_DISTANCE - line * 6.0;
        for hint in hints {
            let width = light.text_width(hint, scale);
            light.text(P2::new(right - width, y), hint, scale, [0.32, 0.4, 0.62]);
            y += line;
        }
    }

    fn draw_banner(&self, light: &mut Light, camera: &Camera, scale: u32) {
        let prompt = match self.phase {
            Phase::Playing => return,
            Phase::Title => "press enter to start",
            Phase::Paused => "press p to resume",
            Phase::Over => "press enter to play again",
        };
        light.dim(0.4);
        let center = camera.center;
        let tint = shaft_tint(self.well.level);
        light.centered_text(
            center.x,
            center.y - 16.0 * scale as f32,
            "lumenshaft",
            scale * 3,
            scaled([tint[0] + 0.6, tint[1] + 0.6, tint[2] + 0.6], 2.2),
        );
        light.centered_text(
            center.x,
            center.y + 12.0 * scale as f32,
            prompt,
            scale,
            [1.4; 3],
        );
    }
}

impl Game for Lumenshaft {
    fn press(&mut self, key: Key) {
        match (self.phase, key) {
            (Phase::Title | Phase::Over, Key::Enter) => self.start(),
            (Phase::Playing, Key::Char('p')) => self.phase = Phase::Paused,
            (Phase::Paused, Key::Char('p')) => self.phase = Phase::Playing,
            (Phase::Playing, Key::Left) => {
                self.well.shift(-1, 0);
            }
            (Phase::Playing, Key::Right) => {
                self.well.shift(1, 0);
            }
            (Phase::Playing, Key::Up) => {
                self.well.rotate();
            }
            (Phase::Playing, Key::Char(' ')) => self.slam(),
            _ => {}
        }
    }

    fn repeat(&mut self, key: Key) {
        if matches!(key, Key::Left | Key::Right) {
            self.press(key);
        }
    }

    fn resize(&mut self, light: &Light) {
        let height = light.height() as f32;
        let unit = height * SHAFT_SHARE_OF_HEIGHT / ROWS as f32;
        self.unit = unit;
        self.camera = Camera {
            center: P2::new(light.width() as f32 / 2.0, height / 2.0),
            focal: unit * EYE_DISTANCE,
            distance: EYE_DISTANCE,
            pixel_aspect: light.pixel_aspect(),
        };
        self.bake_backdrop(light);
    }

    fn step(&mut self, keys: &HeldKeys, dt: f32) {
        self.decay_effects(dt);
        if self.phase == Phase::Playing {
            self.fall_step(keys, dt);
        }
    }

    fn render(&mut self, light: &mut Light) {
        if self.backdrop_level != self.well.level {
            let source = light.clone();
            self.bake_backdrop(&source);
        }
        light.copy_from(&self.backdrop);
        let scale = (light.height() / 270).clamp(1, 4);
        let camera = self.shaken_camera();
        self.draw_ghost(light, &camera);
        self.draw_cubes(light, &camera);
        self.draw_effects(light, &camera);
        light.bloom(0.9, 0.45);
        self.draw_popups(light, &camera, scale);
        self.draw_panel(light, &self.camera, scale);
        Self::draw_hints(light, &self.camera, scale);
        self.draw_banner(light, &self.camera, scale);
    }
}

fn shaft_tint(level: u32) -> Rgb {
    let index = (level.saturating_sub(1) / 2) as usize % SHAFT_TINTS.len();
    SHAFT_TINTS.get(index).copied().unwrap_or([0.1, 0.4, 0.9])
}

fn rows_fallen(cleared: &[i32], row: i32) -> usize {
    cleared
        .iter()
        .enumerate()
        .filter(|(index, gone)| **gone - *index as i32 <= row)
        .count()
}

/// We paint cubes far from the eye's axis first, so that nearer ones cover
/// the sides of farther ones.
fn distance_from_axis(at: (i32, i32)) -> f32 {
    let center = cell_center(at.0, at.1);
    center.x.abs() + center.y.abs()
}

fn cell_center(column: i32, row: i32) -> V3 {
    v3(
        column as f32 + 0.5 - HALF_COLUMNS,
        row as f32 + 0.5 - HALF_ROWS,
        0.0,
    )
}

fn draw_shaft(light: &mut Light, camera: &Camera, tint: Rgb) {
    let place = |at: V3| camera.project(at).map(|spot| spot.0);
    let line_color = scaled(tint, 0.42);
    let far_color = scaled(tint, 0.2);
    for row in 0..=ROWS {
        let y = row as f32 - HALF_ROWS;
        for x in [-HALF_COLUMNS, HALF_COLUMNS] {
            depth_line(light, &place, v3(x, y, 0.0), line_color);
        }
        wall_line(
            light,
            &place,
            v3(-HALF_COLUMNS, y, SHAFT_DEPTH),
            v3(HALF_COLUMNS, y, SHAFT_DEPTH),
            far_color,
        );
    }
    for column in 0..=COLUMNS {
        let x = column as f32 - HALF_COLUMNS;
        for y in [-HALF_ROWS, HALF_ROWS] {
            depth_line(light, &place, v3(x, y, 0.0), line_color);
        }
        wall_line(
            light,
            &place,
            v3(x, -HALF_ROWS, SHAFT_DEPTH),
            v3(x, HALF_ROWS, SHAFT_DEPTH),
            far_color,
        );
    }
    let mouth = [
        v3(-HALF_COLUMNS, -HALF_ROWS, 0.0),
        v3(HALF_COLUMNS, -HALF_ROWS, 0.0),
        v3(HALF_COLUMNS, HALF_ROWS, 0.0),
        v3(-HALF_COLUMNS, HALF_ROWS, 0.0),
    ];
    for (from, to) in mouth.iter().zip(mouth.iter().cycle().skip(1)) {
        wall_line(light, &place, *from, *to, scaled(tint, 2.2));
    }
    if let Some(center) = place(v3(0.0, 0.0, SHAFT_DEPTH)) {
        light.glow(center, light.height() as f32 * 0.3, scaled(tint, 0.3));
    }
}

fn depth_line(light: &mut Light, place: &impl Fn(V3) -> Option<P2>, near: V3, color: Rgb) {
    wall_line(light, place, near, v3(near.x, near.y, SHAFT_DEPTH), color);
}

fn wall_line(light: &mut Light, place: &impl Fn(V3) -> Option<P2>, from: V3, to: V3, color: Rgb) {
    if let (Some(start), Some(end)) = (place(from), place(to)) {
        light.line(start, end, 1.0, color);
    }
}

fn face(light: &mut Light, camera: &Camera, corners: [V3; 4], color: Rgb) {
    let projected = corners.map(|corner| camera.project(corner).map(|spot| spot.0));
    if let [Some(a), Some(b), Some(c), Some(d)] = projected {
        light.polygon(&[a, b, c, d], color, Blend::Over);
    }
}

/// We draw only the sides facing the eye, which sits on the shaft's axis.
fn draw_cube_sides(light: &mut Light, camera: &Camera, x: (f32, f32), y: (f32, f32), color: Rgb) {
    let (left, right) = x;
    let (bottom, top) = y;
    let across = |at_y: f32| {
        [
            v3(left, at_y, 0.0),
            v3(right, at_y, 0.0),
            v3(right, at_y, CUBE_DEPTH),
            v3(left, at_y, CUBE_DEPTH),
        ]
    };
    let along = |at_x: f32| {
        [
            v3(at_x, bottom, 0.0),
            v3(at_x, top, 0.0),
            v3(at_x, top, CUBE_DEPTH),
            v3(at_x, bottom, CUBE_DEPTH),
        ]
    };
    if top < 0.0 {
        face(light, camera, across(top), scaled(color, 0.85));
    }
    if bottom > 0.0 {
        face(light, camera, across(bottom), scaled(color, 0.3));
    }
    if right < 0.0 {
        face(light, camera, along(right), scaled(color, 0.55));
    }
    if left > 0.0 {
        face(light, camera, along(left), scaled(color, 0.55));
    }
}

fn draw_cube(light: &mut Light, camera: &Camera, at: (i32, f32), color: Rgb, glow: f32) {
    let left = at.0 as f32 - HALF_COLUMNS;
    let bottom = at.1 - HALF_ROWS;
    let (right, top) = (left + 1.0, bottom + 1.0);
    draw_cube_sides(light, camera, (left, right), (bottom, top), color);
    let front = [
        v3(left, bottom, 0.0),
        v3(right, bottom, 0.0),
        v3(right, top, 0.0),
        v3(left, top, 0.0),
    ];
    face(light, camera, front, scaled(color, 0.28 + glow * 0.6));
    let inset = 0.17;
    let core = [
        v3(left + inset, bottom + inset, 0.0),
        v3(right - inset, bottom + inset, 0.0),
        v3(right - inset, top - inset, 0.0),
        v3(left + inset, top - inset, 0.0),
    ];
    face(light, camera, core, scaled(color, 1.0 + glow * 2.4));
    stroke(light, camera, front, scaled(color, 0.7 + glow));
}

fn draw_outline(light: &mut Light, camera: &Camera, at: (i32, f32), color: Rgb) {
    let left = at.0 as f32 - HALF_COLUMNS;
    let bottom = at.1 - HALF_ROWS;
    stroke(
        light,
        camera,
        [
            v3(left + 0.06, bottom + 0.06, 0.0),
            v3(left + 0.94, bottom + 0.06, 0.0),
            v3(left + 0.94, bottom + 0.94, 0.0),
            v3(left + 0.06, bottom + 0.94, 0.0),
        ],
        color,
    );
}

fn stroke(light: &mut Light, camera: &Camera, corners: [V3; 4], color: Rgb) {
    let projected = corners.map(|corner| camera.project(corner).map(|spot| spot.0));
    if let [Some(a), Some(b), Some(c), Some(d)] = projected {
        for (from, to) in [(a, b), (b, c), (c, d), (d, a)] {
            light.line(from, to, 1.0, color);
        }
    }
}

fn glowing_face(light: &mut Light, camera: &Camera, corners: [V3; 4], color: Rgb) {
    let projected = corners.map(|corner| camera.project(corner).map(|spot| spot.0));
    if let [Some(a), Some(b), Some(c), Some(d)] = projected {
        light.polygon(&[a, b, c, d], color, Blend::Add);
    }
}

fn draw_streak(light: &mut Light, camera: &Camera, streak: &Streak) {
    let fade = 1.0 - streak.age / STREAK_LIFE;
    let left = streak.column as f32 - HALF_COLUMNS + 0.3;
    let right = left + 0.4;
    let bottom = streak.bottom as f32 - HALF_ROWS + 0.5;
    let top = streak.top as f32 - HALF_ROWS + 1.0;
    let corners = [
        v3(left, bottom, 0.0),
        v3(right, bottom, 0.0),
        v3(right, top, 0.0),
        v3(left, top, 0.0),
    ];
    let color = scaled(hue_of(streak.hue), fade * fade * 0.7);
    glowing_face(light, camera, corners, color);
}

fn draw_beam(light: &mut Light, camera: &Camera, beam: &Beam) {
    let progress = beam.age / BEAM_LIFE;
    let fade = (1.0 - progress).powi(2);
    let half = 0.5 + progress * 1.6;
    let middle = beam.row as f32 + 0.5 - HALF_ROWS;
    let corners = [
        v3(-HALF_COLUMNS, middle - half, 0.0),
        v3(HALF_COLUMNS, middle - half, 0.0),
        v3(HALF_COLUMNS, middle + half, 0.0),
        v3(-HALF_COLUMNS, middle + half, 0.0),
    ];
    glowing_face(light, camera, corners, scaled([0.9, 0.95, 1.2], fade * 1.3));
}

#[cfg(test)]
#[path = "support/artifact.rs"]
mod artifact;

#[cfg(test)]
mod tests {
    use crate::artifact;

    use std::collections::HashSet;

    use katui_game::{Game, HeldKeys, Key, Light};

    use super::{COLUMNS, Lumenshaft, Phase, ROWS, SHAPES, Well, rows_fallen};

    const DT: f32 = 1.0 / 60.0;

    #[test]
    fn every_shape_keeps_its_size_through_four_turns() {
        for shape in 0..SHAPES.len() {
            let mut well = Well::new(11);
            well.falling.hue = shape;
            well.falling.cells = SHAPES[shape].to_vec();
            well.falling.x = 3;
            well.falling.y = 8;
            for _ in 0..4 {
                assert!(well.rotate());
                let unique: HashSet<_> = well.falling.squares().collect();
                assert_eq!(unique.len(), SHAPES[shape].len());
            }
        }
    }

    #[test]
    fn walls_stop_movement_and_a_turn_kicks_off_them() {
        let mut well = Well::new(3);
        well.falling.cells = SHAPES[0].to_vec();
        well.falling.x = 0;
        well.falling.y = 5;
        assert!(!well.shift(-1, 0));
        well.rotate();
        well.falling.x = COLUMNS - 1;
        assert!(well.rotate(), "the wall kick makes room");
        assert!(well.fits(&well.falling));
    }

    #[test]
    fn a_full_row_clears_and_the_rows_above_fall() {
        let mut well = Well::new(5);
        for column in 0..COLUMNS {
            well.filled.insert((column, 0), 0);
        }
        well.filled.insert((4, 1), 2);
        well.falling.cells = vec![(0, 0)];
        well.falling.x = 8;
        well.filled.remove(&(8, 0));
        well.falling.y = 0;
        let clear = well.lock();
        assert_eq!(clear.rows, vec![0]);
        assert_eq!(clear.points, 100);
        assert_eq!(well.filled.get(&(4, 0)), Some(&2));
        assert_eq!(well.lines, 1);
        assert_eq!(well.score, 100);
    }

    #[test]
    fn only_rows_above_a_clear_count_as_fallen() {
        assert_eq!(rows_fallen(&[5], 4), 0);
        assert_eq!(rows_fallen(&[5], 5), 1);
        assert_eq!(rows_fallen(&[5, 6], 4), 0);
        assert_eq!(rows_fallen(&[5, 6], 5), 2);
        assert_eq!(rows_fallen(&[2, 7], 5), 1);
    }

    #[test]
    fn a_turn_at_the_ceiling_stays_inside_the_shaft() {
        let mut well = Well::new(3);
        well.falling.cells = SHAPES[0].to_vec();
        well.falling.x = 3;
        well.falling.y = ROWS - 1;
        assert!(well.rotate());
        assert!(well.falling.squares().all(|(_, row)| row < ROWS));
    }

    #[test]
    fn a_blocked_spawn_ends_the_game() {
        let mut well = Well::new(5);
        for column in 0..COLUMNS {
            for row in ROWS - 3..ROWS {
                well.filled.insert((column, row), 1);
            }
        }
        well.spawn();
        assert!(well.over);
    }

    fn playing_frame() -> (Lumenshaft, Light) {
        let mut game = Lumenshaft::new(77);
        let mut light = Light::new(960, 540, 1.0);
        game.resize(&light);
        game.press(Key::Enter);
        let keys = HeldKeys::default();
        for row in 0..5 {
            for column in (0..COLUMNS).filter(|column| (column + row) % 4 != 0 || row == 0) {
                game.well
                    .filled
                    .insert((column, row), (column + row) as usize % 8);
            }
        }
        game.well.filled.retain(|at, _| at.0 != 4);
        game.well.falling.cells = vec![(0, 0)];
        game.well.falling.x = 4;
        game.well.falling.y = 12;
        game.slam();
        for _ in 0..6 {
            game.step(&keys, DT);
        }
        game.render(&mut light);
        (game, light)
    }

    #[test]
    fn slamming_into_a_gap_clears_the_row() {
        let (game, _) = playing_frame();
        assert_eq!(game.well.lines, 1);
        assert!(!game.sparks.is_empty(), "the cleared row throws shards");
    }

    #[test]
    fn a_played_frame_dumps_a_png_artifact() {
        let (game, light) = playing_frame();
        assert_eq!(game.phase, Phase::Playing);
        artifact::save_frame(&light, "lumenshaft");
    }

    #[test]
    fn a_small_screen_renders_without_trouble() {
        let mut game = Lumenshaft::new(1);
        let mut light = Light::new(160, 90, 0.5);
        game.resize(&light);
        game.render(&mut light);
        assert_eq!(game.phase, Phase::Title);
    }
}
