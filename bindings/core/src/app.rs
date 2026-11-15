//! The bridge between katui's app runner and an app written in another
//! language, and [`run`].

use katui_app::{Control, UiApp, Waker};
use katui_proto::input::InputEvent;
use katui_ui::{CellPixels, Element};
use katui_widgets::Theme;

use crate::failure::Failure;
use crate::{Error, Msg, Node};

/// An app as a binding sees it: each hook calls into the host language and
/// reports a failure in the host's own error type.
pub trait AppHost {
    /// The host's handle for a value it keeps, carried in messages.
    type Value: Clone + Send + 'static;
    type Error;

    /// `error` raised the way the host raises its errors.
    fn refused(error: Error) -> Self::Error;

    /// Hand `waker` to the app, to send messages from other threads.
    fn start(&mut self, waker: Waker<Msg<Self::Value>>) -> Result<(), Self::Error>;

    /// The view of the current state.
    fn view(&self) -> Result<Node<Self::Value>, Self::Error>;

    /// Apply `message`, never [`Msg::Stop`], and say what to do next.
    fn update(&mut self, message: &Msg<Self::Value>) -> Result<Control, Self::Error>;

    fn input(&mut self, event: &InputEvent) -> Result<Control, Self::Error>;

    /// The app's theme for cells of `cell` pixels, or `None` for the dark one.
    fn theme(&self, cell: CellPixels) -> Result<Option<Theme>, Self::Error>;
}

/// A host app, as the runner sees it: the first failure ends the app and
/// waits in its [`Failure`] until the runner returns.
pub struct Bridge<H: AppHost> {
    host: H,
    failure: Failure<H::Error>,
    waker: Option<Waker<Msg<H::Value>>>,
}

impl<H: AppHost> Bridge<H> {
    pub fn new(host: H) -> Self {
        Self {
            host,
            failure: Failure::default(),
            waker: None,
        }
    }

    pub fn failure(&self) -> &Failure<H::Error> {
        &self.failure
    }

    fn fail(&self, error: H::Error) -> Control {
        self.failure.keep(error);
        if let Some(waker) = &self.waker {
            waker.send(Msg::Stop);
        }
        Control::Quit
    }

    pub fn render(&self) -> Node<H::Value> {
        self.host.view().unwrap_or_else(|error| {
            self.fail(error);
            Element::spacer()
        })
    }

    pub fn theme_for(&self, cell: CellPixels) -> Theme {
        match self.host.theme(cell) {
            Ok(theme) => theme.unwrap_or_else(|| Theme::dark(cell)),
            Err(error) => {
                self.fail(error);
                Theme::dark(cell)
            }
        }
    }

    pub fn deliver(&mut self, message: &Msg<H::Value>) -> Control {
        if matches!(message, Msg::Stop) {
            return Control::Quit;
        }
        self.host
            .update(message)
            .unwrap_or_else(|error| self.fail(error))
    }

    pub fn react(&mut self, event: &InputEvent) -> Control {
        self.host
            .input(event)
            .unwrap_or_else(|error| self.fail(error))
    }
}

impl<H: AppHost> UiApp for Bridge<H> {
    type Message = Msg<H::Value>;

    fn start(&mut self, waker: Waker<Self::Message>) {
        self.waker = Some(waker.clone());
        if let Err(error) = self.host.start(waker) {
            self.fail(error);
        }
    }

    fn view(&self) -> Element<Self::Message> {
        self.render()
    }

    fn update(&mut self, message: Self::Message) -> Control {
        self.deliver(&message)
    }

    fn input(&mut self, event: &InputEvent) -> Control {
        self.react(event)
    }

    fn theme(&self, cell: CellPixels) -> Theme {
        self.theme_for(cell)
    }
}

/// Run `host` on the terminal until it quits. The first failure of the
/// host comes back after the terminal is restored.
pub fn run<H: AppHost>(host: H) -> Result<(), H::Error> {
    let bridge = Bridge::new(host);
    let failure = bridge.failure().clone();
    let finished = katui_app::run(bridge);
    if let Some(error) = failure.take() {
        return Err(error);
    }
    finished.map_err(|error| H::refused(Error::Terminal(error.to_string())))
}
