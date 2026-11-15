//! Widgets over katui-ui: a theme sized from the terminal's cell, the
//! state of text fields, text areas, focus, lists, scroll views and trees,
//! and the views that draw them.
//!
//! State types take input events and report what changed; view functions
//! turn state into [`katui_ui::Element`] trees the app places in its own.

mod area;
mod controls;
mod feedback;
mod field;
mod focus;
mod list;
mod popup;
mod scroll;
mod split;
mod table;
mod theme;
mod tree;
mod view;

pub use area::{TextArea, text_area};
pub use controls::{nudge, radio, slider, switch};
pub use feedback::{Toasts, key_hints, spinner, toasts};
pub use field::{Edit, TextField};
pub use focus::Focus;
pub use katui_ui::Scroll;
pub use list::ListState;
pub use popup::{dialog, menu, select, tooltip};
pub use scroll::{ScrollState, scroll_view};
pub use split::{Orientation, split};
pub use table::{Column, table};
pub use theme::Theme;
pub use tree::{TreeNode, TreeState, tree};
pub use view::{
    button, checkbox, heading, hint, label, list, page, panel, primary_button, progress, tabs,
    text_field, title,
};
