//! The pacing instrument: what each pixel strategy costs end to end on
//! this terminal. Every frame is followed by a kitty query, and the
//! frame counts as delivered when the terminal answers it: the terminal
//! parses its stream in order, so the answer proves the frame was
//! consumed. Run with `just pace`.
#![cfg(unix)] // the instrument drives /dev/tty through katui-rt's unix layer
#![allow(clippy::cast_precision_loss, clippy::print_stdout)]

use std::time::{Duration, Instant};

use katui_present::surface::{AnimatedKitty, PatchedKitty};
use katui_proto::kitty::{self, Delete, IdSpace, ImageId};
use katui_proto::probe::{CellPixels, Transport as _};
use katui_rt::tty::TtyTransport;
use katui_scene::placement::CellRect;
use katui_scene::surface::{PixelPos, Rgba, Surface};

const FRAMES: usize = 90;
/// From this share of damaged tiles the animated strategy sends whole frames.
const DENSE_DAMAGE_PERCENT: u64 = 25;
const TILE: u32 = 40;
const SURFACE_PIXELS_PER_CELL: u32 = 5;
const BASE_IMAGE: u32 = 700;
const BARRIER_IMAGE: u32 = 41;
const ANSWER_WAIT: Duration = Duration::from_secs(4);

fn main() {
    let caps = katui_rt::tty::probe_default();
    print!("{}", katui_rt::info::card(&caps));
    let Ok(size) = katui_rt::tty::size() else {
        println!("no controlling terminal");
        return;
    };
    if !caps.kitty {
        println!("no graphics protocol: nothing to measure");
        return;
    }
    let Ok(mut transport) = TtyTransport::open(Duration::from_secs(600)) else {
        println!("no controlling terminal");
        return;
    };
    let area = CellRect::full(size.columns, size.rows);
    let width = u32::from(size.columns) * SURFACE_PIXELS_PER_CELL;
    let height = u32::from(size.rows) * SURFACE_PIXELS_PER_CELL;

    let mut lines = vec![format!(
        "{width}x{height} px, {} frames per row\n{:<10}{:>8}{:>8}{:>9}{:>9}{:>11}",
        FRAMES, "strategy", "damage", "fps", "p50 ms", "p95 ms", "KB/frame"
    )];
    for damage in [5, 20, 60] {
        let mut tiles = PatchedKitty::with_tiles(BASE_IMAGE, cell(), TILE, usize::MAX)
            .for_multiplexer(IdSpace::of_process(std::process::id()), true);
        lines.push(run(
            "tiles",
            damage,
            width,
            height,
            &mut transport,
            |out, surface| tiles.render_frame(out, surface, area),
        ));
        let mut whole = PatchedKitty::with_tiles(BASE_IMAGE, cell(), TILE, 0);
        lines.push(run(
            "whole",
            damage,
            width,
            height,
            &mut transport,
            |out, surface| whole.render_frame(out, surface, area),
        ));
        if caps.animation && damage < DENSE_DAMAGE_PERCENT {
            let mut reel = AnimatedKitty::new(BASE_IMAGE, 60, TILE, usize::MAX);
            lines.push(run(
                "animated",
                damage,
                width,
                height,
                &mut transport,
                |out, surface| reel.render_frame(out, surface, area),
            ));
        }
    }
    drop(transport);
    print!("\x1b[2J\x1b[H");
    for line in lines {
        println!("{line}");
    }
}

/// One strategy at one damage percentage, reported as one table row.
fn run(
    name: &str,
    damage_percent: u64,
    width: u32,
    height: u32,
    transport: &mut TtyTransport,
    mut render: impl FnMut(&mut Vec<u8>, &Surface) -> std::io::Result<usize>,
) -> String {
    let mut surface = speckled(width, height);
    let mut rng = 0x9e37_79b9_7f4a_7c15u64;
    let mut wire = Vec::new();
    let mut waits = Vec::with_capacity(FRAMES);
    let mut sent = 0usize;
    let started = Instant::now();
    for frame in 0..FRAMES {
        churn(&mut surface, damage_percent, &mut rng);
        wire.clear();
        if render(&mut wire, &surface).is_err() {
            return format!("{name}: encoding failed");
        }
        let id = BARRIER_IMAGE + (frame % 8) as u32;
        wire.extend_from_slice(&kitty::barrier(ImageId(id)));
        sent += wire.len();
        let submitted = Instant::now();
        if transport.write(&wire).is_err() {
            return format!("{name}: the tty refused the bytes");
        }
        if !await_answer(transport, id) {
            return format!("{name:<10}{damage_percent:>7}%  stalled at frame {frame}");
        }
        waits.push(submitted.elapsed());
    }
    let elapsed = started.elapsed().as_secs_f32();
    let _ = transport.write(&Delete::AllVisible.encode());
    let space = IdSpace::of_process(std::process::id());
    for (first, last) in [
        (ImageId(BASE_IMAGE), ImageId(BASE_IMAGE + 200_000)),
        (space.image(BASE_IMAGE), space.image(BASE_IMAGE + 200_000)),
    ] {
        let _ = transport.write(&Delete::FreeRange(first, last).encode());
    }
    waits.sort_unstable();
    format!(
        "{name:<10}{damage_percent:>7}%{:>8.1}{:>9.1}{:>9.1}{:>11.1}",
        FRAMES as f32 / elapsed,
        percentile(&waits, 50),
        percentile(&waits, 95),
        sent as f32 / 1024.0 / FRAMES as f32,
    )
}

fn percentile(sorted: &[Duration], percent: usize) -> f32 {
    let index = (sorted.len() * percent / 100).min(sorted.len().saturating_sub(1));
    sorted
        .get(index)
        .map_or(0.0, |wait| wait.as_secs_f32() * 1000.0)
}

/// Wait for the answer naming `id`; `false` when the terminal stays
/// silent past the wait.
fn await_answer(transport: &mut TtyTransport, id: u32) -> bool {
    let deadline = Instant::now() + ANSWER_WAIT;
    let mut seen = Vec::new();
    while Instant::now() < deadline {
        if let Some(chunk) = transport.read_timeout(Duration::from_millis(50)) {
            seen.extend_from_slice(&chunk);
            let answered = kitty::parse_responses(&seen)
                .into_iter()
                .any(|answer| match answer {
                    kitty::Response::Ok { image, .. } | kitty::Response::Error { image, .. } => {
                        image.0 == id
                    }
                });
            if answered {
                return true;
            }
        }
    }
    false
}

/// A static backdrop that compresses as badly as the demos' nebula.
fn speckled(width: u32, height: u32) -> Surface {
    let mut surface = Surface::new(width, height);
    let mut rng = 0x5eed_1234u64;
    for y in 0..height {
        for x in 0..width {
            let shade = (next(&mut rng) % 7) as u8;
            surface.set(
                PixelPos::new(x, y),
                Rgba::solid(12 + shade, 12 + shade, 34 + shade),
            );
        }
    }
    surface
}

/// Change a few pixels in `percent` of the tiles.
fn churn(surface: &mut Surface, percent: u64, rng: &mut u64) {
    for tile_y in 0..surface.height().div_ceil(TILE) {
        for tile_x in 0..surface.width().div_ceil(TILE) {
            if next(rng) % 100 >= percent {
                continue;
            }
            let color = Rgba::solid((next(rng) % 256) as u8, 160, 60);
            for _ in 0..6 {
                let x = tile_x * TILE + (next(rng) % u64::from(TILE)) as u32;
                let y = tile_y * TILE + (next(rng) % u64::from(TILE)) as u32;
                surface.set(PixelPos::new(x, y), color);
            }
        }
    }
}

fn next(state: &mut u64) -> u64 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    *state
}

fn cell() -> CellPixels {
    CellPixels {
        width: 10,
        height: 20,
    }
}
