//! Pixel UI programs in one call: implement [`UiApp`] and hand it to
//! [`run`]. The runner probes the terminal, lays out and rasterizes the
//! view, routes clicks and hovers to their messages, repaints after
//! resizes and sends only what changed.

use std::io;

use katui_draw::{TextEngine, bundled_fonts};
use katui_present::kitty::{Images, KittyScreen, RgbaImage};
use katui_present::surface::PixelScreen;
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

/// A pixel UI program: state that shows itself as an element tree and
/// changes on messages.
pub trait UiApp {
    /// What clicks, hovers, keys and other threads tell the app.
    type Message: Clone + Send + 'static;

    /// Keep `waker` to send messages from other threads.
    fn start(&mut self, _waker: Waker<Self::Message>) {}

    /// The view of the current state. Widgets draw with the app's theme;
    /// [`Theme::current`] gives its colors and sizes for your own elements.
    fn view(&self) -> Element<Self::Message>;

    /// Apply `message` and say what to do next.
    fn update(&mut self, message: Self::Message) -> Control;

    /// React to a key or a paste. Ctrl+C quits before this is called.
    fn input(&mut self, _event: &InputEvent) -> Control {
        Control::Wait
    }

    /// The theme for cells of `cell` pixels. The runner asks before every
    /// frame, so returning another theme switches the look.
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
            Self::Picture(PixelScreen::picture(caps, cell))
        }
    }

    fn repaint(&mut self) {
        match self {
            Self::Kitty(screen) => screen.repaint(),
            Self::Picture(screen) => screen.repaint(),
        }
    }

    fn rows_shown(&self, rows: u16) -> u16 {
        match self {
            Self::Kitty(_) => rows,
            Self::Picture(screen) => screen.rows_shown(rows),
        }
    }
}

struct Runner<A: UiApp> {
    app: A,
    caps: Capabilities,
    cell: CellPixels,
    ui: Ui<A::Message>,
    screen: Screen,
    pointer: Pointer,
    hovered: Option<(usize, Option<A::Message>)>,
}

impl<A: UiApp> Runner<A> {
    fn new(app: A, caps: &Capabilities, cell: CellPixels) -> Result<Self, RtError> {
        let engine = TextEngine::new(bundled_fonts()).map_err(io::Error::other)?;
        let options = UiOptions::for_terminal(FIRST_IMAGE, caps.multiplexed());
        Ok(Self {
            app,
            caps: caps.clone(),
            cell,
            ui: Ui::new(engine, options),
            screen: Screen::for_terminal(caps, cell),
            pointer: Pointer::new(caps.cell),
            hovered: None,
        })
    }

    fn mouse(&mut self, mouse: MouseEvent) -> Control {
        let (x, y) = self.pointer.locate(&mouse, self.cell);
        match mouse.action {
            MouseAction::Press(Button::Left) => {
                let message = self
                    .ui
                    .press(x, y)
                    .or_else(|| self.ui.hit_test(x, y).and_then(|hit| hit.on_click));
                self.send(message)
            }
            MouseAction::Drag(Button::Left) => {
                let message = self.ui.drag(x, y);
                self.send(message)
            }
            MouseAction::Release(Button::Left) => {
                self.ui.release();
                Control::Wait
            }
            MouseAction::WheelUp => {
                let message = self.ui.hit_test(x, y).and_then(|hit| hit.on_wheel_up);
                self.send(message)
            }
            MouseAction::WheelDown => {
                let message = self.ui.hit_test(x, y).and_then(|hit| hit.on_wheel_down);
                self.send(message)
            }
            MouseAction::Move => self.hover(x, y),
            _ => Control::Wait,
        }
    }

    /// Send the leave message of the element the pointer left, then the
    /// hover message of the one it entered.
    fn hover(&mut self, x: f32, y: f32) -> Control {
        let hit = self.ui.hit_test(x, y);
        let target = hit.as_ref().and_then(|hit| hit.hover_target);
        if self.hovered.as_ref().map(|(index, _)| *index) == target {
            return Control::Wait;
        }
        let left = self.hovered.take().and_then(|(_, leave)| leave);
        let mut control = self.send(left);
        if let Some(hit) = hit {
            if let Some(index) = hit.hover_target {
                self.hovered = Some((index, hit.on_leave));
            }
            control = more_urgent(control, self.send(hit.on_hover));
        }
        control
    }

    fn send(&mut self, message: Option<A::Message>) -> Control {
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

/// The more urgent of two controls: quitting beats animating, which beats
/// one redraw, which beats waiting.
fn more_urgent(first: Control, second: Control) -> Control {
    let rank = |control: Control| match control {
        Control::Wait => 0,
        Control::Redraw => 1,
        Control::Animate => 2,
        Control::Quit => 3,
    };
    if rank(second) > rank(first) {
        second
    } else {
        first
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
        self.pointer.resize(size.columns, size.rows);
        let rows = self.screen.rows_shown(size.rows);
        let pixels = (
            u32::from(size.columns) * u32::from(self.cell.width),
            u32::from(rows) * u32::from(self.cell.height),
        );
        let app = &self.app;
        let root = app.theme(self.cell).scope(|_| app.view());
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
        let clamped_scrolls = self.ui.take_clamped_scrolls();
        let corrected = !clamped_scrolls.is_empty();
        for message in clamped_scrolls {
            self.app.update(message);
        }
        Ok(if corrected {
            Control::Redraw
        } else {
            Control::Wait
        })
    }
}

#[cfg(test)]
mod tests {
    use katui_proto::input::{Button, InputEvent, Key, Mods, MouseAction, MouseEvent};
    use katui_proto::probe::Capabilities;
    use katui_rt::headless::Headless;
    use katui_scene::CellSize;
    use katui_ui::{CellPixels, Element, Length, Scroll};
    use katui_widgets::{label, primary_button};

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

        fn view(&self) -> Element<Message> {
            Element::column([
                label(format!("{}", self.count)),
                primary_button("Add one", Message::Add).on_hover(Message::Hover),
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

    #[derive(Default)]
    struct Log {
        scroll: Scroll,
        hovered: bool,
    }

    #[derive(Debug, Clone, PartialEq)]
    enum LogMessage {
        Scrolled(Scroll),
        Enter,
        Leave,
    }

    impl UiApp for Log {
        type Message = LogMessage;

        fn view(&self) -> Element<LogMessage> {
            let lines = (0..40).map(|line| label(format!("line {line}")));
            Element::column([
                Element::column(lines)
                    .scroll(self.scroll.offset)
                    .on_scroll(LogMessage::Scrolled)
                    .size(Length::Fill, Length::Px(80.0)),
                label("footer")
                    .on_hover(LogMessage::Enter)
                    .on_leave(LogMessage::Leave),
            ])
            .size(Length::Fill, Length::Fill)
        }

        fn update(&mut self, message: LogMessage) -> Control {
            match message {
                LogMessage::Scrolled(scroll) => self.scroll = scroll,
                LogMessage::Enter => self.hovered = true,
                LogMessage::Leave => self.hovered = false,
            }
            Control::Redraw
        }

        fn input(&mut self, event: &InputEvent) -> Control {
            if let InputEvent::Key(Key::End, _) = event {
                self.scroll.offset = 1.0e6;
                return Control::Redraw;
            }
            Control::Wait
        }
    }

    #[test]
    fn the_wheel_scrolls_an_overshoot_is_clamped_and_leaving_is_reported() {
        let caps = Capabilities {
            kitty: true,
            ..Capabilities::default()
        };
        let runner = Runner::new(Log::default(), &caps, CELL).expect("the bundled fonts parse");
        let mut driver = Headless::start(
            runner,
            CellSize {
                columns: 40,
                rows: 10,
            },
        );
        driver.frame().expect("drawing works");
        driver.input(mouse(MouseAction::WheelDown, 2, 1));
        let after_wheel = driver.app().app.scroll;
        assert!(after_wheel.offset > 0.0, "the wheel scrolls the log");
        assert!(after_wheel.max > after_wheel.offset);

        driver.type_bytes(b"\x1b[F");
        driver.frame().expect("drawing works");
        let clamped = driver.app().app.scroll;
        assert_eq!(
            clamped.offset, clamped.max,
            "the overshoot comes back clamped"
        );

        driver.input(mouse(MouseAction::Move, 2, 5));
        assert!(driver.app().app.hovered);
        driver.input(mouse(MouseAction::Move, 2, 9));
        assert!(!driver.app().app.hovered, "moving away reports the leave");
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
