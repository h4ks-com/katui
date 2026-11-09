use katui_game::Light;

pub fn save_frame(light: &Light, name: &str) {
    let surface = light.to_surface();
    let path = format!(
        "{}/../../target/katui-{name}-frame.png",
        env!("CARGO_MANIFEST_DIR")
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
