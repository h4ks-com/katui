//! A small page through the HTML subset, written to a PNG.
#![cfg(feature = "html")]

use std::sync::Arc;

use katui_content::{HtmlDefaults, HtmlHost, html};
use katui_draw::{Rgba, TextEngine, bundled_fonts};
use katui_scene::surface::Surface;
use katui_ui::{CellPixels, Ui, UiOptions};

const CELL: CellPixels = CellPixels {
    width: 9,
    height: 19,
};
const SIZE: (u32, u32) = (9 * 90, 19 * 34);

const PAGE: &str = r#"<!doctype html>
<html><head><title>ignored</title><style>
  body { background: #1b1826; color: #e8e4f0; padding: 16px }
  header { display: flex; justify-content: space-between; align-items: center; gap: 12px }
  .badge { background: #8aa8ff; color: #10121e; padding: 2px 8px; border-radius: 6px }
  .card { border: 1px solid #4a4560; border-radius: 10px; padding: 12px; background: rgba(255,255,255,0.04) }
  #secret { display: none }
  a { color: #8aa8ff }
</style></head>
<body>
  <header><h1>Release notes</h1><span class="badge">v0.4</span></header>
  <p>This build adds <b>sixel</b> output, <i>wide glyph</i> layout and a <a href="/widgets">widgets crate</a>.
     Run <code>just demo-todo</code> to try it.</p>
  <div id="secret">this text never shows</div>
  <div class="card">
    <h3>What changed</h3>
    <ul><li>Games draw real pixels in iTerm2</li><li>Apps start with <b>one call</b></li></ul>
    <ol><li>Probe the terminal</li><li>Draw the view</li></ol>
  </div>
  <blockquote>Small frames, sent only when something changed.</blockquote>
  <pre>fn main() {
    run(Todo::default())
}</pre>
</body></html>"#;

struct Host;

impl HtmlHost<String> for Host {
    fn link(&self, href: &str) -> String {
        href.to_owned()
    }

    fn image(&self, _src: &str) -> Option<Arc<Surface>> {
        None
    }
}

#[test]
fn a_page_renders_with_its_styles_and_its_link_answers_clicks() {
    let defaults = HtmlDefaults {
        font_size: 15.0,
        color: Rgba::solid(230, 230, 230),
        link: Rgba::solid(120, 160, 255),
        tint: Rgba::solid(45, 42, 60),
    };
    let root = html(PAGE, &defaults, &Host);
    let engine = TextEngine::new(bundled_fonts()).expect("the bundled fonts parse");
    let mut ui: Ui<String> = Ui::new(engine, UiOptions::default());
    let output = ui.render(&root, SIZE, CELL);
    let surface = output.flatten(CELL, SIZE).expect("whole pixels");
    let path = format!("{}/../../target/katui-html.png", env!("CARGO_MANIFEST_DIR"));
    image::save_buffer(
        path,
        surface.pixels(),
        surface.width(),
        surface.height(),
        image::ExtendedColorType::Rgba8,
    )
    .expect("the target directory is writable");
    let link = (0..SIZE.1 / 4)
        .flat_map(|row| (0..SIZE.0 / 8).map(move |column| (column, row)))
        .find_map(|(column, row)| {
            let (x, y) = (f32::from(column as u16) * 8.0, f32::from(row as u16) * 4.0);
            ui.hit_test(x, y).and_then(|hit| hit.on_click)
        });
    assert_eq!(link.as_deref(), Some("/widgets"));
}

const LINKS: &str = r#"<body>
  <p>Read <a href="a.html">the first link</a> or <a href="b.html">the second</a> today.</p>
  <a href="c.html" style="display: block">A whole block that links</a>
  <p><a href="d.html"><img src="logo.png" width="40" height="20"></a></p>
  <ul><li><p>An item in a paragraph</p></li></ul>
</body>"#;

struct Pictures;

impl HtmlHost<String> for Pictures {
    fn link(&self, href: &str) -> String {
        href.to_owned()
    }

    fn image(&self, src: &str) -> Option<Arc<Surface>> {
        (src == "logo.png").then(|| Arc::new(Surface::new(40, 20)))
    }
}

#[test]
fn every_link_answers_its_own_clicks_wherever_it_sits() {
    let defaults = HtmlDefaults {
        font_size: 15.0,
        color: Rgba::solid(230, 230, 230),
        link: Rgba::solid(120, 160, 255),
        tint: Rgba::solid(45, 42, 60),
    };
    let root = html(LINKS, &defaults, &Pictures);
    let engine = TextEngine::new(bundled_fonts()).expect("the bundled fonts parse");
    let mut ui: Ui<String> = Ui::new(engine, UiOptions::default());
    let output = ui.render(&root, SIZE, CELL);
    let surface = output.flatten(CELL, SIZE).expect("whole pixels");
    let path = format!(
        "{}/../../target/katui-html-links.png",
        env!("CARGO_MANIFEST_DIR")
    );
    image::save_buffer(
        path,
        surface.pixels(),
        surface.width(),
        surface.height(),
        image::ExtendedColorType::Rgba8,
    )
    .expect("the target directory is writable");
    let mut found: Vec<String> = Vec::new();
    for row in 0..SIZE.1 / 4 {
        for column in 0..SIZE.0 / 4 {
            let (x, y) = (f32::from(column as u16) * 4.0, f32::from(row as u16) * 4.0);
            if let Some(href) = ui.hit_test(x, y).and_then(|hit| hit.on_click)
                && !found.contains(&href)
            {
                found.push(href);
            }
        }
    }
    assert_eq!(found, ["a.html", "b.html", "c.html", "d.html"]);
    assert_eq!(
        ui.hit_test(2.0, 8.0).and_then(|hit| hit.on_click),
        None,
        "the text before a link stays plain"
    );
}
