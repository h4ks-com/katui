//! The katui snake demo: a snake that plays itself on a bordered grid.
//! Run with `just demo-snake`.
#![allow(clippy::cast_precision_loss)]

use katui_game::{Blend, Game, HeldKeys, Light, P2, Rng, play, srgb};

const GRID_COLUMNS: i16 = 24;
const GRID_ROWS: i16 = 16;
const TICK_SECONDS: f32 = 0.1;

const GROUND: [u8; 3] = [16, 16, 24];
const BORDER: [u8; 3] = [44, 44, 64];
const BODY: [u8; 3] = [60, 220, 120];
const HEAD: [u8; 3] = [150, 255, 170];
const FOOD: [u8; 3] = [255, 80, 96];

fn main() -> Result<(), katui_rt::RtError> {
    play(Snake::new())
}

struct Run {
    body: Vec<(i16, i16)>,
    dir: (i16, i16),
    food: (i16, i16),
    rng: Rng,
}

impl Run {
    fn new() -> Self {
        let mut run = Self {
            body: vec![(GRID_COLUMNS / 2, GRID_ROWS / 2)],
            dir: (1, 0),
            food: (0, 0),
            rng: Rng::new(0x09e3_779b),
        };
        run.place_food();
        run
    }

    fn tick(&mut self) {
        let head = self.body.first().copied().unwrap_or((0, 0));
        let mut candidates = [
            self.dir,
            (self.dir.1, self.dir.0),
            (-self.dir.1, -self.dir.0),
        ];
        candidates.sort_by_key(|dir| {
            let next = (head.0 + dir.0, head.1 + dir.1);
            (next.0 - self.food.0).abs() + (next.1 - self.food.1).abs()
        });
        let safe = candidates.into_iter().find(|dir| {
            let next = (head.0 + dir.0, head.1 + dir.1);
            inside(next) && !self.body.contains(&next)
        });
        let Some(dir) = safe else {
            *self = Self::new();
            return;
        };
        self.dir = dir;
        let next = (head.0 + dir.0, head.1 + dir.1);
        self.body.insert(0, next);
        if next == self.food {
            self.place_food();
        } else {
            self.body.pop();
        }
    }

    fn place_food(&mut self) {
        for _ in 0..100 {
            let roll = self.rng.next_u32();
            let candidate = (
                grid_coordinate(roll, GRID_COLUMNS),
                grid_coordinate(roll >> 8, GRID_ROWS),
            );
            if !self.body.contains(&candidate) {
                self.food = candidate;
                return;
            }
        }
    }
}

fn inside(cell: (i16, i16)) -> bool {
    cell.0 >= 0 && cell.1 >= 0 && cell.0 < GRID_COLUMNS && cell.1 < GRID_ROWS
}

fn grid_coordinate(value: u32, bound: i16) -> i16 {
    i16::try_from(value % u32::from(bound.unsigned_abs())).unwrap_or(0)
}

/// Where the bordered board sits on screen, in whole pixels so cells
/// never smear into each other.
#[derive(Default)]
struct Board {
    origin: P2,
    cell: P2,
    inset: P2,
}

impl Board {
    fn fit(light: &Light) -> Self {
        let (columns, rows) = (f32::from(GRID_COLUMNS + 2), f32::from(GRID_ROWS + 2));
        let (width, height) = (light.width() as f32, light.height() as f32);
        let tall = (height / rows)
            .min(width * light.pixel_aspect() / columns)
            .floor()
            .max(1.0);
        let wide = (tall / light.pixel_aspect())
            .round()
            .min((width / columns).floor())
            .max(1.0);
        let cell = P2::new(wide, tall);
        Self {
            origin: P2::new(
                ((width - columns * wide) / 2.0).floor(),
                ((height - rows * tall) / 2.0).floor(),
            ),
            cell,
            inset: P2::new(
                f32::from(u8::from(wide > 4.0)),
                f32::from(u8::from(tall > 4.0)),
            ),
        }
    }

    /// Border cells sit at -1, the grid at 0 and up.
    fn fill(&self, light: &mut Light, (column, row): (i16, i16), shade: [u8; 3], inset: P2) {
        let [red, green, blue] = shade;

        let at = P2::new(
            self.origin.x + f32::from(column + 1) * self.cell.x + inset.x,
            self.origin.y + f32::from(row + 1) * self.cell.y + inset.y,
        );
        let (width, height) = (self.cell.x - inset.x * 2.0, self.cell.y - inset.y * 2.0);
        light.rect(at, width, height, srgb(red, green, blue), Blend::Over);
    }
}

struct Snake {
    run: Run,
    board: Board,
    backdrop: Light,
    since_tick: f32,
}

impl Snake {
    fn new() -> Self {
        Self {
            run: Run::new(),
            board: Board::default(),
            backdrop: Light::new(1, 1, 1.0),
            since_tick: 0.0,
        }
    }
}

impl Game for Snake {
    fn resize(&mut self, light: &Light) {
        self.board = Board::fit(light);
        let mut backdrop = Light::new(light.width(), light.height(), light.pixel_aspect());
        for row in -1..=GRID_ROWS {
            for column in -1..=GRID_COLUMNS {
                let shade = if inside((column, row)) {
                    GROUND
                } else {
                    BORDER
                };
                self.board
                    .fill(&mut backdrop, (column, row), shade, P2::default());
            }
        }
        self.backdrop = backdrop;
    }

    fn step(&mut self, _keys: &HeldKeys, dt: f32) {
        self.since_tick += dt;
        if self.since_tick >= TICK_SECONDS {
            self.since_tick -= TICK_SECONDS;
            self.run.tick();
        }
    }

    fn render(&mut self, light: &mut Light) {
        light.copy_from(&self.backdrop);
        let inset = self.board.inset;
        for (index, segment) in self.run.body.iter().enumerate() {
            let shade = if index == 0 { HEAD } else { BODY };
            self.board.fill(light, *segment, shade, inset);
        }
        self.board.fill(light, self.run.food, FOOD, inset);
    }
}

#[cfg(test)]
#[path = "support/artifact.rs"]
mod artifact;

#[cfg(test)]
mod tests {
    use crate::artifact;

    use katui_game::{Game, HeldKeys, Light};

    use super::{Run, Snake, inside};

    #[test]
    fn a_thousand_ticks_stay_inside_and_duplicate_free() {
        let mut run = Run::new();
        for _ in 0..1000 {
            run.tick();
            for (index, segment) in run.body.iter().enumerate() {
                assert!(inside(*segment), "segment {index} left the grid");
                assert_eq!(
                    run.body.iter().position(|other| other == segment),
                    Some(index),
                    "the body never crosses itself"
                );
            }
        }
    }

    #[test]
    fn the_snake_moves_ten_times_a_second() {
        let mut snake = Snake::new();
        for _ in 0..61 {
            snake.step(&HeldKeys::default(), 1.0 / 60.0);
        }
        let mut expected = Run::new();
        for _ in 0..10 {
            expected.tick();
        }
        assert_eq!(snake.run.body, expected.body);
    }

    #[test]
    fn a_played_frame_dumps_a_png_artifact() {
        let mut snake = Snake::new();
        let mut light = Light::new(960, 540, 1.0);
        snake.resize(&light);
        for _ in 0..900 {
            snake.run.tick();
        }
        assert!(
            snake.run.body.len() > Run::new().body.len(),
            "the snake grows as it eats"
        );
        snake.render(&mut light);
        artifact::save_frame(&light, "snake");
    }
}
