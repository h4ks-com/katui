//! The argument shapes widgets take, from the owned values hosts hand over.

use katui_ui::Element;
use katui_widgets::ListState;

use crate::failure::catching;

#[must_use]
pub fn strs(items: &[String]) -> Vec<&str> {
    items.iter().map(String::as_str).collect()
}

/// Each key with what it does, for `key_hints`.
#[must_use]
pub fn hints(hints: &[(String, String)]) -> Vec<(&str, &str)> {
    hints
        .iter()
        .map(|(key, action)| (key.as_str(), action.as_str()))
        .collect()
}

/// The rows of `state` in view, each built by `row(index, selected)`, or
/// the first failure of `row`.
pub fn list<M, E>(
    state: &ListState,
    mut row: impl FnMut(usize, bool) -> Result<Element<M>, E>,
) -> Result<Element<M>, E> {
    catching(|catch| {
        katui_widgets::list(state, |index, selected| {
            catch.or(row(index, selected), Element::spacer)
        })
    })
}
