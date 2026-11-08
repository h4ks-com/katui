//! Text shaping and glyph rasterizing from caller-supplied font bytes.

use std::collections::HashMap;
use std::sync::Arc;

use cosmic_text::fontdb::{self, Family};
use cosmic_text::{
    Attrs, Buffer, CacheKey, FontSystem, Metrics, Shaping, SwashCache, SwashImage, Weight,
};

pub(crate) use cosmic_text::SwashContent;

use crate::DrawError;

// The locale only steers per-script fallback order, so we fix it to keep
// the engine free of environment reads.
const LOCALE: &str = "en-US";

/// How a run of text looks.
#[derive(Debug, Clone, PartialEq)]
pub struct TextStyle {
    /// Font size in pixels.
    pub size: f32,
    /// Distance between baselines in pixels.
    pub line_height: f32,
    /// CSS-style weight, 400 regular and 700 bold.
    pub weight: u16,
}

impl TextStyle {
    /// Regular text of `size` pixels in the default family.
    #[must_use]
    pub fn new(size: f32) -> Self {
        Self {
            size,
            line_height: (size * 1.3).ceil(),
            weight: 400,
        }
    }
}

/// Shapes text and rasterizes glyphs from fonts the caller supplies.
///
/// Glyph bitmaps stay cached for the life of the engine, so a process
/// keeps one engine per font set.
pub struct TextEngine {
    fonts: FontSystem,
    scaler: SwashCache,
    glyphs: HashMap<CacheKey, Option<Arc<SwashImage>>>,
}

impl TextEngine {
    /// An engine over `fonts`. The first font is the default family and
    /// the others serve as fallback for characters it lacks; color emoji
    /// come from a supplied emoji font.
    pub fn new(fonts: Vec<Vec<u8>>) -> Result<Self, DrawError> {
        let mut database = fontdb::Database::new();
        for font in fonts {
            database.load_font_data(font);
        }
        let default_family = database
            .faces()
            .next()
            .and_then(|face| face.families.first())
            .map(|(name, _)| name.clone())
            .ok_or(DrawError::NoUsableFont)?;
        database.set_sans_serif_family(default_family);
        Ok(Self {
            fonts: FontSystem::new_with_locale_and_db(LOCALE.to_owned(), database),
            scaler: SwashCache::new(),
            glyphs: HashMap::new(),
        })
    }

    /// Shapes `text`, wrapping at `max_width` when given.
    pub fn layout(&mut self, text: &str, style: &TextStyle, max_width: Option<f32>) -> TextLayout {
        let mut buffer = Buffer::new_empty(Metrics::new(
            style.size.max(1.0),
            style.line_height.max(1.0),
        ));
        buffer.set_size(max_width, None);
        let attrs = Attrs::new()
            .weight(Weight(style.weight))
            .family(Family::SansSerif);
        buffer.set_text(text, &attrs, Shaping::Advanced, None);
        buffer.shape_until_scroll(&mut self.fonts, false);

        let (mut width, mut height, mut line_count) = (0.0_f32, 0.0_f32, 0);
        let mut placed = Vec::new();
        for run in buffer.layout_runs() {
            width = width.max(run.line_w);
            height = height.max(run.line_top + run.line_height);
            line_count += 1;
            for glyph in run.glyphs {
                let physical = glyph.physical((0.0, run.line_y), 1.0);
                if let Some(image) = self.rasterize(physical.cache_key) {
                    placed.push(PlacedGlyph {
                        image,
                        x: physical.x,
                        y: physical.y,
                    });
                }
            }
        }
        TextLayout {
            glyphs: placed,
            size: (width, height),
            line_count,
        }
    }

    fn rasterize(&mut self, key: CacheKey) -> Option<Arc<SwashImage>> {
        let Self {
            fonts,
            scaler,
            glyphs,
        } = self;
        glyphs
            .entry(key)
            .or_insert_with(|| {
                scaler
                    .get_image_uncached(fonts, key)
                    .filter(|image| image.placement.width > 0 && image.placement.height > 0)
                    .map(Arc::new)
            })
            .clone()
    }
}

pub(crate) struct PlacedGlyph {
    pub(crate) image: Arc<SwashImage>,
    pub(crate) x: i32,
    pub(crate) y: i32,
}

/// Shaped text with its glyph bitmaps, positioned from its top-left
/// corner. Redrawing it reuses the stored glyphs.
pub struct TextLayout {
    glyphs: Vec<PlacedGlyph>,
    size: (f32, f32),
    line_count: usize,
}

impl TextLayout {
    /// The measured `(width, height)` in pixels.
    pub fn size(&self) -> (f32, f32) {
        self.size
    }

    /// The number of lines after wrapping.
    pub fn line_count(&self) -> usize {
        self.line_count
    }

    pub(crate) fn glyphs(&self) -> &[PlacedGlyph] {
        &self.glyphs
    }
}

#[cfg(test)]
#[allow(clippy::cast_sign_loss, clippy::cast_precision_loss)] // Test canvases are small and measured sizes are positive.
mod tests {
    use katui_scene::surface::{PixelPos, Rgba, Surface};

    use super::{TextEngine, TextLayout, TextStyle};
    use crate::testing::{engine, save_png};
    use crate::{Canvas, DrawError, Paint, Point};

    const WHITE: Rgba = Rgba::solid(255, 255, 255);
    const LONG: &str = "The quick brown fox jumps over the lazy dog and keeps running for a while.";

    fn draw(layout: &TextLayout, margin: u32, paint: &Paint) -> Surface {
        let (width, height) = layout.size();
        let mut canvas = Canvas::new(
            width.ceil() as u32 + margin * 2,
            height.ceil() as u32 + margin * 2,
        )
        .expect("measured text plus a margin is a valid canvas size");
        canvas.draw_text(layout, Point::new(margin as f32, margin as f32), paint);
        canvas
            .to_surface()
            .expect("the canvas buffer matches its own dimensions")
    }

    fn alpha(surface: &Surface, x: u32, y: u32) -> u8 {
        surface
            .get(PixelPos::new(x, y))
            .expect("tests read pixels inside the surface")
            .a
    }

    fn ink(surface: &Surface) -> u64 {
        surface
            .pixels()
            .as_chunks::<4>()
            .0
            .iter()
            .map(|pixel| u64::from(pixel[3]))
            .sum()
    }

    #[test]
    fn ink_lands_inside_the_measured_size() {
        let mut engine = engine();
        let layout = engine.layout("Hello, gyp!", &TextStyle::new(32.0), None);
        let (width, height) = layout.size();
        assert!(width > 32.0 && height >= 32.0, "{width}x{height}");
        let margin = 16;
        let surface = draw(&layout, margin, &Paint::Solid(WHITE));
        assert!(ink(&surface) > 0, "the text left ink");
        for y in 0..surface.height() {
            for x in 0..surface.width() {
                let inside = x + 2 >= margin
                    && y + 2 >= margin
                    && x <= margin + width.ceil() as u32 + 1
                    && y <= margin + height.ceil() as u32 + 1;
                assert!(
                    inside || alpha(&surface, x, y) == 0,
                    "ink outside at {x},{y}"
                );
            }
        }
        save_png("text_measured.png", &surface);
    }

    #[test]
    fn narrow_widths_wrap_into_more_lines() {
        let mut engine = engine();
        let style = TextStyle::new(20.0);
        let wide = engine.layout(LONG, &style, None);
        let narrow = engine.layout(LONG, &style, Some(200.0));
        assert_eq!(wide.line_count(), 1);
        assert!(narrow.line_count() > 2, "{}", narrow.line_count());
        assert!(narrow.size().0 <= 200.0);
        assert!(narrow.size().1 > wide.size().1);
        save_png("text_wrapped.png", &draw(&narrow, 8, &Paint::Solid(WHITE)));
    }

    #[test]
    fn hard_newlines_count_as_lines() {
        let mut engine = engine();
        let layout = engine.layout("first\nsecond\r\nthird", &TextStyle::new(20.0), None);
        assert_eq!(layout.line_count(), 3);
    }

    #[test]
    fn the_weight_axis_changes_the_rendered_glyphs() {
        let mut engine = engine();
        let style_at = |weight| TextStyle {
            weight,
            ..TextStyle::new(40.0)
        };
        let light = engine.layout("Hamburgefonstiv", &style_at(300), None);
        let regular = engine.layout("Hamburgefonstiv", &style_at(400), None);
        let bold = engine.layout("Hamburgefonstiv", &style_at(800), None);
        let paint = Paint::Solid(WHITE);
        let (light_ink, regular_ink, bold_ink) = (
            ink(&draw(&light, 4, &paint)),
            ink(&draw(&regular, 4, &paint)),
            ink(&draw(&bold, 4, &paint)),
        );
        assert!(light_ink < regular_ink, "{light_ink} {regular_ink}");
        assert!(regular_ink * 5 < bold_ink * 4, "{regular_ink} {bold_ink}");
        save_png("text_bold.png", &draw(&bold, 4, &paint));
    }

    #[test]
    fn emoji_from_a_supplied_font_render_in_color() {
        let mut engine = engine();
        let layout = engine.layout("a\u{1F600}", &TextStyle::new(48.0), None);
        let surface = draw(&layout, 8, &Paint::Solid(WHITE));
        let colored = surface
            .pixels()
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|pixel| pixel[3] > 200 && pixel[0].abs_diff(pixel[2]) > 80)
            .count();
        assert!(colored > 200, "colored pixels: {colored}");
        assert!(layout.size().0 > 48.0, "the emoji advanced the line");
        save_png("text_emoji.png", &surface);
    }

    #[test]
    fn text_respects_clips_and_draws_linear_paints_in_the_first_stop() {
        let mut engine = engine();
        let layout = engine.layout("MMMMMMMM", &TextStyle::new(40.0), None);
        let (width, height) = layout.size();
        let mut canvas = Canvas::new(width as u32 + 8, height as u32 + 8)
            .expect("measured text plus a margin is a valid canvas size");
        let half = (width / 2.0).floor();
        canvas.push_clip(crate::Rect::new(0.0, 0.0, half, height + 8.0));
        canvas.draw_text(
            &layout,
            Point::default(),
            &Paint::Linear {
                from: Point::new(0.0, 0.0),
                to: Point::new(half, 0.0),
                stops: vec![
                    crate::ColorStop {
                        offset: 0.0,
                        color: Rgba::solid(255, 0, 0),
                    },
                    crate::ColorStop {
                        offset: 1.0,
                        color: Rgba::solid(0, 0, 255),
                    },
                ],
            },
        );
        let surface = canvas
            .to_surface()
            .expect("the canvas buffer matches its own dimensions");
        let right_ink = (half as u32..surface.width())
            .flat_map(|x| (0..surface.height()).map(move |y| (x, y)))
            .any(|(x, y)| alpha(&surface, x, y) != 0);
        assert!(!right_ink, "clip kept text out of the right half");
        let reddest = surface
            .pixels()
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|pixel| pixel[3] == 255 && pixel[0] > 180)
            .count();
        let bluest = surface
            .pixels()
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|pixel| pixel[3] == 255 && pixel[2] > 180)
            .count();
        assert!(reddest > 0 && bluest == 0, "{reddest} {bluest}");
        save_png("text_clipped.png", &surface);
    }

    #[test]
    fn font_bytes_that_do_not_parse_are_rejected() {
        assert!(matches!(
            TextEngine::new(vec![vec![1, 2, 3]]),
            Err(DrawError::NoUsableFont)
        ));
    }

    #[test]
    fn empty_text_lays_out_as_one_line() {
        let mut engine = engine();
        let layout = engine.layout("", &TextStyle::new(20.0), Some(100.0));
        assert_eq!(layout.line_count(), 1);
    }
}
