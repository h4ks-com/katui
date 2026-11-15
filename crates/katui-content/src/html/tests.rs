use std::sync::Arc;

use katui_draw::Rgba;
use katui_scene::surface::Surface;
use scraper::{Html, Selector};

use super::{Cascade, HtmlDefaults, HtmlHost, Renderer, Style};

struct Host;

impl HtmlHost<String> for Host {
    fn link(&self, href: &str) -> String {
        href.to_owned()
    }

    fn image(&self, _src: &str) -> Option<Arc<Surface>> {
        None
    }
}

fn defaults() -> HtmlDefaults {
    HtmlDefaults {
        font_size: 16.0,
        color: Rgba::solid(20, 20, 20),
        link: Rgba::solid(0, 0, 200),
        tint: Rgba::solid(230, 230, 230),
    }
}

#[test]
fn the_children_of_a_flex_container_become_boxes_of_their_own() {
    let document = Html::parse_document(
        "<div style=\"display: flex\"><span>one</span><p>two</p></div><span>three</span>",
    );
    let renderer = Renderer {
        cascade: Cascade::new(&document),
        host: &Host as &dyn HtmlHost<String>,
    };
    let root = Style::root(&defaults());
    let pick = |css: &str| {
        let selector = Selector::parse(css).expect("a valid selector");
        document
            .select(&selector)
            .next()
            .expect("the element exists")
    };
    let flex = renderer.style(pick("div"), &root);
    for child in ["div > span", "div > p"] {
        assert_eq!(
            renderer.style(pick(child), &flex).display,
            super::style::Display::Item,
            "{child} is a flex item"
        );
    }
    assert_eq!(
        renderer.style(pick("body > span"), &root).display,
        super::style::Display::Inline,
        "a span in normal flow stays inline"
    );
}

#[test]
fn ids_beat_classes_beat_types_and_the_style_attribute_beats_all() {
    let document = Html::parse_document(
        "<style>#lead { color: green } .note { color: blue; font-size: 20px } p { color: red }</style>\
         <p id=lead class=note>one</p><p class=note style=\"color: #fff\">two</p><p>three</p>",
    );
    let renderer = Renderer {
        cascade: Cascade::new(&document),
        host: &Host as &dyn HtmlHost<String>,
    };
    let root = Style::root(&defaults());
    let selector = Selector::parse("p").expect("a valid selector");
    let colors: Vec<(Rgba, f32)> = document
        .select(&selector)
        .map(|paragraph| {
            let style = renderer.style(paragraph, &root);
            (style.color, style.size)
        })
        .collect();
    assert_eq!(
        colors,
        [
            (Rgba::solid(0, 128, 0), 20.0),
            (Rgba::solid(255, 255, 255), 20.0),
            (Rgba::solid(255, 0, 0), 16.0)
        ]
    );
}
