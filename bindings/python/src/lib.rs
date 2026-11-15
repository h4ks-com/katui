//! The native module behind the `katui` Python package.
#![cfg(unix)]
// pyo3 hands every argument over as an owned value and every method a
// `&self`, so these two pedantic lints flag the shape pyo3 requires
#![allow(clippy::needless_pass_by_value, clippy::trivially_copy_pass_by_ref)]

mod app;
mod convert;
mod element;
mod game;
mod html;
mod message;
mod state;
mod testing;
mod theme;
mod widgets;

use pyo3::prelude::*;

#[pymodule]
fn _katui(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add("__version__", env!("CARGO_PKG_VERSION"))?;
    module.add_class::<app::PyControl>()?;
    module.add_class::<app::PyWaker>()?;
    module.add_function(wrap_pyfunction!(app::run, module)?)?;

    module.add_class::<element::PyElement>()?;
    module.add_class::<element::PyImage>()?;
    module.add_class::<element::PySpan>()?;
    module.add_class::<theme::PyTheme>()?;
    module.add_function(wrap_pyfunction!(html::html, module)?)?;
    module.add_function(wrap_pyfunction!(html::markdown, module)?)?;

    module.add_class::<state::PyEvent>()?;
    module.add_class::<state::PyEdit>()?;
    module.add_class::<state::PyScroll>()?;
    module.add_class::<state::PyTextField>()?;
    module.add_class::<state::PyTextArea>()?;
    module.add_class::<state::PyListState>()?;
    module.add_class::<state::PyScrollState>()?;
    module.add_class::<state::PyTreeNode>()?;
    module.add_class::<state::PyTreeState>()?;
    module.add_class::<state::PyFocus>()?;
    module.add_class::<state::PyToasts>()?;
    module.add_class::<widgets::PyColumn>()?;
    for function in [
        wrap_pyfunction!(widgets::label, module)?,
        wrap_pyfunction!(widgets::title, module)?,
        wrap_pyfunction!(widgets::heading, module)?,
        wrap_pyfunction!(widgets::hint, module)?,
        wrap_pyfunction!(widgets::button, module)?,
        wrap_pyfunction!(widgets::primary_button, module)?,
        wrap_pyfunction!(widgets::panel, module)?,
        wrap_pyfunction!(widgets::page, module)?,
        wrap_pyfunction!(widgets::text_field, module)?,
        wrap_pyfunction!(widgets::text_area, module)?,
        wrap_pyfunction!(widgets::list_view, module)?,
        wrap_pyfunction!(widgets::checkbox, module)?,
        wrap_pyfunction!(widgets::switch, module)?,
        wrap_pyfunction!(widgets::progress, module)?,
        wrap_pyfunction!(widgets::tabs, module)?,
        wrap_pyfunction!(widgets::radio, module)?,
        wrap_pyfunction!(widgets::slider, module)?,
        wrap_pyfunction!(widgets::nudge, module)?,
        wrap_pyfunction!(widgets::spinner, module)?,
        wrap_pyfunction!(widgets::toasts, module)?,
        wrap_pyfunction!(widgets::key_hints, module)?,
        wrap_pyfunction!(widgets::dialog, module)?,
        wrap_pyfunction!(widgets::select, module)?,
        wrap_pyfunction!(widgets::menu, module)?,
        wrap_pyfunction!(widgets::tooltip, module)?,
        wrap_pyfunction!(widgets::scroll_view, module)?,
        wrap_pyfunction!(widgets::split, module)?,
        wrap_pyfunction!(widgets::table, module)?,
        wrap_pyfunction!(widgets::tree, module)?,
    ] {
        module.add_function(function)?;
    }

    module.add_class::<game::PyLight>()?;
    module.add_class::<game::PyKeys>()?;
    module.add_class::<game::PyV3>()?;
    module.add_class::<game::PyCamera>()?;
    module.add_class::<game::PyRng>()?;
    module.add_class::<game::PySpark>()?;
    module.add_class::<game::PySparks>()?;
    module.add_function(wrap_pyfunction!(game::srgb, module)?)?;
    module.add_function(wrap_pyfunction!(game::scaled, module)?)?;
    module.add_function(wrap_pyfunction!(game::lerp3, module)?)?;
    module.add_function(wrap_pyfunction!(game::play, module)?)?;
    module.add_function(wrap_pyfunction!(game::frame, module)?)?;
    for (name, value) in game::GAME_CONSTANTS {
        module.add(name, value)?;
    }

    module.add_class::<testing::PyScreen>()?;
    Ok(())
}
