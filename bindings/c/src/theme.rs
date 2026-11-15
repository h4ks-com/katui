//! `KatuiTheme`: the colors and cell-based sizes every widget draws with.

use katui_draw::Paint;
use katui_ui::CellPixels;
use katui_widgets::Theme;

use crate::element::{KatuiColor, paint_color};
use crate::ffi::owned;

/// The colors and sizes every widget draws with. A gradient color reads as
/// transparent.
pub struct KatuiTheme(pub(crate) Theme);

pub(crate) fn cell(width: u16, height: u16) -> CellPixels {
    CellPixels { width, height }
}

/// The dark theme for cells of `cell_width` by `cell_height` pixels.
#[unsafe(no_mangle)]
pub extern "C" fn katui_theme_dark(cell_width: u16, cell_height: u16) -> *mut KatuiTheme {
    owned(KatuiTheme(Theme::dark(cell(cell_width, cell_height))))
}

/// The light theme for cells of `cell_width` by `cell_height` pixels.
#[unsafe(no_mangle)]
pub extern "C" fn katui_theme_light(cell_width: u16, cell_height: u16) -> *mut KatuiTheme {
    owned(KatuiTheme(Theme::light(cell(cell_width, cell_height))))
}

/// A copy of the theme the widgets draw with right now: the app's, inside
/// a view.
#[unsafe(no_mangle)]
pub extern "C" fn katui_theme_current() -> *mut KatuiTheme {
    owned(KatuiTheme(Theme::current().as_ref().clone()))
}

/// A new theme, `theme` with every size times `factor`.
#[unsafe(no_mangle)]
pub extern "C" fn katui_theme_scaled(theme: Option<&KatuiTheme>, factor: f32) -> *mut KatuiTheme {
    theme.map_or(std::ptr::null_mut(), |theme| {
        owned(KatuiTheme(theme.0.clone().scaled(factor)))
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn katui_theme_free(theme: Option<Box<KatuiTheme>>) {
    drop(theme);
}

/// Write the cell size the sizes follow into `width` and `height`.
#[unsafe(no_mangle)]
pub extern "C" fn katui_theme_cell(
    theme: Option<&KatuiTheme>,
    width: Option<&mut u16>,
    height: Option<&mut u16>,
) {
    let cell = theme.map_or(cell(0, 0), |theme| theme.0.cell);
    if let Some(width) = width {
        *width = cell.width;
    }
    if let Some(height) = height {
        *height = cell.height;
    }
}

fn size(theme: Option<&KatuiTheme>, of: impl FnOnce(&Theme) -> f32) -> f32 {
    theme.map_or(0.0, |theme| of(&theme.0))
}

#[unsafe(no_mangle)]
pub extern "C" fn katui_theme_cell_width(theme: Option<&KatuiTheme>) -> f32 {
    size(theme, Theme::cell_width)
}

#[unsafe(no_mangle)]
pub extern "C" fn katui_theme_cell_height(theme: Option<&KatuiTheme>) -> f32 {
    size(theme, Theme::cell_height)
}

/// The body text size, in pixels.
#[unsafe(no_mangle)]
pub extern "C" fn katui_theme_body(theme: Option<&KatuiTheme>) -> f32 {
    size(theme, Theme::body)
}

#[unsafe(no_mangle)]
pub extern "C" fn katui_theme_small(theme: Option<&KatuiTheme>) -> f32 {
    size(theme, Theme::small)
}

#[unsafe(no_mangle)]
pub extern "C" fn katui_theme_heading(theme: Option<&KatuiTheme>) -> f32 {
    size(theme, Theme::heading)
}

#[unsafe(no_mangle)]
pub extern "C" fn katui_theme_title(theme: Option<&KatuiTheme>) -> f32 {
    size(theme, Theme::title)
}

/// The corner radius, in pixels.
#[unsafe(no_mangle)]
pub extern "C" fn katui_theme_radius(theme: Option<&KatuiTheme>) -> f32 {
    size(theme, Theme::radius)
}

/// `count` cells across, in pixels.
#[unsafe(no_mangle)]
pub extern "C" fn katui_theme_columns(theme: Option<&KatuiTheme>, count: f32) -> f32 {
    size(theme, |theme| theme.cell_width() * count)
}

/// `count` cells down, in pixels.
#[unsafe(no_mangle)]
pub extern "C" fn katui_theme_rows(theme: Option<&KatuiTheme>, count: f32) -> f32 {
    size(theme, |theme| theme.cell_height() * count)
}

/// The accent color while `active`, the border color otherwise.
#[unsafe(no_mangle)]
pub extern "C" fn katui_theme_outline(theme: Option<&KatuiTheme>, active: bool) -> KatuiColor {
    theme.map_or_else(KatuiColor::default, |theme| {
        paint_color(&theme.0.outline(active))
    })
}

fn read(theme: Option<&KatuiTheme>, of: impl FnOnce(&Theme) -> KatuiColor) -> KatuiColor {
    theme.map_or_else(KatuiColor::default, |theme| of(&theme.0))
}

fn write(theme: Option<&mut KatuiTheme>, set: impl FnOnce(&mut Theme)) {
    if let Some(theme) = theme {
        set(&mut theme.0);
    }
}

/// The page behind everything.
#[unsafe(no_mangle)]
pub extern "C" fn katui_theme_background(theme: Option<&KatuiTheme>) -> KatuiColor {
    read(theme, |theme| paint_color(&theme.background))
}

#[unsafe(no_mangle)]
pub extern "C" fn katui_theme_set_background(theme: Option<&mut KatuiTheme>, color: KatuiColor) {
    write(theme, |theme| theme.background = Paint::Solid(color.into()));
}

/// Panels and fields.
#[unsafe(no_mangle)]
pub extern "C" fn katui_theme_raised(theme: Option<&KatuiTheme>) -> KatuiColor {
    read(theme, |theme| paint_color(&theme.raised))
}

#[unsafe(no_mangle)]
pub extern "C" fn katui_theme_set_raised(theme: Option<&mut KatuiTheme>, color: KatuiColor) {
    write(theme, |theme| theme.raised = Paint::Solid(color.into()));
}

/// Hovered or selected rows.
#[unsafe(no_mangle)]
pub extern "C" fn katui_theme_highlight(theme: Option<&KatuiTheme>) -> KatuiColor {
    read(theme, |theme| paint_color(&theme.highlight))
}

#[unsafe(no_mangle)]
pub extern "C" fn katui_theme_set_highlight(theme: Option<&mut KatuiTheme>, color: KatuiColor) {
    write(theme, |theme| theme.highlight = Paint::Solid(color.into()));
}

/// Hairline borders.
#[unsafe(no_mangle)]
pub extern "C" fn katui_theme_border(theme: Option<&KatuiTheme>) -> KatuiColor {
    read(theme, |theme| paint_color(&theme.border))
}

#[unsafe(no_mangle)]
pub extern "C" fn katui_theme_set_border(theme: Option<&mut KatuiTheme>, color: KatuiColor) {
    write(theme, |theme| theme.border = Paint::Solid(color.into()));
}

/// Body text.
#[unsafe(no_mangle)]
pub extern "C" fn katui_theme_text(theme: Option<&KatuiTheme>) -> KatuiColor {
    read(theme, |theme| theme.text.into())
}

#[unsafe(no_mangle)]
pub extern "C" fn katui_theme_set_text(theme: Option<&mut KatuiTheme>, color: KatuiColor) {
    write(theme, |theme| theme.text = color.into());
}

/// Hints, placeholders and secondary text.
#[unsafe(no_mangle)]
pub extern "C" fn katui_theme_muted(theme: Option<&KatuiTheme>) -> KatuiColor {
    read(theme, |theme| theme.muted.into())
}

#[unsafe(no_mangle)]
pub extern "C" fn katui_theme_set_muted(theme: Option<&mut KatuiTheme>, color: KatuiColor) {
    write(theme, |theme| theme.muted = color.into());
}

/// The color of the action that matters most.
#[unsafe(no_mangle)]
pub extern "C" fn katui_theme_accent(theme: Option<&KatuiTheme>) -> KatuiColor {
    read(theme, |theme| theme.accent.into())
}

#[unsafe(no_mangle)]
pub extern "C" fn katui_theme_set_accent(theme: Option<&mut KatuiTheme>, color: KatuiColor) {
    write(theme, |theme| theme.accent = color.into());
}

/// Text on the accent color.
#[unsafe(no_mangle)]
pub extern "C" fn katui_theme_on_accent(theme: Option<&KatuiTheme>) -> KatuiColor {
    read(theme, |theme| theme.on_accent.into())
}

#[unsafe(no_mangle)]
pub extern "C" fn katui_theme_set_on_accent(theme: Option<&mut KatuiTheme>, color: KatuiColor) {
    write(theme, |theme| theme.on_accent = color.into());
}

/// Errors.
#[unsafe(no_mangle)]
pub extern "C" fn katui_theme_danger(theme: Option<&KatuiTheme>) -> KatuiColor {
    read(theme, |theme| theme.danger.into())
}

#[unsafe(no_mangle)]
pub extern "C" fn katui_theme_set_danger(theme: Option<&mut KatuiTheme>, color: KatuiColor) {
    write(theme, |theme| theme.danger = color.into());
}
