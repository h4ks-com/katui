//! Pixel UI programs in one call: implement [`UiApp`] and hand it to
//! [`run`]. The runner probes the terminal, lays out and rasterizes the
//! view, routes clicks and hovers to their messages, repaints after
//! resizes and sends only what changed.

use std::io;

use katui_draw::{TextEngine, bundled_fonts};
use katui_present::kitty::{Images, KittyScreen, RgbaImage};
use katui_present::surface::{PatchedKitty, PixelScreen};
use katui_proto::input::{Button, InputEvent, Key, Mods, MouseAction, MouseEvent, Pointer};
use katui_proto::probe::Capabilities;
use katui_rt::RtError;
use katui_rt::app::{App, AppEvent, run_app};
use katui_scene::CellSize;
use katui_ui::{CellPixels, Element, Ui, UiOptions};
use katui_widgets::Theme;

pub use katui_rt::app::Control;
pub use katui_rt::events::Waker;

const FRAMES_PER_SECOND: u32 = 30;
const FIRST_IMAGE: u32 = 1_000;
/// A size that keeps every upload inside what a multiplexer carries.
const MULTIPLEXER_LAYER_CELLS: (u16, u16) = (40, 16);

/// A pixel UI program: state that shows itself as an element tree and
/// changes on messages.
pub trait UiApp {
    /// What clicks, hovers, keys and other threads tell the app.
    type Message: Clone + Send + 'static;

    /// Keep `waker` to send messages from other threads.
    fn start(&mut self, _waker: Waker<Self::Message>) {}

    /// The view of the current state.
    fn view(&self, theme: &Theme) -> Element<Self::Message>;

    /// Apply `message` and say what to do next.
    fn update(&mut self, message: Self::Message) -> Control;

    /// React to a key or a paste. Ctrl+C quits before this is called.
    fn input(&mut self, _event: &InputEvent) -> Control {
        Control::Wait
    }

    /// The theme for cells of `cell` pixels.
    fn theme(&self, cell: CellPixels) -> Theme {
        Theme::dark(cell)
    }
}

/// Run `app` on the terminal until it quits.
pub fn run<A: UiApp>(app: A) -> Result<(), RtError> {
    let caps = katui_rt::tty::probe_default();
    let cell = katui_rt::tty::cell_size(&caps);
    let mut runner = Runner::new(app, &caps, cell)?;
    run_app(&mut runner, io::stdout(), &caps, FRAMES_PER_SECOND)
}

/// How frames reach the terminal: as kitty placements of the view's
/// layers, or as one flattened picture through sixel or blocks.
enum Screen {
    Kitty(KittyScreen),
    Picture(PixelScreen),
}

impl Screen {
    fn for_terminal(caps: &Capabilities, cell: CellPixels) -> Self {
        if caps.kitty {
            Self::Kitty(KittyScreen::new(caps))
        } else {
            Self::Picture(PixelScreen::new(
                caps,
                cell,
                PatchedKitty::new(FIRST_IMAGE, cell),
            ))
        }
    }

    fn repaint(&mut self) {
        match self {
            Self::Kitty(screen) => screen.repaint(),
            Self::Picture(screen) => screen.repaint(),
        }
    }
}

struct Runner<A: UiApp> {
    app: A,
    caps: Capabilities,
    cell: CellPixels,
    theme: Theme,
    ui: Ui<A::Message>,
    screen: Screen,
    pointer: Pointer,
    grid: CellSize,
}

impl<A: UiApp> Runner<A> {
    fn new(app: A, caps: &Capabilities, cell: CellPixels) -> Result<Self, RtError> {
        let engine = TextEngine::new(bundled_fonts()).map_err(io::Error::other)?;
        let options = UiOptions {
            first_image_id: FIRST_IMAGE,
            max_layer_cells: caps.multiplexed().then_some(MULTIPLEXER_LAYER_CELLS),
        };
        Ok(Self {
            theme: app.theme(cell),
            app,
            caps: caps.clone(),
            cell,
            ui: Ui::new(engine, options),
            screen: Screen::for_terminal(caps, cell),
            pointer: Pointer::new(caps.cell),
            grid: CellSize {
                columns: 0,
                rows: 0,
            },
        })
    }

    fn mouse(&mut self, mouse: MouseEvent) -> Control {
        let (x, y) = self
            .pointer
            .locate(&mouse, self.cell, self.grid.columns, self.grid.rows);
        let Some(hit) = self.ui.hit_test(x, y) else {
            return Control::Wait;
        };
        let message = match mouse.action {
            MouseAction::Press(Button::Left) => hit.on_click,
            MouseAction::Move => hit.on_hover,
            _ => None,
        };
        message.map_or(Control::Wait, |message| self.app.update(message))
    }

    fn input(&mut self, event: InputEvent) -> Control {
        match event {
            InputEvent::Key(Key::Char('c'), Mods { ctrl: true, .. }) => Control::Quit,
            InputEvent::Mouse(mouse) => self.mouse(mouse),
            InputEvent::Mode(..) => {
                self.pointer.observe(&event);
                Control::Wait
            }
            other => self.app.input(&other),
        }
    }
}

impl<A: UiApp> App for Runner<A> {
    type Message = A::Message;

    fn start(&mut self, waker: Waker<A::Message>) {
        self.app.start(waker);
    }

    fn update(&mut self, event: AppEvent<A::Message>) -> Control {
        match event {
            AppEvent::Input(event) => self.input(event),
            AppEvent::Message(message) => self.app.update(message),
            AppEvent::Resized(_) => {
                self.cell = katui_rt::tty::cell_size(&self.caps);
                self.theme = self.app.theme(self.cell);
                self.screen.repaint();
                Control::Redraw
            }
            AppEvent::Repaint => {
                self.screen.repaint();
                Control::Redraw
            }
        }
    }

    fn draw(&mut self, out: &mut Vec<u8>, size: CellSize) -> io::Result<Control> {
        self.grid = size;
        let pixels = (
            u32::from(size.columns) * u32::from(self.cell.width),
            u32::from(size.rows) * u32::from(self.cell.height),
        );
        let root = self.app.view(&self.theme);
        let output = self.ui.render(&root, pixels, self.cell);
        match &mut self.screen {
            Screen::Kitty(screen) => {
                let mut images = Images::default();
                for layer in &output.images {
                    if let Some(image) = RgbaImage::new(layer.width(), layer.height(), layer.rgba())
                    {
                        images.insert(layer.id, image);
                    }
                }
                screen.present(out, output.placements, &images)?;
            }
            Screen::Picture(screen) => {
                if let Some(picture) = output.flatten(self.cell, pixels) {
                    screen.render_frame(out, &picture, size.columns, size.rows)?;
                }
            }
        }
        Ok(Control::Wait)
    }
}

#[cfg(test)]
mod tests {
    use katui_proto::input::{Button, InputEvent, Key, Mods, MouseAction, MouseEvent};
    use katui_proto::probe::Capabilities;
    use katui_rt::headless::Headless;
    use katui_scene::CellSize;
    use katui_ui::{CellPixels, Element, Length};
    use katui_widgets::{Emphasis, Theme, button, label};

    use super::{Control, Runner, UiApp};

    const CELL: CellPixels = CellPixels {
        width: 8,
        height: 16,
    };

    #[derive(Default)]
    struct Counter {
        count: u32,
        hovered: bool,
    }

    #[derive(Debug, Clone, PartialEq)]
    enum Message {
        Add,
        Hover,
    }

    impl UiApp for Counter {
        type Message = Message;

        fn view(&self, theme: &Theme) -> Element<Message> {
            Element::column([
                label(format!("{}", self.count), theme.body(), theme.text),
                button("Add one", Message::Add, Emphasis::Primary, theme).on_hover(Message::Hover),
            ])
            .size(Length::Fill, Length::Fill)
        }

        fn update(&mut self, message: Message) -> Control {
            match message {
                Message::Add => self.count += 1,
                Message::Hover => self.hovered = true,
            }
            Control::Redraw
        }

        fn input(&mut self, event: &InputEvent) -> Control {
            match event {
                InputEvent::Key(Key::Char('+'), _) => self.update(Message::Add),
                _ => Control::Wait,
            }
        }
    }

    fn mouse(action: MouseAction, x: u32, y: u32) -> InputEvent {
        InputEvent::Mouse(MouseEvent {
            action,
            x,
            y,
            mods: Mods::NONE,
        })
    }

    #[test]
    fn clicks_hovers_and_keys_reach_the_app_and_frames_show_it() {
        let caps = Capabilities {
            kitty: true,
            ..Capabilities::default()
        };
        let runner = Runner::new(Counter::default(), &caps, CELL).expect("the bundled fonts parse");
        let mut driver = Headless::start(
            runner,
            CellSize {
                columns: 40,
                rows: 10,
            },
        );
        let first = driver
            .frame()
            .expect("drawing works")
            .expect("a first frame");
        assert!(
            String::from_utf8_lossy(&first).contains("a=t,"),
            "the view transmits"
        );
        driver.input(mouse(MouseAction::Move, 2, 1));
        driver.input(mouse(MouseAction::Press(Button::Left), 2, 1));
        driver.type_bytes(b"+");
        assert_eq!(driver.app().app.count, 2, "one click and one key");
        assert!(driver.app().app.hovered);
        let next = driver
            .frame()
            .expect("drawing works")
            .expect("a changed frame");
        assert!(!next.is_empty(), "the new count goes out");
        driver.type_bytes(b"\x03");
        assert!(driver.quit(), "ctrl c quits");
    }

    #[test]
    fn a_terminal_without_kitty_graphics_gets_the_view_as_one_picture() {
        let size = CellSize {
            columns: 20,
            rows: 6,
        };
        let draw = |caps: Capabilities| {
            let runner =
                Runner::new(Counter::default(), &caps, CELL).expect("the bundled fonts parse");
            let mut driver = Headless::start(runner, size);
            let frame = driver
                .frame()
                .expect("drawing works")
                .expect("a first frame");
            String::from_utf8_lossy(&frame).into_owned()
        };
        let sixel = draw(Capabilities {
            sixel: true,
            ..Capabilities::default()
        });
        assert!(sixel.contains("\x1bP0;1;0q"), "a sixel terminal gets sixel");
        let plain = draw(Capabilities::default());
        assert!(plain.contains("\x1b[38;2;"), "a plain terminal gets blocks");
        assert!(!plain.contains("\x1b_G"), "and no graphics commands");
    }
}
