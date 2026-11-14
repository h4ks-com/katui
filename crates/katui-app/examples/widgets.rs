//! The katui widget tour: a menu bar, a file tree beside a scrolling page
//! of controls, a table, a note, toasts and a dialog. Run with
//! `just demo-widgets`.
#![cfg(unix)] // the runner drives /dev/tty through katui-rt's unix layer

use std::time::{Duration, Instant};

use katui_app::{Control, UiApp, Waker, run};
use katui_draw::{Paint, Rgba};
use katui_proto::input::{InputEvent, Key};
use katui_ui::{Align, CellPixels, Element, Length};
use katui_widgets::{
    Column, Edit, Emphasis, Focus, ListState, Orientation, Scroll, ScrollState, TextArea, Theme,
    Toasts, TreeNode, TreeState, button, dialog, key_hints, label, menu, panel, radio, scroll_view,
    select, slider, spinner, split, switch, table, text_area, toasts, tooltip, tree,
};

const SIZES: [&str; 3] = ["Small", "Medium", "Large"];
const SCALES: [f32; 3] = [0.85, 1.0, 1.2];
const THEMES: [&str; 3] = ["Dusk", "Forest", "Paper"];
const FILE_MENU: [&str; 3] = ["Save the note", "Clear the note", "Quit"];
const TICK: Duration = Duration::from_millis(250);

fn main() -> Result<(), katui_rt::RtError> {
    run(Tour::new())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Field {
    Note,
    Table,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Popup {
    None,
    FileMenu,
    Themes,
    Quit,
}

#[derive(Debug, Clone)]
enum Message {
    Tick,
    ToggleFileMenu,
    File(usize),
    ToggleThemes,
    Theme(usize),
    Size(usize),
    Busy,
    Volume(f32),
    Row(usize),
    Scrolled(Scroll),
    Open(u64),
    Pick(u64),
    Resize(f32),
    ShowTip,
    HideTip,
    Dismiss,
    ConfirmQuit,
}

struct Tour {
    popup: Popup,
    theme: usize,
    size: usize,
    busy: bool,
    volume: f32,
    rows: Vec<Vec<String>>,
    table: ListState,
    note: TextArea,
    page: ScrollState,
    files: TreeState,
    sidebar: f32,
    tip: bool,
    toasts: Toasts,
    focus: Focus<Field>,
    started: Instant,
}

impl Tour {
    fn new() -> Self {
        let rows: Vec<Vec<String>> = [
            ("ripgrep", "search", "41k"),
            ("fzf", "finder", "62k"),
            ("bat", "pager", "47k"),
            ("fd", "finder", "30k"),
            ("delta", "diff", "21k"),
        ]
        .iter()
        .map(|(name, kind, stars)| {
            vec![(*name).to_owned(), (*kind).to_owned(), (*stars).to_owned()]
        })
        .collect();
        let mut files = TreeState::default();
        files.toggle(1);
        Self {
            popup: Popup::None,
            theme: 0,
            size: 1,
            busy: false,
            volume: 0.4,
            table: ListState::new(rows.len(), 4),
            rows,
            note: TextArea::new(4),
            page: ScrollState::default(),
            files,
            sidebar: 0.25,
            tip: false,
            toasts: Toasts::default(),
            focus: Focus::new(Field::Note, [Field::Table]),
            started: Instant::now(),
        }
    }

    fn toast(&mut self, text: impl Into<String>) {
        self.toasts.push(text, Instant::now());
    }

    fn menu_bar(&self, theme: &Theme) -> Element<Message> {
        Element::row([
            menu(
                "File",
                &FILE_MENU,
                self.popup == Popup::FileMenu,
                theme,
                Message::ToggleFileMenu,
                Message::File,
            ),
            Element::spacer(),
            key_hints(
                &[
                    ("Tab", "switch field"),
                    ("Esc", "close"),
                    ("Ctrl+C", "quit"),
                ],
                theme,
            ),
        ])
        .align(Align::Center)
        .padding_xy(theme.cell_width(), theme.cell_height() * 0.3)
        .width(Length::Fill)
        .background(theme.raised.clone())
    }

    fn controls(&self, theme: &Theme) -> Element<Message> {
        let busy: Element<Message> = if self.busy {
            Element::row([
                spinner(self.started.elapsed(), theme),
                label("Working", theme.body(), theme.muted),
            ])
            .gap(theme.cell_width())
            .align(Align::Center)
        } else {
            label(
                "Turn on the switch to start working",
                theme.body(),
                theme.muted,
            )
        };
        panel(
            [
                label("Settings", theme.body() * 1.2, theme.text).weight(800),
                label("Theme", theme.small(), theme.muted),
                select(
                    &THEMES,
                    self.theme,
                    self.popup == Popup::Themes,
                    theme,
                    Message::ToggleThemes,
                    Message::Theme,
                ),
                label("Size", theme.small(), theme.muted),
                radio(&SIZES, self.size, theme, Message::Size),
                switch("Work in the background", self.busy, Message::Busy, theme),
                busy,
                label(
                    format!("Volume {:.0}%", self.volume * 100.0),
                    theme.small(),
                    theme.muted,
                ),
                slider(self.volume, theme, Message::Volume),
                tooltip(
                    button("Save", Message::File(0), Emphasis::Primary, theme),
                    "Save the note to the disk",
                    self.tip,
                    Message::ShowTip,
                    Message::HideTip,
                    theme,
                ),
            ],
            theme,
        )
        .width(Length::Fill)
    }

    fn data(&self, theme: &Theme) -> Element<Message> {
        panel(
            [
                label("Tools", theme.body() * 1.2, theme.text).weight(800),
                table(
                    &[
                        Column::fixed("Name", 12.0),
                        Column::fixed("Kind", 10.0),
                        Column::fill("Stars"),
                    ],
                    &self.rows,
                    &self.table,
                    theme,
                    Message::Row,
                ),
                label("Note", theme.small(), theme.muted),
                text_area(
                    &self.note,
                    "Write a note",
                    self.focus.is(Field::Note),
                    theme,
                ),
            ],
            theme,
        )
        .width(Length::Fill)
    }

    fn files(theme: &Theme, state: &TreeState) -> Element<Message> {
        let roots = files();
        Element::column([tree(&roots, state, theme, Message::Open, Message::Pick)])
            .padding(theme.cell_height() * 0.5)
            .size(Length::Fill, Length::Fill)
            .background(theme.raised.clone())
    }
}

fn files() -> Vec<TreeNode> {
    vec![
        TreeNode::branch(
            1,
            "src",
            vec![
                TreeNode::leaf(2, "main.rs"),
                TreeNode::branch(
                    3,
                    "widgets",
                    vec![TreeNode::leaf(4, "tree.rs"), TreeNode::leaf(5, "table.rs")],
                ),
            ],
        ),
        TreeNode::leaf(6, "Cargo.toml"),
        TreeNode::leaf(7, "README.md"),
    ]
}

/// The dark theme turned green.
fn forest(cell: CellPixels) -> Theme {
    Theme {
        background: Paint::Solid(Rgba::solid(18, 28, 22)),
        raised: Paint::Solid(Rgba::solid(28, 42, 33)),
        highlight: Paint::Solid(Rgba::solid(42, 64, 50)),
        border: Paint::Solid(Rgba::solid(58, 84, 66)),
        accent: Rgba::solid(120, 214, 150),
        ..Theme::dark(cell)
    }
}

impl UiApp for Tour {
    type Message = Message;

    fn theme(&self, cell: CellPixels) -> Theme {
        let theme = match self.theme {
            1 => forest(cell),
            2 => Theme::light(cell),
            _ => Theme::dark(cell),
        };
        theme.scaled(SCALES.get(self.size).copied().unwrap_or(1.0))
    }

    fn start(&mut self, waker: Waker<Message>) {
        std::thread::spawn(move || {
            while waker.send(Message::Tick) {
                std::thread::sleep(TICK);
            }
        });
    }

    fn view(&self, theme: &Theme) -> Element<Message> {
        let page = scroll_view(
            &self.page,
            [self.controls(theme), self.data(theme)],
            theme,
            Message::Scrolled,
        )
        .padding(theme.cell_height());
        let body = split(
            Tour::files(theme, &self.files),
            page,
            self.sidebar,
            Orientation::Horizontal,
            theme,
            Message::Resize,
        );
        let mut root = vec![self.menu_bar(theme), body, toasts(&self.toasts, theme)];
        if self.popup == Popup::Quit {
            root.push(dialog(
                "Quit the tour?",
                label("The note is not saved anywhere.", theme.body(), theme.muted),
                [
                    button("Stay", Message::Dismiss, Emphasis::Plain, theme),
                    button("Quit", Message::ConfirmQuit, Emphasis::Primary, theme),
                ],
                Message::Dismiss,
                theme,
            ));
        }
        Element::column(root)
            .size(Length::Fill, Length::Fill)
            .background(theme.background.clone())
    }

    fn update(&mut self, message: Message) -> Control {
        match message {
            Message::Tick => {
                let expired = self.toasts.expire(Instant::now());
                return if self.busy || expired {
                    Control::Redraw
                } else {
                    Control::Wait
                };
            }
            Message::ToggleFileMenu => self.toggle(Popup::FileMenu),
            Message::ToggleThemes => self.toggle(Popup::Themes),
            Message::File(0) => {
                self.popup = Popup::None;
                let lines = self.note.text().lines().count();
                self.toast(format!("Saved {lines} lines"));
            }
            Message::File(1) => {
                self.popup = Popup::None;
                self.note.take();
                self.toast("Cleared the note");
            }
            Message::File(_) => self.popup = Popup::Quit,
            Message::Theme(index) => {
                self.theme = index;
                self.popup = Popup::None;
                self.toast(format!(
                    "Picked {}",
                    THEMES.get(index).copied().unwrap_or_default()
                ));
            }
            Message::Size(index) => self.size = index,
            Message::Busy => self.busy = !self.busy,
            Message::Volume(volume) => self.volume = volume,
            Message::Row(index) => {
                self.table.select(index);
                self.focus.set(Field::Table);
            }
            Message::Scrolled(scroll) => self.page.apply(scroll),
            Message::Open(id) => self.files.toggle(id),
            Message::Pick(id) => self.files.select(id),
            Message::Resize(fraction) => self.sidebar = fraction,
            Message::ShowTip => self.tip = true,
            Message::HideTip => self.tip = false,
            Message::Dismiss => self.popup = Popup::None,
            Message::ConfirmQuit => return Control::Quit,
        }
        Control::Redraw
    }

    fn input(&mut self, event: &InputEvent) -> Control {
        if let InputEvent::Key(Key::Escape, _) = event {
            self.popup = Popup::None;
            return Control::Redraw;
        }
        if self.focus.handle(event) {
            return Control::Redraw;
        }
        let handled = match self.focus.current() {
            Field::Note => self.note.handle(event) != Edit::Ignored,
            Field::Table => self.table.handle(event),
        };
        if handled || self.page.handle(event, 40.0) {
            Control::Redraw
        } else {
            Control::Wait
        }
    }
}

impl Tour {
    fn toggle(&mut self, popup: Popup) {
        self.popup = if self.popup == popup {
            Popup::None
        } else {
            popup
        };
    }
}

#[cfg(test)]
mod tests {
    use katui_app::UiApp as _;
    use katui_draw::{TextEngine, bundled_fonts};
    use katui_ui::{CellPixels, Ui, UiOptions};

    use super::{Message, Tour};

    const CELL: CellPixels = CellPixels {
        width: 8,
        height: 17,
    };

    fn render(tour: &Tour, name: &str) -> Vec<u8> {
        let engine = TextEngine::new(bundled_fonts()).expect("the bundled fonts parse");
        let mut ui: Ui<Message> = Ui::new(engine, UiOptions::default());
        let size = (8 * 120, 17 * 40);
        let theme = tour.theme(CELL);
        let output = ui.render(&tour.view(&theme), size, CELL);
        let surface = output.flatten(CELL, size).expect("whole pixels");
        let path = format!(
            "{}/../../target/katui-tour-{name}.png",
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
        surface.pixels().to_vec()
    }

    #[test]
    fn the_theme_and_size_settings_change_the_screen() {
        let mut tour = Tour::new();
        let dusk = render(&tour, "dusk");
        tour.update(Message::Theme(2));
        let paper = render(&tour, "paper");
        tour.update(Message::Size(2));
        let large = render(&tour, "paper-large");
        tour.update(Message::Theme(1));
        let forest = render(&tour, "forest-large");
        assert_ne!(dusk, paper, "the theme changes the colors");
        assert_ne!(paper, large, "the size changes the layout");
        assert_ne!(large, forest);
    }
}
