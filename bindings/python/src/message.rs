//! What clicks, drags and timers carry back to Python: the object the app
//! handed over, or a callable and the value to call it with.

use std::sync::Arc;

use katui_ui::Scroll;
use pyo3::prelude::*;

use crate::state::PyScroll;

/// A Python object we can clone without holding the interpreter.
pub(crate) type Obj = Arc<Py<PyAny>>;

pub(crate) fn obj(value: &Bound<'_, PyAny>) -> Obj {
    Arc::new(value.clone().unbind())
}

#[derive(Clone)]
pub(crate) enum Msg {
    Value(Obj),
    Index(Obj, usize),
    Id(Obj, u64),
    Float(Obj, f32),
    Point(Obj, f32, f32),
    Scroll(Obj, Scroll),
    Link(Obj, String),
    /// Python raised while we drew, so the app has to end.
    Stop,
}

impl Msg {
    /// The message as the app's `update` receives it.
    pub(crate) fn resolve(&self, py: Python<'_>) -> PyResult<Option<Py<PyAny>>> {
        let called = match self {
            Self::Value(value) => return Ok(Some(value.clone_ref(py))),
            Self::Stop => return Ok(None),
            Self::Index(make, index) => make.call1(py, (*index,)),
            Self::Id(make, id) => make.call1(py, (*id,)),
            Self::Float(make, value) => make.call1(py, (*value,)),
            Self::Point(make, x, y) => make.call1(py, (*x, *y)),
            Self::Scroll(make, scroll) => make.call1(py, (PyScroll::from(*scroll),)),
            Self::Link(make, href) => make.call1(py, (href.as_str(),)),
        };
        called.map(Some)
    }
}
