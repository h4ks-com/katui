//! The pacing instrument: what each pixel strategy costs end to end on
//! this terminal. Every frame is followed by a kitty query, and the
//! frame counts as delivered when the terminal answers it: the terminal
//! parses its stream in order, so the answer proves the frame was
//! consumed. Run with `just pace`.
#![allow(clippy::cast_precision_loss, clippy::print_stdout)]

use std::time::{Duration, Instant};

use katui_present::surface::{AnimatedKitty, PatchedKitty};
use katui_proto::kitty::{self, Delete, IdSpace, ImageId};
use katui_proto::probe::{Capabilities, CellPixels, Transport as _};
use katui_rt::tty::TtyTransport;
use katui_scene::placement::CellRect;
use katui_scene::surface::{PixelPos, Rgba, Surface};

const FRAMES: usize = 90;
/// From this share of damaged tiles the animated strategy sends whole frames.
const DENSE_DAMAGE_PERCENT: u64 = 25;
const TILE: u32 = 40;
/// Divide the terminal's pixel size by this, as a game shrinks its frame
/// under load: `PACE_DIVISOR=2 just pace`.
static PIXEL_DIVISOR: std::sync::LazyLock<u32> = std::sync::LazyLock::new(|| {
    std::env::var("PACE_DIVISOR")
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|divisor| *divisor > 0)
        .unwrap_or(1)
});
const BASE_IMAGE: u32 = 700;
const BARRIER_IMAGE: u32 = 41;
const ANSWER_WAIT: Duration = Duration::from_secs(4);

fn main() {
    let mut caps = katui_rt::tty::probe_default();
    // `PACE_FILES=1 just pace` measures temp files on a terminal that also reads shared memory
    if caps.shared_memory && std::env::var_os("PACE_FILES").is_some() {
        caps.shared_memory = false;
        caps.local_files = true;
    }
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
    let cell = katui_rt::tty::cell_size(&caps);
    let width = u32::from(size.columns) * u32::from(cell.width) / *PIXEL_DIVISOR;
    let height = u32::from(size.rows) * u32::from(cell.height) / *PIXEL_DIVISOR;
    let medium = medium_name(&caps);

    let mut lines = vec![format!(
        "{width}x{height} px, {} frames per row, local medium: {medium}\n{:<14}{:>8}{:>8}{:>9}{:>9}{:>11}",
        FRAMES, "strategy", "damage", "fps", "p50 ms", "p95 ms", "KB/frame"
    )];
    for damage in [5, 20, 60, 100] {
        for (name, budget, local) in [
            ("tiles", usize::MAX, false),
            ("whole", 0, false),
            ("tiles+local", usize::MAX, true),
            ("whole+local", 0, true),
        ] {
            let Some(mut patched) = presenter(&caps, cell, budget, local) else {
                continue;
            };
            lines.push(run(
                name,
                damage,
                width,
                height,
                &mut transport,
                |out, surface| patched.render_frame(out, surface, area),
            ));
        }
        if caps.animation && !caps.multiplexed() && damage < DENSE_DAMAGE_PERCENT {
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

/// The presenter a strategy row measures, or none for a local row on a
/// terminal without a local medium.
fn presenter(
    caps: &Capabilities,
    cell: CellPixels,
    budget: usize,
    local: bool,
) -> Option<PatchedKitty> {
    let mut patched = PatchedKitty::with_tiles(BASE_IMAGE, cell, TILE, budget);
    if caps.multiplexed() {
        patched = patched.for_multiplexer(
            IdSpace::of_process(std::process::id()),
            caps.relative_placements,
        );
    }
    if local {
        patched = patched.with_stash(katui_rt::stash::local_stash(caps)?);
    }
    Some(patched)
}

fn medium_name(caps: &Capabilities) -> &'static str {
    if caps.shared_memory {
        "shm"
    } else if caps.local_files {
        "file"
    } else {
        "inline"
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
            return format!("{name:<14}{damage_percent:>7}%  stalled at frame {frame}");
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
        "{name:<14}{damage_percent:>7}%{:>8.1}{:>9.1}{:>9.1}{:>11.1}",
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
