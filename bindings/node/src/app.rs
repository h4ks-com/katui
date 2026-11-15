//! The bridge between katui's app runner and a JavaScript app, and `run`.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use katui_app::{Control as UiControl, UiApp, Waker as UiWaker};
use katui_proto::input::InputEvent;
use katui_ui::{CellPixels, Element as UiElement};
use katui_widgets::Theme as UiTheme;
use napi::bindgen_prelude::{Function, JsObjectValue, JsValuesTupleIntoVec, Object, Unknown};
use napi::{Env, Error, JsValue, Result, Status, ValueType};

use crate::convert::{function, instance, invalid, is_nothing};
use crate::element::{Node, node};
use crate::message::{Msg, Obj, fetch, hold};
use crate::reclaim::{collector, reclaim};
use crate::state::Event;
use crate::theme::Theme;

/// What to do after a message or an event.
#[napi]
pub enum Control {
    /// Sleep until something happens.
    Wait,
    /// Draw once.
    Redraw,
    /// Keep drawing at the frame rate.
    Animate,
    /// End the app.
    Quit,
}

impl From<Control> for UiControl {
    fn from(control: Control) -> Self {
        match control {
            Control::Wait => Self::Wait,
            Control::Redraw => Self::Redraw,
            Control::Animate => Self::Animate,
            Control::Quit => Self::Quit,
        }
    }
}

/// The first error JavaScript raised, kept until the runner returns.
pub(crate) type Failure = Rc<RefCell<Option<Error>>>;

pub(crate) fn take(failure: &Failure) -> Option<Error> {
    failure.borrow_mut().take()
}

pub(crate) fn keep(failure: &Failure, error: Error) {
    failure.borrow_mut().get_or_insert(error);
}

/// Calls the method `name` of `target` with `args`, or gives `None` when
/// `target` has no such method.
pub(crate) fn call_optional<'env, Args: JsValuesTupleIntoVec>(
    target: &Object<'env>,
    name: &str,
    args: Args,
) -> Result<Option<Unknown<'env>>> {
    let member: Unknown<'_> = target.get_named_property(name)?;
    match member.get_type()? {
        ValueType::Undefined | ValueType::Null => Ok(None),
        ValueType::Function => {
            let method: Function<'env, Args, Unknown<'env>> = target.get_named_property(name)?;
            method.bind(target)?.call(args).map(Some)
        }
        _ => Err(invalid(format!("make {name} a method"))),
    }
}

/// What a JavaScript callback returned, as a `Control`: nothing means `default`.
pub(crate) fn control(returned: Option<Unknown<'_>>, default: UiControl) -> Result<UiControl> {
    let Some(value) = returned else {
        return Ok(default);
    };
    if is_nothing(&value)? {
        return Ok(default);
    }
    let wrong = || invalid("return a Control, or nothing for the default");
    if value.get_type()? != ValueType::Number {
        return Err(wrong());
    }
    let number = value.coerce_to_number()?.get_uint32()?;
    let control = match number {
        0 => Control::Wait,
        1 => Control::Redraw,
        2 => Control::Animate,
        3 => Control::Quit,
        _ => return Err(wrong()),
    };
    Ok(control.into())
}

/// The object `held` keeps, as an object.
pub(crate) fn object<'env>(env: &'env Env, held: &Obj) -> Result<Object<'env>> {
    fetch(env, held)
}

/// A JavaScript app, as the runner sees it.
pub(crate) struct Bridge {
    env: Env,
    app: Obj,
    collect: Obj,
    failure: Failure,
    waker: Option<UiWaker<Msg>>,
}

impl Bridge {
    pub(crate) fn new(env: Env, app: Object<'_>, failure: Failure) -> Result<Self> {
        Ok(Self {
            env,
            app: hold(&env, app.to_unknown())?,
            collect: collector(&env)?,
            failure,
            waker: None,
        })
    }

    fn fail(&self, error: Error) -> UiControl {
        keep(&self.failure, error);
        if let Some(waker) = &self.waker {
            waker.send(Msg::Stop);
        }
        UiControl::Quit
    }

    /// Runs `body` in a handle scope of its own, so the values it makes are
    /// freed when it returns, and turns an error into a quit.
    fn scoped(&self, body: impl FnOnce(&Env, Object<'_>) -> Result<UiControl>) -> UiControl {
        let env = self.env;
        let outcome = env.run_in_scope(|| body(&env, object(&env, &self.app)?));
        outcome.unwrap_or_else(|error| self.fail(error))
    }

    pub(crate) fn render(&self) -> Node {
        let env = self.env;
        let built = env.run_in_scope(|| {
            reclaim(&env, &self.collect)?;
            let app = object(&env, &self.app)?;
            let view = call_optional(&app, "view", ())?
                .ok_or_else(|| invalid("give the app a view method"))?;
            node(&env, view)
        });
        built.unwrap_or_else(|error| {
            self.fail(error);
            UiElement::spacer()
        })
    }

    pub(crate) fn theme_for(&self, cell: CellPixels) -> UiTheme {
        let env = self.env;
        let picked = env.run_in_scope(|| {
            let app = object(&env, &self.app)?;
            let returned = call_optional(&app, "theme", (cell.width, cell.height))?;
            match returned {
                Some(theme) if !is_nothing(&theme)? => {
                    let theme = instance::<Theme>(&env, theme, "return a Theme from theme")?;
                    Ok(Some(theme.inner.clone()))
                }
                _ => Ok(None),
            }
        });
        match picked {
            Ok(theme) => theme.unwrap_or_else(|| UiTheme::dark(cell)),
            Err(error) => {
                self.fail(error);
                UiTheme::dark(cell)
            }
        }
    }

    pub(crate) fn deliver(&self, message: &Msg) -> UiControl {
        self.scoped(|env, app| match message.resolve(env)? {
            None => Ok(UiControl::Quit),
            Some(resolved) => match call_optional(&app, "update", resolved)? {
                Some(returned) => control(Some(returned), UiControl::Redraw),
                None => default_update(resolved),
            },
        })
    }

    pub(crate) fn react(&self, event: &InputEvent) -> UiControl {
        self.scoped(|_, app| {
            let returned = call_optional(&app, "input", Event::new(event.clone()))?;
            control(returned, UiControl::Wait)
        })
    }

    fn begin(&self, waker: UiWaker<Msg>) {
        self.scoped(|_, app| {
            call_optional(&app, "start", Waker { inner: waker })?;
            Ok(UiControl::Wait)
        });
    }
}

/// What `App.update` does: call a function message and use what it returns.
fn default_update(message: Unknown<'_>) -> Result<UiControl> {
    if message.get_type()? != ValueType::Function {
        return Ok(UiControl::Redraw);
    }
    let call = function::<(), Unknown<'_>>(message)?;
    control(Some(call.call(())?), UiControl::Redraw)
}

impl UiApp for Bridge {
    type Message = Msg;

    fn start(&mut self, waker: UiWaker<Msg>) {
        self.waker = Some(waker.clone());
        self.begin(waker);
    }

    fn view(&self) -> UiElement<Msg> {
        self.render()
    }

    fn update(&mut self, message: Msg) -> UiControl {
        self.deliver(&message)
    }

    fn input(&mut self, event: &InputEvent) -> UiControl {
        self.react(event)
    }

    fn theme(&self, cell: CellPixels) -> UiTheme {
        self.theme_for(cell)
    }
}

/// Sends messages to a running app.
#[napi]
pub struct Waker {
    inner: UiWaker<Msg>,
}

#[napi]
impl Waker {
    /// Hand `message` to the app's `update`. `false` once the app has quit.
    #[napi]
    pub fn send(&self, env: Env, message: Unknown<'_>) -> Result<bool> {
        Ok(self.inner.send(Msg::Value(hold(&env, message)?)))
    }

    /// Hand `message` to `update` every `seconds` until the app quits.
    #[napi]
    pub fn every(&self, env: Env, seconds: f64, message: Unknown<'_>) -> Result<()> {
        let message = hold(&env, message)?;
        self.inner
            .every(Duration::from_secs_f64(seconds.max(0.001)), move || {
                Msg::Value(Arc::clone(&message))
            });
        Ok(())
    }
}

pub(crate) fn terminal_error(error: &katui_rt::RtError) -> Error {
    Error::new(Status::GenericFailure, error.to_string())
}

/// Run `app` on the terminal until it quits. It blocks until then, and an
/// exception thrown inside the app reaches the caller after the terminal is
/// restored.
#[napi(ts_args_type = "app: AppLike")]
pub fn run(env: Env, app: Object<'_>) -> Result<()> {
    let failure = Failure::default();
    let bridge = Bridge::new(env, app, Rc::clone(&failure))?;
    let finished = katui_app::run(bridge);
    if let Some(error) = take(&failure) {
        return Err(error);
    }
    finished.map_err(|error| terminal_error(&error))
}
