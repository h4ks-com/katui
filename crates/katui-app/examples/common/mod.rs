#![allow(clippy::expect_used)]

use katui_draw::{TextEngine, bundled_fonts};
use katui_ui::{CellPixels, Element, Ui, UiOptions};

/// Render `view` at `size` pixels, write it to
/// `target/screenshots/<name>.png` and return its RGBA bytes.
pub(crate) fn render_to_png<M: Clone + 'static>(
    view: &Element<M>,
    size: (u32, u32),
    cell: CellPixels,
    name: &str,
) -> Vec<u8> {
    let engine = TextEngine::new(bundled_fonts()).expect("the bundled fonts parse");
    let mut ui: Ui<M> = Ui::new(engine, UiOptions::default());
    let output = ui.render(view, size, cell);
    let surface = output.flatten(cell, size).expect("whole pixels");
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../target/screenshots");
    std::fs::create_dir_all(dir).expect("the target directory is writable");
    image::save_buffer(
        format!("{dir}/{name}.png"),
        surface.pixels(),
        surface.width(),
        surface.height(),
        image::ExtendedColorType::Rgba8,
    )
    .expect("the target directory is writable");
    surface.pixels().to_vec()
}
