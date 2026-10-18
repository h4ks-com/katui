//! The katui snake demo: a self-playing game on a pixel surface, full
//! frames over the kitty graphics protocol, quarter blocks everywhere
//! else. Run with `just demo-snake`.

use std::io::{self, Write as _};
use std::time::Duration;

use katui_proto::probe::{Capabilities, CellPixels, Env};
use katui_rt::clock::FrameClock;
use katui_scene::placement::CellRect;
use katui_scene::surface::{Rgba, Surface};

const GRID_COLUMNS: i16 = 24;
const GRID_ROWS: i16 = 16;
const FRAMES: u64 = 400;

/// Game colors, on dark ground.
const GROUND: Rgba = Rgba::solid(16, 16, 24);
const BORDER: Rgba = Rgba::solid(44, 44, 64);
const SNAKE: Rgba = Rgba::solid(60, 220, 120);
const HEAD: Rgba = Rgba::solid(150, 255, 170);
const FOOD: Rgba = Rgba::solid(255, 80, 96);

fn main() -> io::Result<()> {
    let caps = probe_capabilities();
    let pixel_cell = match caps.cell {
        Some(cell) if caps.kitty => cell,
        _ => CellPixels {
            width: 2,
            height: 2,
        },
    };
    let kitty = caps.kitty && caps.cell.is_some();
    let board_columns = u32::from(GRID_COLUMNS as u16 + 2);
    let board_rows = u32::from(GRID_ROWS as u16 + 2);
    let mut presenter = katui_present::surface::KittySurface::new(500);
    let mut game = Game::new();
    let mut clock = FrameClock::new(20);
    let mut out = io::stdout();
    out.write_all(b"\x1b[?25l")?;
    for frame in 0..FRAMES {
        if frame % 2 == 0 {
            game.tick();
        }
        let mut surface = Surface::new(
            board_columns * u32::from(pixel_cell.width),
            board_rows * u32::from(pixel_cell.height),
        );
        draw(&mut surface, &game, pixel_cell);
        if kitty {
            presenter.render_frame(
                &mut out,
                &surface,
                CellRect {
                    x: 1,
                    y: 1,
                    columns: board_columns as u16,
                    rows: board_rows as u16,
                },
            )?;
        } else {
            katui_present::surface::render_blocks(&mut out, &surface)?;
        }
        out.flush()?;
        clock.wait_for_next_frame();
    }
    out.write_all(b"\x1b_Ga=d,d=a,q=2\x1b\\")?;
    out.write_all(b"\x1b[0m\x1b[?25h\x1b[H")?;
    out.flush()
}

fn probe_capabilities() -> Capabilities {
    let env = Env::capture();
    match katui_rt::tty::StdTransport::open(Duration::from_millis(300)) {
        Ok(mut transport) => katui_proto::probe::probe(&mut transport, &env),
        Err(_) => katui_proto::probe::sniff(&env),
    }
}

/// One snake run: body head-first, a direction, food, and a tiny
/// xorshift so no random dependency rides along.
struct Game {
    body: Vec<(i16, i16)>,
    dir: (i16, i16),
    food: (i16, i16),
    rng: u32,
}

impl Game {
    fn new() -> Self {
        let mut game = Self {
            body: vec![(GRID_COLUMNS / 2, GRID_ROWS / 2)],
            dir: (1, 0),
            food: (0, 0),
            rng: 0x09e3_779b,
        };
        game.place_food();
        game
    }

    fn tick(&mut self) {
        let head = self.body.first().copied().unwrap_or((0, 0));
        let mut candidates = [
            self.dir,
            (self.dir.0, -self.dir.1),
            (-self.dir.0, self.dir.1),
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
            self.rng = self.rng.rotate_left(7) ^ self.rng.wrapping_mul(0x02f1_cd33);
            let candidate = (
                grid_coordinate(self.rng, GRID_COLUMNS),
                grid_coordinate(self.rng >> 8, GRID_ROWS),
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

/// A random grid coordinate under `bound`, cast-free.
fn grid_coordinate(value: u32, bound: i16) -> i16 {
    i16::try_from(value % u32::from(bound.unsigned_abs())).unwrap_or(0)
}

/// Paint the game: a one-cell border, then every body cell and the food,
/// each inset a pixel where the cell is big enough to carry the gap.
fn draw(surface: &mut Surface, game: &Game, cell: CellPixels) {
    let inset = u32::from(cell.width > 4 && cell.height > 4);
    for row in -1..=GRID_ROWS {
        for column in -1..=GRID_COLUMNS {
            let color = if inside((column, row)) {
                GROUND
            } else {
                BORDER
            };
            fill_cell(surface, column, row, cell, color, 0);
        }
    }
    for (index, segment) in game.body.iter().enumerate() {
        let color = if index == 0 { HEAD } else { SNAKE };
        fill_cell(surface, segment.0, segment.1, cell, color, inset);
    }
    fill_cell(surface, game.food.0, game.food.1, cell, FOOD, inset);
}

fn fill_cell(
    surface: &mut Surface,
    column: i16,
    row: i16,
    cell: CellPixels,
    color: Rgba,
    inset: u32,
) {
    // the border cells sit at -1 and wrap to the surface edge through the
    // +1 offset below, which unsigned arithmetic carries exactly
    let x = u32::from(column.cast_unsigned() + 1) * u32::from(cell.width) + inset;
    let y = u32::from(row.cast_unsigned() + 1) * u32::from(cell.height) + inset;
    surface.fill_rect(
        katui_scene::surface::PixelRect {
            x,
            y,
            width: u32::from(cell.width) - inset * 2,
            height: u32::from(cell.height) - inset * 2,
        },
        color,
    );
}
