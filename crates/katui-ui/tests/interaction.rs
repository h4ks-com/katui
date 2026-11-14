//! Popups, scrolling, presses, drags and hover leaves on real layouts.
#![allow(clippy::expect_used, clippy::float_cmp)]

use katui_draw::{Paint, Rgba, TextEngine, bundled_fonts};
use katui_ui::{CellPixels, Element, Length, Scroll, Ui, UiOptions};

const CELL: CellPixels = CellPixels {
    width: 10,
    height: 20,
};
const SCREEN: (u32, u32) = (400, 240);

#[derive(Debug, Clone, PartialEq)]
enum Message {
    Under,
    Popup,
    Scrolled(Scroll),
    Pressed(f32, f32),
    Leave,
    Hover,
}

fn ui() -> Ui<Message> {
    let engine = TextEngine::new(bundled_fonts()).expect("fixture fonts parse");
    Ui::new(engine, UiOptions::default())
}

fn block(height: f32) -> Element<Message> {
    Element::row([])
        .size(Length::Fill, Length::Px(height))
        .background(Paint::Solid(Rgba::solid(40, 40, 60)))
}

#[test]
fn a_popup_stays_out_of_the_layout_and_takes_clicks_above_later_siblings() {
    let mut ui = ui();
    let view = Element::column([
        Element::column([
            block(20.0),
            block(60.0)
                .popup(0.0, 20.0)
                .on_click(Message::Popup)
                .width(Length::Px(100.0)),
        ]),
        block(80.0).on_click(Message::Under),
    ])
    .width(Length::Fill);
    let output = ui.render(&view, SCREEN, CELL);
    let hit = ui
        .hit_test(5.0, 30.0)
        .expect("the popup is under the pointer");
    assert_eq!(
        hit.on_click,
        Some(Message::Popup),
        "the popup wins the click"
    );
    let below = ui.hit_test(5.0, 90.0).expect("the sibling is there");
    assert_eq!(
        below.on_click,
        Some(Message::Under),
        "the sibling sits right after the 20 pixel row, the popup took no space"
    );
    let popup_z = output.placements.iter().map(|p| p.z).max();
    assert!(popup_z.is_some(), "the popup is drawn");
    assert!(
        output.layer_areas.keys().any(|key| *key >= 1 << 62),
        "the popup gets a layer of its own above the rest"
    );
}

fn scrolled(offset: f32) -> Element<Message> {
    Element::column([Element::column((0..10).map(|_| block(40.0)))
        .size(Length::Fill, Length::Px(120.0))
        .scroll(offset)
        .on_scroll(Message::Scrolled)])
}

#[test]
fn scrolling_past_the_end_is_corrected_and_the_wheel_moves_by_lines() {
    let mut ui = ui();
    ui.render(&scrolled(9_000.0), SCREEN, CELL);
    let max = 400.0 - 120.0;
    assert_eq!(
        ui.take_corrections(),
        vec![Message::Scrolled(Scroll { offset: max, max })]
    );
    ui.render(&scrolled(100.0), SCREEN, CELL);
    assert!(ui.take_corrections().is_empty(), "an offset in range stays");
    let hit = ui
        .hit_test(50.0, 50.0)
        .expect("the list is under the pointer");
    assert_eq!(
        hit.on_wheel_down,
        Some(Message::Scrolled(Scroll { offset: 160.0, max }))
    );
    assert_eq!(
        hit.on_wheel_up,
        Some(Message::Scrolled(Scroll { offset: 40.0, max }))
    );
}

#[test]
fn dragging_the_scrollbar_scrolls_to_where_the_pointer_is() {
    let mut ui = ui();
    ui.render(&scrolled(0.0), SCREEN, CELL);
    let edge = 400.0 - 2.0;
    let top = ui.press(edge, 0.0).expect("the scrollbar takes the press");
    assert_eq!(
        top,
        Message::Scrolled(Scroll {
            offset: 0.0,
            max: 280.0
        })
    );
    let bottom = ui.drag(edge, 500.0).expect("the drag follows the pointer");
    assert_eq!(
        bottom,
        Message::Scrolled(Scroll {
            offset: 280.0,
            max: 280.0
        })
    );
    ui.release();
    assert_eq!(ui.drag(edge, 0.0), None, "a release ends the drag");
}

#[test]
fn a_press_reports_fractions_and_a_grip_handler_needs_its_grip() {
    let mut ui = ui();
    let slider = Element::column([Element::row([])
        .size(Length::Px(200.0), Length::Px(20.0))
        .on_press(Message::Pressed)]);
    ui.render(&slider, SCREEN, CELL);
    assert_eq!(ui.press(50.0, 10.0), Some(Message::Pressed(0.25, 0.5)));
    assert_eq!(
        ui.drag(-40.0, 10.0),
        Some(Message::Pressed(0.0, 0.5)),
        "a drag outside the box clamps to its edge"
    );
    ui.release();

    let split = Element::column([Element::row([
        block(20.0).width(Length::Px(100.0)),
        block(20.0).width(Length::Px(10.0)).grip(),
        block(20.0).width(Length::Px(100.0)),
    ])
    .size(Length::Px(210.0), Length::Px(20.0))
    .on_grip_press(Message::Pressed)]);
    ui.render(&split, SCREEN, CELL);
    assert_eq!(ui.press(50.0, 10.0), None, "a pane is not the grip");
    assert!(ui.press(105.0, 10.0).is_some(), "the grip starts the drag");
}

#[test]
fn moving_between_elements_changes_the_hover_target() {
    let mut ui = ui();
    let view = Element::column([
        block(40.0)
            .on_hover(Message::Hover)
            .on_leave(Message::Leave),
        block(40.0)
            .on_hover(Message::Hover)
            .on_leave(Message::Leave),
    ])
    .width(Length::Fill);
    ui.render(&view, SCREEN, CELL);
    let first = ui.hit_test(5.0, 10.0).expect("the first row");
    let second = ui.hit_test(5.0, 50.0).expect("the second row");
    assert_eq!(first.on_leave, Some(Message::Leave));
    assert_ne!(first.hover_target, second.hover_target);
}

#[test]
fn an_opaque_box_keeps_clicks_from_what_lies_under_it() {
    let mut ui = ui();
    let view = Element::stack([
        block(200.0).on_click(Message::Under),
        block(100.0).width(Length::Px(100.0)).opaque(),
    ])
    .width(Length::Fill);
    ui.render(&view, SCREEN, CELL);
    let hit = ui.hit_test(10.0, 10.0).expect("the opaque box");
    assert_eq!(hit.on_click, None);
    let beside = ui.hit_test(200.0, 10.0).expect("the box under");
    assert_eq!(beside.on_click, Some(Message::Under));
}

#[test]
fn an_opaque_card_inside_a_clickable_backdrop_keeps_its_clicks() {
    let mut ui = ui();
    let view = Element::column([block(50.0).width(Length::Px(100.0)).opaque()])
        .size(Length::Fill, Length::Fill)
        .on_click(Message::Under);
    ui.render(&view, SCREEN, CELL);
    assert_eq!(ui.hit_test(10.0, 10.0).and_then(|hit| hit.on_click), None);
    assert_eq!(
        ui.hit_test(300.0, 200.0).and_then(|hit| hit.on_click),
        Some(Message::Under)
    );
}

#[test]
fn a_fill_popup_spans_its_parent_only() {
    let mut ui = ui();
    let view = Element::column([Element::column([block(10.0)
        .width(Length::Fill)
        .on_click(Message::Popup)
        .popup(0.0, 0.0)])
    .size(Length::Px(100.0), Length::Px(0.0))]);
    ui.render(&view, SCREEN, CELL);
    assert!(
        ui.hit_test(50.0, 5.0).is_some(),
        "inside its parent's width"
    );
    assert!(ui.hit_test(150.0, 5.0).is_none(), "and not past it");
}
