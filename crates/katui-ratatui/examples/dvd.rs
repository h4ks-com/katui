//! The katui dvd demo: the immortal bouncing logo, self-playing, color
//! per bounce, full-screen flash on the corner hit. Pixel surfaces over
//! the kitty protocol, quarter blocks everywhere else. Run with
//! `just demo-dvd`.

use std::io::{self, Write as _};
use std::time::Duration;

use katui_proto::probe::{Capabilities, CellPixels, Env};
use katui_rt::clock::FrameClock;
use katui_scene::placement::CellRect;
use katui_scene::surface::{Rgba, Surface};

/// 5x7 bitmap glyphs, one `u8` per row, high bit left. The whole font
/// this demo needs, in the tradition of embedded console fonts.
const GLYPHS: [(&str, [u8; 7]); 6] = [
    (
        "K",
        [
            0b10001, 0b10010, 0b10100, 0b11000, 0b10100, 0b10010, 0b10001,
        ],
    ),
    (
        "A",
        [
            0b01110, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001,
        ],
    ),
    (
        "T",
        [
            0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100,
        ],
    ),
    (
        "U",
        [
            0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110,
        ],
    ),
    (
        "I",
        [
            0b01110, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110,
        ],
    ),
    (
        "!",
        [
            0b00100, 0b00100, 0b00100, 0b00100, 0b00000, 0b00000, 0b00100,
        ],
    ),
];

const FRAMES: u64 = 900;

/// The logo's pixel footprint at one font-pixel per cell of `scale`.
const SCALE: u32 = 4;

const NIGHT: Rgba = Rgba::solid(12, 12, 18);
const FLASH: Rgba = Rgba::solid(255, 255, 255);

/// The classic dvd palette, cycled per bounce.
const PALETTE: [Rgba; 6] = [
    Rgba::solid(235, 51, 35),
    Rgba::solid(235, 97, 35),
    Rgba::solid(247, 181, 56),
    Rgba::solid(63, 181, 75),
    Rgba::solid(49, 133, 222),
    Rgba::solid(118, 48, 204),
];

fn logo_size() -> (u32, u32) {
    let columns = 6 * 6 - 1;
    (columns * SCALE, 7 * SCALE)
}

fn main() -> io::Result<()> {
    let caps = probe_capabilities();
    let (columns, rows) = katui_rt::tty::size().map_err(io::Error::other)?;
    let kitty = caps.kitty && caps.cell.is_some();
    let pixel_cell = match caps.cell {
        Some(cell) if kitty => cell,
        _ => CellPixels {
            width: 2,
            height: 2,
        },
    };
    let area = CellRect {
        x: 0,
        y: 0,
        columns,
        rows,
    };
    let width = u32::from(columns) * u32::from(pixel_cell.width);
    let height = u32::from(rows) * u32::from(pixel_cell.height);
    let mut game = Dvd::new(width, height);
    let mut presenter = katui_present::surface::KittySurface::new(700);
    let mut clock = FrameClock::new(30);
    let mut out = io::stdout();
    out.write_all(b"\x1b[?25l")?;
    for _ in 0..FRAMES {
        game.step();
        let mut surface = Surface::new(width, height);
        draw(&mut surface, &game);
        if kitty {
            presenter.render_frame(&mut out, &surface, area)?;
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

/// One logo on the move: position, velocity in pixels per frame, the
/// bounce palette index, the corner-hit flash counter, and the field the
/// logo bounces inside.
struct Dvd {
    x: i64,
    y: i64,
    dx: i64,
    dy: i64,
    color: Rgba,
    palette_index: usize,
    flash_frames: u32,
    rng: u32,
    bounds_width: u32,
    bounds_height: u32,
}

impl Dvd {
    fn new(width: u32, height: u32) -> Self {
        let (logo_width, logo_height) = logo_size();
        let span_x = width.saturating_sub(logo_width).max(1);
        let span_y = height.saturating_sub(logo_height).max(1);
        Self {
            x: i64::from(span_x / 4),
            y: i64::from(span_y / 4),
            // coprime speeds, so the loop never settles into a cycle
            dx: 7,
            dy: 5,
            color: PALETTE[0],
            palette_index: 0,
            flash_frames: 0,
            rng: 0x1b7f_159d,
            bounds_width: width,
            bounds_height: height,
        }
    }

    fn step(&mut self) {
        let (logo_width, logo_height) = logo_size();
        let limit_x = i64::from(self.bounds_width.saturating_sub(logo_width)).max(0);
        let limit_y = i64::from(self.bounds_height.saturating_sub(logo_height)).max(0);
        self.x += self.dx;
        self.y += self.dy;
        let mut bounced = false;
        if self.x <= 0 || self.x >= limit_x {
            self.dx = -self.dx;
            self.x = self.x.clamp(0, limit_x);
            bounced = true;
        }
        if self.y <= 0 || self.y >= limit_y {
            self.dy = -self.dy;
            self.y = self.y.clamp(0, limit_y);
            bounced = true;
        }
        if bounced {
            self.rng = self.rng.rotate_left(5) ^ self.rng.wrapping_mul(0x6d2b_79f5);
            let jump = 1 + (self.rng as usize) % 4;
            self.palette_index = (self.palette_index + jump) % PALETTE.len();
            if let Some(color) = PALETTE.get(self.palette_index) {
                self.color = *color;
            }
        }
        let at_left = self.x <= 0;
        let at_right = self.x >= limit_x;
        let at_top = self.y <= 0;
        let at_bottom = self.y >= limit_y;
        if (at_left || at_right) && (at_top || at_bottom) {
            self.flash_frames = 6;
        }
        self.flash_frames = self.flash_frames.saturating_sub(1);
    }
}
/// Draw the night, the flash, and the logo.
fn draw(surface: &mut Surface, dvd: &Dvd) {
    surface.fill_rect(
        katui_scene::surface::PixelRect {
            x: 0,
            y: 0,
            width: surface.width(),
            height: surface.height(),
        },
        NIGHT,
    );
    if dvd.flash_frames > 0 {
        surface.fill_rect(
            katui_scene::surface::PixelRect {
                x: 0,
                y: 0,
                width: surface.width(),
                height: surface.height(),
            },
            FLASH,
        );
    }
    let (logo_width, logo_height) = logo_size();
    let pen_x = u32::try_from(dvd.x.max(0))
        .unwrap_or(0)
        .min(surface.width().saturating_sub(logo_width));
    let pen_y = u32::try_from(dvd.y.max(0))
        .unwrap_or(0)
        .min(surface.height().saturating_sub(logo_height));
    for (position, (_, rows)) in GLYPHS.iter().enumerate() {
        for (row, bits) in rows.iter().enumerate() {
            for column in 0..5u8 {
                let lit = (bits << column) & 0b10000 != 0;
                if lit {
                    surface.fill_rect(
                        katui_scene::surface::PixelRect {
                            x: pen_x + (position as u32 * 6 + u32::from(column)) * SCALE,
                            y: pen_y + row as u32 * SCALE,
                            width: SCALE,
                            height: SCALE,
                        },
                        dvd.color,
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Dvd, FRAMES, draw};
    use katui_scene::surface::Surface;

    #[test]
    fn the_logo_stays_inside_and_bounces_forever() {
        let mut dvd = Dvd::new(640, 360);
        let mut flashes = 0;
        for _ in 0..20_000 {
            dvd.step();
            let (logo_width, logo_height) = super::logo_size();
            assert!(dvd.x >= 0 && dvd.y >= 0, "never above the top left");
            assert!(
                dvd.x + i64::from(logo_width) <= i64::from(dvd.bounds_width),
                "never past the right edge"
            );
            assert!(
                dvd.y + i64::from(logo_height) <= i64::from(dvd.bounds_height),
                "never past the bottom"
            );
            if dvd.flash_frames > 0 {
                flashes += 1;
            }
        }
        assert!(flashes > 0, "a corner is eventually hit");
    }

    #[test]
    fn the_logo_actually_draws_colored_pixels() {
        let mut dvd = Dvd::new(640, 360);
        for _ in 0..50 {
            dvd.step();
        }
        let mut surface = Surface::new(640, 360);
        draw(&mut surface, &dvd);
        let mut logo_pixels = 0;
        for y in 0..surface.height() {
            for x in 0..surface.width() {
                if let Some(color) = surface.get(x, y)
                    && color != super::NIGHT
                    && color != super::FLASH
                {
                    logo_pixels += 1;
                }
            }
        }
        // six glyphs light 74 font pixels; at 4x4 scale that is the logo
        assert!(
            logo_pixels >= 74 * super::SCALE * super::SCALE,
            "the six glyphs paint a real logo, found {logo_pixels}"
        );
    }

    #[test]
    fn a_headless_run_streams_alternating_frames() {
        let mut presenter = katui_present::surface::KittySurface::new(700);
        let mut dvd = Dvd::new(320, 180);
        let area = katui_scene::placement::CellRect {
            x: 0,
            y: 0,
            columns: 80,
            rows: 24,
        };
        let mut wire = Vec::new();
        for _ in 0..FRAMES {
            dvd.step();
            let mut surface = Surface::new(320, 180);
            draw(&mut surface, &dvd);
            presenter
                .render_frame(&mut wire, &surface, area)
                .expect("writing to a Vec cannot fail");
        }
        let text = String::from_utf8_lossy(&wire);
        assert!(text.contains("i=700,"), "the first alternating id runs");
        assert!(text.contains("i=701,"), "the second alternating id runs");
    }
}
