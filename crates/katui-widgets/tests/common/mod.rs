#![allow(clippy::expect_used)]

use std::path::PathBuf;

use katui_draw::{TextEngine, bundled_fonts};
use katui_ui::{CellPixels, Element, Ui, UiOptions};

pub(crate) const CELL: CellPixels = CellPixels {
    width: 9,
    height: 19,
};

pub(crate) fn ui<M: Clone + 'static>() -> Ui<M> {
    let engine = TextEngine::new(bundled_fonts()).expect("the bundled fonts parse");
    Ui::new(engine, UiOptions::default())
}

/// Render `view` at `size` pixels and write it to `target/<name>.png`.
pub(crate) fn render_to_png<M: Clone + 'static>(
    ui: &mut Ui<M>,
    view: &Element<M>,
    size: (u32, u32),
    name: &str,
) {
    let output = ui.render(view, size, CELL);
    let surface = output.flatten(CELL, size).expect("whole pixels");
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("../../target/{name}.png"));
    image::save_buffer(
        path,
        surface.pixels(),
        surface.width(),
        surface.height(),
        image::ExtendedColorType::Rgba8,
    )
    .expect("the target directory is writable");
}
