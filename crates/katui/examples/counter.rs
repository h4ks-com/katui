//! A click counter, the smallest katui program. Run with `just demo-counter`.
#![cfg(unix)] // the runner drives /dev/tty through katui-rt's unix layer

use katui::app::{Control, UiApp, run};
use katui::ui::{Align, Element};
use katui::widgets::{label, panel, primary_button};

#[derive(Default)]
struct Counter(u32);

impl UiApp for Counter {
    type Message = ();

    fn view(&self) -> Element<()> {
        panel([
            label(format!("Clicked {} times", self.0)),
            primary_button("Add one", ()),
        ])
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

#[cfg(test)]
mod tests {
    use katui::app::UiApp as _;
    use katui::draw::{TextEngine, bundled_fonts};
    use katui::ui::{CellPixels, Ui, UiOptions};
    use katui::widgets::Theme;

    use super::Counter;

    #[test]
    fn three_clicks_show_on_the_screen() {
        const CELL: CellPixels = CellPixels {
            width: 9,
            height: 19,
        };
        let mut counter = Counter::default();
        for _ in 0..3 {
            counter.update(());
        }
        assert_eq!(counter.0, 3);
        let size = (9 * 40, 19 * 6);
        let view = Theme::dark(CELL).scope(|_| counter.view());
        let engine = TextEngine::new(bundled_fonts()).expect("the bundled fonts parse");
        let mut ui: Ui<()> = Ui::new(engine, UiOptions::default());
        let surface = ui
            .render(&view, size, CELL)
            .flatten(CELL, size)
            .expect("whole pixels");
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../target/screenshots");
        std::fs::create_dir_all(dir).expect("the target directory is writable");
        image::save_buffer(
            format!("{dir}/counter.png"),
            surface.pixels(),
            size.0,
            size.1,
            image::ExtendedColorType::Rgba8,
        )
        .expect("the png writer saves under target");
    }
}
