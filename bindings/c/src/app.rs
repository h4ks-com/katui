//! The bridge between katui's app runner and a C app, and `katui_run`.

use std::cell::RefCell;
use std::ffi::c_void;
use std::rc::Rc;
use std::time::Duration;

use katui_app::{Control, UiApp, Waker};
use katui_proto::input::InputEvent;
use katui_ui::{CellPixels, Element};
use katui_widgets::Theme;

use crate::element::{KatuiElement, Node, node};
use crate::ffi::{Error, KatuiStatus, borrow, free, guard, owned, status, take};
use crate::message::{KatuiMessage, Msg, plain, with_message};
use crate::state::KatuiEvent;
use crate::theme::KatuiTheme;

/// What to do after a message or an event.
pub type KatuiControl = u32;
/// Sleep until something happens.
pub const KATUI_CONTROL_WAIT: KatuiControl = 0;
/// Draw once.
pub const KATUI_CONTROL_REDRAW: KatuiControl = 1;
/// Keep drawing at the frame rate.
pub const KATUI_CONTROL_ANIMATE: KatuiControl = 2;
/// End the app.
pub const KATUI_CONTROL_QUIT: KatuiControl = 3;

fn control(control: KatuiControl) -> Control {
    match control {
        KATUI_CONTROL_WAIT => Control::Wait,
        KATUI_CONTROL_ANIMATE => Control::Animate,
        KATUI_CONTROL_QUIT => Control::Quit,
        _ => Control::Redraw,
    }
}

/// A pixel UI program as function pointers over your state at `user`.
/// `view` and `update` are required; leave the others NULL for their
/// defaults. katui calls them all on the thread that runs the app.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct KatuiApp {
    /// Handed to every callback as is.
    pub user: *mut c_void,
    /// Keep `waker` to send messages from other threads. The app owns it
    /// and frees it with `katui_waker_free`.
    pub start: Option<unsafe extern "C" fn(user: *mut c_void, waker: *mut KatuiWaker)>,
    /// The view of the current state. katui takes over the element; NULL
    /// ends the app with `KATUI_STATUS_CALLBACK`.
    pub view: Option<unsafe extern "C" fn(user: *mut c_void) -> *mut KatuiElement>,
    /// Apply `message`, which katui owns, and say what to do next.
    pub update: Option<
        unsafe extern "C" fn(user: *mut c_void, message: *const KatuiMessage) -> KatuiControl,
    >,
    /// React to a key or a paste; `event` lives during the call. NULL
    /// waits for the next event.
    pub input:
        Option<unsafe extern "C" fn(user: *mut c_void, event: *const KatuiEvent) -> KatuiControl>,
    /// The theme for cells of `cell_width` by `cell_height` pixels, asked
    /// before every frame. katui takes over the theme; NULL, or a NULL
    /// callback, means the dark theme.
    pub theme: Option<
        unsafe extern "C" fn(
            user: *mut c_void,
            cell_width: u16,
            cell_height: u16,
        ) -> *mut KatuiTheme,
    >,
}

/// The first broken contract a callback made, kept until the runner returns.
pub(crate) type Failure = Rc<RefCell<Option<Error>>>;

/// A C app, as the runner sees it.
pub(crate) struct Bridge {
    app: KatuiApp,
    failure: Failure,
    waker: Option<Waker<Msg>>,
}

impl Bridge {
    pub(crate) fn new(app: &KatuiApp, failure: Failure) -> Result<Self, Error> {
        if app.view.is_none() {
            return Err(Error::Null("view callback"));
        }
        if app.update.is_none() {
            return Err(Error::Null("update callback"));
        }
        Ok(Self {
            app: *app,
            failure,
            waker: None,
        })
    }

    fn fail(&self, error: Error) -> Control {
        self.failure.borrow_mut().get_or_insert(error);
        if let Some(waker) = &self.waker {
            waker.send(Msg::Stop);
        }
        Control::Quit
    }

    pub(crate) fn render(&self) -> Node {
        let made = self.app.view.map(|view| unsafe { view(self.app.user) });
        made.and_then(|element| unsafe { node(element) })
            .unwrap_or_else(|| {
                self.fail(Error::Callback("return an element from view, not NULL"));
                Element::spacer()
            })
    }

    pub(crate) fn theme_for(&self, cell: CellPixels) -> Theme {
        let picked = self
            .app
            .theme
            .map(|theme| unsafe { theme(self.app.user, cell.width, cell.height) });
        picked
            .and_then(|theme| unsafe { take(theme) })
            .map_or_else(|| Theme::dark(cell), |theme| theme.0)
    }

    pub(crate) fn deliver(&self, message: &Msg) -> Control {
        let Msg::User(key, payload) = message else {
            return Control::Quit;
        };
        let Some(update) = self.app.update else {
            return Control::Redraw;
        };
        with_message(*key, payload, |message| {
            control(unsafe { update(self.app.user, message) })
        })
    }

    pub(crate) fn react(&self, event: &InputEvent) -> Control {
        let Some(input) = self.app.input else {
            return Control::Wait;
        };
        let event = KatuiEvent(event.clone());
        control(unsafe { input(self.app.user, &raw const event) })
    }

    pub(crate) fn failure(&self) -> Option<Error> {
        self.failure.borrow_mut().take()
    }
}

impl UiApp for Bridge {
    type Message = Msg;

    fn start(&mut self, waker: Waker<Msg>) {
        self.waker = Some(waker.clone());
        if let Some(start) = self.app.start {
            unsafe { start(self.app.user, owned(KatuiWaker(waker))) };
        }
    }

    fn view(&self) -> Element<Msg> {
        self.render()
    }

    fn update(&mut self, message: Msg) -> Control {
        self.deliver(&message)
    }

    fn input(&mut self, event: &InputEvent) -> Control {
        self.react(event)
    }

    fn theme(&self, cell: CellPixels) -> Theme {
        self.theme_for(cell)
    }
}

/// Sends messages to a running app from any thread.
pub struct KatuiWaker(Waker<Msg>);

/// Hand `key` to the app's `update`, from any thread. False once the app
/// has quit.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_waker_send(waker: *const KatuiWaker, key: u64) -> bool {
    unsafe { borrow(waker) }.is_some_and(|waker| waker.0.send(plain(key)))
}

/// Hand `key` to `update` every `seconds` until the app quits.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_waker_every(waker: *const KatuiWaker, seconds: f64, key: u64) {
    if let Some(waker) = unsafe { borrow(waker) } {
        let interval = Duration::try_from_secs_f64(seconds.max(0.001)).unwrap_or(Duration::MAX);
        waker.0.every(interval, move || plain(key));
    }
}

/// Free a waker. NULL does nothing.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_waker_free(waker: *mut KatuiWaker) {
    unsafe { free(waker) };
}

/// Run `app` on the terminal until it quits. `app` is copied; `user` has
/// to live until this returns.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_run(app: *const KatuiApp) -> KatuiStatus {
    let ran = guard(|| {
        let app = unsafe { borrow(app) }.ok_or(Error::Null("app"))?;
        let failure = Failure::default();
        let finished = katui_app::run(Bridge::new(app, Rc::clone(&failure))?);
        if let Some(error) = failure.borrow_mut().take() {
            return Err(error);
        }
        finished.map_err(|error| Error::Io(error.to_string()))
    });
    status(ran)
}
