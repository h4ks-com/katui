//! The bridge between katui's app runner and a JavaScript app, and `run`.

use std::sync::Arc;

use katui_app::{Control as UiControl, Waker as UiWaker};
use katui_bindings::app::AppHost;
use katui_bindings::state::interval;
use katui_proto::input::InputEvent;
use katui_ui::CellPixels;
use katui_widgets::Theme as UiTheme;
use napi::bindgen_prelude::{Function, JsObjectValue, JsValuesTupleIntoVec, Object, Unknown};
use napi::{Env, Error, JsValue, Result, ValueType};

use crate::convert::{function, instance, invalid, is_nothing, raise};
use crate::element::{Node, node};
use crate::event::Event;
use crate::message::{Msg, Obj, fetch, hold, resolve};
use crate::reclaim::{collector, reclaim};
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

/// A JavaScript app, as the shared bridge calls it.
pub(crate) struct NodeApp {
    env: Env,
    app: Obj,
    collect: Obj,
}

impl NodeApp {
    pub(crate) fn new(env: Env, app: Object<'_>) -> Result<Self> {
        Ok(Self {
            env,
            app: hold(&env, app.to_unknown())?,
            collect: collector(&env)?,
        })
    }

    /// Runs `body` in a handle scope of its own, so the values it makes are
    /// freed when it returns.
    fn scoped<T>(&self, body: impl FnOnce(&Env, Object<'_>) -> Result<T>) -> Result<T> {
        let env = self.env;
        env.run_in_scope(|| body(&env, object(&env, &self.app)?))
    }
}

impl AppHost for NodeApp {
    type Value = Obj;
    type Error = Error;

    fn refused(error: katui_bindings::Error) -> Error {
        raise(error)
    }

    fn start(&mut self, waker: UiWaker<Msg>) -> Result<()> {
        self.scoped(|_, app| call_optional(&app, "start", Waker { inner: waker }).map(drop))
    }

    fn view(&self) -> Result<Node> {
        self.scoped(|env, app| {
            reclaim(env, &self.collect)?;
            let view = call_optional(&app, "view", ())?
                .ok_or_else(|| invalid("give the app a view method"))?;
            node(env, view)
        })
    }

    fn update(&mut self, message: &Msg) -> Result<UiControl> {
        self.scoped(|env, app| match resolve(env, message)? {
            None => Ok(UiControl::Quit),
            Some(resolved) => match call_optional(&app, "update", resolved)? {
                Some(returned) => control(Some(returned), UiControl::Redraw),
                None => default_update(resolved),
            },
        })
    }

    fn input(&mut self, event: &InputEvent) -> Result<UiControl> {
        self.scoped(|_, app| {
            let returned = call_optional(&app, "input", Event::new(event.clone()))?;
            control(returned, UiControl::Wait)
        })
    }

    fn theme(&self, cell: CellPixels) -> Result<Option<UiTheme>> {
        self.scoped(
            |env, app| match call_optional(&app, "theme", (cell.width, cell.height))? {
                Some(theme) if !is_nothing(&theme)? => {
                    let theme = instance::<Theme>(env, theme, "return a Theme from theme")?;
                    Ok(Some(theme.inner.clone()))
                }
                _ => Ok(None),
            },
        )
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
            .every(interval(seconds), move || Msg::Value(Arc::clone(&message)));
        Ok(())
    }
}

/// Run `app` on the terminal until it quits. It blocks until then, and an
/// exception thrown inside the app reaches the caller after the terminal is
/// restored.
#[napi(ts_args_type = "app: AppLike")]
pub fn run(env: Env, app: Object<'_>) -> Result<()> {
    katui_bindings::app::run(NodeApp::new(env, app)?)
}
