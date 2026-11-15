//! The bridge between katui's app runner and a C app, and `katui_run`.

use std::ffi::c_void;

use katui_app::{Control, Waker};
use katui_bindings::app::AppHost;
use katui_bindings::state::interval;
use katui_proto::input::InputEvent;
use katui_ui::CellPixels;
use katui_widgets::Theme;

use crate::element::{KatuiElement, Node};
use crate::event::KatuiEvent;
use crate::ffi::{Error, KatuiStatus, guard, raise, status};
use crate::message::{KatuiMessage, Msg, with_message};
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
    pub start: Option<unsafe extern "C" fn(user: *mut c_void, waker: Box<KatuiWaker>)>,
    /// The view of the current state. katui takes over the element; NULL
    /// ends the app with `KATUI_STATUS_CALLBACK`.
    pub view: Option<unsafe extern "C" fn(user: *mut c_void) -> Option<Box<KatuiElement>>>,
    /// Apply `message`, which katui owns, and say what to do next.
    pub update:
        Option<unsafe extern "C" fn(user: *mut c_void, message: &KatuiMessage) -> KatuiControl>,
    /// React to a key or a paste; `event` lives during the call. NULL
    /// waits for the next event.
    pub input: Option<unsafe extern "C" fn(user: *mut c_void, event: &KatuiEvent) -> KatuiControl>,
    /// The theme for cells of `cell_width` by `cell_height` pixels, asked
    /// before every frame. katui takes over the theme; NULL, or a NULL
    /// callback, means the dark theme.
    pub theme: Option<
        unsafe extern "C" fn(
            user: *mut c_void,
            cell_width: u16,
            cell_height: u16,
        ) -> Option<Box<KatuiTheme>>,
    >,
}

/// A C app, as the shared bridge calls it.
pub(crate) struct CApp(KatuiApp);

impl CApp {
    /// # Safety
    /// Every callback of `app` takes its `user`, which outlives the result.
    pub(crate) unsafe fn new(app: &KatuiApp) -> Result<Self, Error> {
        if app.view.is_none() {
            return Err(Error::Null("view callback"));
        }
        if app.update.is_none() {
            return Err(Error::Null("update callback"));
        }
        Ok(Self(*app))
    }
}

impl AppHost for CApp {
    type Value = u64;
    type Error = Error;

    fn refused(error: katui_bindings::Error) -> Error {
        raise(error)
    }

    fn start(&mut self, waker: Waker<Msg>) -> Result<(), Error> {
        if let Some(start) = self.0.start {
            // SAFETY: `CApp::new` requires `start` to take `user`, which outlives us.
            unsafe { start(self.0.user, Box::new(KatuiWaker(waker))) };
        }
        Ok(())
    }

    fn view(&self) -> Result<Node, Error> {
        // SAFETY: `CApp::new` requires `view` to take `user`, which outlives us.
        let made = self.0.view.and_then(|view| unsafe { view(self.0.user) });
        made.map(|element| element.0)
            .ok_or(Error::Callback("return an element from view, not NULL"))
    }

    fn update(&mut self, message: &Msg) -> Result<Control, Error> {
        let Some(update) = self.0.update else {
            return Ok(Control::Redraw);
        };
        Ok(with_message(message, |message| {
            // SAFETY: `CApp::new` requires `update` to take `user`, which outlives us.
            control(unsafe { update(self.0.user, message) })
        }))
    }

    fn input(&mut self, event: &InputEvent) -> Result<Control, Error> {
        let Some(input) = self.0.input else {
            return Ok(Control::Wait);
        };
        let event = KatuiEvent(event.clone());
        // SAFETY: `CApp::new` requires `input` to take `user`, which outlives us.
        Ok(control(unsafe { input(self.0.user, &event) }))
    }

    fn theme(&self, cell: CellPixels) -> Result<Option<Theme>, Error> {
        let picked = self.0.theme.and_then(|theme| {
            // SAFETY: `CApp::new` requires `theme` to take `user`, which outlives us.
            unsafe { theme(self.0.user, cell.width, cell.height) }
        });
        Ok(picked.map(|theme| theme.0))
    }
}

/// Sends messages to a running app from any thread.
pub struct KatuiWaker(Waker<Msg>);

/// Hand `key` to the app's `update`, from any thread. False once the app
/// has quit.
#[unsafe(no_mangle)]
pub extern "C" fn katui_waker_send(waker: Option<&KatuiWaker>, key: u64) -> bool {
    waker.is_some_and(|waker| waker.0.send(Msg::Value(key)))
}

/// Hand `key` to `update` every `seconds` until the app quits.
#[unsafe(no_mangle)]
pub extern "C" fn katui_waker_every(waker: Option<&KatuiWaker>, seconds: f64, key: u64) {
    if let Some(waker) = waker {
        waker.0.every(interval(seconds), move || Msg::Value(key));
    }
}

/// Free a waker. NULL does nothing.
#[unsafe(no_mangle)]
pub extern "C" fn katui_waker_free(waker: Option<Box<KatuiWaker>>) {
    drop(waker);
}

/// Run `app` on the terminal until it quits. `app` is copied; `user` has
/// to live until this returns.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_run(app: Option<&KatuiApp>) -> KatuiStatus {
    let ran = guard(|| {
        let app = app.ok_or(Error::Null("app"))?;
        // SAFETY: the header requires `user` to live until `katui_run` returns.
        katui_bindings::app::run(unsafe { CApp::new(app) }?)
    });
    status(ran)
}
