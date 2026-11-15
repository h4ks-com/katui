//! What one full-motion frame costs on its way to a kitty terminal, step
//! by step, at 720p and 1080p: `cargo run --release --example frame_cost`.
//! Every frame changes every pixel, as a game or a video does.

#![allow(clippy::print_stdout, clippy::indexing_slicing)]

use std::hint::black_box;
use std::io::Write as _;
use std::time::{Duration, Instant};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use flate2::Compression;
use flate2::write::ZlibEncoder;
use katui_present::surface::PatchedKitty;
use katui_proto::kitty::IdSpace;
use katui_proto::probe::CellPixels;
use katui_rt::stash::TempFileStash;
use katui_scene::placement::CellRect;
use katui_scene::surface::{Surface, diff_tiles};

const FRAMES: u32 = 40;
const CELL: CellPixels = CellPixels {
    width: 10,
    height: 20,
};

/// A BGRA frame of a moving gradient with grain, the way a decoder or a
/// browser hands one over.
fn bgra_frame(width: u32, height: u32, frame: u32) -> Vec<u8> {
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            let grain = (x.wrapping_mul(7) ^ y.wrapping_mul(13) ^ frame.wrapping_mul(31)) & 7;
            pixels.extend_from_slice(&[
                ((x * 255 / width + frame * 3) % 256) as u8,
                ((y * 255 / height + frame * 2) % 256) as u8,
                (((x + y) * 255 / (width + height) + frame) % 256) as u8 ^ grain as u8,
                255,
            ]);
        }
    }
    pixels
}

fn to_surface(bgra: &[u8], width: u32, height: u32) -> Option<Surface> {
    let mut rgba = bgra.to_vec();
    for pixel in rgba.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
    }
    Surface::from_rgba(width, height, rgba)
}

fn median(mut samples: Vec<Duration>) -> f64 {
    samples.sort_unstable();
    samples
        .get(samples.len() / 2)
        .map_or(0.0, |middle| middle.as_secs_f64() * 1000.0)
}

fn time(mut step: impl FnMut(u32)) -> f64 {
    median(
        (0..FRAMES)
            .map(|frame| {
                let started = Instant::now();
                step(frame);
                started.elapsed()
            })
            .collect(),
    )
}

fn remove_stashed_files() {
    let prefix = format!("tty-graphics-protocol-katui-{}-", std::process::id());
    let Ok(entries) = std::fs::read_dir(std::env::temp_dir()) else {
        return;
    };
    for entry in entries.flatten() {
        if entry.file_name().to_string_lossy().starts_with(&prefix) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

fn measure(width: u32, height: u32) {
    let bgra: Vec<Vec<u8>> = (0..FRAMES)
        .map(|frame| bgra_frame(width, height, frame))
        .collect();
    let surfaces: Vec<Surface> = bgra
        .iter()
        .filter_map(|frame| to_surface(frame, width, height))
        .collect();
    let area = CellRect::full(
        u16::try_from(width / u32::from(CELL.width)).unwrap_or(u16::MAX),
        u16::try_from(height / u32::from(CELL.height)).unwrap_or(u16::MAX),
    );
    let frame = |index: u32| &surfaces[(index as usize) % surfaces.len()];

    let convert = time(|index| {
        black_box(to_surface(
            &bgra[(index as usize) % bgra.len()],
            width,
            height,
        ));
    });
    let diff = time(|index| {
        black_box(diff_tiles(frame(index), frame(index + 1), 64));
    });
    let clone = time(|index| {
        black_box(frame(index).clone());
    });
    let unchanged = time(|index| {
        black_box(diff_tiles(frame(index), frame(index), 64));
    });
    let zlib = time(|index| {
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::fast());
        let _ = encoder.write_all(frame(index).pixels());
        black_box(encoder.finish().ok());
    });
    let encoded = time(|index| {
        black_box(STANDARD.encode(frame(index).pixels()));
    });
    let mut direct = PatchedKitty::new(900, CELL);
    let mut wire = Vec::new();
    let mut direct_bytes = 0;
    let send_direct = time(|index| {
        wire.clear();
        direct_bytes = direct
            .render_frame(&mut wire, frame(index), area)
            .unwrap_or(0);
    });
    let mut stashed = PatchedKitty::new(900, CELL).with_stash(Box::new(TempFileStash::new()));
    let mut file_times = Vec::new();
    for index in 0..FRAMES {
        wire.clear();
        let started = Instant::now();
        let _ = stashed.render_frame(&mut wire, frame(index), area);
        file_times.push(started.elapsed());
        remove_stashed_files();
    }
    let send_file = median(file_times);
    let mut tiled = PatchedKitty::new(900, CELL).for_multiplexer(IdSpace::of_process(1), true);
    let mut tiled_bytes = 0;
    let send_tiled = time(|index| {
        wire.clear();
        tiled_bytes = tiled
            .render_frame(&mut wire, frame(index), area)
            .unwrap_or(0);
    });
    println!(
        "{width}x{height}: convert {convert:.1} ms, diff {diff:.1} ms, copy {clone:.1} ms, diff of an unchanged frame {unchanged:.1} ms, \
         zlib {zlib:.1} ms, base64 of the raw pixels {encoded:.1} ms, \
         whole frame through the escape code {send_direct:.1} ms ({} KB), through a temp file {send_file:.1} ms, \
         as tiles inside tmux {send_tiled:.1} ms ({} KB)",
        direct_bytes / 1024,
        tiled_bytes / 1024
    );
}

fn main() {
    measure(1280, 720);
    measure(1920, 1080);
}
