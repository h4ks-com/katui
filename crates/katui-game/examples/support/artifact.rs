use katui_game::Light;

pub(crate) fn save_frame(light: &Light, name: &str) {
    let surface = light.to_surface();
    save_rgba(surface.pixels(), (surface.width(), surface.height()), name);
}

pub(crate) fn save_rgba(pixels: &[u8], (width, height): (u32, u32), name: &str) {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../target/screenshots");
    std::fs::create_dir_all(dir).expect("the target directory is writable");
    image::save_buffer(
        format!("{dir}/{name}.png"),
        pixels,
        width,
        height,
        image::ColorType::Rgba8,
    )
    .expect("the png writer saves under target");
}
