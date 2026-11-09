//! Widgets over katui-ui: a theme sized from the terminal's cell, the
//! state of text fields, focus and lists, and the views that draw them.
//!
//! State types take input events and report what changed; view functions
//! turn state into [`katui_ui::Element`] trees the app places in its own.

mod field;
mod focus;
mod list;
mod theme;
mod view;

pub use field::{Edit, TextField};
pub use focus::Focus;
pub use list::ListState;
pub use theme::Theme;
pub use view::{Emphasis, button, checkbox, label, list, panel, progress, tabs, text_field};
