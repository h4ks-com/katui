# Pixel UI apps

This guide builds a shopping list: you type an item, press
<kbd>Enter</kbd> to add it, move through the list with the arrow keys and
tick items off. On the way you meet everything a katui app is made of.

It assumes you followed [Getting started](getting-started.md) and have a
project with the `app` feature.

## The whole app

Here is the finished program. The sections below take it apart.

```rust,no_run
use katui::app::{Control, UiApp, run};
use katui::proto::input::{InputEvent, Key};
use katui::ui::Element;
use katui::widgets::{Edit, ListState, TextField, Theme, checkbox, label, list, panel, text_field};

const VISIBLE_ROWS: usize = 10;

#[derive(Default)]
struct Shopping {
    items: Vec<(String, bool)>,
    draft: TextField,
    selection: ListState,
}

#[derive(Debug, Clone)]
enum Message {
    Toggle(usize),
}

impl UiApp for Shopping {
    type Message = Message;

    fn view(&self, theme: &Theme) -> Element<Message> {
        let rows = list(&self.selection, theme, |index, _selected| {
            let (name, bought) = self.items.get(index).cloned().unwrap_or_default();
            checkbox(&name, bought, Message::Toggle(index), theme)
        });
        panel(
            [
                label("Shopping", theme.title(), theme.text).weight(800),
                text_field(&self.draft, "Add an item", true, theme),
                rows,
            ],
            theme,
        )
        .width(theme.columns(40.0))
    }

    fn update(&mut self, message: Message) -> Control {
        let Message::Toggle(index) = message;
        if let Some((_, bought)) = self.items.get_mut(index) {
            *bought = !*bought;
        }
        Control::Redraw
    }

    fn input(&mut self, event: &InputEvent) -> Control {
        if self.selection.handle(event) {
            return Control::Redraw;
        }
        if let InputEvent::Key(Key::Tab, _) = event {
            return self.update(Message::Toggle(self.selection.selected()));
        }
        match self.draft.handle(event) {
            Edit::Submitted => {
                let item = self.draft.take();
                if !item.trim().is_empty() {
                    self.items.push((item.trim().to_owned(), false));
                    self.selection.resize(self.items.len(), VISIBLE_ROWS);
                }
                Control::Redraw
            }
            Edit::Changed | Edit::Moved => Control::Redraw,
            Edit::Ignored => Control::Wait,
        }
    }
}

fn main() -> Result<(), katui::rt::RtError> {
    run(Shopping::default())
}
```

## State and messages

Everything the app knows lives in one struct, here `Shopping`. Nothing else
holds state: the screen is always a picture of this struct.

A message is something that happened, named in your own words.
`Message::Toggle(3)` means "the user ticked item 3". You decide what
messages exist by writing the enum.

Messages come from two places:

- Elements send them when you click. `checkbox(..., Message::Toggle(index), ...)`
  sends that message when the checkbox is clicked. Any element can do this
  with `.on_click(message)`, and `.on_hover(message)` sends one when the
  pointer moves over it.
- Other threads send them through a `Waker`. See
  [Work in the background](#work-in-the-background).

`update` receives each message, changes the state, and returns what to do
next:

| `Control` | What happens |
|---|---|
| `Redraw` | katui calls `view` once and draws the changes |
| `Wait` | Nothing is drawn; katui sleeps until the next event |
| `Animate` | katui keeps drawing at 30 frames a second |
| `Quit` | The app ends and the terminal is restored |

## Keyboard input

Keys and pastes go to `input`, not to `update`, because most of them mean
something only to the control that has the focus. The shopping list hands
each event to its controls in turn and stops at the first one that wants
it:

1. `ListState::handle` takes <kbd>Up</kbd>, <kbd>Down</kbd>,
   <kbd>Page Up</kbd>, <kbd>Page Down</kbd>, <kbd>Home</kbd> and
   <kbd>End</kbd> and moves the selection.
2. <kbd>Tab</kbd> ticks the selected item, by sending itself the same
   message a click would.
3. `TextField::handle` takes typing, <kbd>Backspace</kbd>, the cursor keys
   and paste. It tells you what happened with an `Edit`, and
   `Edit::Submitted` means <kbd>Enter</kbd>.

<kbd>Ctrl</kbd>+<kbd>C</kbd> always quits, before `input` is called.

When a screen has several controls that take keys, `Focus` keeps track of
which one has the focus and moves it with <kbd>Tab</kbd> and
<kbd>Shift</kbd>+<kbd>Tab</kbd>.

## Widgets

The widgets are plain functions that return elements. They all take the
`theme`, which holds the colors and sizes so your app looks consistent.
Widgets that keep state between frames have a state type you store in your
app struct and pass to the widget in `view`.

| Widget | State | What it shows |
|---|---|---|
| `label`, `button`, `panel` | | Text, a button that sends a message, a raised card |
| `text_field` | `TextField` | A one-line text input |
| `text_area` | `TextArea` | A multi-line text input that scrolls with its cursor |
| `list` | `ListState` | The rows of a list, built by a function you pass |
| `table` | `ListState` | Rows cut into `Column`s under a header |
| `tree` | `TreeState` | Nodes that open to show their children |
| `scroll_view` | `ScrollState` | Content that scrolls with the wheel, the keys and a scrollbar |
| `checkbox`, `switch`, `radio` | | On and off, and one choice of several |
| `slider` | | A value from 0.0 to 1.0 you press or drag to |
| `tabs`, `progress`, `spinner` | | Tabs, a progress bar, three dots while you work |
| `select`, `menu` | | A list that opens below a field or a menu title |
| `tooltip` | | A hint under an element while the pointer is on it |
| `dialog` | | A box over a dimmed screen, for a question |
| `toasts` | `Toasts` | Short messages in a corner that go away on their own |
| `key_hints` | | Keys drawn as key caps with what they do |
| `split` | | Two panes and a divider you drag |

The widget tour shows all of them working together. Run it with
`just demo-widgets` and read `crates/katui-app/examples/widgets.rs`.

## Scrolling

A `scroll_view` keeps its position in a `ScrollState`. It sends a `Scroll`
message whenever the wheel or the scrollbar moves it; store it with
`apply`:

```rust,ignore
Message::Scrolled(scroll) => self.page.apply(scroll),
```

`ScrollState::handle` moves it with the arrow, page, home and end keys. You
never have to know how tall the content is: when an offset runs past the
end, katui sends the clamped position back on the next frame.

## Popups and dialogs

`select`, `menu`, `tooltip`, `dialog` and `toasts` draw above everything
else, so they never push the rest of the layout around. Your state decides
whether one is open: `select` and `menu` take an `open` flag and send their
toggle message when the user clicks them or anywhere outside them. Keep one
of them open at a time.

Put a `dialog` and `toasts` last in your root element, and the dialog only
while it is open. A click on the dimmed screen sends the dialog's dismiss
message, and clicks inside it reach only its own buttons.

To make your own popup, call `.popup(x, y)` on any element: it leaves the
layout and draws on top, `x` and `y` pixels from its parent's top left.

## Dragging

`slider` and `split` follow the pointer while you hold the button. Your own
elements can do the same with `.on_press(|x, y| ...)`, which sends a
message with the pointer's position as fractions of the element's box, on
the press and on every move until the release.

## Layout

Elements are laid out with flexbox. A `row` places its children left to
right, a `column` top to bottom, and a `stack` on top of each other.

```rust,ignore
Element::column([header, body, footer])
    .gap(8.0)
    .padding(16.0)
    .size(Length::Fill, Length::Fill)
    .align(Align::Center)
```

- `Length::Px(n)` is an exact size, `Length::Fill` takes all the space the
  parent offers, and `Length::Content` fits the content.
- `theme.columns(n)` and `theme.rows(n)` give a length of `n` terminal
  cells, so your layout scales with the font.
- `.grow(1.0)` lets an element take the leftover space, and
  `Element::spacer()` is an empty element that does exactly that.
- `.align` places children across the main axis and `.justify` along it.
- `.background(paint)`, `.border(width, paint)` and `.radius(r)` style the
  box. `.color`, `.font_size` and `.weight` style text.

## Work in the background

`view`, `update` and `input` run on the app's thread, so slow work belongs
on another one. katui hands you a `Waker` in `start`. Clone it into a
thread and send messages back:

```rust,ignore
fn start(&mut self, waker: Waker<Message>) {
    std::thread::spawn(move || {
        while waker.send(Message::Tick) {
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
    });
}
```

`send` returns `false` once the app has quit, which ends the loop.

## Next

To see a bigger app, read the to-do demo in
`crates/katui-app/examples/todo.rs` and run it with `just demo-todo`. It
adds filter tabs and a progress bar to the ideas above. The widget tour,
`just demo-widgets`, puts every widget in one app.
