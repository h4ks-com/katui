#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::cast_possible_truncation
)]

use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use katui_draw::{ColorStop, Paint, Point, Rgba, TextEngine, bundled_fonts};
use katui_scene::placement::{ImageId, Placement};
use katui_scene::surface::Surface;
use katui_ui::{Align, CellPixels, Element, Length, RenderOutput, Ui, UiOptions};

const CELL: CellPixels = CellPixels {
    width: 10,
    height: 20,
};
const SCREEN: (u32, u32) = (400, 240);

fn ui() -> Ui<u32> {
    ui_with(UiOptions::default())
}

fn ui_with(options: UiOptions) -> Ui<u32> {
    let engine = TextEngine::new(bundled_fonts()).expect("fixture fonts parse");
    Ui::new(engine, options)
}

fn solid(r: u8, g: u8, b: u8) -> Paint {
    Paint::Solid(Rgba::solid(r, g, b))
}

fn cell_layer(key: u64, rows: u16, text: &str) -> Element<u32> {
    Element::column([Element::text(text)])
        .width(Length::Fill)
        .height(Length::Px(f32::from(rows * CELL.height)))
        .background(solid(30, 34, 48))
        .layer(key)
}

fn three_layers(middle: &str) -> Element<u32> {
    Element::column([
        cell_layer(1, 2, "first"),
        cell_layer(2, 2, middle),
        cell_layer(3, 2, "third"),
    ])
    .width(Length::Fill)
}

fn list(offset: f32, layers: u64) -> Element<u32> {
    Element::column([
        Element::column((0..layers).map(|key| cell_layer(key, 2, "message")))
            .size(Length::Fill, Length::Px(120.0))
            .scroll(offset),
    ])
}

fn image_ids(output: &RenderOutput) -> Vec<ImageId> {
    output
        .placements
        .iter()
        .map(|placement| placement.image)
        .collect()
}

fn new_ids(output: &RenderOutput) -> Vec<ImageId> {
    output.new_images.iter().map(|image| image.id).collect()
}

fn placement_for(output: &RenderOutput, image: ImageId) -> &Placement {
    output
        .placements
        .iter()
        .find(|placement| placement.image == image)
        .expect("tests look up images placed in this output")
}

#[test]
fn the_same_tree_twice_uploads_nothing_and_places_identically() {
    let mut ui = ui();
    let first = ui.render(&three_layers("middle"), SCREEN, CELL);
    let second = ui.render(&three_layers("middle"), SCREEN, CELL);
    assert_eq!(first.placements.len(), 3);
    assert_eq!(first.new_images.len(), 3);
    assert!(second.new_images.is_empty());
    assert_eq!(first.placements, second.placements);
}

#[test]
fn changing_one_layers_text_replaces_exactly_that_image() {
    let mut ui = ui();
    let first = ui.render(&three_layers("middle"), SCREEN, CELL);
    let second = ui.render(&three_layers("edited"), SCREEN, CELL);
    assert_eq!(second.new_images.len(), 1);
    let before = image_ids(&first);
    let after = image_ids(&second);
    assert_eq!(before[0], after[0]);
    assert_ne!(before[1], after[1]);
    assert_eq!(before[2], after[2]);
    assert!(after[1] > before[2], "ids count up: {after:?}");
}

#[test]
fn image_ids_start_at_the_given_id_and_only_increase() {
    let mut ui = ui_with(UiOptions {
        first_image_id: 500,
        ..UiOptions::default()
    });
    let first = ui.render(&three_layers("a"), SCREEN, CELL);
    let second = ui.render(&three_layers("b"), SCREEN, CELL);
    assert_eq!(
        new_ids(&first),
        vec![ImageId(500), ImageId(501), ImageId(502)]
    );
    assert_eq!(new_ids(&second), vec![ImageId(503)]);
}

#[test]
fn scrolling_moves_and_crops_without_new_images() {
    let mut ui = ui();
    let at_top = ui.render(&list(0.0, 8), SCREEN, CELL);
    assert_eq!(at_top.placements.len(), 3, "the viewport fits three layers");
    let first_image = at_top.placements[0].image;
    assert_eq!(at_top.placements[0].area.y, 0);
    assert_eq!(at_top.placements[0].crop, None);

    let scrolled = ui.render(&list(20.0, 8), SCREEN, CELL);
    assert!(
        scrolled.new_images.len() <= 1,
        "only the entering layer rasterizes"
    );
    let cropped = placement_for(&scrolled, first_image);
    assert_eq!((cropped.area.y, cropped.area.rows), (0, 1));
    assert_eq!(
        cropped.crop,
        Some(katui_scene::placement::PixelRect {
            x: 0,
            y: 20,
            width: 400,
            height: 20,
        })
    );

    let back = ui.render(&list(0.0, 8), SCREEN, CELL);
    assert!(back.new_images.is_empty(), "every layer is cached");
    assert_eq!(back.placements, at_top.placements);

    let far = ui.render(&list(60.0, 8), SCREEN, CELL);
    assert!(
        far.placements
            .iter()
            .all(|placement| placement.image != first_image),
        "a layer fully outside the viewport has no placement",
    );
    assert_eq!(far.placements.len(), 4, "two whole layers and two cropped");
}

#[test]
fn a_fractional_scroll_offset_reuses_the_images() {
    let mut ui = ui();
    let offsets = [0.0, 7.0, 13.5, 40.0];
    ui.render(&list(0.0, 8), SCREEN, CELL);
    ui.render(&list(60.0, 8), SCREEN, CELL);
    for offset in offsets {
        let output = ui.render(&list(offset, 8), SCREEN, CELL);
        assert!(output.new_images.is_empty(), "offset {offset}");
    }
}

#[test]
fn scrolled_layers_keep_their_z_values() {
    let mut ui = ui();
    let at_top = ui.render(&list(0.0, 8), SCREEN, CELL);
    let scrolled = ui.render(&list(20.0, 8), SCREEN, CELL);
    for placement in &at_top.placements {
        if let Some(moved) = scrolled
            .placements
            .iter()
            .find(|candidate| candidate.image == placement.image)
        {
            assert_eq!(placement.z, moved.z);
        }
    }
    assert!(
        at_top
            .placements
            .windows(2)
            .all(|pair| pair[0].z < pair[1].z)
    );
}

#[test]
fn every_placed_image_is_available_after_scrolling_back() {
    let mut ui = ui();
    let at_top = ui.render(&list(0.0, 8), SCREEN, CELL);
    ui.render(&list(60.0, 8), SCREEN, CELL);
    let back = ui.render(&list(0.0, 8), SCREEN, CELL);
    let placed: HashSet<ImageId> = back.images.iter().map(|image| image.id).collect();
    for placement in &at_top.placements {
        assert!(placed.contains(&placement.image));
    }
}

#[test]
fn hit_testing_finds_the_innermost_and_honors_clips() {
    let mut ui = ui();
    let root = Element::column([Element::column([Element::row([Element::text("reply")
        .size(Length::Px(40.0), Length::Px(20.0))
        .id(30)
        .on_click(30)])
    .height(Length::Px(200.0))
    .width(Length::Fill)
    .on_click(1)
    .on_hover(2)])
    .size(Length::Fill, Length::Px(100.0))
    .scroll(0.0)]);
    ui.render(&root, SCREEN, CELL);

    let inner = ui
        .hit_test(10.0, 10.0)
        .expect("the reply element covers 10,10");
    assert_eq!(inner.id, Some(30));
    assert_eq!(inner.on_click, Some(30));
    assert_eq!(inner.on_hover, Some(2), "hover bubbles to the row");

    let outer = ui.hit_test(200.0, 50.0).expect("the row covers 200,50");
    assert_eq!(outer.on_click, Some(1));

    assert_eq!(ui.hit_test(200.0, 150.0), None, "clipped by the scroll box");
}

#[test]
fn each_linked_span_of_a_paragraph_takes_its_own_clicks() {
    let span = |text: &str| katui_draw::TextSpan {
        text: text.to_owned(),
        ..katui_draw::TextSpan::default()
    };
    let mut ui = ui();
    let paragraph = Element::rich(vec![span("WWWW"), span("MMMM"), span("WWWW")])
        .font_size(20.0)
        .on_click(9)
        .on_span_click(1, 1)
        .on_span_click(2, 2);
    ui.render(&Element::column([paragraph]), SCREEN, CELL);
    let at = |x: f32| ui.hit_test(x, 10.0).and_then(|hit| hit.on_click);
    assert_eq!(at(5.0), Some(9), "plain text takes the paragraph's click");
    let clicks: Vec<Option<u32>> = (0..400_u16).map(|x| at(f32::from(x))).collect();
    let first = |wanted: u32| clicks.iter().position(|click| *click == Some(wanted));
    let (one, two) = (
        first(1).expect("span 1 is clickable"),
        first(2).expect("span 2 is clickable"),
    );
    assert!(one < two, "the spans keep their order along the line");
}

#[test]
fn hovering_a_row_changes_only_its_layer() {
    let view = |hovered: Option<u64>| -> Element<u32> {
        Element::column((1..=3).map(|key| {
            let background = if hovered == Some(key) {
                solid(70, 80, 110)
            } else {
                solid(30, 34, 48)
            };
            Element::row([Element::text("message")])
                .width(Length::Fill)
                .height(Length::Px(40.0))
                .background(background)
                .on_hover(key as u32)
                .layer(key)
        }))
        .width(Length::Fill)
    };
    let mut ui = ui();
    let plain = ui.render(&view(None), SCREEN, CELL);
    let hovered = ui.render(&view(Some(2)), SCREEN, CELL);
    assert_eq!(hovered.new_images.len(), 1);
    assert_eq!(hovered.placements[0], plain.placements[0]);
    assert_eq!(hovered.placements[2], plain.placements[2]);
    let hit = ui.hit_test(5.0, 50.0).expect("the second row covers 5,50");
    assert_eq!(hit.on_hover, Some(2));
}

#[test]
fn a_layer_snaps_down_and_rounds_its_size_up_to_whole_cells() {
    let root = Element::column([Element::column([])
        .size(Length::Px(25.0), Length::Px(21.0))
        .background(solid(10, 10, 10))
        .layer(9)])
    .padding_xy(13.0, 7.0);
    let mut ui = ui();
    let output = ui.render(&root, SCREEN, CELL);
    assert_eq!(output.placements.len(), 1);
    let area = output.placements[0].area;
    assert_eq!((area.x, area.y, area.columns, area.rows), (1, 0, 3, 2));
    assert_eq!(output.placements[0].crop, None);
    let image = &output.new_images[0];
    assert_eq!((image.width(), image.height()), (30, 40));
    assert_eq!(image.rgba().len(), 30 * 40 * 4);
}

fn wide_layer() -> Element<u32> {
    Element::column([Element::text("a wide layer")])
        .size(Length::Px(250.0), Length::Px(160.0))
        .background(solid(30, 34, 48))
        .layer(4)
}

#[test]
fn a_split_layer_covers_its_cells_exactly_without_gaps() {
    let mut ui = ui_with(UiOptions {
        max_layer_cells: Some((10, 3)),
        ..UiOptions::default()
    });
    let root = Element::column([wide_layer()]);
    let output = ui.render(&root, SCREEN, CELL);
    assert_eq!(output.placements.len(), 9, "25x8 cells cut at 10x3");
    let mut covered = HashSet::new();
    for placement in &output.placements {
        assert_eq!(placement.crop, None);
        assert!(placement.area.columns <= 10 && placement.area.rows <= 3);
        for row in 0..placement.area.rows {
            for column in 0..placement.area.columns {
                assert!(
                    covered.insert((placement.area.x + column, placement.area.y + row)),
                    "tiles overlap at {column},{row}",
                );
            }
        }
    }
    assert_eq!(covered.len(), 25 * 8, "split layers leave no cell gap");
    for image in &output.new_images {
        let placement = placement_for(&output, image.id);
        assert_eq!(image.width(), u32::from(placement.area.columns) * 10);
        assert_eq!(image.height(), u32::from(placement.area.rows) * 20);
    }

    let unchanged = ui.render(&root, SCREEN, CELL);
    assert!(unchanged.new_images.is_empty());
    assert_eq!(unchanged.placements, output.placements);
}

#[test]
fn split_tiles_recompose_the_layer_pixels() {
    let mut split = ui_with(UiOptions {
        max_layer_cells: Some((10, 3)),
        ..UiOptions::default()
    });
    let mut whole = ui();
    let root = Element::column([wide_layer()]);
    let tiled = split.render(&root, SCREEN, CELL);
    let single = whole.render(&root, SCREEN, CELL);
    assert_eq!(tiled.flatten(CELL, SCREEN), single.flatten(CELL, SCREEN));
}

#[test]
fn absent_keys_are_evicted_and_get_fresh_images_on_return() {
    let mut ui = ui();
    let first = ui.render(&three_layers("middle"), SCREEN, CELL);
    ui.render(
        &Element::column([cell_layer(1, 2, "first")]).width(Length::Fill),
        SCREEN,
        CELL,
    );
    let back = ui.render(&three_layers("middle"), SCREEN, CELL);
    assert_eq!(back.new_images.len(), 2, "layers 2 and 3 were evicted");
    assert_eq!(back.placements[0], first.placements[0]);
    assert_ne!(back.placements[1].image, first.placements[1].image);
}

#[test]
fn content_outside_layers_lands_in_a_root_layer_under_them() {
    let root = Element::stack([
        Element::column([Element::text("backdrop")])
            .size(Length::Fill, Length::Fill)
            .background(solid(12, 14, 20)),
        cell_layer(1, 2, "above"),
    ]);
    let mut ui = ui();
    let output = ui.render(&root, SCREEN, CELL);
    assert_eq!(output.placements.len(), 2);
    assert_eq!(
        (
            output.placements[0].area.columns,
            output.placements[0].area.rows
        ),
        (40, 12)
    );
    assert!(output.placements[0].z < output.placements[1].z);
}

fn avatar(hue: u8) -> Arc<Surface> {
    let pixels = (0..48 * 48_u32)
        .flat_map(|i| {
            let shade = ((i % 48) * 4) as u8;
            [hue, shade, 200 - shade / 2, 255]
        })
        .collect();
    Arc::new(Surface::from_rgba(48, 48, pixels).expect("the avatar yields 4 bytes per pixel"))
}

fn chat_message(
    key: u64,
    author: &str,
    body: &str,
    face: &Arc<Surface>,
    hovered: bool,
) -> Element<u32> {
    let background = if hovered {
        solid(52, 58, 82)
    } else {
        solid(30, 34, 48)
    };
    Element::row([
        Element::image(Arc::clone(face))
            .size(Length::Px(40.0), Length::Px(40.0))
            .radius(20.0),
        Element::column([
            Element::text(author)
                .weight(700)
                .font_size(15.0)
                .color(Rgba::solid(150, 190, 255)),
            Element::text(body)
                .font_size(16.0)
                .color(Rgba::solid(232, 234, 240)),
        ])
        .gap(4.0)
        .width(Length::Fill),
    ])
    .gap(12.0)
    .padding(10.0)
    .width(Length::Fill)
    .background(background)
    .radius(10.0)
    .on_hover(key as u32)
    .layer(key)
}

fn chat_view(offset: f32, hovered: u64) -> Element<u32> {
    let (mallow, sage) = (avatar(220), avatar(90));
    let messages = (10..18_u64).map(|key| {
        let face = if key % 2 == 0 { &mallow } else { &sage };
        chat_message(
            key,
            if key % 2 == 0 { "acme" } else { "reviewer" },
            match key % 3 {
                0 => "Layers keep their pixels, so scrolling only moves placements \u{1F680}",
                1 => "Short note \u{1F389}",
                _ => "A longer message that wraps onto a second line because it is wide enough to need one.",
            },
            face,
            key == hovered,
        )
    });
    let header = Element::row([
        Element::text("general").weight(700).font_size(20.0),
        Element::spacer(),
        Element::text("8 members")
            .font_size(14.0)
            .color(Rgba::solid(160, 165, 185)),
    ])
    .align(Align::Center)
    .padding_xy(16.0, 0.0)
    .size(Length::Fill, Length::Px(48.0))
    .background(Paint::Linear {
        from: Point::new(0.0, 0.0),
        to: Point::new(640.0, 0.0),
        stops: vec![
            ColorStop {
                offset: 0.0,
                color: Rgba::solid(92, 72, 180),
            },
            ColorStop {
                offset: 1.0,
                color: Rgba::solid(40, 120, 170),
            },
        ],
    })
    .layer(1);
    let list = Element::column(messages)
        .gap(8.0)
        .padding(8.0)
        .size(Length::Fill, Length::Fill)
        .scroll(offset);
    let composer = Element::row([Element::text("Write a message")
        .font_size(16.0)
        .color(Rgba::solid(150, 155, 175))])
    .align(Align::Center)
    .padding_xy(16.0, 0.0)
    .size(Length::Fill, Length::Px(56.0))
    .background(solid(36, 40, 56))
    .border(1.0, solid(70, 76, 100))
    .layer(2);
    Element::column([header, list, composer])
        .size(Length::Fill, Length::Fill)
        .background(solid(18, 20, 28))
}

#[test]
fn writes_a_composed_chat_png() {
    let size = (640, 560);
    let mut ui = ui();
    let output = ui.render(&chat_view(30.0, 12), size, CELL);
    assert!(output.placements.len() >= 8);

    let surface = output
        .flatten(CELL, size)
        .expect("the size holds whole pixels");
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/katui-ui");
    fs::create_dir_all(&dir).expect("the target directory is writable");
    image::save_buffer(
        dir.join("chat.png"),
        surface.pixels(),
        surface.width(),
        surface.height(),
        image::ExtendedColorType::Rgba8,
    )
    .expect("the surface pixels match its dimensions and the target directory is writable");
}

#[test]
fn stack_children_share_the_whole_box() {
    let mut view = ui();
    let tree: Element<u32> = Element::stack([
        Element::row([cell_layer(1, 2, "under")]).size(Length::Fill, Length::Fill),
        Element::column([cell_layer(2, 2, "over")])
            .justify(Align::End)
            .size(Length::Fill, Length::Fill),
    ]);
    let output = view.render(&tree, SCREEN, CELL);
    let under = output.layer_areas[&1];
    let over = output.layer_areas[&2];
    assert_eq!(under.columns, 40, "the first child spans the width");
    assert_eq!(over.y + over.rows, 12, "the second reaches the bottom");
    assert_eq!(under.y, 0, "and they overlap from the top of the box");
}

#[test]
fn a_split_layer_joins_back_into_its_whole_surface() {
    let mut view = ui_with(UiOptions {
        max_layer_cells: Some((10, 1)),
        ..UiOptions::default()
    });
    let output = view.render(&three_layers("joined"), SCREEN, CELL);
    assert!(output.placements.len() > 3, "the layers really were split");
    let whole = view.layer_surface(1).expect("layer 1 was rendered");
    let area = output.layer_areas[&1];
    assert_eq!(whole.width(), u32::from(area.columns) * 10);
    assert_eq!(whole.height(), u32::from(area.rows) * 20);
    assert!(
        view.layer_surface(99).is_none(),
        "an unknown key has no pixels"
    );
}
