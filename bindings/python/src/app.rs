//! The bridge between katui's app runner and a Python `App`, and `run`.

use std::sync::Arc;

use katui_app::{Control, Waker};
use katui_bindings::app::AppHost;
use katui_bindings::state::interval;
use katui_proto::input::InputEvent;
use katui_ui::CellPixels;
use katui_widgets::Theme;
use pyo3::exceptions::PyTypeError;
use pyo3::prelude::*;
use pyo3::types::PyType;

use crate::convert::raise;
use crate::element::{Node, PyElement};
use crate::event::PyEvent;
use crate::message::{Msg, Obj, obj, resolve};
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

/// A Python app, as the shared bridge calls it.
pub(crate) struct PyApp(pub(crate) Py<PyAny>);

impl AppHost for PyApp {
    type Value = Obj;
    type Error = PyErr;

    fn refused(error: katui_bindings::Error) -> PyErr {
        raise(error)
    }

    fn start(&mut self, waker: Waker<Msg>) -> PyResult<()> {
        Python::attach(|py| {
            call_optional(self.0.bind(py), "start", (PyWaker { inner: waker },)).map(drop)
        })
    }

    fn view(&self) -> PyResult<Node> {
        Python::attach(|py| {
            let view = self.0.bind(py).call_method0("view")?;
            Ok(view.cast::<PyElement>()?.borrow().inner.clone())
        })
    }

    fn update(&mut self, message: &Msg) -> PyResult<Control> {
        Python::attach(|py| {
            let resolved = resolve(py, message)?;
            let returned = self.0.bind(py).call_method1("update", (resolved,))?;
            control(Some(&returned), Control::Redraw)
        })
    }

    fn input(&mut self, event: &InputEvent) -> PyResult<Control> {
        Python::attach(|py| {
            let event = PyEvent {
                inner: event.clone(),
            };
            let returned = call_optional(self.0.bind(py), "input", (event,))?;
            control(returned.as_ref(), Control::Wait)
        })
    }

    fn theme(&self, cell: CellPixels) -> PyResult<Option<Theme>> {
        Python::attach(|py| {
            match call_optional(self.0.bind(py), "theme", ((cell.width, cell.height),))? {
                Some(theme) if !theme.is_none() => Ok(Some(theme.extract::<PyTheme>()?.inner)),
                _ => Ok(None),
            }
        })
    }
}

/// Sends messages to a running app from any thread.
#[pyclass(name = "Waker", module = "katui", frozen)]
pub(crate) struct PyWaker {
    inner: Waker<Msg>,
}

#[pymethods]
impl PyWaker {
    #[classmethod]
    fn __class_getitem__<'py>(
        cls: &Bound<'py, PyType>,
        item: &Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        crate::convert::generic_alias(cls, item)
    }

    /// Hand `message` to the app's `update`. `False` once the app has quit.
    fn send(&self, py: Python<'_>, message: &Bound<'_, PyAny>) -> bool {
        let message = Msg::Value(obj(message));
        py.detach(|| self.inner.send(message))
    }

    /// Hand `message` to `update` every `seconds` until the app quits.
    fn every(&self, seconds: f64, message: &Bound<'_, PyAny>) {
        let message = obj(message);
        self.inner
            .every(interval(seconds), move || Msg::Value(Arc::clone(&message)));
    }
}

/// Run `app` on the terminal until it quits.
#[pyfunction]
pub(crate) fn run(py: Python<'_>, app: Py<PyAny>) -> PyResult<()> {
    py.detach(|| katui_bindings::app::run(PyApp(app)))
}
