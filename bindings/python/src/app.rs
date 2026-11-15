//! The bridge between katui's app runner and a Python `App`, and `run`.

use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use katui_app::{Control, UiApp, Waker};
use katui_proto::input::InputEvent;
use katui_ui::{CellPixels, Element};
use katui_widgets::Theme;
use pyo3::exceptions::{PyOSError, PyTypeError};
use pyo3::prelude::*;

use crate::element::{Node, PyElement};
use crate::message::{Msg, obj};
use crate::state::PyEvent;
use crate::theme::PyTheme;

/// What to do after a message or an event.
#[pyclass(name = "Control", module = "katui", eq, eq_int, frozen, from_py_object)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum PyControl {
    /// Sleep until something happens.
    #[pyo3(name = "WAIT")]
    Wait,
    /// Draw once.
    #[pyo3(name = "REDRAW")]
    Redraw,
    /// Keep drawing at the frame rate.
    #[pyo3(name = "ANIMATE")]
    Animate,
    /// End the app.
    #[pyo3(name = "QUIT")]
    Quit,
}

impl From<PyControl> for Control {
    fn from(control: PyControl) -> Self {
        match control {
            PyControl::Wait => Self::Wait,
            PyControl::Redraw => Self::Redraw,
            PyControl::Animate => Self::Animate,
            PyControl::Quit => Self::Quit,
        }
    }
}

/// The first error Python raised, kept until the runner returns.
pub(crate) type Failure = Arc<Mutex<Option<PyErr>>>;

pub(crate) fn take(failure: &Failure) -> Option<PyErr> {
    failure
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .take()
}

pub(crate) fn keep(failure: &Failure, error: PyErr) {
    failure
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get_or_insert(error);
}

/// Calls `name` on `target` with `args`, or gives `None` when `target` has
/// no such method.
pub(crate) fn call_optional<'py>(
    target: &Bound<'py, PyAny>,
    name: &str,
    args: impl pyo3::call::PyCallArgs<'py>,
) -> PyResult<Option<Bound<'py, PyAny>>> {
    if !target.hasattr(name)? {
        return Ok(None);
    }
    target.call_method1(name, args).map(Some)
}

/// What a Python method returned, as a `Control`: `None` means `default`.
pub(crate) fn control(returned: Option<&Bound<'_, PyAny>>, default: Control) -> PyResult<Control> {
    match returned {
        Some(value) if !value.is_none() => {
            let control: PyControl = value.extract().map_err(|_| {
                PyTypeError::new_err("return a katui.Control, or None for the default")
            })?;
            Ok(control.into())
        }
        _ => Ok(default),
    }
}

/// A Python app, as the runner sees it.
pub(crate) struct Bridge {
    app: Py<PyAny>,
    failure: Failure,
    waker: Option<Waker<Msg>>,
}

impl Bridge {
    pub(crate) fn new(app: Py<PyAny>, failure: Failure) -> Self {
        Self {
            app,
            failure,
            waker: None,
        }
    }

    fn fail(&self, error: PyErr) -> Control {
        keep(&self.failure, error);
        if let Some(waker) = &self.waker {
            waker.send(Msg::Stop);
        }
        Control::Quit
    }

    pub(crate) fn render(&self) -> Node {
        Python::attach(|py| {
            let built = self
                .app
                .bind(py)
                .call_method0("view")
                .and_then(|view| Ok(view.cast::<PyElement>()?.borrow().inner.clone()));
            built.unwrap_or_else(|error| {
                self.fail(error);
                Element::spacer()
            })
        })
    }

    pub(crate) fn theme_for(&self, cell: CellPixels) -> Theme {
        Python::attach(|py| {
            let picked = call_optional(self.app.bind(py), "theme", ((cell.width, cell.height),))
                .and_then(|theme| match theme {
                    Some(theme) if !theme.is_none() => Ok(Some(theme.extract::<PyTheme>()?.inner)),
                    _ => Ok(None),
                });
            match picked {
                Ok(theme) => theme.unwrap_or_else(|| Theme::dark(cell)),
                Err(error) => {
                    self.fail(error);
                    Theme::dark(cell)
                }
            }
        })
    }

    pub(crate) fn deliver(&mut self, message: &Msg) -> Control {
        Python::attach(|py| {
            let outcome = message.resolve(py).and_then(|resolved| match resolved {
                None => Ok(Control::Quit),
                Some(resolved) => {
                    let returned = self.app.bind(py).call_method1("update", (resolved,))?;
                    control(Some(&returned), Control::Redraw)
                }
            });
            outcome.unwrap_or_else(|error| self.fail(error))
        })
    }

    pub(crate) fn react(&mut self, event: &InputEvent) -> Control {
        Python::attach(|py| {
            let event = PyEvent {
                inner: event.clone(),
            };
            let outcome = call_optional(self.app.bind(py), "input", (event,))
                .and_then(|returned| control(returned.as_ref(), Control::Wait));
            outcome.unwrap_or_else(|error| self.fail(error))
        })
    }
}

impl UiApp for Bridge {
    type Message = Msg;

    fn start(&mut self, waker: Waker<Msg>) {
        self.waker = Some(waker.clone());
        Python::attach(|py| {
            if let Err(error) =
                call_optional(self.app.bind(py), "start", (PyWaker { inner: waker },))
            {
                self.fail(error);
            }
        });
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
#[pyclass(name = "Waker", module = "katui", frozen)]
pub(crate) struct PyWaker {
    inner: Waker<Msg>,
}

#[pymethods]
impl PyWaker {
    /// Hand `message` to the app's `update`. `False` once the app has quit.
    fn send(&self, py: Python<'_>, message: &Bound<'_, PyAny>) -> bool {
        let message = Msg::Value(obj(message));
        py.detach(|| self.inner.send(message))
    }

    /// Hand `message` to `update` every `seconds` until the app quits.
    fn every(&self, seconds: f64, message: &Bound<'_, PyAny>) {
        let message = obj(message);
        self.inner
            .every(Duration::from_secs_f64(seconds.max(0.001)), move || {
                Msg::Value(Arc::clone(&message))
            });
    }
}

/// Run `app` on the terminal until it quits.
#[pyfunction]
pub(crate) fn run(py: Python<'_>, app: Py<PyAny>) -> PyResult<()> {
    let failure = Failure::default();
    let bridge = Bridge::new(app, Arc::clone(&failure));
    let finished = py.detach(|| katui_app::run(bridge));
    if let Some(error) = take(&failure) {
        return Err(error);
    }
    finished.map_err(|error| PyOSError::new_err(error.to_string()))
}
