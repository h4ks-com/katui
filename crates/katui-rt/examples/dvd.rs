//! The katui dvd demo: the immortal bouncing logo over a starfield,
//! self-escalating, exit any time with `q`, `Esc` or `Ctrl-C`. Pixel
//! surfaces over the kitty protocol, quarter blocks everywhere else.
//! Run with `just demo-dvd`.
#![cfg(unix)] // the demos drive /dev/tty through katui-rt's unix layer

use std::collections::VecDeque;
use std::io::{self, Write as _};

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

/// A sixty second backstop; the quit key ends it sooner.
const MAX_FRAMES: u64 = 1800;

/// The classic dvd palette, cycled per bounce.
const PALETTE: [Rgba; 6] = [
    Rgba::solid(235, 51, 35),
    Rgba::solid(235, 97, 35),
    Rgba::solid(247, 181, 56),
    Rgba::solid(63, 181, 75),
    Rgba::solid(49, 133, 222),
    Rgba::solid(118, 48, 204),
];

const FLASH: Rgba = Rgba::solid(255, 255, 255);

/// One twinkling star: a fixed pixel, its own sine, and the frame it
/// last changed shine. Stars re-brighten on a staggered cadence so the
/// damage per frame stays a scatter, not a storm.
struct Star {
    x: u32,
    y: u32,
    phase: f64,
    speed: f64,
    last_frame: u64,
    shine: u8,
}

impl Star {
    #[allow(clippy::cast_sign_loss, clippy::cast_precision_loss)]
    // brightness stays inside 0..=255 by construction; frame counts
    // never leave the f64 mantissa range
    fn shine_at(phase: f64, speed: f64, frame: u64) -> u8 {
        let brightness = 90.0 + 90.0 * (phase + speed * frame as f64).sin();
        brightness as u8
    }

    /// Repaint at most every 16 frames; unchanged brightness keeps the
    /// old value and costs no bytes.
    fn repaint(&mut self, frame: u64) -> bool {
        if frame.saturating_sub(self.last_frame) < 16 {
            return false;
        }
        let shine = Self::shine_at(self.phase, self.speed, frame);
        if shine == self.shine {
            return false;
        }
        self.shine = shine;
        self.last_frame = frame;
        true
    }
}

/// The precomputed backdrop plus its stars, built once for the run.
struct Backdrop {
    gradient: Surface,
    stars: Vec<Star>,
}

fn main() -> io::Result<()> {
    let caps = katui_rt::tty::probe_default();
    let size = katui_rt::tty::size().map_err(io::Error::other)?;
    let (columns, rows) = (size.columns, size.rows);
    // the pixel-cell fallback lives beside the probe it belongs to
    let kitty = caps.kitty;
    let pixel_cell = katui_rt::tty::pixel_cell(&caps);
    let width = u32::from(columns) * u32::from(pixel_cell.width);
    let height = u32::from(rows) * u32::from(pixel_cell.height);
    let scale = font_scale(width, height);
    let mut dvd = Dvd::new(width, height, scale);
    let mut backdrop = build_backdrop(width, height);
    let mut presenter = katui_present::surface::PatchedKitty::new(700, pixel_cell);
    let mut blocks = katui_present::surface::BlocksSurface::new();
    let stage = katui_rt::session::Stage::enter(
        io::stdout(),
        katui_rt::session::Teardown {
            graphics: true,
            home: true,
            clear: true,
        },
    )
    .map_err(io::Error::other)?;

    stage
        .run(30, MAX_FRAMES, |out, frame| {
            dvd.step();
            let mut surface = Surface::new(width, height);
            draw_frame(&mut surface, &mut backdrop, &dvd, frame);
            if kitty {
                presenter.render_frame(
                    out,
                    &surface,
                    CellRect {
                        x: 0,
                        y: 0,
                        columns,
                        rows,
                    },
                )?;
            } else {
                blocks.render_frame(out, &surface)?;
            }
            out.flush()?;
            Ok(true)
        })
        .map_err(io::Error::other)
}

/// One font pixel covers `scale` screen pixels; size the logo to roughly
/// a third of the screen without either axis dominating.
fn font_scale(width: u32, height: u32) -> u32 {
    let by_height = height / 28;
    let by_width = width / 88;
    by_height.min(by_width).clamp(4, 40)
}

/// One logo on the move, escalating with every corner hit.
struct Dvd {
    x: i64,
    y: i64,
    dx: i64,
    dy: i64,
    base_dx: i64,
    base_dy: i64,
    color: Rgba,
    palette_index: usize,
    flash_frames: u32,
    rng: u32,
    logo_width: u32,
    logo_height: u32,
    bounds_width: u32,
    bounds_height: u32,
    scale: u32,
    trail: VecDeque<(i64, i64)>,
}

impl Dvd {
    fn new(width: u32, height: u32, scale: u32) -> Self {
        let logo_width = 35 * scale;
        let logo_height = 7 * scale;
        let span_x = width.saturating_sub(logo_width).max(1);
        let span_y = height.saturating_sub(logo_height).max(1);
        // a screen crossing every ~70 frames, coprime so the path never
        // closes into a loop
        let dx = i64::from((width / 70).max(8));
        let dy = i64::from((height / 70).max(6));
        Self {
            x: i64::from(span_x / 3),
            y: i64::from(span_y / 3),
            dx,
            dy,
            base_dx: dx,
            base_dy: dy,
            color: PALETTE[0],
            palette_index: 0,
            flash_frames: 0,
            rng: 0x1b7f_159d,
            logo_width,
            logo_height,
            bounds_width: width,
            bounds_height: height,
            scale,
            trail: VecDeque::new(),
        }
    }

    fn step(&mut self) {
        let limit_x = i64::from(self.bounds_width.saturating_sub(self.logo_width));
        let limit_y = i64::from(self.bounds_height.saturating_sub(self.logo_height));
        self.x += self.dx;
        self.y += self.dy;
        let hit_x = self.x <= 0 || self.x >= limit_x;
        let hit_y = self.y <= 0 || self.y >= limit_y;
        if hit_x {
            self.dx = -self.dx;
            self.x = self.x.clamp(0, limit_x);
        }
        if hit_y {
            self.dy = -self.dy;
            self.y = self.y.clamp(0, limit_y);
        }
        if hit_x || hit_y {
            self.shift_color();
        }
        if hit_x && hit_y {
            self.flash_frames = 6;
            self.escalate();
        }
        self.flash_frames = self.flash_frames.saturating_sub(1);
        self.trail.push_front((self.x, self.y));
        while self.trail.len() > 5 {
            self.trail.pop_back();
        }
    }

    fn shift_color(&mut self) {
        self.rng = self.rng.rotate_left(5) ^ self.rng.wrapping_mul(0x6d2b_79f5);
        let jump = 1 + (self.rng as usize) % 4;
        self.palette_index = (self.palette_index + jump) % PALETTE.len();
        if let Some(color) = PALETTE.get(self.palette_index) {
            self.color = *color;
        }
    }

    /// Every corner hit speeds the logo up, capped at twice the start.
    fn escalate(&mut self) {
        let limit_x = self.base_dx * 2;
        let limit_y = self.base_dy * 2;
        let bump_x = (self.base_dx / 8).max(1) * self.dx.signum();
        let bump_y = (self.base_dy / 8).max(1) * self.dy.signum();
        self.dx = (self.dx + bump_x).clamp(-limit_x, limit_x);
        self.dy = (self.dy + bump_y).clamp(-limit_y, limit_y);
    }
}

/// A vertical dusk gradient with a scatter of stars.
fn build_backdrop(width: u32, height: u32) -> Backdrop {
    let mut gradient = Surface::new(width, height);
    let top = [46.0, 14.0, 88.0];
    let bottom = [4.0, 6.0, 18.0];
    for row in 0..height {
        let share = f64::from(row) / f64::from(height.max(1));
        let channel = |stop_a: f64, stop_b: f64| -> u8 {
            #[allow(clippy::cast_sign_loss)]
            // both stops sit inside 0..=255 and share stays inside 0..=1
            {
                (stop_a + (stop_b - stop_a) * share) as u8
            }
        };
        gradient.fill_rect(
            katui_scene::surface::PixelRect {
                x: 0,
                y: row,
                width,
                height: 1,
            },
            Rgba::solid(
                channel(top[0], bottom[0]),
                channel(top[1], bottom[1]),
                channel(top[2], bottom[2]),
            ),
        );
    }
    let mut rng = 0x2545_f491_u32;
    let mut stars = Vec::new();
    for _ in 0..(width / 12).max(40) {
        rng = rng.rotate_left(7) ^ rng.wrapping_mul(0x02f1_cd33);
        let x = rng % width.max(1);
        rng = rng.rotate_left(3) ^ rng.wrapping_mul(0x811c_9dc5);
        let y = rng % height.max(1);
        let phase = f64::from(rng % 628) / 100.0;
        let speed = 0.02 + f64::from(rng % 7) * 0.01;
        stars.push(Star {
            x,
            y,
            phase,
            speed,
            last_frame: 0,
            shine: Star::shine_at(phase, speed, 0),
        });
    }
    Backdrop { gradient, stars }
}

/// One whole frame: backdrop, every star at its current shine, the
/// trail of ghosts, the logo, the corner flash.
fn draw_frame(surface: &mut Surface, backdrop: &mut Backdrop, dvd: &Dvd, frame: u64) {
    surface.blit(&backdrop.gradient, 0, 0);
    for star in &mut backdrop.stars {
        star.repaint(frame);
        surface.set(
            katui_scene::surface::PixelPos::new(star.x, star.y),
            Rgba::solid(star.shine, star.shine, star.shine),
        );
    }
    for (index, (ghost_x, ghost_y)) in dvd.trail.iter().enumerate().skip(1) {
        let dim = 1 + index as u8;
        draw_logo(
            surface,
            *ghost_x,
            *ghost_y,
            dvd.scale,
            dim_color(dvd.color, dim),
        );
    }
    draw_logo(surface, dvd.x, dvd.y, dvd.scale, dvd.color);
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
}

fn dim_color(color: Rgba, dim: u8) -> Rgba {
    Rgba::solid(color.r / dim, color.g / dim, color.b / dim)
}

fn draw_logo(surface: &mut Surface, pen_x: i64, pen_y: i64, scale: u32, color: Rgba) {
    let logo_width = 35 * scale;
    let logo_height = 7 * scale;
    let origin_x = u32::try_from(pen_x.max(0))
        .unwrap_or(0)
        .min(surface.width().saturating_sub(logo_width));
    let origin_y = u32::try_from(pen_y.max(0))
        .unwrap_or(0)
        .min(surface.height().saturating_sub(logo_height));
    for (position, (_, rows)) in GLYPHS.iter().enumerate() {
        for (row, bits) in rows.iter().enumerate() {
            for column in 0..5u8 {
                if (bits << column) & 0b10000 != 0 {
                    surface.fill_rect(
                        katui_scene::surface::PixelRect {
                            x: origin_x + (position as u32 * 6 + u32::from(column)) * scale,
                            y: origin_y + row as u32 * scale,
                            width: scale,
                            height: scale,
                        },
                        color,
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Dvd, MAX_FRAMES, build_backdrop, draw_frame, draw_logo, font_scale};
    use katui_scene::surface::{Rgba, Surface};

    #[test]
    fn the_font_scale_grows_with_the_screen() {
        assert!(font_scale(2000, 1200) >= 14, "a big screen gets a big logo");
        assert!(font_scale(320, 180) <= 6, "a tiny screen stays small");
    }

    #[test]
    fn the_logo_stays_inside_and_escalates() {
        let mut dvd = Dvd::new(1920, 1080, font_scale(1920, 1080));
        let speed_before = dvd.dx.abs() + dvd.dy.abs();
        let mut flashes = 0;
        for _ in 0..60_000 {
            dvd.step();
            assert!(dvd.x >= 0 && dvd.y >= 0, "never above the top left");
            assert!(
                dvd.x + i64::from(dvd.logo_width) <= i64::from(dvd.bounds_width),
                "never past the right edge"
            );
            assert!(
                dvd.y + i64::from(dvd.logo_height) <= i64::from(dvd.bounds_height),
                "never past the bottom"
            );
            if dvd.flash_frames > 0 {
                flashes += 1;
            }
        }
        assert!(flashes > 0, "a corner is eventually hit");
        assert!(
            dvd.dx.abs() + dvd.dy.abs() > speed_before,
            "corner hits escalate the pace"
        );
    }

    #[test]
    fn the_speed_cap_holds() {
        let mut dvd = Dvd::new(1920, 1080, 8);
        for _ in 0..200_000 {
            dvd.step();
            assert!(dvd.dx.abs() <= dvd.base_dx * 2, "x speed stays capped");
            assert!(dvd.dy.abs() <= dvd.base_dy * 2, "y speed stays capped");
        }
    }

    #[test]
    fn the_backdrop_paints_every_pixel_and_stars_twinkle() {
        let mut backdrop = build_backdrop(160, 90);
        let mut surface = Surface::new(160, 90);
        draw_frame(&mut surface, &mut backdrop, &Dvd::new(160, 90, 4), 0);
        for y in 0..90 {
            for x in 0..160 {
                assert!(
                    surface
                        .get(katui_scene::surface::PixelPos::new(x, y))
                        .is_some(),
                    "every pixel is painted"
                );
            }
        }
        let first = surface;
        let mut second = Surface::new(160, 90);
        draw_frame(&mut second, &mut backdrop, &Dvd::new(160, 90, 4), 64);
        let mut changed = 0;
        for y in 0..90 {
            for x in 0..160 {
                if first.get(katui_scene::surface::PixelPos::new(x, y))
                    != second.get(katui_scene::surface::PixelPos::new(x, y))
                {
                    changed += 1;
                }
            }
        }
        assert!(changed > 0, "stars twinkle between frames");
    }

    #[test]
    fn the_logo_paints_at_any_scale() {
        for scale in [4u32, 12, 24] {
            let mut surface = Surface::new(1600, 900);
            draw_logo(&mut surface, 100, 100, scale, Rgba::solid(255, 0, 0));
            let mut lit = 0usize;
            for y in 0..900u32 {
                for x in 0..1600u32 {
                    if surface.get(katui_scene::surface::PixelPos::new(x, y))
                        == Some(Rgba::solid(255, 0, 0))
                    {
                        lit += 1;
                    }
                }
            }
            assert_eq!(
                lit,
                (74 * scale * scale) as usize,
                "scale {scale} keeps the glyphs crisp"
            );
        }
    }

    #[test]
    fn a_headless_run_transmits_once_then_patches() {
        let mut presenter = katui_present::surface::PatchedKitty::new(
            700,
            katui_proto::probe::CellPixels {
                width: 4,
                height: 6,
            },
        );
        let mut backdrop = build_backdrop(320, 180);
        let mut dvd = Dvd::new(320, 180, 4);
        let area = katui_scene::placement::CellRect {
            x: 0,
            y: 0,
            columns: 80,
            rows: 24,
        };
        let mut wire = Vec::new();
        for frame in 0..MAX_FRAMES.min(120) {
            dvd.step();
            let mut surface = Surface::new(320, 180);
            draw_frame(&mut surface, &mut backdrop, &dvd, frame);
            presenter
                .render_frame(&mut wire, &surface, area)
                .expect("writing to a Vec cannot fail");
        }
        let text = String::from_utf8_lossy(&wire);
        // whole-surface transmits (s=320) stop after the first frame;
        // tile transmits are the small ones and keep coming
        assert!(
            text.matches("s=320,v=180").count() <= 2,
            "at most the first frame and one flash retransmit whole"
        );
        assert!(
            text.matches("a=t").count() > 2,
            "every moving frame retransmits its damaged tiles"
        );
        assert!(text.contains("a=p,i=700"), "the base stays placed");
    }
}
