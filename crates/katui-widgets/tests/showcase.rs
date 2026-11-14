//! The newer widgets on two screens, each written to a PNG: the controls
//! with their popups open, and a split screen under a dialog. The tests
//! find each widget's messages on the rendered layout.
#![allow(
    clippy::expect_used,
    clippy::float_cmp,
    clippy::cast_precision_loss,
    clippy::trivially_copy_pass_by_ref
)]

use std::path::PathBuf;
use std::time::{Duration, Instant};

use katui_draw::TextEngine;
use katui_proto::input::{InputEvent, Key, Mods};
use katui_ui::{CellPixels, Element, Length, Ui, UiOptions};
use katui_widgets::{
    Column, Emphasis, ListState, Orientation, Scroll, ScrollState, TextArea, Theme, Toasts,
    TreeNode, TreeState, button, dialog, key_hints, label, menu, panel, radio, scroll_view, select,
    slider, spinner, split, switch, table, text_area, toasts, tooltip, tree,
};

const TEXT_FONT: &[u8] =
    include_bytes!("../../katui-draw/assets/AtkinsonHyperlegibleNext[wght].ttf");
const CELL: CellPixels = CellPixels {
    width: 9,
    height: 19,
};
const SIZE: (u32, u32) = (9 * 110, 19 * 36);

#[derive(Debug, Clone, PartialEq)]
enum Message {
    ToggleSelect,
    Choose(usize),
    ToggleMenu,
    Menu(usize),
    Radio(usize),
    Switch,
    Slide(f32),
    Row(usize),
    Show,
    Hide,
    Scrolled(Scroll),
    Open(u64),
    Pick(u64),
    Resize(f32),
    Dismiss,
    Confirm,
}

fn ui() -> Ui<Message> {
    let engine = TextEngine::new(vec![TEXT_FONT.to_vec()]).expect("the font parses");
    Ui::new(engine, UiOptions::default())
}

fn save(ui: &mut Ui<Message>, view: &Element<Message>, name: &str) {
    let output = ui.render(view, SIZE, CELL);
    let surface = output.flatten(CELL, SIZE).expect("whole pixels");
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("../../target/{name}.png"));
    image::save_buffer(
        &path,
        surface.pixels(),
        surface.width(),
        surface.height(),
        image::ExtendedColorType::Rgba8,
    )
    .expect("the target directory is writable");
}

/// Every point on a 4 pixel grid, top to bottom.
fn grid() -> impl Iterator<Item = (f32, f32)> {
    (0..SIZE.1 / 4)
        .flat_map(|row| (0..SIZE.0 / 4).map(move |column| (column as f32 * 4.0, row as f32 * 4.0)))
}

fn clicks(ui: &Ui<Message>) -> Vec<Message> {
    let mut found: Vec<Message> = Vec::new();
    for (x, y) in grid() {
        if let Some(message) = ui.hit_test(x, y).and_then(|hit| hit.on_click)
            && !found.contains(&message)
        {
            found.push(message);
        }
    }
    found
}

fn where_clicks(ui: &Ui<Message>, wanted: &Message) -> Option<(f32, f32)> {
    grid().find(|(x, y)| ui.hit_test(*x, *y).and_then(|hit| hit.on_click).as_ref() == Some(wanted))
}

const FRUITS: [&str; 4] = ["apple", "banana", "cherry", "damson"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Open {
    Nothing,
    Select,
    Menu,
}

fn controls(
    open: Open,
    theme: &Theme,
    area: &TextArea,
    scroll: &ScrollState,
    toasts_shown: &Toasts,
) -> Element<Message> {
    let rows = vec![
        vec!["ripgrep", "search", "41k"],
        vec!["fzf", "finder", "62k"],
        vec!["bat", "pager", "47k"],
    ];
    let mut picked = ListState::new(rows.len(), 3);
    picked.select(1);
    let lines = (1..=20).map(|line| label(format!("Log line {line}"), theme.body(), theme.text));
    let left = panel(
        [
            Element::row([
                menu(
                    "File",
                    &["New", "Open", "Quit"],
                    open == Open::Menu,
                    theme,
                    Message::ToggleMenu,
                    Message::Menu,
                ),
                menu(
                    "Edit",
                    &["Undo"],
                    false,
                    theme,
                    Message::ToggleMenu,
                    Message::Menu,
                ),
            ]),
            select(
                &FRUITS,
                2,
                open == Open::Select,
                theme,
                Message::ToggleSelect,
                Message::Choose,
            ),
            radio(&["Small", "Large"], 1, theme, Message::Radio),
            switch("Dark mode", true, Message::Switch, theme),
            slider(0.3, theme, Message::Slide),
            tooltip(
                button("Hover me", Message::Show, Emphasis::Plain, theme),
                "Shows a hint",
                true,
                Message::Show,
                Message::Hide,
                theme,
            ),
            spinner(Duration::from_millis(300), theme),
        ],
        theme,
    )
    .width(theme.columns(40.0));
    let right = panel(
        [
            table(
                &[
                    Column::fixed("Name", 12.0),
                    Column::fixed("Kind", 10.0),
                    Column::fill("Stars"),
                ],
                &rows,
                &picked,
                theme,
                Message::Row,
            ),
            text_area(area, "Write a note", true, theme),
            Element::column([scroll_view(scroll, lines, theme, Message::Scrolled)])
                .size(Length::Fill, theme.rows(6.0)),
            key_hints(&[("Ctrl+S", "save"), ("Esc", "close")], theme),
        ],
        theme,
    )
    .width(theme.columns(56.0));
    Element::column([
        Element::row([left, right])
            .gap(theme.cell_width() * 2.0)
            .padding(theme.cell_height()),
        toasts(toasts_shown, theme),
    ])
    .size(Length::Fill, Length::Fill)
    .background(theme.background.clone())
}

#[test]
fn the_controls_render_with_their_popups_open_and_answer_clicks() {
    let theme = Theme::dark(CELL);
    let mut area = TextArea::new(3);
    for key in "first line"
        .chars()
        .map(Key::Char)
        .chain([Key::Enter])
        .chain("second".chars().map(Key::Char))
    {
        area.handle(&InputEvent::Key(key, Mods::NONE));
    }
    let scroll = ScrollState::default();
    let mut shown = Toasts::default();
    shown.push("Saved the note", Instant::now());
    let mut ui = ui();
    let screen = |open| controls(open, &theme, &area, &scroll, &shown);

    save(&mut ui, &screen(Open::Nothing), "katui-widgets-controls");
    let found = clicks(&ui);
    for wanted in [
        Message::ToggleSelect,
        Message::ToggleMenu,
        Message::Radio(0),
        Message::Switch,
        Message::Row(2),
    ] {
        assert!(
            found.contains(&wanted),
            "{wanted:?} answers a click, found {found:?}"
        );
    }

    save(&mut ui, &screen(Open::Select), "katui-widgets-select");
    let found = clicks(&ui);
    assert!(found.contains(&Message::Choose(0)) && found.contains(&Message::Choose(3)));
    assert!(
        found.contains(&Message::ToggleSelect) && !found.contains(&Message::Radio(0)),
        "a click anywhere outside the open list closes it, found {found:?}"
    );

    save(&mut ui, &screen(Open::Menu), "katui-widgets-menu");
    let found = clicks(&ui);
    assert!(found.contains(&Message::Menu(0)) && found.contains(&Message::Menu(2)));

    ui.render(&screen(Open::Nothing), SIZE, CELL);
    let slid = grid().find_map(|(x, y)| {
        let pressed = ui.press(x, y);
        ui.release();
        match pressed {
            Some(Message::Slide(value)) => Some(value),
            _ => None,
        }
    });
    assert!(slid.is_some(), "the slider takes presses");
}

fn files() -> Vec<TreeNode> {
    vec![
        TreeNode::branch(
            1,
            "src",
            vec![TreeNode::leaf(2, "main.rs"), TreeNode::leaf(3, "lib.rs")],
        ),
        TreeNode::leaf(4, "Cargo.toml"),
    ]
}

fn workspace(theme: &Theme, tree_state: &TreeState, open_dialog: bool) -> Element<Message> {
    let sidebar = Element::column([tree(
        &files(),
        tree_state,
        theme,
        Message::Open,
        Message::Pick,
    )])
    .padding(theme.cell_height())
    .background(theme.raised.clone());
    let editor = Element::column([label("fn main() {}", theme.body(), theme.text)])
        .padding(theme.cell_height());
    let mut root = vec![split(
        sidebar,
        editor,
        0.3,
        Orientation::Horizontal,
        theme,
        Message::Resize,
    )];
    if open_dialog {
        root.push(dialog(
            "Delete lib.rs?",
            label("Delete the file from the disk.", theme.body(), theme.muted),
            [
                button("Cancel", Message::Dismiss, Emphasis::Plain, theme),
                button("Delete", Message::Confirm, Emphasis::Primary, theme),
            ],
            Message::Dismiss,
            theme,
        ));
    }
    Element::column(root)
        .size(Length::Fill, Length::Fill)
        .background(theme.background.clone())
}

#[test]
fn a_split_tree_and_dialog_render_and_answer_presses() {
    let theme = Theme::dark(CELL);
    let mut tree_state = TreeState::default();
    tree_state.toggle(1);
    tree_state.select(3);
    let mut ui = ui();

    save(
        &mut ui,
        &workspace(&theme, &tree_state, false),
        "katui-widgets-split",
    );
    let found = clicks(&ui);
    for wanted in [Message::Open(1), Message::Pick(2), Message::Pick(4)] {
        assert!(
            found.contains(&wanted),
            "{wanted:?} answers a click, found {found:?}"
        );
    }
    let divider = 0.3 * SIZE.0 as f32;
    let pressed = (-6..=6)
        .find_map(|step| ui.press(divider + step as f32, 300.0))
        .expect("the divider starts a drag");
    assert!(matches!(pressed, Message::Resize(_)));
    assert_eq!(
        ui.drag(SIZE.0 as f32 * 0.5, 300.0),
        Some(Message::Resize(0.5))
    );
    ui.release();
    assert_eq!(
        ui.press(SIZE.0 as f32 * 0.7, 300.0),
        None,
        "a pane is not the divider"
    );

    save(
        &mut ui,
        &workspace(&theme, &tree_state, true),
        "katui-widgets-dialog",
    );
    let confirm = where_clicks(&ui, &Message::Confirm).expect("the dialog's button");
    assert!(confirm.1 > 0.0);
    assert_eq!(
        ui.hit_test(5.0, 5.0).and_then(|hit| hit.on_click),
        Some(Message::Dismiss),
        "a click outside the dialog dismisses it"
    );
    assert!(
        !clicks(&ui).contains(&Message::Open(1)),
        "the dialog covers the tree"
    );
}
