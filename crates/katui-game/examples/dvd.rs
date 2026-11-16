//! The bouncing dvd logo over a starfield, speeding up on every corner
//! hit. Run with `just demo-dvd`.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]

use std::collections::VecDeque;
use std::f32::consts::TAU;

use katui_game::{Blend, Game, HeldKeys, Light, P2, Rgb, Rng, play, srgb};

const LOGO: &str = "KATUI!";

const PALETTE: [[u8; 3]; 6] = [
    [235, 51, 35],
    [235, 97, 35],
    [247, 181, 56],
    [63, 181, 75],
    [49, 133, 222],
    [118, 48, 204],
];
const DUSK_TOP: [f32; 3] = [46.0, 14.0, 88.0];
const DUSK_BOTTOM: [f32; 3] = [4.0, 6.0, 18.0];

const CROSSING_SECONDS: f32 = 70.0 / 30.0;
const FLASH_SECONDS: f32 = 0.2;
const TRAIL_INTERVAL: f32 = 1.0 / 30.0;
const TRAIL_LENGTH: usize = 5;
/// We hold each star's shine for a while so a frame repaints a few
/// scattered pixels, not the whole sky.
const TWINKLE_INTERVAL: f32 = 16.0 / 30.0;

fn main() -> Result<(), katui_rt::RtError> {
    play(Dvd::new(0x2545_f491))
}

struct Star {
    at: P2,
    phase: f32,
    speed: f32,
}

impl Star {
    fn shine(&self, seconds: f32) -> u8 {
        let held = (seconds / TWINKLE_INTERVAL).floor() * TWINKLE_INTERVAL;
        (90.0 + 90.0 * (self.phase + self.speed * held).sin()) as u8
    }
}

struct Logo {
    at: P2,
    velocity: P2,
    base_speed: P2,
    font_scale: u32,
    size: P2,
    bounds: P2,
    palette_index: usize,
    flash: f32,
    trail: VecDeque<P2>,
    since_trail: f32,
}

impl Logo {
    fn new(light: &Light) -> Self {
        let bounds = P2::new(light.width() as f32, light.height() as f32);
        let font_scale = (bounds.y / 28.0)
            .min(bounds.x * light.pixel_aspect() / 88.0)
            .floor()
            .max(1.0) as u32;
        let size = P2::new(
            light.text_width(LOGO, font_scale),
            Light::text_height(font_scale),
        );
        let base_speed = P2::new(bounds.x / CROSSING_SECONDS, bounds.y / CROSSING_SECONDS);
        Self {
            at: P2::new(
                (bounds.x - size.x).max(0.0) / 3.0,
                (bounds.y - size.y).max(0.0) / 3.0,
            ),
            velocity: base_speed,
            base_speed,
            font_scale,
            size,
            bounds,
            palette_index: 0,
            flash: 0.0,
            trail: VecDeque::new(),
            since_trail: 0.0,
        }
    }

    fn step(&mut self, dt: f32, rng: &mut Rng) {
        let limit = P2::new(
            (self.bounds.x - self.size.x).max(0.0),
            (self.bounds.y - self.size.y).max(0.0),
        );
        self.at.x += self.velocity.x * dt;
        self.at.y += self.velocity.y * dt;
        let hit_x = bounce(&mut self.at.x, &mut self.velocity.x, limit.x);
        let hit_y = bounce(&mut self.at.y, &mut self.velocity.y, limit.y);
        if hit_x || hit_y {
            let jump = 1 + rng.next_u32() as usize % 4;
            self.palette_index = (self.palette_index + jump) % PALETTE.len();
        }
        if hit_x && hit_y {
            self.flash = FLASH_SECONDS;
            self.escalate();
        }
        self.flash = (self.flash - dt).max(0.0);
        self.follow(dt);
    }

    fn escalate(&mut self) {
        let bump = |speed: f32, base: f32| {
            (speed + base / 8.0 * speed.signum()).clamp(-base * 2.0, base * 2.0)
        };
        self.velocity = P2::new(
            bump(self.velocity.x, self.base_speed.x),
            bump(self.velocity.y, self.base_speed.y),
        );
    }

    fn follow(&mut self, dt: f32) {
        self.since_trail += dt;
        if self.since_trail >= TRAIL_INTERVAL {
            self.since_trail -= TRAIL_INTERVAL;
            self.trail.push_front(self.at);
            self.trail.truncate(TRAIL_LENGTH);
        }
    }

    fn color(&self) -> [u8; 3] {
        PALETTE.get(self.palette_index).copied().unwrap_or([255; 3])
    }

    /// We draw the logo opaquely, since glowing text would brighten wherever
    /// the trail overlaps it.
    fn draw(&self, light: &mut Light) {
        let color = self.color();
        for (index, ghost) in self.trail.iter().enumerate().skip(1) {
            let dim = 1 + index as u8;
            let [red, green, blue] = color.map(|channel| channel / dim);
            let shade = srgb(red, green, blue);
            light.blended_text(*ghost, LOGO, self.font_scale, shade, Blend::Over);
        }
        let [red, green, blue] = color;
        let shade = srgb(red, green, blue);
        light.blended_text(self.at, LOGO, self.font_scale, shade, Blend::Over);
        if self.flash > 0.0 {
            let white = srgb(255, 255, 255);
            light.rect(
                P2::default(),
                self.bounds.x,
                self.bounds.y,
                white,
                Blend::Over,
            );
        }
    }
}

/// We never count a hit on an axis with no room, or the palette would flip every step.
fn bounce(at: &mut f32, velocity: &mut f32, limit: f32) -> bool {
    let hit = limit > 0.0 && (*at <= 0.0 || *at >= limit);
    if hit {
        *velocity = -*velocity;
    }
    *at = at.clamp(0.0, limit);
    hit
}

fn dusk(share: f32) -> Rgb {
    let lerp = |top: f32, bottom: f32| (top + (bottom - top) * share) as u8;
    let ([top_red, top_green, top_blue], [bottom_red, bottom_green, bottom_blue]) =
        (DUSK_TOP, DUSK_BOTTOM);
    srgb(
        lerp(top_red, bottom_red),
        lerp(top_green, bottom_green),
        lerp(top_blue, bottom_blue),
    )
}

struct Dvd {
    rng: Rng,
    backdrop: Light,
    stars: Vec<Star>,
    logo: Logo,
    seconds: f32,
}

impl Dvd {
    fn new(seed: u32) -> Self {
        let light = Light::new(1, 1, 1.0);
        Self {
            rng: Rng::new(seed),
            logo: Logo::new(&light),
            backdrop: light,
            stars: Vec::new(),
            seconds: 0.0,
        }
    }
}

impl Game for Dvd {
    fn resize(&mut self, light: &Light) {
        let (width, height) = (light.width() as f32, light.height() as f32);
        let mut backdrop = Light::new(light.width(), light.height(), light.pixel_aspect());
        backdrop.paint(|_, y| dusk(y as f32 / height));
        self.backdrop = backdrop;
        let rng = &mut self.rng;
        self.stars = (0..(light.width() / 12).max(40))
            .map(|_| Star {
                at: P2::new(
                    rng.range(0.0, width).floor(),
                    rng.range(0.0, height).floor(),
                ),
                phase: rng.range(0.0, TAU),
                speed: rng.range(0.6, 2.4),
            })
            .collect();
        self.logo = Logo::new(light);
    }

    fn step(&mut self, _keys: &HeldKeys, dt: f32) {
        self.seconds += dt;
        self.logo.step(dt, &mut self.rng);
    }

    fn render(&mut self, light: &mut Light) {
        light.copy_from(&self.backdrop);
        for star in &self.stars {
            let shine = star.shine(self.seconds);
            light.rect(star.at, 1.0, 1.0, srgb(shine, shine, shine), Blend::Over);
        }
        self.logo.draw(light);
    }
}

#[cfg(test)]
#[path = "support/artifact.rs"]
mod artifact;

#[cfg(test)]
mod tests {
    use crate::artifact;

    use katui_game::{Game, HeldKeys, Light};

    use super::Dvd;

    const DT: f32 = 1.0 / 60.0;

    fn dvd(width: u32, height: u32) -> (Dvd, Light) {
        let mut dvd = Dvd::new(7);
        let light = Light::new(width, height, 1.0);
        dvd.resize(&light);
        (dvd, light)
    }

    #[test]
    fn the_logo_stays_inside_escalates_and_keeps_its_cap() {
        let (mut dvd, _) = dvd(960, 540);
        let start = dvd.logo.velocity.x.abs() + dvd.logo.velocity.y.abs();
        let mut flashes = 0;
        for _ in 0..200_000 {
            dvd.step(&HeldKeys::default(), DT);
            let logo = &dvd.logo;
            assert!(logo.at.x >= 0.0 && logo.at.y >= 0.0);
            assert!(logo.at.x + logo.size.x <= logo.bounds.x);
            assert!(logo.at.y + logo.size.y <= logo.bounds.y);
            assert!(logo.velocity.x.abs() <= logo.base_speed.x * 2.0 + 1e-3);
            assert!(logo.velocity.y.abs() <= logo.base_speed.y * 2.0 + 1e-3);
            flashes += usize::from(logo.flash > 0.0);
        }
        assert!(flashes > 0, "a corner is eventually hit");
        let end = dvd.logo.velocity.x.abs() + dvd.logo.velocity.y.abs();
        assert!(end > start, "corner hits escalate the pace");
    }

    #[test]
    fn a_screen_smaller_than_the_logo_never_counts_a_hit() {
        let (mut dvd, mut light) = dvd(20, 5);
        for _ in 0..600 {
            dvd.step(&HeldKeys::default(), DT);
            assert_eq!(dvd.logo.palette_index, 0);
            assert!(dvd.logo.flash <= 0.0);
            assert_eq!((dvd.logo.at.x, dvd.logo.at.y), (0.0, 0.0));
        }
        dvd.render(&mut light);
    }

    #[test]
    fn stars_twinkle_between_frames() {
        let (mut dvd, mut light) = dvd(320, 180);
        dvd.render(&mut light);
        let first = light.to_surface();
        dvd.seconds = 3.0;
        dvd.render(&mut light);
        assert_ne!(first.pixels(), light.to_surface().pixels());
    }

    #[test]
    fn a_bouncing_frame_dumps_a_png_artifact() {
        let (mut dvd, mut light) = dvd(960, 540);
        for _ in 0..150 {
            dvd.step(&HeldKeys::default(), DT);
        }
        dvd.logo.flash = 0.0;
        dvd.render(&mut light);
        artifact::save_frame(&light, "dvd");
    }
}
