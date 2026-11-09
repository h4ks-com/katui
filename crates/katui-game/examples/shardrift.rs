//! Shardrift: fly an inertial dart through a nebula and split faceted
//! crystal rocks into smaller and smaller shards. Run with
//! `just demo-shardrift`.
#![cfg(unix)] // the runner drives /dev/tty through katui-rt's unix layer
#![allow(clippy::cast_precision_loss, clippy::cast_sign_loss)]

use std::f32::consts::{PI, TAU};

use katui_game::{
    Blend, Game, HeldKeys, Key, Light, P2, Rgb, Rng, Spark, Sparks, V3, play, scaled, v3,
};

const FIELD_HEIGHT: f32 = 100.0;
const TURN_SPEED: f32 = 3.8;
const THRUST: f32 = 62.0;
const BRAKE: f32 = 2.6;
const TOP_SPEED: f32 = 85.0;
const SHIP_RADIUS: f32 = 2.2;
const BULLET_SPEED: f32 = 125.0;
const BULLET_LIFE: f32 = 0.62;
const BLAST_LIFE: f32 = 0.55;
const FIRE_INTERVAL: f32 = 0.17;
const SAFE_SECONDS: f32 = 2.4;
const START_LIVES: u32 = 3;
const ROCK_SIZES: [f32; 3] = [9.5, 5.5, 3.0];
const ROCK_POINTS: [u32; 3] = [10, 25, 60];
const ROCK_TINTS: [Rgb; 3] = [[0.45, 0.3, 1.0], [0.1, 0.75, 0.9], [1.0, 0.6, 0.2]];
const SUN: V3 = v3(-0.45, -0.6, -0.66);

const TURN_LEFT: [Key; 2] = [Key::Left, Key::Char('a')];
const TURN_RIGHT: [Key; 2] = [Key::Right, Key::Char('d')];
const FORWARD: [Key; 2] = [Key::Up, Key::Char('w')];
const BACKWARD: [Key; 2] = [Key::Down, Key::Char('s')];
const FIRE: [Key; 2] = [Key::Char(' '), Key::Char('f')];

const GOLDEN: f32 = 1.618;
const CRYSTAL_POINTS: [V3; 12] = [
    v3(-1.0, GOLDEN, 0.0),
    v3(1.0, GOLDEN, 0.0),
    v3(-1.0, -GOLDEN, 0.0),
    v3(1.0, -GOLDEN, 0.0),
    v3(0.0, -1.0, GOLDEN),
    v3(0.0, 1.0, GOLDEN),
    v3(0.0, -1.0, -GOLDEN),
    v3(0.0, 1.0, -GOLDEN),
    v3(GOLDEN, 0.0, -1.0),
    v3(GOLDEN, 0.0, 1.0),
    v3(-GOLDEN, 0.0, -1.0),
    v3(-GOLDEN, 0.0, 1.0),
];
const CRYSTAL_FACES: [[usize; 3]; 20] = [
    [0, 11, 5],
    [0, 5, 1],
    [0, 1, 7],
    [0, 7, 10],
    [0, 10, 11],
    [1, 5, 9],
    [5, 11, 4],
    [11, 10, 2],
    [10, 7, 6],
    [7, 1, 8],
    [3, 9, 4],
    [3, 4, 2],
    [3, 2, 6],
    [3, 6, 8],
    [3, 8, 9],
    [4, 9, 5],
    [2, 4, 11],
    [6, 2, 10],
    [8, 6, 7],
    [9, 8, 1],
];

fn main() -> Result<(), katui_rt::RtError> {
    play(Shardrift::new(0x0dd5_beef))
}

fn wrap_around(at: V3, field_width: f32) -> V3 {
    v3(
        at.x.rem_euclid(field_width),
        at.y.rem_euclid(FIELD_HEIGHT),
        0.0,
    )
}

struct Rock {
    at: V3,
    velocity: V3,
    size: usize,
    points: Vec<V3>,
    spin: V3,
    angle: V3,
    flash: f32,
}

struct Bullet {
    at: V3,
    velocity: V3,
    age: f32,
}

struct Blast {
    at: V3,
    color: Rgb,
    age: f32,
    reach: f32,
}

struct Ship {
    at: V3,
    velocity: V3,
    heading: f32,
    bank: f32,
    safe: f32,
    thrusting: bool,
}

impl Ship {
    fn fresh(field_width: f32) -> Self {
        Self {
            at: v3(field_width / 2.0, FIELD_HEIGHT / 2.0, 0.0),
            velocity: V3::default(),
            heading: -PI / 2.0,
            bank: 0.0,
            safe: SAFE_SECONDS,
            thrusting: false,
        }
    }

    fn nose(&self) -> V3 {
        v3(self.heading.cos(), self.heading.sin(), 0.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Title,
    Playing,
    Paused,
    Over,
}

struct Shardrift {
    phase: Phase,
    rng: Rng,
    ship: Ship,
    rocks: Vec<Rock>,
    bullets: Vec<Bullet>,
    blasts: Vec<Blast>,
    sparks: Sparks,
    backdrop: Light,
    field_width: f32,
    pixels_per_unit: (f32, f32),
    score: u32,
    lives: u32,
    wave: u32,
    cooldown: f32,
    respawn: f32,
    shake: f32,
    time: f32,
}

impl Shardrift {
    fn new(seed: u32) -> Self {
        Self {
            phase: Phase::Title,
            rng: Rng::new(seed),
            ship: Ship::fresh(160.0),
            rocks: Vec::new(),
            bullets: Vec::new(),
            blasts: Vec::new(),
            sparks: Sparks::default(),
            backdrop: Light::new(1, 1, 1.0),
            field_width: 160.0,
            pixels_per_unit: (1.0, 1.0),
            score: 0,
            lives: START_LIVES,
            wave: 0,
            cooldown: 0.0,
            respawn: 0.0,
            shake: 0.0,
            time: 0.0,
        }
    }

    fn start(&mut self) {
        self.score = 0;
        self.lives = START_LIVES;
        self.wave = 0;
        self.ship = Ship::fresh(self.field_width);
        self.rocks.clear();
        self.bullets.clear();
        self.phase = Phase::Playing;
        self.next_wave();
    }

    fn next_wave(&mut self) {
        self.wave += 1;
        for _ in 0..self.wave + 3 {
            let at = self.spawn_point();
            let velocity = self.drift(9.0, 18.0);
            let rock = self.make_rock(at, velocity, 0);
            self.rocks.push(rock);
        }
    }

    fn spawn_point(&mut self) -> V3 {
        for _ in 0..24 {
            let candidate = v3(
                self.rng.range(0.0, self.field_width),
                self.rng.range(0.0, FIELD_HEIGHT),
                0.0,
            );
            if self.wrapped_distance(candidate, self.ship.at) > 32.0 {
                return candidate;
            }
        }
        v3(0.0, 0.0, 0.0)
    }

    fn drift(&mut self, low: f32, high: f32) -> V3 {
        let angle = self.rng.range(0.0, TAU);
        let speed = self.rng.range(low, high);
        v3(angle.cos() * speed, angle.sin() * speed, 0.0)
    }

    fn make_rock(&mut self, at: V3, velocity: V3, size: usize) -> Rock {
        let radius = ROCK_SIZES.get(size).copied().unwrap_or(3.0);
        let unit = 1.0 / (1.0 + GOLDEN * GOLDEN).sqrt();
        let points = CRYSTAL_POINTS
            .iter()
            .map(|point| *point * (unit * radius * self.rng.range(0.72, 1.18)))
            .collect();
        Rock {
            at,
            velocity,
            size,
            points,
            spin: v3(
                self.rng.range(-1.6, 1.6),
                self.rng.range(-1.6, 1.6),
                self.rng.range(-0.8, 0.8),
            ),
            angle: v3(
                self.rng.range(0.0, TAU),
                self.rng.range(0.0, TAU),
                self.rng.range(0.0, TAU),
            ),
            flash: 0.0,
        }
    }

    fn wrapped(&self, at: V3) -> V3 {
        wrap_around(at, self.field_width)
    }

    fn wrapped_distance(&self, a: V3, b: V3) -> f32 {
        let nearest = |delta: f32, span: f32| (delta + span / 2.0).rem_euclid(span) - span / 2.0;
        let dx = nearest(a.x - b.x, self.field_width);
        let dy = nearest(a.y - b.y, FIELD_HEIGHT);
        (dx * dx + dy * dy).sqrt()
    }

    fn steer(&mut self, keys: &HeldKeys, dt: f32) {
        let turn = f32::from(i8::from(keys.any_held(&TURN_RIGHT)))
            - f32::from(i8::from(keys.any_held(&TURN_LEFT)));
        self.ship.heading += turn * TURN_SPEED * dt;
        self.ship.bank += (turn * 0.75 - self.ship.bank) * (dt * 9.0).min(1.0);
        self.ship.thrusting = keys.any_held(&FORWARD);
        if self.ship.thrusting {
            self.ship.velocity = self.ship.velocity + self.ship.nose() * (THRUST * dt);
            self.exhaust();
        }
        if keys.any_held(&BACKWARD) {
            self.ship.velocity = self.ship.velocity * (1.0 - (BRAKE * dt).min(1.0));
        }
        let speed = self.ship.velocity.dot(self.ship.velocity).sqrt();
        if speed > TOP_SPEED {
            self.ship.velocity = self.ship.velocity * (TOP_SPEED / speed);
        }
        self.ship.at = self.wrapped(self.ship.at + self.ship.velocity * dt);
        self.ship.safe = (self.ship.safe - dt).max(0.0);
    }

    fn exhaust(&mut self) {
        let back = self.ship.nose() * -1.0;
        for _ in 0..3 {
            let scatter = self.rng.direction() * 7.0;
            let speed = self.rng.range(22.0, 44.0);
            self.sparks.add(Spark {
                at: self.ship.at + back * 2.2,
                velocity: self.ship.velocity + back * speed + scatter,
                color: [2.6, 1.2, 0.35],
                age: 0.0,
                life: self.rng.range(0.15, 0.4),
                width: 1.6,
            });
        }
    }

    fn fire(&mut self, keys: &HeldKeys, dt: f32) {
        self.cooldown = (self.cooldown - dt).max(0.0);
        if self.cooldown > 0.0 || !keys.any_held(&FIRE) {
            return;
        }
        self.cooldown = FIRE_INTERVAL;
        let nose = self.ship.nose();
        self.bullets.push(Bullet {
            at: self.ship.at + nose * 3.0,
            velocity: self.ship.velocity + nose * BULLET_SPEED,
            age: 0.0,
        });
    }

    fn move_things(&mut self, dt: f32) {
        let width = self.field_width;
        let wrap = |at: V3| wrap_around(at, width);
        for rock in &mut self.rocks {
            rock.at = wrap(rock.at + rock.velocity * dt);
            rock.angle = rock.angle + rock.spin * dt;
            rock.flash = (rock.flash - dt * 5.0).max(0.0);
        }
        self.bullets.retain_mut(|bullet| {
            bullet.age += dt;
            bullet.at = wrap(bullet.at + bullet.velocity * dt);
            bullet.age < BULLET_LIFE
        });
    }

    fn first_rock_hit(&self, at: V3, margin: f32) -> Option<usize> {
        self.rocks.iter().position(|rock| {
            let radius = ROCK_SIZES.get(rock.size).copied().unwrap_or(3.0);
            self.wrapped_distance(at, rock.at) < radius + margin
        })
    }

    fn resolve_bullets(&mut self) {
        let bullets = std::mem::take(&mut self.bullets);
        for bullet in bullets {
            match self.first_rock_hit(bullet.at, 0.5) {
                Some(hit) => self.shatter(hit, bullet.velocity),
                None => self.bullets.push(bullet),
            }
        }
    }

    fn shatter(&mut self, index: usize, impact: V3) {
        if index >= self.rocks.len() {
            return;
        }
        let rock = self.rocks.swap_remove(index);
        let tint = ROCK_TINTS.get(rock.size).copied().unwrap_or([1.0; 3]);
        let radius = ROCK_SIZES.get(rock.size).copied().unwrap_or(3.0);
        self.score += ROCK_POINTS.get(rock.size).copied().unwrap_or(0);
        self.burst(rock.at, tint, radius);
        self.shake = self.shake.max(0.12 + radius * 0.025);
        if rock.size + 1 < ROCK_SIZES.len() {
            for _ in 0..2 {
                let push = self.drift(12.0, 26.0) + impact * 0.07;
                let piece = self.make_rock(rock.at, rock.velocity + push, rock.size + 1);
                self.rocks.push(piece);
            }
        }
    }

    fn burst(&mut self, at: V3, tint: Rgb, radius: f32) {
        self.blasts.push(Blast {
            at,
            color: tint,
            age: 0.0,
            reach: radius * 3.2,
        });
        let count = (radius * 9.0) as u32;
        for _ in 0..count {
            let direction = self.drift(radius * 2.0, radius * 8.0);
            self.sparks.add(Spark {
                at,
                velocity: direction,
                color: scaled(tint, 3.2),
                age: 0.0,
                life: self.rng.range(0.3, 0.9),
                width: 2.0,
            });
        }
    }

    fn resolve_ship(&mut self, dt: f32) {
        if self.respawn > 0.0 {
            self.respawn -= dt;
            if self.respawn <= 0.0 {
                self.ship = Ship::fresh(self.field_width);
            }
            return;
        }
        if self.ship.safe > 0.0 {
            return;
        }
        if let Some(hit) = self.first_rock_hit(self.ship.at, SHIP_RADIUS) {
            self.shatter(hit, self.ship.velocity);
            self.burst(self.ship.at, [1.0, 0.65, 0.3], 8.0);
            self.shake = 0.7;
            self.lives = self.lives.saturating_sub(1);
            if self.lives == 0 {
                self.phase = Phase::Over;
            } else {
                self.respawn = 1.5;
            }
        }
    }

    fn age_effects(&mut self, dt: f32) {
        self.time += dt;
        self.shake = (self.shake - dt * 1.6).max(0.0);
        self.sparks.step(dt, V3::default());
        self.blasts.retain_mut(|blast| {
            blast.age += dt;
            blast.age < BLAST_LIFE
        });
    }

    fn pixel(&self, at: V3, shake: (f32, f32)) -> P2 {
        P2::new(
            at.x * self.pixels_per_unit.0 + shake.0,
            at.y * self.pixels_per_unit.1 + shake.1,
        )
    }

    fn shake_offset(&mut self) -> (f32, f32) {
        let strength = self.shake * self.shake * self.pixels_per_unit.1 * 3.0;
        (
            self.rng.range(-strength, strength),
            self.rng.range(-strength, strength),
        )
    }

    fn copies_near_edges(&self, at: V3, reach: f32) -> Vec<V3> {
        let spans = |value: f32, span: f32| {
            let mut offsets = vec![0.0];
            if value < reach {
                offsets.push(span);
            }
            if value > span - reach {
                offsets.push(-span);
            }
            offsets
        };
        let mut copies = Vec::new();
        for dx in spans(at.x, self.field_width) {
            for dy in spans(at.y, FIELD_HEIGHT) {
                copies.push(v3(at.x + dx, at.y + dy, 0.0));
            }
        }
        copies
    }

    fn draw_rock(&self, light: &mut Light, rock: &Rock, at: V3, shake: (f32, f32)) {
        let tint = ROCK_TINTS.get(rock.size).copied().unwrap_or([1.0; 3]);
        let turned: Vec<V3> = rock
            .points
            .iter()
            .map(|point| {
                point
                    .rotated_x(rock.angle.x)
                    .rotated_y(rock.angle.y)
                    .rotated_z(rock.angle.z)
            })
            .collect();
        let corner = |index: usize| turned.get(index).copied().unwrap_or_default();
        for [first, second, third] in CRYSTAL_FACES {
            let (a, b, c) = (corner(first), corner(second), corner(third));
            let normal = (b - a).cross(c - a).normalized();
            if normal.z >= 0.0 {
                continue;
            }
            let lit = normal.dot(SUN).max(0.0);
            let rim = (1.0 + normal.z).powi(3);
            let brightness = 0.1 + lit * 0.95 + rock.flash * 2.0;
            let mut color = scaled(tint, brightness);
            color = [
                color[0] + rim * 0.5,
                color[1] + rim * 0.5,
                color[2] + rim * 0.7,
            ];
            let points = [a, b, c].map(|point| self.pixel(at + point, shake));
            light.polygon(&points, color, Blend::Over);
            let edge = scaled(tint, 0.55 + rim);
            for (from, to) in [
                (points[0], points[1]),
                (points[1], points[2]),
                (points[2], points[0]),
            ] {
                light.line(from, to, 1.0, edge);
            }
        }
    }

    fn draw_rocks(&self, light: &mut Light, shake: (f32, f32)) {
        for rock in &self.rocks {
            let reach = ROCK_SIZES.get(rock.size).copied().unwrap_or(3.0) * 1.3;
            for at in self.copies_near_edges(rock.at, reach) {
                self.draw_rock(light, rock, at, shake);
            }
        }
    }

    fn draw_ship(&self, light: &mut Light, shake: (f32, f32)) {
        if self.respawn > 0.0 || matches!(self.phase, Phase::Title | Phase::Over) {
            return;
        }
        let blink = self.ship.safe > 0.0 && (self.time * 14.0).sin() < 0.0;
        if blink {
            return;
        }
        let place = |local: V3| {
            let turned = local.rotated_x(self.ship.bank).rotated_z(self.ship.heading);
            self.pixel(self.ship.at + turned, shake)
        };
        let nose = place(v3(3.4, 0.0, 0.0));
        let port = place(v3(-2.4, -2.5, 0.0));
        let starboard = place(v3(-2.4, 2.5, 0.0));
        let keel = place(v3(-1.1, 0.0, -1.0));
        let port_light = 0.65 + self.ship.bank * 0.5;
        let starboard_light = 0.65 - self.ship.bank * 0.5;
        light.polygon(
            &[nose, port, keel],
            scaled([1.2, 0.8, 0.3], port_light),
            Blend::Over,
        );
        light.polygon(
            &[nose, keel, starboard],
            scaled([1.2, 0.8, 0.3], starboard_light),
            Blend::Over,
        );
        for (from, to) in [
            (nose, port),
            (nose, starboard),
            (port, keel),
            (starboard, keel),
        ] {
            light.line(from, to, 1.2, [2.0, 1.5, 0.8]);
        }
        let height = self.pixels_per_unit.1;
        if self.ship.thrusting {
            let flicker = 0.75 + 0.25 * (self.time * 60.0).sin();
            light.glow(keel, height * 5.0, scaled([1.8, 0.75, 0.2], flicker));
        }
        light.glow(
            self.pixel(self.ship.at, shake),
            height * 6.0,
            [0.14, 0.09, 0.03],
        );
    }

    fn draw_bullets(&self, light: &mut Light, shake: (f32, f32)) {
        for bullet in &self.bullets {
            let head = self.pixel(bullet.at, shake);
            let tail = self.pixel(bullet.at - bullet.velocity * 0.035, shake);
            let far = (head.x - tail.x).abs() > light.width() as f32 / 2.0
                || (head.y - tail.y).abs() > light.height() as f32 / 2.0;
            if !far {
                light.line(head, tail, 2.0, [2.4, 3.0, 3.6]);
            }
            light.glow(head, self.pixels_per_unit.1 * 2.4, [1.2, 1.8, 2.6]);
        }
    }

    fn draw_blasts(&self, light: &mut Light, shake: (f32, f32)) {
        let unit = self.pixels_per_unit.1;
        for blast in &self.blasts {
            let progress = blast.age / BLAST_LIFE;
            let fade = (1.0 - progress).powi(2);
            let center = self.pixel(blast.at, shake);
            let radius = blast.reach * unit * progress.sqrt();
            light.ring(center, radius, unit * 1.3, scaled(blast.color, fade * 2.6));
            light.glow(center, radius * 0.9, scaled(blast.color, fade * 0.9));
        }
    }

    fn draw_lives(&self, light: &mut Light, scale: u32) {
        let unit = scale as f32 * 3.0;
        let wide = unit / light.pixel_aspect();
        for life in 0..self.lives {
            let x = 8.0 * scale as f32 + (life as f32 + 0.5) * wide * 2.6;
            let y = 22.0 * scale as f32;
            light.polygon(
                &[
                    P2::new(x, y - unit),
                    P2::new(x + wide * 0.8, y + unit),
                    P2::new(x, y + unit * 0.45),
                    P2::new(x - wide * 0.8, y + unit),
                ],
                [1.6, 1.1, 0.4],
                Blend::Add,
            );
        }
    }

    fn draw_status(&self, light: &mut Light, scale: u32) {
        let margin = 6.0 * scale as f32;
        light.text(
            P2::new(margin, margin),
            &self.score.to_string(),
            scale * 2,
            [2.0, 2.0, 2.2],
        );
        self.draw_lives(light, scale);
        let wave = format!("wave {}", self.wave);
        let width = light.text_width(&wave, scale);
        light.text(
            P2::new(light.width() as f32 - width - margin, margin),
            &wave,
            scale,
            [0.6, 0.75, 1.1],
        );
    }

    fn draw_banner(&self, light: &mut Light, scale: u32) {
        let prompt = match self.phase {
            Phase::Playing => return,
            Phase::Title => "press enter to launch",
            Phase::Paused => "press p to resume",
            Phase::Over => "press enter to launch again",
        };
        light.dim(0.45);
        let center_x = light.width() as f32 / 2.0;
        let center_y = light.height() as f32 / 2.0;
        light.centered_text(
            center_x,
            center_y - 18.0 * scale as f32,
            "shardrift",
            scale * 4,
            scaled([0.8, 0.65, 1.2], 2.3),
        );
        light.centered_text(
            center_x,
            center_y + 12.0 * scale as f32,
            prompt,
            scale,
            [1.4; 3],
        );
        light.centered_text(
            center_x,
            center_y + 26.0 * scale as f32,
            "arrows or wasd steer   space fire   p pause   q quit",
            scale,
            [0.5, 0.6, 0.85],
        );
    }
}

impl Game for Shardrift {
    fn press(&mut self, key: Key) {
        match (self.phase, key) {
            (Phase::Title | Phase::Over, Key::Enter) => self.start(),
            (Phase::Playing, Key::Char('p')) => self.phase = Phase::Paused,
            (Phase::Paused, Key::Char('p')) => self.phase = Phase::Playing,
            _ => {}
        }
    }

    fn resize(&mut self, light: &Light) {
        let down = light.height() as f32 / FIELD_HEIGHT;
        let across = down / light.pixel_aspect();
        self.pixels_per_unit = (across, down);
        self.field_width = light.width() as f32 / across;
        let mut backdrop = Light::new(light.width(), light.height(), light.pixel_aspect());
        let stars = star_field(&mut self.rng, light.width(), light.height());
        backdrop.paint(|x, y| {
            nebula(
                x as f32 * light.pixel_aspect(),
                y as f32,
                light.height() as f32,
            )
        });
        for (spot, brightness) in stars {
            backdrop.rect(spot, 1.0, 1.0, [brightness; 3], Blend::Add);
        }
        backdrop.bloom(0.7, 0.6);
        self.backdrop = backdrop;
        self.ship.at = self.wrapped(self.ship.at);
    }

    fn step(&mut self, keys: &HeldKeys, dt: f32) {
        self.age_effects(dt);
        if self.phase != Phase::Playing {
            return;
        }
        if self.respawn <= 0.0 {
            self.steer(keys, dt);
            self.fire(keys, dt);
        }
        self.move_things(dt);
        self.resolve_bullets();
        self.resolve_ship(dt);
        if self.rocks.is_empty() && self.phase == Phase::Playing {
            self.next_wave();
        }
    }

    fn render(&mut self, light: &mut Light) {
        light.copy_from(&self.backdrop);
        let shake = self.shake_offset();
        self.draw_rocks(light, shake);
        self.draw_blasts(light, shake);
        self.sparks.draw(light, |at| Some(self.pixel(at, shake)));
        self.draw_bullets(light, shake);
        self.draw_ship(light, shake);
        light.bloom(0.9, 0.55);
        let scale = (light.height() / 270).clamp(1, 4);
        self.draw_status(light, scale);
        self.draw_banner(light, scale);
    }
}

fn star_field(rng: &mut Rng, width: u32, height: u32) -> Vec<(P2, f32)> {
    (0..(width * height / 900))
        .map(|_| {
            let spot = P2::new(
                rng.range(0.0, width as f32).floor(),
                rng.range(0.0, height as f32).floor(),
            );
            let brightness = rng.range(0.1, 1.0);
            (spot, brightness * brightness * 1.6)
        })
        .collect()
}

fn lattice(x: i32, y: i32) -> f32 {
    let mixed = x
        .wrapping_mul(374_761)
        .wrapping_add(y.wrapping_mul(668_265));
    let scrambled = mixed ^ mixed.wrapping_shr(13);
    let squared = scrambled.wrapping_mul(
        scrambled
            .wrapping_mul(scrambled)
            .wrapping_mul(60_493)
            .wrapping_add(19_990_303),
    );
    (squared.wrapping_add(1_376_312_589) & 0x7fff_ffff) as f32 / 2_147_483_648.0
}

fn smooth_noise(x: f32, y: f32) -> f32 {
    let (cell_x, cell_y) = (x.floor(), y.floor());
    let ease = |t: f32| t * t * (3.0 - 2.0 * t);
    let (across, down) = (ease(x - cell_x), ease(y - cell_y));
    let (left, top) = (cell_x as i32, cell_y as i32);
    let upper = lattice(left, top) + (lattice(left + 1, top) - lattice(left, top)) * across;
    let lower =
        lattice(left, top + 1) + (lattice(left + 1, top + 1) - lattice(left, top + 1)) * across;
    upper + (lower - upper) * down
}

fn nebula(x: f32, y: f32, height: f32) -> Rgb {
    let (u, v) = (x / height * 3.0, y / height * 3.0);
    let cloud = smooth_noise(u, v) * 0.6
        + smooth_noise(u * 2.3, v * 2.3) * 0.3
        + smooth_noise(u * 5.1, v * 5.1) * 0.1;
    let warm = smooth_noise(u * 0.8 + 9.0, v * 0.8 + 4.0);
    let density = ((cloud - 0.42) * 2.4).clamp(0.0, 1.0);
    let body = density * density * 0.22;
    [
        body * (0.35 + warm * 1.1) + 0.004,
        body * (0.15 + (1.0 - warm) * 0.5) + 0.004,
        body * (0.95 + (1.0 - warm) * 0.5) + 0.012,
    ]
}

#[cfg(test)]
mod tests {
    use katui_game::{Game, HeldKeys, Key, Light, v3};

    use super::{FIRE, FORWARD, Phase, ROCK_SIZES, Shardrift, TURN_LEFT, V3};
    use std::time::{Duration, Instant};

    const DT: f32 = 1.0 / 60.0;

    fn game() -> (Shardrift, Light) {
        let mut game = Shardrift::new(21);
        let light = Light::new(960, 540, 1.0);
        game.resize(&light);
        game.press(Key::Enter);
        (game, light)
    }

    #[test]
    fn a_large_rock_splits_in_two_smaller_ones() {
        let (mut game, _) = game();
        game.rocks.truncate(1);
        game.rocks[0].size = 0;
        game.shatter(0, V3::default());
        assert_eq!(game.rocks.len(), 2);
        assert!(game.rocks.iter().all(|rock| rock.size == 1));
        assert_eq!(game.score, 10);
    }

    #[test]
    fn the_smallest_rock_vanishes() {
        let (mut game, _) = game();
        game.rocks.truncate(1);
        game.rocks[0].size = ROCK_SIZES.len() - 1;
        game.shatter(0, V3::default());
        assert!(game.rocks.is_empty());
    }

    #[test]
    fn distance_is_measured_across_the_seam() {
        let (game, _) = game();
        let near = game.wrapped_distance(v3(1.0, 50.0, 0.0), v3(game.field_width - 1.0, 50.0, 0.0));
        assert!((near - 2.0).abs() < 1e-3);
    }

    #[test]
    fn momentum_outlasts_the_thrust() {
        let (mut game, _) = game();
        game.rocks.clear();
        game.ship.safe = 100.0;
        let mut keys = HeldKeys::default();
        let start = Instant::now();
        keys.press(FORWARD[0], start);
        keys.tick(start + Duration::from_millis(10));
        game.step(&keys, DT);
        let speed = game.ship.velocity.dot(game.ship.velocity);
        assert!(speed > 0.0);
        let idle = HeldKeys::default();
        game.step(&idle, DT);
        assert!((game.ship.velocity.dot(game.ship.velocity) - speed).abs() < 1e-3);
    }

    #[test]
    fn a_collision_costs_a_life_and_the_last_one_ends_the_run() {
        let (mut game, _) = game();
        game.rocks.truncate(1);
        game.ship.safe = 0.0;
        game.lives = 1;
        game.rocks[0].at = game.ship.at;
        game.step(&HeldKeys::default(), DT);
        assert_eq!(game.lives, 0);
        assert_eq!(game.phase, Phase::Over);
    }

    #[test]
    fn a_played_frame_dumps_a_png_artifact() {
        let (mut game, mut light) = game();
        let mut keys = HeldKeys::default();
        let start = Instant::now();
        keys.release(Key::Tab);
        keys.press(FORWARD[0], start);
        keys.press(TURN_LEFT[0], start);
        keys.press(FIRE[0], start);
        game.ship.safe = 100.0;
        for _ in 0..80 {
            game.step(&keys, DT);
        }
        game.ship.safe = 0.0;
        game.render(&mut light);
        let surface = light.to_surface();
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../target/katui-shardrift-frame.png"
        );
        image::save_buffer(
            path,
            surface.pixels(),
            surface.width(),
            surface.height(),
            image::ColorType::Rgba8,
        )
        .expect("the png writer saves under target");
    }
}
