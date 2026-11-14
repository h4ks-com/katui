//! A table: a header row and the rows of a list, cut into columns.

use katui_ui::{Align, Element, Length};

use crate::list::ListState;
use crate::theme::Theme;
use crate::view::label;

/// One column of a [`table`]: its title and its width in cells. A column
/// without a width takes the space the others leave.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Column<'a> {
    /// The header text.
    pub title: &'a str,
    /// The width in cells, or `None` to fill the rest.
    pub cells: Option<f32>,
}

impl<'a> Column<'a> {
    /// A column `cells` wide.
    #[must_use]
    pub const fn fixed(title: &'a str, cells: f32) -> Self {
        Self {
            title,
            cells: Some(cells),
        }
    }

    /// A column taking the space the others leave.
    #[must_use]
    pub const fn fill(title: &'a str) -> Self {
        Self { title, cells: None }
    }
}

/// The rows of `state` in view under a header of `columns`, each row's
/// cells from `rows`, the selected row highlighted. A click on a row sends
/// `pick(index)`.
pub fn table<M, S: AsRef<str>>(
    columns: &[Column<'_>],
    rows: &[Vec<S>],
    state: &ListState,
    theme: &Theme,
    pick: impl Fn(usize) -> M,
) -> Element<M> {
    let header = line(
        columns,
        columns.iter().map(|column| column.title),
        theme,
        true,
    );
    let divider = Element::row([])
        .size(Length::Fill, Length::Px(1.0))
        .background(theme.border.clone());
    let body = state.in_view().filter_map(|index| {
        let cells = rows.get(index)?;
        let row = line(columns, cells.iter().map(AsRef::as_ref), theme, false)
            .radius(theme.cell_height() * 0.3)
            .on_click(pick(index));
        Some(if index == state.selected() {
            row.background(theme.highlight.clone())
        } else {
            row
        })
    });
    let mut all = vec![header, divider];
    all.extend(body);
    Element::column(all)
        .gap(theme.cell_height() * 0.1)
        .width(Length::Fill)
}

fn line<'a, M>(
    columns: &[Column<'_>],
    cells: impl Iterator<Item = &'a str>,
    theme: &Theme,
    header: bool,
) -> Element<M> {
    let (color, weight) = if header {
        (theme.muted, 700)
    } else {
        (theme.text, 400)
    };
    let parts = columns.iter().zip(cells).map(|(column, text)| {
        let cell = Element::row([label(text, theme.body(), color).weight(weight)]);
        match column.cells {
            Some(cells) => cell.width(theme.columns(cells)),
            None => cell.width(Length::Fill),
        }
    });
    Element::row(parts)
        .gap(theme.cell_width())
        .align(Align::Center)
        .padding_xy(theme.cell_width(), theme.cell_height() * 0.2)
        .width(Length::Fill)
}
