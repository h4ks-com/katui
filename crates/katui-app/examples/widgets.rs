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
    Column, Edit, Focus, ListState, Orientation, Scroll, ScrollState, TextArea, Theme, Toasts,
    TreeNode, TreeState, button, dialog, heading, hint, key_hints, label, menu, panel,
    primary_button, radio, scroll_view, select, slider, spinner, split, switch, table, text_area,
    toasts, tooltip, tree,
};

const SIZES: [&str; 3] = ["Small", "Medium", "Large"];
const SCALES: [f32; 3] = [0.85, 1.0, 1.2];
const THEMES: [&str; 3] = ["Dusk", "Forest", "Paper"];
const FILE_MENU: [&str; 3] = ["Count the note's lines", "Clear the note", "Quit"];
const TICK: Duration = Duration::from_millis(250);

fn main() -> Result<(), katui_rt::RtError> {
    run(Tour::new())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Focused {
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
    focus: Focus<Focused>,
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
            focus: Focus::new(Focused::Note, [Focused::Table]),
            started: Instant::now(),
        }
    }

    fn toast(&mut self, text: impl Into<String>) {
        self.toasts.push(text, Instant::now());
    }

    fn menu_bar(&self) -> Element<Message> {
        let theme = Theme::current();
        Element::row([
            menu(
                "File",
                &FILE_MENU,
                self.popup == Popup::FileMenu,
                Message::ToggleFileMenu,
                Message::File,
            ),
            Element::spacer(),
            key_hints(&[
                ("Tab", "switch field"),
                ("Esc", "close the popup"),
                ("Ctrl+C", "quit"),
            ]),
        ])
        .align(Align::Center)
        .padding_xy(theme.cell_width(), theme.cell_height() * 0.3)
        .width(Length::Fill)
        .background(theme.raised.clone())
    }

    fn controls(&self) -> Element<Message> {
        let theme = Theme::current();
        let busy: Element<Message> = if self.busy {
            Element::row([
                spinner(self.started.elapsed()),
                label("Turn off the switch to stop working").color(theme.muted),
            ])
            .gap(theme.cell_width())
            .align(Align::Center)
        } else {
            label("Turn on the switch to start working").color(theme.muted)
        };
        panel([
            heading("Settings"),
            hint("Theme"),
            select(
                &THEMES,
                self.theme,
                self.popup == Popup::Themes,
                Message::ToggleThemes,
                Message::Theme,
            ),
            hint("Size"),
            radio(&SIZES, self.size, Message::Size),
            switch("Work in the background", self.busy, Message::Busy),
            busy,
            hint(format!("Volume {:.0}%", self.volume * 100.0)),
            slider(self.volume, Message::Volume),
            tooltip(
                primary_button("Count lines", Message::File(0)),
                "Count the lines in the note",
                self.tip,
                Message::ShowTip,
                Message::HideTip,
            ),
        ])
        .width(Length::Fill)
    }

    fn data(&self) -> Element<Message> {
        panel([
            heading("Tools"),
            table(
                &[
                    Column::fixed("Name", 12.0),
                    Column::fixed("Kind", 10.0),
                    Column::fill("Stars"),
                ],
                &self.rows,
                &self.table,
                Message::Row,
            ),
            hint("Note"),
            text_area(&self.note, "Write a note", self.focus.is(Focused::Note)),
        ])
        .width(Length::Fill)
    }

    fn file_tree(state: &TreeState) -> Element<Message> {
        let theme = Theme::current();
        let roots = sample_files();
        Element::column([tree(&roots, state, Message::Open, Message::Pick)])
            .padding(theme.cell_height() * 0.5)
            .size(Length::Fill, Length::Fill)
            .background(theme.raised.clone())
    }

    /// Apply a message that only changes how the screen is arranged.
    fn arrange(&mut self, message: &Message) {
        match message {
            Message::Scrolled(scroll) => self.page.apply(*scroll),
            Message::Open(id) => self.files.toggle(*id),
            Message::Pick(id) => self.files.select(*id),
            Message::Resize(fraction) => self.sidebar = *fraction,
            Message::ShowTip => self.tip = true,
            Message::HideTip => self.tip = false,
            _ => {}
        }
    }

    /// Redraw while working or when a toast expired.
    fn tick(&mut self) -> Control {
        let expired = self.toasts.expire(Instant::now());
        if self.busy || expired {
            Control::Redraw
        } else {
            Control::Wait
        }
    }

    /// Run entry `index` of the File menu.
    fn file(&mut self, index: usize) {
        self.popup = Popup::None;
        match index {
            0 => {
                let lines = self.note.text().lines().count();
                self.toast(format!("The note has {lines} lines"));
            }
            1 => {
                self.note.take();
            }
            _ => self.popup = Popup::Quit,
        }
    }

    fn toggle(&mut self, popup: Popup) {
        self.popup = if self.popup == popup {
            Popup::None
        } else {
            popup
        };
    }
}

fn sample_files() -> Vec<TreeNode> {
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
        waker.every(TICK, || Message::Tick);
    }

    fn view(&self) -> Element<Message> {
        let theme = Theme::current();
        let page = scroll_view(
            &self.page,
            [self.controls(), self.data()],
            Message::Scrolled,
        )
        .padding(theme.cell_height());
        let body = split(
            Tour::file_tree(&self.files),
            page,
            self.sidebar,
            Orientation::Horizontal,
            Message::Resize,
        );
        let mut root = vec![self.menu_bar(), body, toasts(&self.toasts)];
        if self.popup == Popup::Quit {
            root.push(dialog(
                "Quit the tour?",
                label("Quit now to discard the note.").color(theme.muted),
                [
                    button("Stay", Message::Dismiss),
                    primary_button("Quit", Message::ConfirmQuit),
                ],
                Message::Dismiss,
            ));
        }
        Element::column(root)
            .size(Length::Fill, Length::Fill)
            .background(theme.background.clone())
    }

    fn update(&mut self, message: Message) -> Control {
        match message {
            Message::Tick => return self.tick(),
            Message::ToggleFileMenu => self.toggle(Popup::FileMenu),
            Message::ToggleThemes => self.toggle(Popup::Themes),
            Message::File(index) => self.file(index),
            Message::Theme(index) => {
                self.theme = index;
                self.popup = Popup::None;
            }
            Message::Size(index) => self.size = index,
            Message::Busy => self.busy = !self.busy,
            Message::Volume(volume) => self.volume = volume,
            Message::Row(index) => {
                self.table.select(index);
                self.focus.set(Focused::Table);
            }
            Message::Scrolled(_)
            | Message::Open(_)
            | Message::Pick(_)
            | Message::Resize(_)
            | Message::ShowTip
            | Message::HideTip => self.arrange(&message),
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
            Focused::Note => self.note.handle(event) != Edit::Ignored,
            Focused::Table => self.table.handle(event),
        };
        if handled || self.page.handle(event, 40.0) {
            Control::Redraw
        } else {
            Control::Wait
        }
    }
}

#[cfg(test)]
mod common;

#[cfg(test)]
mod tests {
    use katui_app::UiApp as _;
    use katui_ui::CellPixels;

    use super::common::render_to_png;
    use super::{Message, Tour};

    const CELL: CellPixels = CellPixels {
        width: 8,
        height: 17,
    };

    fn render(tour: &Tour, name: &str) -> Vec<u8> {
        let view = tour.theme(CELL).scope(|_| tour.view());
        render_to_png(
            &view,
            (8 * 120, 17 * 40),
            CELL,
            &format!("katui-tour-{name}"),
        )
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
