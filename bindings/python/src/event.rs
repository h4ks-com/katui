//! `Event`: the key presses, mouse events, pastes and focus changes apps
//! and widgets read.

use katui_bindings::event;
use katui_proto::input::{Button, InputEvent, Key, Mods, MouseAction};
use pyo3::prelude::*;

use crate::convert::raise;

/// A key press, a release, a mouse event, a paste or a focus change.
#[pyclass(name = "Event", module = "katui", frozen)]
pub(crate) struct PyEvent {
    pub(crate) inner: InputEvent,
}

/// The key named `name`: one character, `"enter"`, `"up"`, `"f1"` and so on.
pub(crate) fn key(name: &str) -> PyResult<Key> {
    event::key(name).map_err(raise)
}

#[pymethods]
impl PyEvent {
    /// A press of the key `name`, for tests and for handing keys to widgets.
    #[staticmethod]
    #[pyo3(signature = (name, ctrl=false, alt=false, shift=false, super_key=false, hyper=false, meta=false))]
    #[allow(clippy::fn_params_excessive_bools)]
    fn press(
        name: &str,
        ctrl: bool,
        alt: bool,
        shift: bool,
        super_key: bool,
        hyper: bool,
        meta: bool,
    ) -> PyResult<Self> {
        let mods = Mods {
            shift,
            alt,
            ctrl,
            super_key,
            hyper,
            meta,
        };
        let inner = event::press(name, mods).map_err(raise)?;
        Ok(Self { inner })
    }

    #[staticmethod]
    fn paste(text: String) -> Self {
        Self {
            inner: InputEvent::Paste(text),
        }
    }

    /// `"key"`, `"key_repeat"`, `"key_up"`, `"mouse"`, `"paste"`, `"focus"`
    /// or `"other"`.
    #[getter]
    fn kind(&self) -> &'static str {
        event::kind(&self.inner).name()
    }

    /// The key's name: one character, `"enter"`, `"up"`, `"f1"` and so on.
    #[getter(key)]
    fn key_of(&self) -> Option<String> {
        event::key_name(&self.inner)
    }

    #[getter]
    fn ctrl(&self) -> bool {
        event::mods(&self.inner).ctrl
    }

    #[getter]
    fn alt(&self) -> bool {
        event::mods(&self.inner).alt
    }

    #[getter]
    fn shift(&self) -> bool {
        event::mods(&self.inner).shift
    }

    #[getter]
    fn super_key(&self) -> bool {
        event::mods(&self.inner).super_key
    }

    #[getter]
    fn hyper(&self) -> bool {
        event::mods(&self.inner).hyper
    }

    #[getter]
    fn meta(&self) -> bool {
        event::mods(&self.inner).meta
    }

    /// The pasted text.
    #[getter]
    fn text(&self) -> Option<String> {
        event::pasted(&self.inner).map(str::to_owned)
    }

    /// Whether the window has the focus, for a focus event.
    #[getter]
    fn focused(&self) -> Option<bool> {
        event::focused(&self.inner)
    }

    /// The cell under the pointer, `(column, row)`, for a mouse event.
    #[getter]
    fn position(&self) -> Option<(u32, u32)> {
        event::position(&self.inner)
    }

    /// `"press"`, `"release"`, `"drag"`, `"move"`, `"wheel_up"`,
    /// `"wheel_down"`, `"wheel_left"` or `"wheel_right"`, for a mouse event.
    #[getter]
    fn action(&self) -> Option<&'static str> {
        event::action(&self.inner).map(MouseAction::name)
    }

    /// `"left"`, `"middle"`, `"right"`, `"back"` or `"forward"`, for a press,
    /// release or drag.
    #[getter]
    fn button(&self) -> Option<&'static str> {
        event::button(&self.inner).map(Button::name)
    }

    fn __repr__(&self) -> String {
        format!("Event({:?})", self.inner)
    }
}
