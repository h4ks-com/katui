//! A click counter, the smallest katui program. Run with `just demo-counter`.
#![cfg(unix)] // the runner drives /dev/tty through katui-rt's unix layer

use katui::app::{Control, UiApp, run};
use katui::ui::{Align, Element};
use katui::widgets::{Emphasis, Theme, button, label, panel};

#[derive(Default)]
struct Counter(u32);

impl UiApp for Counter {
    type Message = ();

    fn view(&self, theme: &Theme) -> Element<()> {
        panel(
            [
                label(
                    format!("Clicked {} times", self.0),
                    theme.body(),
                    theme.text,
                ),
                button("Add one", (), Emphasis::Primary, theme),
            ],
            theme,
        )
        .align(Align::Center)
    }

    fn update(&mut self, (): ()) -> Control {
        self.0 += 1;
        Control::Redraw
    }
}

fn main() -> Result<(), katui::rt::RtError> {
    run(Counter::default())
}
