//! Every widget on one screen, rendered through katui-ui and written to a
//! PNG, plus the message each widget sends when clicked.

mod common;

use common::{CELL, render_to_png, ui};
use katui_proto::input::{InputEvent, Key, Mods};
use katui_ui::{Element, Length, Ui};
use katui_widgets::{
    Focus, ListState, TextField, Theme, button, checkbox, focusable_button, hint, label, list,
    panel, primary_button, progress, tabs, text_field, title,
};

const SIZE: (u32, u32) = (9 * 80, 19 * 30);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Control {
    Name,
    Fruit,
}

#[derive(Debug, Clone, PartialEq)]
enum Message {
    Save,
    Cancel,
    Toggle,
    Tab(usize),
    Pick(usize),
}

const FRUITS: [&str; 8] = [
    "apple",
    "banana",
    "cherry",
    "damson",
    "elderberry",
    "fig",
    "grape",
    "honeydew",
];

fn screen(
    name: &TextField,
    fruits: &ListState,
    focus: &Focus<Control>,
    theme: &Theme,
) -> Element<Message> {
    let fields = panel([
        title("Order"),
        tabs(&["Basket", "Delivery", "Review"], 0, Message::Tab),
        hint("Name"),
        text_field(name, "Type your name", focus.is(Control::Name)),
        checkbox("Gift wrap", true, Message::Toggle),
        progress(0.6),
        Element::row([
            button("Cancel", Message::Cancel),
            primary_button("Save", Message::Save),
        ])
        .gap(theme.cell_width()),
    ])
    .width(theme.columns(44.0));
    let picker = panel([
        hint("Fruit"),
        list(fruits, |index, selected| {
            let color = if selected { theme.text } else { theme.muted };
            Element::row([label(FRUITS.get(index).copied().unwrap_or_default()).color(color)])
                .on_click(Message::Pick(index))
        }),
    ])
    .width(theme.columns(28.0));
    Element::row([fields, picker])
        .gap(theme.cell_width() * 2.0)
        .padding(theme.cell_height())
        .size(Length::Fill, Length::Fill)
        .background(theme.background.clone())
}

#[test]
fn every_widget_renders_and_answers_its_clicks() {
    let theme = Theme::dark(CELL);
    let mut name = TextField::with_text("Ada Lovelace");
    name.handle(&InputEvent::Key(Key::Left, Mods::NONE));
    let mut fruits = ListState::new(FRUITS.len(), 5);
    fruits.select(6);
    let focus = Focus::new(Control::Name, [Control::Fruit]);

    let mut ui: Ui<Message> = ui();
    let view = theme
        .clone()
        .scope(|theme| screen(&name, &fruits, &focus, theme));
    render_to_png(&mut ui, &view, SIZE, "katui-widgets");

    let click = |x: f32, y: f32| ui.hit_test(x, y).and_then(|hit| hit.on_click.clone());
    assert_eq!(click(168.0, 276.0), Some(Message::Save));
    assert_eq!(click(81.0, 276.0), Some(Message::Cancel));
    assert_eq!(click(157.0, 103.0), Some(Message::Tab(1)));
    assert_eq!(click(70.0, 217.0), Some(Message::Toggle));
    assert_eq!(click(470.0, 163.0), Some(Message::Pick(5)));
    assert_eq!(click(600.0, 500.0), None);
}

#[test]
fn a_focused_button_wears_the_accent_ring() {
    let theme = Theme::dark(CELL);
    let accent = theme.accent;
    let rings = |focused: bool| {
        let view = theme.clone().scope(|theme| {
            Element::row([focusable_button("Leave", Message::Save, focused)])
                .padding(theme.cell_height())
                .background(theme.background.clone())
        });
        let mut ui: Ui<Message> = ui();
        let output = ui.render(&view, (160, 80), CELL);
        let surface = output.flatten(CELL, (160, 80)).expect("whole pixels");
        surface
            .pixels()
            .as_chunks::<4>()
            .0
            .iter()
            .any(|pixel| pixel[..3] == [accent.r, accent.g, accent.b])
    };
    assert!(rings(true), "focus shows");
    assert!(!rings(false), "no ring without focus");
}
