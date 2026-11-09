//! Text shaping and glyph rasterizing from caller-supplied font bytes.

use std::collections::HashMap;
use std::sync::Arc;

use cosmic_text::fontdb::{self, Family};
use cosmic_text::{
    Attrs, Buffer, CacheKey, FontSystem, LayoutRun, Metrics, Shaping, Style as FontStyle,
    SwashCache, SwashImage, Weight,
};
use katui_scene::surface::Rgba;

pub(crate) use cosmic_text::SwashContent;

use crate::{DrawError, Rect};

// The locale only steers per-script fallback order, so we fix it to keep
// the engine free of environment reads.
const LOCALE: &str = "en-US";

const BOLD_WEIGHT: u16 = 700;

/// A stretch of text with its own look inside one wrapped run.
#[allow(clippy::struct_excessive_bools)] // each is an independent text style
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct TextSpan {
    pub text: String,
    /// Its color, or the color the whole text is drawn in.
    pub color: Option<Rgba>,
    /// A color behind the glyphs.
    pub background: Option<Rgba>,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
}

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
        let mut buffer = new_buffer(style, max_width);
        buffer.set_text(text, &base_attrs(style), Shaping::Advanced, None);
        self.place(buffer, &[])
    }

    /// Shapes styled `spans` as one run of text that wraps across them.
    pub fn layout_spans(
        &mut self,
        spans: &[TextSpan],
        style: &TextStyle,
        max_width: Option<f32>,
    ) -> TextLayout {
        let mut buffer = new_buffer(style, max_width);
        let base = base_attrs(style);
        let rich = spans.iter().enumerate().map(|(index, span)| {
            let mut attrs = base.clone().metadata(index);
            if span.bold {
                attrs = attrs.weight(Weight(style.weight.max(BOLD_WEIGHT)));
            }
            if span.italic {
                attrs = attrs.style(FontStyle::Italic);
            }
            (span.text.as_str(), attrs)
        });
        buffer.set_rich_text(rich, &base, Shaping::Advanced, None);
        self.place(buffer, spans)
    }

    fn place(&mut self, mut buffer: Buffer, spans: &[TextSpan]) -> TextLayout {
        buffer.shape_until_scroll(&mut self.fonts, false);
        let mut layout = TextLayout {
            glyphs: Vec::new(),
            backgrounds: Vec::new(),
            lines: Vec::new(),
            size: (0.0, 0.0),
            line_count: 0,
        };
        for run in buffer.layout_runs() {
            layout.size.0 = layout.size.0.max(run.line_w);
            layout.size.1 = layout.size.1.max(run.line_top + run.line_height);
            layout.line_count += 1;
            for glyph in run.glyphs {
                let span = spans.get(glyph.metadata);
                if let Some(span) = span {
                    layout.add_marks(span, glyph.x, glyph.w, &run);
                }
                let physical = glyph.physical((0.0, run.line_y), 1.0);
                if let Some(image) = self.rasterize(physical.cache_key) {
                    layout.glyphs.push(PlacedGlyph {
                        image,
                        x: physical.x,
                        y: physical.y,
                        color: span.and_then(|span| span.color),
                    });
                }
            }
        }
        layout
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

fn new_buffer(style: &TextStyle, max_width: Option<f32>) -> Buffer {
    let mut buffer = Buffer::new_empty(Metrics::new(
        style.size.max(1.0),
        style.line_height.max(1.0),
    ));
    buffer.set_size(max_width, None);
    buffer
}

fn base_attrs(style: &TextStyle) -> Attrs<'static> {
    Attrs::new()
        .weight(Weight(style.weight))
        .family(Family::SansSerif)
}

pub(crate) struct PlacedGlyph {
    pub(crate) image: Arc<SwashImage>,
    pub(crate) x: i32,
    pub(crate) y: i32,
    pub(crate) color: Option<Rgba>,
}

/// Shaped text with its glyph bitmaps, positioned from its top-left
/// corner. Redrawing it reuses the stored glyphs.
pub struct TextLayout {
    glyphs: Vec<PlacedGlyph>,
    backgrounds: Vec<(Rect, Rgba)>,
    lines: Vec<(Rect, Option<Rgba>)>,
    size: (f32, f32),
    line_count: usize,
}

impl TextLayout {
    fn add_marks(&mut self, span: &TextSpan, x: f32, width: f32, run: &LayoutRun<'_>) {
        // we round outward so neighboring glyphs share an edge with no gap
        let left = x.floor();
        let width = (x + width).ceil() - left;
        if let Some(color) = span.background {
            self.backgrounds
                .push((Rect::new(left, run.line_top, width, run.line_height), color));
        }
        let thickness = (run.line_height / 16.0).ceil();
        if span.underline {
            let line = Rect::new(left, run.line_y + thickness, width, thickness);
            self.lines.push((line, span.color));
        }
        if span.strike {
            let line = Rect::new(left, run.line_y - run.line_height * 0.28, width, thickness);
            self.lines.push((line, span.color));
        }
    }

    pub(crate) fn backgrounds(&self) -> &[(Rect, Rgba)] {
        &self.backgrounds
    }

    pub(crate) fn lines(&self) -> &[(Rect, Option<Rgba>)] {
        &self.lines
    }

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
mod span_tests {
    use katui_scene::surface::{PixelPos, Rgba, Surface};

    use super::{TextSpan, TextStyle};
    use crate::testing::engine;
    use crate::{Canvas, Paint, Point};

    const RED: Rgba = Rgba::solid(255, 0, 0);
    const BLUE: Rgba = Rgba::solid(0, 0, 255);
    const INK: Rgba = Rgba::solid(9, 9, 9);

    fn span(text: &str) -> TextSpan {
        TextSpan {
            text: text.to_owned(),
            ..TextSpan::default()
        }
    }

    fn drawn(spans: &[TextSpan]) -> Surface {
        let layout = engine().layout_spans(spans, &TextStyle::new(24.0), None);
        let mut canvas = Canvas::new(400, 60).expect("400x60 is a valid canvas size");
        canvas.draw_text(&layout, Point::new(0.0, 0.0), &Paint::Solid(INK));
        canvas.to_surface().expect("a 400x60 canvas converts")
    }

    fn count(surface: &Surface, wanted: Rgba) -> usize {
        (0..surface.height())
            .flat_map(|y| (0..surface.width()).map(move |x| PixelPos::new(x, y)))
            .filter(|at| surface.get(*at) == Some(wanted))
            .count()
    }

    #[test]
    fn each_span_draws_in_its_own_color() {
        let surface = drawn(&[
            TextSpan {
                color: Some(RED),
                ..span("MMMM")
            },
            TextSpan {
                color: Some(BLUE),
                ..span("MMMM")
            },
        ]);
        assert!(count(&surface, RED) > 20, "the first span is red");
        assert!(count(&surface, BLUE) > 20, "the second span is blue");
    }

    #[test]
    fn a_background_fills_behind_its_span_only() {
        let surface = drawn(&[
            span("MM"),
            TextSpan {
                background: Some(RED),
                ..span("MM")
            },
        ]);
        let filled = count(&surface, RED);
        assert!(filled > 200, "{filled} pixels of background");
        assert_eq!(
            surface.get(PixelPos::new(1, 2)).map(|pixel| pixel.a),
            Some(0)
        );
    }

    #[test]
    fn underline_and_strike_each_add_a_line() {
        let plain = drawn(&[span("MMMM")]);
        let lined = drawn(&[TextSpan {
            underline: true,
            strike: true,
            ..span("MMMM")
        }]);
        assert!(count(&lined, INK) > count(&plain, INK));
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
