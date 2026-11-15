//! `KatuiTheme`: the colors and cell-based sizes every widget draws with.

use katui_draw::Paint;
use katui_ui::CellPixels;
use katui_widgets::Theme;

use crate::element::{KatuiColor, paint_color};
use crate::ffi::{borrow, borrow_mut, free, owned};

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
pub unsafe extern "C" fn katui_theme_scaled(
    theme: *const KatuiTheme,
    factor: f32,
) -> *mut KatuiTheme {
    unsafe { borrow(theme) }.map_or(std::ptr::null_mut(), |theme| {
        owned(KatuiTheme(theme.0.clone().scaled(factor)))
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_theme_free(theme: *mut KatuiTheme) {
    unsafe { free(theme) };
}

/// Write the cell size the sizes follow into `width` and `height`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_theme_cell(
    theme: *const KatuiTheme,
    width: *mut u16,
    height: *mut u16,
) {
    let cell = unsafe { borrow(theme) }.map_or(cell(0, 0), |theme| theme.0.cell);
    if let Some(width) = unsafe { borrow_mut(width) } {
        *width = cell.width;
    }
    if let Some(height) = unsafe { borrow_mut(height) } {
        *height = cell.height;
    }
}

/// # Safety
/// `theme` is NULL or a live theme.
unsafe fn size(theme: *const KatuiTheme, of: impl FnOnce(&Theme) -> f32) -> f32 {
    unsafe { borrow(theme) }.map_or(0.0, |theme| of(&theme.0))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_theme_cell_width(theme: *const KatuiTheme) -> f32 {
    unsafe { size(theme, Theme::cell_width) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_theme_cell_height(theme: *const KatuiTheme) -> f32 {
    unsafe { size(theme, Theme::cell_height) }
}

/// The body text size, in pixels.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_theme_body(theme: *const KatuiTheme) -> f32 {
    unsafe { size(theme, Theme::body) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_theme_small(theme: *const KatuiTheme) -> f32 {
    unsafe { size(theme, Theme::small) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_theme_heading(theme: *const KatuiTheme) -> f32 {
    unsafe { size(theme, Theme::heading) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_theme_title(theme: *const KatuiTheme) -> f32 {
    unsafe { size(theme, Theme::title) }
}

/// The corner radius, in pixels.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_theme_radius(theme: *const KatuiTheme) -> f32 {
    unsafe { size(theme, Theme::radius) }
}

/// `count` cells across, in pixels.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_theme_columns(theme: *const KatuiTheme, count: f32) -> f32 {
    unsafe { size(theme, |theme| theme.cell_width() * count) }
}

/// `count` cells down, in pixels.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_theme_rows(theme: *const KatuiTheme, count: f32) -> f32 {
    unsafe { size(theme, |theme| theme.cell_height() * count) }
}

/// The accent color while `active`, the border color otherwise.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_theme_outline(theme: *const KatuiTheme, active: bool) -> KatuiColor {
    unsafe { borrow(theme) }.map_or_else(KatuiColor::default, |theme| {
        paint_color(&theme.0.outline(active))
    })
}

/// # Safety
/// `theme` is NULL or a live theme.
unsafe fn read(theme: *const KatuiTheme, of: impl FnOnce(&Theme) -> KatuiColor) -> KatuiColor {
    unsafe { borrow(theme) }.map_or_else(KatuiColor::default, |theme| of(&theme.0))
}

/// # Safety
/// `theme` is NULL or a live theme nothing else uses now.
unsafe fn write(theme: *mut KatuiTheme, set: impl FnOnce(&mut Theme)) {
    if let Some(theme) = unsafe { borrow_mut(theme) } {
        set(&mut theme.0);
    }
}

/// The page behind everything.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_theme_background(theme: *const KatuiTheme) -> KatuiColor {
    unsafe { read(theme, |theme| paint_color(&theme.background)) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_theme_set_background(theme: *mut KatuiTheme, color: KatuiColor) {
    unsafe { write(theme, |theme| theme.background = Paint::Solid(color.into())) }
}

/// Panels and fields.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_theme_raised(theme: *const KatuiTheme) -> KatuiColor {
    unsafe { read(theme, |theme| paint_color(&theme.raised)) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_theme_set_raised(theme: *mut KatuiTheme, color: KatuiColor) {
    unsafe { write(theme, |theme| theme.raised = Paint::Solid(color.into())) }
}

/// Hovered or selected rows.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_theme_highlight(theme: *const KatuiTheme) -> KatuiColor {
    unsafe { read(theme, |theme| paint_color(&theme.highlight)) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_theme_set_highlight(theme: *mut KatuiTheme, color: KatuiColor) {
    unsafe { write(theme, |theme| theme.highlight = Paint::Solid(color.into())) }
}

/// Hairline borders.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_theme_border(theme: *const KatuiTheme) -> KatuiColor {
    unsafe { read(theme, |theme| paint_color(&theme.border)) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_theme_set_border(theme: *mut KatuiTheme, color: KatuiColor) {
    unsafe { write(theme, |theme| theme.border = Paint::Solid(color.into())) }
}

/// Body text.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_theme_text(theme: *const KatuiTheme) -> KatuiColor {
    unsafe { read(theme, |theme| theme.text.into()) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_theme_set_text(theme: *mut KatuiTheme, color: KatuiColor) {
    unsafe { write(theme, |theme| theme.text = color.into()) }
}

/// Hints, placeholders and secondary text.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_theme_muted(theme: *const KatuiTheme) -> KatuiColor {
    unsafe { read(theme, |theme| theme.muted.into()) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_theme_set_muted(theme: *mut KatuiTheme, color: KatuiColor) {
    unsafe { write(theme, |theme| theme.muted = color.into()) }
}

/// The color of the action that matters most.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_theme_accent(theme: *const KatuiTheme) -> KatuiColor {
    unsafe { read(theme, |theme| theme.accent.into()) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_theme_set_accent(theme: *mut KatuiTheme, color: KatuiColor) {
    unsafe { write(theme, |theme| theme.accent = color.into()) }
}

/// Text on the accent color.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_theme_on_accent(theme: *const KatuiTheme) -> KatuiColor {
    unsafe { read(theme, |theme| theme.on_accent.into()) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_theme_set_on_accent(theme: *mut KatuiTheme, color: KatuiColor) {
    unsafe { write(theme, |theme| theme.on_accent = color.into()) }
}

/// Errors.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_theme_danger(theme: *const KatuiTheme) -> KatuiColor {
    unsafe { read(theme, |theme| theme.danger.into()) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_theme_set_danger(theme: *mut KatuiTheme, color: KatuiColor) {
    unsafe { write(theme, |theme| theme.danger = color.into()) }
}
