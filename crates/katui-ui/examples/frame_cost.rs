//! Measures a chat view of 40 message layers re-rendered after one layer
//! changed. Run with `cargo run --release -p katui-ui --example frame_cost`.

#![allow(clippy::print_stdout, clippy::expect_used)]

use std::time::{Duration, Instant};

use katui_draw::{Paint, Rgba, TextEngine, bundled_fonts};
use katui_ui::{CellPixels, Element, Length, Ui, UiOptions};

const CELL: CellPixels = CellPixels {
    width: 10,
    height: 20,
};
const SCREEN: (u32, u32) = (1600, 900);
const ROUNDS: u32 = 500;

fn message(key: u64, edit: u32) -> Element<u32> {
    let body = if key == 5 {
        format!("edited message revision {edit} \u{1F680}")
    } else {
        format!("message {key} says something long enough to wrap in a narrow window")
    };
    Element::row([
        Element::column([])
            .size(Length::Px(40.0), Length::Px(40.0))
            .background(Paint::Solid(Rgba::solid(120, 90, 220)))
            .radius(20.0),
        Element::column([
            Element::text(format!("author {key}")).weight(700),
            Element::text(body),
        ])
        .gap(4.0)
        .width(Length::Fill),
    ])
    .gap(12.0)
    .padding(10.0)
    .width(Length::Fill)
    .background(Paint::Solid(Rgba::solid(30, 34, 48)))
    .radius(10.0)
    .layer(key)
}

fn view(edit: u32) -> Element<u32> {
    Element::column((0..40).map(|key| message(key, edit)))
        .size(Length::Fill, Length::Fill)
        .gap(6.0)
        .scroll(0.0)
}

fn main() {
    let engine = TextEngine::new(bundled_fonts()).expect("the bundled fonts parse");
    let mut ui: Ui<u32> = Ui::new(engine, UiOptions::default());
    let cold = Instant::now();
    let first = ui.render(&view(0), SCREEN, CELL);
    println!(
        "cold render: {:?}, {} placements, {} images",
        cold.elapsed(),
        first.placements.len(),
        first.new_images.len()
    );

    let mut total = Duration::ZERO;
    let mut worst = Duration::ZERO;
    let mut uploads = 0;
    for edit in 1..=ROUNDS {
        let tree = view(edit);
        let start = Instant::now();
        let output = ui.render(&tree, SCREEN, CELL);
        let spent = start.elapsed();
        total += spent;
        worst = worst.max(spent);
        uploads += output.new_images.len();
    }
    println!(
        "one layer changed: mean {:?}, worst {:?}, {} uploads over {ROUNDS} renders",
        total / ROUNDS,
        worst,
        uploads
    );
}
