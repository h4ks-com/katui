//! Canvas and text rasterizing onto katui pixel surfaces.
//!
//! The crate is pure: callers pass font bytes in and read RGBA surfaces
//! out.

mod blend;
mod canvas;
mod text;

pub use canvas::Canvas;

/// The fonts this crate ships: Atkinson Hyperlegible Next for text and a
/// Noto Color Emoji subset, both under the SIL Open Font License.
#[must_use]
pub fn bundled_fonts() -> Vec<Vec<u8>> {
    vec![TEXT_FONT.to_vec(), EMOJI_FONT.to_vec()]
}

const TEXT_FONT: &[u8] = include_bytes!("../assets/AtkinsonHyperlegibleNext[wght].ttf");
const EMOJI_FONT: &[u8] = include_bytes!("../assets/NotoColorEmoji-subset.ttf");
pub use katui_scene::surface::Rgba;
pub use text::{TextEngine, TextLayout, TextSpan, TextStyle};

/// A position in pixels; `x` grows right and `y` grows down.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Point {
    /// Horizontal position.
    pub x: f32,
    /// Vertical position.
    pub y: f32,
}

impl Point {
    /// A point at (`x`, `y`).
    #[must_use]
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

/// A rectangle in pixels.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rect {
    /// Left edge.
    pub x: f32,
    /// Top edge.
    pub y: f32,
    /// Width.
    pub width: f32,
    /// Height.
    pub height: f32,
}

impl Rect {
    /// A rectangle at (`x`, `y`) of `width` x `height`.
    #[must_use]
    pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    fn right(&self) -> f32 {
        self.x + self.width
    }

    fn bottom(&self) -> f32 {
        self.y + self.height
    }
}

/// One color of a gradient.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ColorStop {
    /// Position along the gradient, from 0 to 1.
    pub offset: f32,
    /// Straight-alpha color at this position.
    pub color: Rgba,
}

/// What a shape or text is filled with.
#[derive(Debug, Clone, PartialEq)]
pub enum Paint {
    /// One straight-alpha color.
    Solid(Rgba),
    /// A gradient along the line `from` to `to`; `stops` ascend by offset
    /// and colors extend past both ends.
    Linear {
        /// Where offset 0 sits.
        from: Point,
        /// Where offset 1 sits.
        to: Point,
        /// Colors along the line.
        stops: Vec<ColorStop>,
    },
}

/// Why a canvas or text engine could not be built.
#[derive(Debug, thiserror::Error)]
pub enum DrawError {
    /// The dimensions are zero or exceed what a pixmap can hold.
    #[error("invalid canvas size {width}x{height}")]
    InvalidSize {
        /// Requested width.
        width: u32,
        /// Requested height.
        height: u32,
    },
    /// Every supplied byte buffer failed to parse as a font.
    #[error("every supplied font failed to parse")]
    NoUsableFont,
}

#[cfg(test)]
pub(crate) mod testing {
    use std::fs;
    use std::path::PathBuf;

    use katui_scene::surface::Surface;

    use crate::text::TextEngine;

    pub(crate) fn engine() -> TextEngine {
        TextEngine::new(super::bundled_fonts()).expect("fixture fonts parse")
    }

    pub(crate) fn save_png(name: &str, surface: &Surface) {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/katui-draw");
        fs::create_dir_all(&dir).expect("the target directory is writable");
        image::save_buffer(
            dir.join(name),
            surface.pixels(),
            surface.width(),
            surface.height(),
            image::ExtendedColorType::Rgba8,
        )
        .expect("the surface pixels match its dimensions and the target directory is writable");
    }
}

#[cfg(test)]
mod tests {
    use katui_scene::surface::{Rgba, Surface};

    use super::testing::{engine, save_png};
    use super::{Canvas, ColorStop, Paint, Point, Rect, TextStyle};

    const PANEL: Rgba = Rgba::solid(36, 40, 56);
    const INK: Rgba = Rgba::solid(232, 234, 240);

    fn checker(size: u32) -> Surface {
        let pixels = (0..size * size)
            .flat_map(|i| {
                let (x, y) = (i % size, i / size);
                if (x / 8 + y / 8) % 2 == 0 {
                    [240, 160, 60, 255]
                } else {
                    [60, 120, 220, 255]
                }
            })
            .collect();
        Surface::from_rgba(size, size, pixels).expect("the checker yields 4 bytes per pixel")
    }

    #[test]
    fn writes_a_sample_of_panels_gradient_image_and_text() {
        let mut canvas = Canvas::new(960, 560).expect("960x560 is a valid canvas size");
        canvas.clear(Rgba::solid(18, 20, 28));

        let header = Rect::new(32.0, 32.0, 896.0, 96.0);
        let accent = Paint::Linear {
            from: Point::new(32.0, 0.0),
            to: Point::new(928.0, 0.0),
            stops: vec![
                ColorStop {
                    offset: 0.0,
                    color: Rgba::solid(124, 92, 252),
                },
                ColorStop {
                    offset: 0.5,
                    color: Rgba::solid(60, 180, 240),
                },
                ColorStop {
                    offset: 1.0,
                    color: Rgba::solid(90, 230, 170),
                },
            ],
        };
        canvas.fill_round_rect(header, 20.0, &accent);

        let body = Rect::new(32.0, 160.0, 560.0, 368.0);
        canvas.fill_round_rect(body, 18.0, &Paint::Solid(PANEL));
        canvas.stroke_round_rect(body, 18.0, 1.5, &Paint::Solid(Rgba::solid(70, 76, 100)));

        canvas.draw_surface(&checker(128), Rect::new(624.0, 160.0, 304.0, 368.0), 24.0);

        let mut text = engine();
        let title = text.layout(
            "A pretty chat panel \u{1F680}",
            &TextStyle {
                weight: 700,
                ..TextStyle::new(40.0)
            },
            None,
        );
        canvas.draw_text(
            &title,
            Point::new(60.0, 56.0),
            &Paint::Solid(Rgba::solid(16, 18, 26)),
        );

        let message = text.layout(
            "Wrapped text flows inside the panel at any width, keeps its \
             measured size, and mixes color emoji from a supplied font \u{1F389}\u{1F600}.\n\n\
             A second paragraph starts after a blank line.",
            &TextStyle::new(26.0),
            Some(504.0),
        );
        canvas.draw_text(&message, Point::new(60.0, 188.0), &Paint::Solid(INK));

        let surface = canvas
            .to_surface()
            .expect("the canvas buffer matches its own dimensions");
        save_png("sample.png", &surface);
        assert!(message.line_count() > 3);
    }
}
