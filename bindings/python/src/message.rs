//! What clicks, drags and timers carry back to Python: the object the app
//! handed over, or a callable and the value to call it with.

use std::sync::Arc;

use pyo3::prelude::*;

use crate::state::PyScroll;

/// A Python object we can clone without holding the interpreter.
pub(crate) type Obj = Arc<Py<PyAny>>;

pub(crate) type Msg = katui_bindings::Msg<Obj>;

pub(crate) fn obj(value: &Bound<'_, PyAny>) -> Obj {
    Arc::new(value.clone().unbind())
}

pub(crate) fn value(message: &Bound<'_, PyAny>) -> Msg {
    Msg::Value(obj(message))
}

/// The message as the app's `update` receives it.
pub(crate) fn resolve(py: Python<'_>, message: &Msg) -> PyResult<Py<PyAny>> {
    match message {
        Msg::Value(value) => Ok(value.clone_ref(py)),
        Msg::Href(href) => Ok(href.into_pyobject(py)?.into_any().unbind()),
        Msg::Stop => Ok(py.None()),
        Msg::Index(make, index) => make.call1(py, (*index,)),
        Msg::Id(make, id) => make.call1(py, (*id,)),
        Msg::Float(make, value) => make.call1(py, (*value,)),
        Msg::Point(make, x, y) => make.call1(py, (*x, *y)),
        Msg::Scroll(make, scroll) => make.call1(py, (PyScroll::from(*scroll),)),
        Msg::Link(make, href) => make.call1(py, (href.as_str(),)),
    }
}
