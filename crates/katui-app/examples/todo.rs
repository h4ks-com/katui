//! The katui to-do demo: a task list on the widgets crate. Run with
//! `just demo-todo`.
#![cfg(unix)] // the runner drives /dev/tty through katui-rt's unix layer
#![allow(clippy::cast_precision_loss)]

use katui_app::{Control, UiApp, run};
use katui_proto::input::{InputEvent, Key};
use katui_ui::{Align, Element};
use katui_widgets::{
    Edit, ListState, TextField, Theme, checkbox, hint, list, page, panel, progress, tabs,
    text_field, title,
};

const VISIBLE_TASKS: usize = 10;
const FILTERS: [&str; 3] = ["All", "Active", "Done"];

fn main() -> Result<(), katui_rt::RtError> {
    run(Todo::default())
}

#[derive(Default)]
struct Todo {
    tasks: Vec<(String, bool)>,
    draft: TextField,
    filter: usize,
    selection: ListState,
}

#[derive(Debug, Clone)]
enum Message {
    Toggle(usize),
    Filter(usize),
}

impl Todo {
    /// The indices of the tasks the filter shows.
    fn shown(&self) -> Vec<usize> {
        (0..self.tasks.len())
            .filter(|index| match (self.filter, self.tasks.get(*index)) {
                (1, Some((_, done))) => !done,
                (2, Some((_, done))) => *done,
                _ => true,
            })
            .collect()
    }

    fn resize_selection(&mut self) {
        let shown = self.shown().len();
        self.selection.resize(shown, VISIBLE_TASKS);
    }

    fn toggle_selected(&mut self) {
        if let Some(index) = self.shown().get(self.selection.selected()).copied() {
            self.update(Message::Toggle(index));
        }
    }

    fn remove_selected(&mut self) {
        if let Some(index) = self.shown().get(self.selection.selected()).copied() {
            self.tasks.remove(index);
            self.resize_selection();
        }
    }
}

impl UiApp for Todo {
    type Message = Message;

    fn view(&self) -> Element<Message> {
        let theme = Theme::current();
        let shown = self.shown();
        let done = self.tasks.iter().filter(|(_, done)| *done).count();
        let rows = list(&self.selection, |row, _| {
            let index = shown.get(row).copied().unwrap_or_default();
            let (text, done) = self.tasks.get(index).cloned().unwrap_or_default();
            checkbox(&text, done, Message::Toggle(index))
        });
        let body = if shown.is_empty() {
            hint("Type a task and press Enter to add it")
        } else {
            rows
        };
        let fraction = if self.tasks.is_empty() {
            0.0
        } else {
            done as f32 / self.tasks.len() as f32
        };
        let card = panel([
            title("Tasks"),
            tabs(&FILTERS, self.filter, Message::Filter),
            text_field(&self.draft, "Add a task", true),
            body,
            progress(fraction),
            hint("Press Up and Down to pick a task, Tab to toggle it, Delete to remove it"),
        ])
        .width(theme.columns(60.0));
        page([card]).align(Align::Center)
    }

    fn update(&mut self, message: Message) -> Control {
        match message {
            Message::Toggle(index) => {
                if let Some((_, done)) = self.tasks.get_mut(index) {
                    *done = !*done;
                }
            }
            Message::Filter(filter) => self.filter = filter,
        }
        self.resize_selection();
        Control::Redraw
    }

    fn input(&mut self, event: &InputEvent) -> Control {
        if self.selection.handle(event) {
            return Control::Redraw;
        }
        match event {
            InputEvent::Key(Key::Tab, _) => self.toggle_selected(),
            InputEvent::Key(Key::Delete, _) if self.draft.text().is_empty() => {
                self.remove_selected();
            }
            _ => match self.draft.handle(event) {
                Edit::Submitted if !self.draft.text().trim().is_empty() => {
                    let task = self.draft.take();
                    self.tasks.push((task.trim().to_owned(), false));
                    self.resize_selection();
                }
                Edit::Ignored | Edit::Submitted => return Control::Wait,
                Edit::Changed | Edit::Moved => {}
            },
        }
        Control::Redraw
    }
}

#[cfg(test)]
mod common;

#[cfg(test)]
mod tests {
    use katui_proto::input::{InputEvent, Key, Mods};
    use katui_ui::CellPixels;
    use katui_widgets::Theme;

    use super::common::render_to_png;
    use super::{Message, Todo};
    use katui_app::UiApp as _;

    const CELL: CellPixels = CellPixels {
        width: 9,
        height: 19,
    };

    fn press(todo: &mut Todo, key: Key) {
        todo.input(&InputEvent::Key(key, Mods::NONE));
    }

    fn add(todo: &mut Todo, task: &str) {
        for letter in task.chars() {
            press(todo, Key::Char(letter));
        }
        press(todo, Key::Enter);
    }

    #[test]
    fn tasks_are_added_toggled_filtered_and_removed() {
        let mut todo = Todo::default();
        add(&mut todo, "water the plants");
        add(&mut todo, "   ");
        add(&mut todo, "call the bakery");
        add(&mut todo, "fix the bike");
        assert_eq!(todo.tasks.len(), 3, "a blank task is ignored");
        press(&mut todo, Key::Down);
        press(&mut todo, Key::Tab);
        assert!(todo.tasks[1].1, "tab toggles the selected task");
        todo.update(Message::Filter(1));
        assert_eq!(todo.shown(), vec![0, 2], "active hides done tasks");

        let view = Theme::dark(CELL).scope(|_| todo.view());
        render_to_png(&view, (9 * 90, 19 * 22), CELL, "todo");

        todo.update(Message::Filter(0));
        press(&mut todo, Key::Delete);
        assert_eq!(todo.tasks.len(), 2, "delete removes the selected task");
    }
}
