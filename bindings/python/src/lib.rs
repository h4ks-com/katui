//! The native module behind the `katui` Python package.
// pyo3 hands every argument over as an owned value and every method a
// `&self`, so these two pedantic lints flag the shape pyo3 requires
#![allow(clippy::needless_pass_by_value, clippy::trivially_copy_pass_by_ref)]
// other languages call these functions, where `#[must_use]` means nothing
#![allow(clippy::must_use_candidate)]

mod app;
mod convert;
mod element;
mod event;
mod game;
mod html;
mod message;
mod state;
mod testing;
mod theme;
mod widgets;

use pyo3::prelude::*;

/// Adds each class to `module`.
macro_rules! add_classes {
    ($module:ident, $($class:ty),+ $(,)?) => {
        $($module.add_class::<$class>()?;)+
    };
}

/// Adds each `#[pyfunction]` to `module`.
macro_rules! add_functions {
    ($module:ident, $($function:path),+ $(,)?) => {
        $($module.add_function(wrap_pyfunction!($function, $module)?)?;)+
    };
}

#[pymodule]
#[pyo3(name = "_katui")]
fn katui_module(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add("__version__", env!("CARGO_PKG_VERSION"))?;
    add_classes!(
        module,
        app::PyControl,
        app::PyWaker,
        element::PyElement,
        element::PyImage,
        element::PySpan,
        theme::PyTheme,
        event::PyEvent,
        state::PyEdit,
        state::PyScroll,
        state::PyTextField,
        state::PyTextArea,
        state::PyListState,
        state::PyScrollState,
        state::PyTreeNode,
        state::PyTreeState,
        state::PyFocus,
        state::PyToasts,
        widgets::PyColumn,
        game::PyLight,
        game::PyKeys,
        game::PyV3,
        game::PyCamera,
        game::PyRng,
        game::PySpark,
        game::PySparks,
        testing::PyScreen,
    );
    add_functions!(
        module,
        app::run,
        html::anchor_id,
        html::html,
        html::markdown,
        widgets::label,
        widgets::title,
        widgets::heading,
        widgets::hint,
        widgets::button,
        widgets::focusable_button,
        widgets::primary_button,
        widgets::panel,
        widgets::page,
        widgets::text_field,
        widgets::text_area,
        widgets::list_view,
        widgets::checkbox,
        widgets::switch,
        widgets::progress,
        widgets::tabs,
        widgets::radio,
        widgets::slider,
        widgets::nudge,
        widgets::spinner,
        widgets::toasts,
        widgets::key_hints,
        widgets::dialog,
        widgets::select,
        widgets::menu,
        widgets::tooltip,
        widgets::scroll_view,
        widgets::split,
        widgets::table,
        widgets::tree,
        game::srgb,
        game::scaled,
        game::lerp3,
        game::play,
        game::frame,
    );
    for (name, value) in game::GAME_CONSTANTS {
        module.add(name, value)?;
    }
    Ok(())
}
