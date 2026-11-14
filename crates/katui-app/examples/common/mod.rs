#![allow(clippy::expect_used)]

use katui_draw::{TextEngine, bundled_fonts};
use katui_ui::{CellPixels, Element, Ui, UiOptions};

/// Render `view` at `size` pixels, write it to `target/<name>.png` and
/// return its RGBA bytes.
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
    let path = format!("{}/../../target/{name}.png", env!("CARGO_MANIFEST_DIR"));
    image::save_buffer(
        path,
        surface.pixels(),
        surface.width(),
        surface.height(),
        image::ExtendedColorType::Rgba8,
    )
    .expect("the target directory is writable");
    surface.pixels().to_vec()
}
