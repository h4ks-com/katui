//! The theme: semantic colors plus sizes that follow the terminal's cell,
//! so type and spacing stay in proportion at every font size.

use std::cell::RefCell;
use std::rc::Rc;

use katui_draw::{Paint, Rgba};
use katui_ui::{CellPixels, Length};

/// The cell widgets size themselves by outside [`Theme::scope`].
const FALLBACK_CELL: CellPixels = CellPixels {
    width: 10,
    height: 20,
};

thread_local! {
    static CURRENT: RefCell<Option<Rc<Theme>>> = const { RefCell::new(None) };
}

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

    /// The light theme at `cell`.
    #[must_use]
    pub fn light(cell: CellPixels) -> Self {
        Self {
            cell,
            background: Paint::Solid(Rgba::solid(246, 244, 239)),
            raised: Paint::Solid(Rgba::solid(255, 255, 255)),
            highlight: Paint::Solid(Rgba::solid(226, 231, 244)),
            border: Paint::Solid(Rgba::solid(210, 206, 198)),
            text: Rgba::solid(30, 32, 40),
            muted: Rgba::solid(110, 112, 124),
            accent: Rgba::solid(52, 96, 210),
            on_accent: Rgba::solid(255, 255, 255),
            danger: Rgba::solid(196, 40, 40),
        }
    }

    /// The same theme with every size and type size times `factor`, for a
    /// zoom setting.
    #[must_use]
    pub fn scaled(mut self, factor: f32) -> Self {
        let scale = |pixels: u16| {
            let scaled = (f32::from(pixels) * factor.max(0.1)).round();
            u16::try_from(scaled as i64).unwrap_or(pixels).max(1)
        };
        self.cell = CellPixels {
            width: scale(self.cell.width),
            height: scale(self.cell.height),
        };
        self
    }

    /// The theme widgets draw with: the one [`Theme::scope`] set on this
    /// thread, or the dark theme at a 10 by 20 pixel cell outside any scope.
    #[must_use]
    pub fn current() -> Rc<Self> {
        CURRENT
            .with_borrow(Clone::clone)
            .unwrap_or_else(|| Rc::new(Self::dark(FALLBACK_CELL)))
    }

    /// Run `build` with this theme as [`Theme::current`], so the widgets it
    /// builds draw with it. The app runner does this around every view.
    pub fn scope<R>(self, build: impl FnOnce(&Self) -> R) -> R {
        let theme = Rc::new(self);
        let outer = CURRENT.replace(Some(Rc::clone(&theme)));
        let elements = build(&theme);
        CURRENT.set(outer);
        elements
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

    /// Section heading text size.
    #[must_use]
    pub fn heading(&self) -> f32 {
        self.body() * 1.25
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

    /// The accent color while `active`, the border color otherwise.
    #[must_use]
    pub fn outline(&self, active: bool) -> Paint {
        if active {
            Paint::Solid(self.accent)
        } else {
            self.border.clone()
        }
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

#[cfg(test)]
mod tests {
    use katui_ui::CellPixels;

    use super::Theme;

    #[test]
    fn scaling_grows_every_size_with_the_cell() {
        let cell = CellPixels {
            width: 10,
            height: 20,
        };
        let large = Theme::dark(cell).scaled(1.5);
        assert_eq!((large.cell.width, large.cell.height), (15, 30));
        assert!(large.body() > Theme::dark(cell).body());
        assert_eq!(Theme::light(cell).scaled(1.0).cell, cell);
    }

    #[test]
    fn a_scope_sets_the_current_theme_and_restores_the_outer_one() {
        let cell = CellPixels {
            width: 7,
            height: 14,
        };
        let light = Theme::light(cell);
        let text = light.text;
        let inner = light.scope(|_| Theme::current().text);
        assert_eq!(inner, text);
        assert_ne!(Theme::current().text, text);
    }
}
