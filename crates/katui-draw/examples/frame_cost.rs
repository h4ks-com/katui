//! Measures one interactive-rate frame: a full clear plus twenty laid-out
//! lines of text on a 2500x1400 canvas. Run with
//! `cargo run --release -p katui-draw --example frame_cost`.

#![allow(clippy::print_stdout, clippy::expect_used, clippy::cast_precision_loss)]

use std::hint::black_box;
use std::time::{Duration, Instant};

use katui_draw::{Canvas, Paint, Point, Rect, Rgba, TextEngine, TextLayout, TextStyle};

const FONT: &[u8] = include_bytes!("../assets/AtkinsonHyperlegibleNext[wght].ttf");
const WIDTH: u32 = 2500;
const HEIGHT: u32 = 1400;
const ROUNDS: u32 = 200;

fn average(mut run: impl FnMut()) -> Duration {
    let start = Instant::now();
    for _ in 0..ROUNDS {
        run();
    }
    start.elapsed() / ROUNDS
}

fn draw_frame(canvas: &mut Canvas, lines: &[TextLayout], ink: &Paint) {
    canvas.clear(Rgba::solid(18, 20, 28));
    for (line, row) in lines.iter().zip(0u16..) {
        canvas.draw_text(line, Point::new(40.0, 20.0 + f32::from(row) * 34.0), ink);
    }
}

fn main() {
    let mut engine = TextEngine::new(vec![FONT.to_vec()]).expect("font parses");
    let style = TextStyle::new(24.0);
    let text = "The quick brown fox jumps over the lazy dog, then keeps going until it reaches the edge of the panel.";
    let layout_start = Instant::now();
    let lines: Vec<TextLayout> = (0..20).map(|_| engine.layout(text, &style, None)).collect();
    println!(
        "layout 20 lines (first, cold): {:?}",
        layout_start.elapsed()
    );

    let mut canvas = Canvas::new(WIDTH, HEIGHT).expect("2500x1400 is a valid canvas size");
    let ink = Paint::Solid(Rgba::solid(232, 234, 240));
    println!(
        "clear + 20 text lines: {:?}",
        average(|| draw_frame(&mut canvas, &lines, &ink))
    );

    let panel = Rect::new(0.0, 0.0, 2500.0, 1400.0);
    let fill = Paint::Solid(Rgba::solid(36, 40, 56));
    println!(
        "full-canvas fill_round_rect: {:?}",
        average(|| canvas.fill_round_rect(panel, 24.0, &fill))
    );

    println!(
        "to_surface (full): {:?}",
        average(|| {
            black_box(
                canvas
                    .to_surface()
                    .expect("the canvas buffer matches its own dimensions"),
            );
        })
    );
}
