//! Every widget on one screen, rendered through katui-ui and written to a
//! PNG, plus the clicks each one answers.

use std::path::PathBuf;

use katui_draw::TextEngine;
use katui_proto::input::{InputEvent, Key, Mods};
use katui_ui::{CellPixels, Element, Length, Ui, UiOptions};
use katui_widgets::{
    Emphasis, Focus, ListState, TextField, Theme, button, checkbox, label, list, panel, progress,
    tabs, text_field,
};

const TEXT_FONT: &[u8] =
    include_bytes!("../../katui-draw/assets/AtkinsonHyperlegibleNext[wght].ttf");
const CELL: CellPixels = CellPixels {
    width: 9,
    height: 19,
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
    let fields = panel(
        [
            label("Order", theme.title(), theme.text).weight(800),
            tabs(&["Basket", "Delivery", "Review"], 0, theme, Message::Tab),
            label("Name", theme.small(), theme.muted),
            text_field(name, "Type your name", focus.is(Control::Name), theme),
            checkbox("Gift wrap", true, Message::Toggle, theme),
            progress(0.6, theme),
            Element::row([
                button("Cancel", Message::Cancel, Emphasis::Plain, theme),
                button("Save", Message::Save, Emphasis::Primary, theme),
            ])
            .gap(theme.cell_width()),
        ],
        theme,
    )
    .width(theme.columns(44.0));
    let picker = panel(
        [
            label("Fruit", theme.small(), theme.muted),
            list(fruits, theme, |index, selected| {
                let color = if selected { theme.text } else { theme.muted };
                Element::row([label(
                    FRUITS.get(index).copied().unwrap_or_default(),
                    theme.body(),
                    color,
                )])
                .on_click(Message::Pick(index))
            }),
        ],
        theme,
    )
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
    let focus = Focus::new([Control::Name, Control::Fruit]).expect("two controls");

    let engine = TextEngine::new(vec![TEXT_FONT.to_vec()]).expect("the font parses");
    let mut ui: Ui<Message> = Ui::new(engine, UiOptions::default());
    let output = ui.render(&screen(&name, &fruits, &focus, &theme), SIZE, CELL);
    let surface = output.flatten(CELL, SIZE).expect("whole pixels");
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/katui-widgets.png");
    image::save_buffer(
        &path,
        surface.pixels(),
        surface.width(),
        surface.height(),
        image::ExtendedColorType::Rgba8,
    )
    .expect("the target directory is writable");

    let click = |x: f32, y: f32| ui.hit_test(x, y).and_then(|hit| hit.on_click.clone());
    assert_eq!(click(168.0, 276.0), Some(Message::Save));
    assert_eq!(click(81.0, 276.0), Some(Message::Cancel));
    assert_eq!(click(157.0, 103.0), Some(Message::Tab(1)));
    assert_eq!(click(70.0, 217.0), Some(Message::Toggle));
    assert_eq!(click(470.0, 163.0), Some(Message::Pick(5)));
    assert_eq!(click(600.0, 500.0), None);
}
