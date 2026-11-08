//! The RGBA canvas: shapes through tiny-skia, text through direct pixel
//! loops.

use std::mem;

use katui_scene::surface::{Rgba, Surface};
use tiny_skia::{
    Color, FillRule, FilterQuality, GradientStop, IntSize, LinearGradient, Mask, Paint as SkPaint,
    Path, PathBuilder, Pattern, Pixmap, Point as SkPoint, Rect as SkRect, Shader, SpreadMode,
    Stroke, Transform,
};

use crate::blend::{Ink, coordinate, demultiply, premultiply_reduced};
use crate::text::{PlacedGlyph, SwashContent, TextLayout};
use crate::{DrawError, Paint, Point, Rect};

const QUARTER_CIRCLE_BEZIER_KAPPA: f32 = 0.552_284_8;

/// A pixel-aligned rectangle already limited to the canvas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Bounds {
    left: u32,
    top: u32,
    right: u32,
    bottom: u32,
}

impl Bounds {
    fn covering(rect: Rect, limit: Self) -> Self {
        let left = floor_to_pixel(rect.x).clamp(limit.left, limit.right);
        let top = floor_to_pixel(rect.y).clamp(limit.top, limit.bottom);
        Self {
            left,
            top,
            right: ceil_to_pixel(rect.right()).clamp(left, limit.right),
            bottom: ceil_to_pixel(rect.bottom()).clamp(top, limit.bottom),
        }
    }

    fn intersect(self, other: Self) -> Self {
        Self {
            left: self.left.max(other.left),
            top: self.top.max(other.top),
            right: self.right.min(other.right),
            bottom: self.bottom.min(other.bottom),
        }
    }

    fn is_empty(self) -> bool {
        self.left >= self.right || self.top >= self.bottom
    }

    fn contains(self, other: Self) -> bool {
        self.left <= other.left
            && self.top <= other.top
            && self.right >= other.right
            && self.bottom >= other.bottom
    }
}

fn floor_to_pixel(value: f32) -> u32 {
    (value.floor() as i32).max(0).unsigned_abs()
}

fn ceil_to_pixel(value: f32) -> u32 {
    (value.ceil() as i32).max(0).unsigned_abs()
}

/// A premultiplied RGBA drawing surface in pixels, origin top-left.
pub struct Canvas {
    pixmap: Pixmap,
    clip_mask: Mask,
    clips: Vec<Bounds>,
    scratch: Vec<u8>,
}

impl Canvas {
    /// A transparent canvas of `width` x `height` pixels.
    pub fn new(width: u32, height: u32) -> Result<Self, DrawError> {
        let invalid = DrawError::InvalidSize { width, height };
        let pixmap = Pixmap::new(width, height).ok_or(DrawError::InvalidSize { width, height })?;
        let clip_mask = Mask::new(width, height).ok_or(invalid)?;
        Ok(Self {
            pixmap,
            clip_mask,
            clips: Vec::new(),
            scratch: Vec::new(),
        })
    }

    /// The width in pixels.
    pub fn width(&self) -> u32 {
        self.pixmap.width()
    }

    /// The height in pixels.
    pub fn height(&self) -> u32 {
        self.pixmap.height()
    }

    /// Replaces every pixel with `color`, ignoring the clip stack.
    pub fn clear(&mut self, color: Rgba) {
        self.pixmap.fill(skia_color(color));
    }

    /// Restricts drawing to the pixels `rect` touches, intersected with
    /// the current clip, until the matching [`Canvas::pop_clip`].
    pub fn push_clip(&mut self, rect: Rect) {
        let clip = Bounds::covering(rect, self.clip_bounds());
        self.swap_clip_mask(self.clips.last().copied(), Some(clip));
        self.clips.push(clip);
    }

    /// Removes the most recent clip, when one exists.
    pub fn pop_clip(&mut self) {
        let removed = self.clips.pop();
        self.swap_clip_mask(removed, self.clips.last().copied());
    }

    /// Fills `rect`, antialiasing fractional edges.
    pub fn fill_rect(&mut self, rect: Rect, paint: &Paint) {
        let (Some(needs_mask), Some(shape), Some(paint)) = (
            self.clip_state(rect),
            SkRect::from_xywh(rect.x, rect.y, rect.width, rect.height),
            skia_paint(paint),
        ) else {
            return;
        };
        let mask = needs_mask.then_some(&self.clip_mask);
        self.pixmap
            .fill_rect(shape, &paint, Transform::identity(), mask);
    }

    /// Fills `rect` with corners of `radius`.
    pub fn fill_round_rect(&mut self, rect: Rect, radius: f32, paint: &Paint) {
        if radius <= 0.0 {
            self.fill_rect(rect, paint);
            return;
        }
        let (Some(needs_mask), Some(path), Some(paint)) = (
            self.clip_state(rect),
            round_rect_path(rect, radius),
            skia_paint(paint),
        ) else {
            return;
        };
        let mask = needs_mask.then_some(&self.clip_mask);
        self.pixmap.fill_path(
            &path,
            &paint,
            FillRule::Winding,
            Transform::identity(),
            mask,
        );
    }

    /// Strokes a border of `width` just inside `rect`, so its outer edge
    /// matches a fill of the same `rect` and `radius`.
    pub fn stroke_round_rect(&mut self, rect: Rect, radius: f32, width: f32, paint: &Paint) {
        let half = width / 2.0;
        let centerline = Rect::new(
            rect.x + half,
            rect.y + half,
            rect.width - width,
            rect.height - width,
        );
        let (Some(needs_mask), Some(path), Some(paint)) = (
            self.clip_state(rect),
            round_rect_path(centerline, (radius - half).max(0.0)),
            skia_paint(paint),
        ) else {
            return;
        };
        let stroke = Stroke {
            width,
            ..Stroke::default()
        };
        let mask = needs_mask.then_some(&self.clip_mask);
        self.pixmap
            .stroke_path(&path, &paint, &stroke, Transform::identity(), mask);
    }

    /// Scales `image` (straight alpha) into `dest` with rounded corners.
    /// Drawing time grows with the source pixel count, so a caller that
    /// redraws an image every frame keeps a copy near its display size.
    pub fn draw_surface(&mut self, image: &Surface, dest: Rect, corner_radius: f32) {
        let (Some(needs_mask), Some(path)) =
            (self.clip_state(dest), round_rect_path(dest, corner_radius))
        else {
            return;
        };
        let reduction = box_reduction(image, dest);
        let (width, height) =
            premultiply_reduced(image.pixels(), image.width(), reduction, &mut self.scratch);
        let Some(size) = IntSize::from_wh(width, height) else {
            return;
        };
        let Some(source) = Pixmap::from_vec(mem::take(&mut self.scratch), size) else {
            return;
        };
        {
            let placement = Transform::from_row(
                dest.width / coordinate(width),
                0.0,
                0.0,
                dest.height / coordinate(height),
                dest.x,
                dest.y,
            );
            let paint = SkPaint {
                shader: Pattern::new(
                    source.as_ref(),
                    SpreadMode::Pad,
                    FilterQuality::Bilinear,
                    1.0,
                    placement,
                ),
                ..SkPaint::default()
            };
            let mask = needs_mask.then_some(&self.clip_mask);
            self.pixmap.fill_path(
                &path,
                &paint,
                FillRule::Winding,
                Transform::identity(),
                mask,
            );
        }
        self.scratch = source.take();
    }

    /// Draws `layout` with its top-left corner at `origin`, rounded to
    /// whole pixels so glyphs keep their rasterized crispness. Color
    /// glyphs keep their colors and take only opacity from `paint`. A
    /// linear `paint` draws the whole text in its first stop's color.
    pub fn draw_text(&mut self, layout: &TextLayout, origin: Point, paint: &Paint) {
        let ink = Ink::new(paint);
        let origin = (origin.x.round() as i32, origin.y.round() as i32);
        let clip = self.clip_bounds();
        let stride = self.pixmap.width() as usize * 4;
        let pixels = self.pixmap.data_mut();
        for glyph in layout.glyphs() {
            blit_glyph(pixels, stride, clip, glyph, origin, &ink);
        }
    }

    /// The canvas as a straight-alpha surface, the format kitty displays.
    pub fn to_surface(&self) -> Result<Surface, DrawError> {
        let (width, height) = (self.width(), self.height());
        let mut pixels = vec![0; self.pixmap.data().len()];
        let source = self.pixmap.data().as_chunks::<4>().0;
        for (out, pixel) in pixels.as_chunks_mut::<4>().0.iter_mut().zip(source) {
            *out = demultiply(*pixel);
        }
        Surface::from_rgba(width, height, pixels).ok_or(DrawError::InvalidSize { width, height })
    }

    fn full_bounds(&self) -> Bounds {
        Bounds {
            left: 0,
            top: 0,
            right: self.pixmap.width(),
            bottom: self.pixmap.height(),
        }
    }

    fn clip_bounds(&self) -> Bounds {
        self.clips
            .last()
            .copied()
            .unwrap_or_else(|| self.full_bounds())
    }

    /// `None` when nothing in `rect` is visible, otherwise whether the
    /// draw crosses the clip edge and needs the mask.
    fn clip_state(&self, rect: Rect) -> Option<bool> {
        let clip = self.clip_bounds();
        let drawn = Bounds::covering(rect, self.full_bounds());
        if drawn.intersect(clip).is_empty() {
            None
        } else {
            Some(!self.clips.is_empty() && !clip.contains(drawn))
        }
    }

    // We rewrite only the old and new clip areas so a push costs the clip
    // area rather than the whole canvas.
    fn swap_clip_mask(&mut self, old: Option<Bounds>, new: Option<Bounds>) {
        let stride = self.clip_mask.width() as usize;
        for (bounds, value) in [(old, 0), (new, 255)] {
            let Some(bounds) = bounds else { continue };
            let span = bounds.left as usize..bounds.right as usize;
            for row in rows_mut(self.clip_mask.data_mut(), stride, bounds) {
                if let Some(cells) = row.get_mut(span.clone()) {
                    cells.fill(value);
                }
            }
        }
    }
}

fn rows_mut(data: &mut [u8], stride: usize, area: Bounds) -> impl Iterator<Item = &mut [u8]> {
    data.chunks_exact_mut(stride)
        .skip(area.top as usize)
        .take(area.bottom.saturating_sub(area.top) as usize)
}

fn skia_color(color: Rgba) -> Color {
    Color::from_rgba8(color.r, color.g, color.b, color.a)
}

fn skia_paint(paint: &Paint) -> Option<SkPaint<'static>> {
    let shader = match paint {
        Paint::Solid(color) => Shader::SolidColor(skia_color(*color)),
        Paint::Linear { from, to, stops } => LinearGradient::new(
            SkPoint::from_xy(from.x, from.y),
            SkPoint::from_xy(to.x, to.y),
            stops
                .iter()
                .map(|stop| GradientStop::new(stop.offset, skia_color(stop.color)))
                .collect(),
            SpreadMode::Pad,
            Transform::identity(),
        )?,
    };
    Some(SkPaint {
        shader,
        ..SkPaint::default()
    })
}

fn round_rect_path(rect: Rect, radius: f32) -> Option<Path> {
    if rect.width <= 0.0 || rect.height <= 0.0 {
        return None;
    }
    let radius = radius.clamp(0.0, rect.width.min(rect.height) / 2.0);
    let handle = radius * (1.0 - QUARTER_CIRCLE_BEZIER_KAPPA);
    let (left, top, right, bottom) = (rect.x, rect.y, rect.right(), rect.bottom());
    let mut path = PathBuilder::new();
    path.move_to(left + radius, top);
    path.line_to(right - radius, top);
    path.cubic_to(
        right - handle,
        top,
        right,
        top + handle,
        right,
        top + radius,
    );
    path.line_to(right, bottom - radius);
    path.cubic_to(
        right,
        bottom - handle,
        right - handle,
        bottom,
        right - radius,
        bottom,
    );
    path.line_to(left + radius, bottom);
    path.cubic_to(
        left + handle,
        bottom,
        left,
        bottom - handle,
        left,
        bottom - radius,
    );
    path.line_to(left, top + radius);
    path.cubic_to(left, top + handle, left + handle, top, left + radius, top);
    path.close();
    path.finish()
}

// tiny-skia has no mipmaps, so we average whole-number blocks ourselves to
// keep large downscales from aliasing.
fn box_reduction(image: &Surface, dest: Rect) -> u32 {
    let ratio =
        (coordinate(image.width()) / dest.width).min(coordinate(image.height()) / dest.height);
    floor_to_pixel(ratio).clamp(1, image.width().min(image.height()).max(1))
}

fn blit_glyph(
    pixels: &mut [u8],
    stride: usize,
    clip: Bounds,
    glyph: &PlacedGlyph,
    origin: (i32, i32),
    ink: &Ink,
) {
    let image = &glyph.image;
    let bytes_per_pixel = match image.content {
        SwashContent::Mask => 1,
        SwashContent::Color => 4,
        SwashContent::SubpixelMask => return,
    };
    let left = origin
        .0
        .saturating_add(glyph.x)
        .saturating_add(image.placement.left);
    let top = origin
        .1
        .saturating_add(glyph.y)
        .saturating_sub(image.placement.top);
    let x0 = left.max(clip.left.cast_signed());
    let y0 = top.max(clip.top.cast_signed());
    let x1 = left
        .saturating_add(image.placement.width.cast_signed())
        .min(clip.right.cast_signed());
    let y1 = top
        .saturating_add(image.placement.height.cast_signed())
        .min(clip.bottom.cast_signed());
    if x0 >= x1 || y0 >= y1 {
        return;
    }
    let (first_column, first_row) = (
        (x0 - left).unsigned_abs() as usize,
        (y0 - top).unsigned_abs() as usize,
    );
    let columns = (x1 - x0).unsigned_abs() as usize;
    let canvas_x = x0.unsigned_abs();
    let canvas_y = y0.unsigned_abs();
    let source_span = first_column * bytes_per_pixel..(first_column + columns) * bytes_per_pixel;
    let target_span = canvas_x as usize * 4..(canvas_x as usize + columns) * 4;
    let source_rows = image
        .data
        .chunks_exact(image.placement.width as usize * bytes_per_pixel)
        .skip(first_row);
    let target_rows = pixels
        .chunks_exact_mut(stride)
        .skip(canvas_y as usize)
        .take((y1 - y0).unsigned_abs() as usize);
    for (target, source) in target_rows.zip(source_rows) {
        let (Some(target), Some(source)) = (
            target.get_mut(target_span.clone()),
            source.get(source_span.clone()),
        ) else {
            continue;
        };
        let target = target.as_chunks_mut::<4>().0;
        if bytes_per_pixel == 1 {
            ink.blend_mask(target, source);
        } else {
            ink.blend_color(target, source.as_chunks::<4>().0);
        }
    }
}

#[cfg(test)]
mod tests {
    use katui_scene::surface::{PixelPos, Rgba, Surface};

    use super::Canvas;
    use crate::testing::save_png;
    use crate::{ColorStop, Paint, Point, Rect};

    const WHITE: Rgba = Rgba::solid(255, 255, 255);
    const RED: Rgba = Rgba::solid(255, 0, 0);
    const BLUE: Rgba = Rgba::solid(0, 0, 255);

    fn pixel(surface: &Surface, x: u32, y: u32) -> Rgba {
        surface
            .get(PixelPos::new(x, y))
            .expect("tests read pixels inside the surface")
    }

    fn near(actual: u8, expected: u8, tolerance: u8) -> bool {
        actual.abs_diff(expected) <= tolerance
    }

    fn surface_of(canvas: &Canvas) -> Surface {
        canvas
            .to_surface()
            .expect("the canvas buffer matches its own dimensions")
    }

    #[test]
    fn rounded_rects_cut_their_corners() {
        let mut canvas = Canvas::new(40, 40).expect("40x40 is a valid canvas size");
        canvas.fill_round_rect(Rect::new(0.0, 0.0, 40.0, 40.0), 12.0, &Paint::Solid(WHITE));
        let surface = surface_of(&canvas);
        for (x, y) in [(0, 0), (39, 0), (0, 39), (39, 39)] {
            assert_eq!(pixel(&surface, x, y).a, 0, "corner {x},{y}");
        }
        assert_eq!(pixel(&surface, 20, 0).a, 255);
        assert_eq!(pixel(&surface, 0, 20).a, 255);
        assert!(pixel(&surface, 4, 4).a > 200, "inside the arc");
        save_png("canvas_round_rect.png", &surface);
    }

    #[test]
    fn strokes_stay_inside_the_rect_and_leave_the_middle_alone() {
        let mut canvas = Canvas::new(40, 40).expect("40x40 is a valid canvas size");
        canvas.stroke_round_rect(
            Rect::new(0.0, 0.0, 40.0, 40.0),
            6.0,
            2.0,
            &Paint::Solid(RED),
        );
        let surface = surface_of(&canvas);
        assert_eq!(pixel(&surface, 20, 0).a, 255);
        assert_eq!(pixel(&surface, 20, 2).a, 0);
        assert_eq!(pixel(&surface, 20, 20).a, 0);
    }

    #[test]
    fn linear_gradients_interpolate_between_stops() {
        let mut canvas = Canvas::new(256, 4).expect("256x4 is a valid canvas size");
        let paint = Paint::Linear {
            from: Point::new(0.0, 0.0),
            to: Point::new(256.0, 0.0),
            stops: vec![
                ColorStop {
                    offset: 0.0,
                    color: Rgba::solid(0, 0, 0),
                },
                ColorStop {
                    offset: 1.0,
                    color: WHITE,
                },
            ],
        };
        canvas.fill_rect(Rect::new(0.0, 0.0, 256.0, 4.0), &paint);
        let surface = surface_of(&canvas);
        assert!(near(pixel(&surface, 0, 1).r, 0, 2));
        assert!(near(pixel(&surface, 128, 1).r, 128, 3));
        assert!(near(pixel(&surface, 255, 1).r, 255, 2));
        let reds: Vec<u8> = (0..256).map(|x| pixel(&surface, x, 1).r).collect();
        assert!(reds.windows(2).all(|pair| pair[0] <= pair[1]), "monotonic");
        save_png("canvas_gradient.png", &surface);
    }

    #[test]
    fn half_transparent_colors_convert_to_straight_alpha() {
        let color = Rgba {
            r: 200,
            g: 100,
            b: 50,
            a: 128,
        };
        let mut canvas = Canvas::new(4, 4).expect("4x4 is a valid canvas size");
        canvas.clear(color);
        let cleared = pixel(&surface_of(&canvas), 1, 1);
        assert_eq!(cleared.a, 128);
        assert!(near(cleared.r, 200, 1) && near(cleared.g, 100, 1) && near(cleared.b, 50, 1));

        let mut canvas = Canvas::new(4, 4).expect("4x4 is a valid canvas size");
        canvas.fill_rect(Rect::new(0.0, 0.0, 4.0, 4.0), &Paint::Solid(color));
        let filled = pixel(&surface_of(&canvas), 1, 1);
        assert!(near(filled.a, 128, 1));
        assert!(near(filled.r, 200, 2) && near(filled.g, 100, 2) && near(filled.b, 50, 2));
    }

    #[test]
    fn the_clip_stack_limits_shapes_and_restores() {
        let mut canvas = Canvas::new(20, 20).expect("20x20 is a valid canvas size");
        canvas.push_clip(Rect::new(5.0, 5.0, 10.0, 10.0));
        canvas.push_clip(Rect::new(8.0, 0.0, 20.0, 20.0));
        canvas.fill_round_rect(Rect::new(0.0, 0.0, 20.0, 20.0), 1.0, &Paint::Solid(RED));
        canvas.pop_clip();
        canvas.fill_rect(Rect::new(0.0, 16.0, 20.0, 4.0), &Paint::Solid(BLUE));
        canvas.fill_rect(Rect::new(5.0, 5.0, 1.0, 10.0), &Paint::Solid(WHITE));
        canvas.pop_clip();
        canvas.fill_rect(Rect::new(0.0, 0.0, 2.0, 2.0), &Paint::Solid(BLUE));
        let surface = surface_of(&canvas);
        assert_eq!(pixel(&surface, 10, 10), RED);
        assert_eq!(pixel(&surface, 6, 10).a, 0, "inner clip");
        assert_eq!(pixel(&surface, 16, 10).a, 0, "outer clip");
        assert_eq!(pixel(&surface, 10, 17).a, 0, "outer clip still active");
        assert_eq!(pixel(&surface, 5, 10), WHITE, "popped back to outer clip");
        assert_eq!(pixel(&surface, 1, 1), BLUE, "fully popped");
    }

    #[test]
    fn surfaces_scale_into_a_rounded_rectangle() {
        let image = Surface::from_rgba(2, 1, vec![255, 0, 0, 255, 0, 0, 255, 255])
            .expect("8 bytes fill a 2x1 surface");
        let mut canvas = Canvas::new(40, 40).expect("40x40 is a valid canvas size");
        canvas.draw_surface(&image, Rect::new(0.0, 0.0, 40.0, 40.0), 10.0);
        let surface = surface_of(&canvas);
        assert_eq!(pixel(&surface, 0, 0).a, 0, "rounded corner");
        let left = pixel(&surface, 2, 20);
        let right = pixel(&surface, 37, 20);
        assert!(left.r > 200 && left.b < 55, "{left:?}");
        assert!(right.b > 200 && right.r < 55, "{right:?}");
        save_png("canvas_image.png", &surface);
    }

    #[test]
    fn large_downscales_average_instead_of_aliasing() {
        let pixels = (0..64u32 * 64)
            .flat_map(|i| {
                let value = if (i % 64 + i / 64) % 2 == 0 { 255 } else { 0 };
                [value, value, value, 255]
            })
            .collect();
        let image =
            Surface::from_rgba(64, 64, pixels).expect("the checker yields 4 bytes per pixel");
        let mut canvas = Canvas::new(8, 8).expect("8x8 is a valid canvas size");
        canvas.draw_surface(&image, Rect::new(0.0, 0.0, 8.0, 8.0), 0.0);
        let middle = pixel(&surface_of(&canvas), 4, 4);
        assert!(near(middle.r, 128, 6), "{middle:?}");
    }

    #[test]
    fn inverted_clips_hide_everything_and_stay_nestable() {
        let mut canvas = Canvas::new(8, 8).expect("8x8 is a valid canvas size");
        canvas.push_clip(Rect::new(6.0, 6.0, -4.0, -4.0));
        canvas.push_clip(Rect::new(0.0, 0.0, 8.0, 8.0));
        canvas.fill_rect(Rect::new(0.0, 0.0, 8.0, 8.0), &Paint::Solid(RED));
        canvas.pop_clip();
        canvas.pop_clip();
        assert_eq!(pixel(&surface_of(&canvas), 4, 4).a, 0);
    }

    #[test]
    fn zero_size_canvases_are_rejected() {
        assert!(Canvas::new(0, 10).is_err());
    }
}
