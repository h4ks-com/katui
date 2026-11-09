//! The theme: semantic colors plus sizes that follow the terminal's cell,
//! so type and spacing stay in proportion at every font size.

use katui_draw::{Paint, Rgba};
use katui_ui::{CellPixels, Length};

/// The colors and sizes every widget draws with.
#[derive(Debug, Clone)]
pub struct Theme {
    /// The cell size the sizes follow.
    pub cell: CellPixels,
    /// The page behind everything.
    pub background: Paint,
    /// Panels and fields.
    pub raised: Paint,
    /// Hovered or selected rows.
    pub highlight: Paint,
    /// Hairline borders.
    pub border: Paint,
    /// Body text.
    pub text: Rgba,
    /// Hints, placeholders and secondary text.
    pub muted: Rgba,
    /// The color of the action that matters most.
    pub accent: Rgba,
    /// Text on the accent color.
    pub on_accent: Rgba,
    /// Errors.
    pub danger: Rgba,
}

impl Theme {
    /// The dark theme at `cell`.
    #[must_use]
    pub fn dark(cell: CellPixels) -> Self {
        Self {
            cell,
            background: Paint::Solid(Rgba::solid(24, 22, 33)),
            raised: Paint::Solid(Rgba::solid(38, 35, 52)),
            highlight: Paint::Solid(Rgba::solid(56, 52, 78)),
            border: Paint::Solid(Rgba::solid(70, 66, 92)),
            text: Rgba::solid(236, 233, 245),
            muted: Rgba::solid(150, 145, 170),
            accent: Rgba::solid(130, 170, 255),
            on_accent: Rgba::solid(16, 18, 30),
            danger: Rgba::solid(255, 110, 110),
        }
    }

    /// The width of one cell, in pixels.
    #[must_use]
    pub fn cell_width(&self) -> f32 {
        f32::from(self.cell.width)
    }

    /// The height of one cell, in pixels.
    #[must_use]
    pub fn cell_height(&self) -> f32 {
        f32::from(self.cell.height)
    }

    /// Body text size.
    #[must_use]
    pub fn body(&self) -> f32 {
        (self.cell_height() * 0.78).max(11.0)
    }

    /// Small text size, for hints and labels.
    #[must_use]
    pub fn small(&self) -> f32 {
        (self.cell_height() * 0.66).max(10.0)
    }

    /// Title text size.
    #[must_use]
    pub fn title(&self) -> f32 {
        (self.cell_height() * 1.5).max(18.0)
    }

    /// The corner radius of fields and buttons.
    #[must_use]
    pub fn radius(&self) -> f32 {
        self.cell_height() * 0.6
    }

    /// `count` cells across.
    #[must_use]
    pub fn columns(&self, count: f32) -> Length {
        Length::Px(self.cell_width() * count)
    }

    /// `count` cells down.
    #[must_use]
    pub fn rows(&self, count: f32) -> Length {
        Length::Px(self.cell_height() * count)
    }
}
