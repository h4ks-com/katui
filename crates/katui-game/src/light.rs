// pixel sizes stay far below 2^24, and float-to-integer casts saturate, so
// a coordinate left of or above the buffer becomes index 0
#![allow(clippy::cast_precision_loss, clippy::cast_sign_loss)]

use std::cell::RefCell;

use katui_draw::{Canvas, Paint, Point, Rgba, TextEngine, TextLayout, TextStyle};
use katui_scene::surface::Surface;

use crate::font::{ADVANCE, GLYPH_ROWS, glyph};

thread_local! {
    // We load the fonts once per thread, since parsing them takes far longer than a frame.
    static UI_FONT: RefCell<Option<TextEngine>> =
        RefCell::new(TextEngine::new(katui_draw::bundled_fonts()).ok());
}

/// What `use_layout` makes of `text` laid out in the UI font at `size`
/// pixels, or `None` when the font failed to load.
fn with_ui_layout<T>(
    text: &str,
    size: f32,
    use_layout: impl FnOnce(&TextLayout) -> T,
) -> Option<T> {
    UI_FONT.with_borrow_mut(|engine| {
        let layout = engine.as_mut()?.layout(text, &TextStyle::new(size), None);
        Some(use_layout(&layout))
    })
}

fn rasterized(layout: &TextLayout) -> Option<Surface> {
    let (width, height) = layout.size();
    let mut canvas =
        Canvas::new(width.ceil().max(1.0) as u32, height.ceil().max(1.0) as u32).ok()?;
    let ink = Paint::Solid(Rgba::solid(255, 255, 255));
    canvas.draw_text(layout, Point::new(0.0, 0.0), &ink);
    canvas.to_surface().ok()
}

/// Linear light; values above 1 bloom.
pub type Rgb = [f32; 3];

/// `color` with every channel multiplied by `factor`.
#[must_use]
pub fn scaled(color: Rgb, factor: f32) -> Rgb {
    color.map(|channel| channel * factor)
}

/// The light that shows as the sRGB color `red`, `green`, `blue` once
/// tone-mapped, for art picked in ordinary color values.
#[must_use]
pub fn srgb(red: u8, green: u8, blue: u8) -> Rgb {
    [red, green, blue].map(|byte| {
        let shown = ((f32::from(byte) + 0.02) / 255.0).min(0.99996);
        shown * shown / (1.0 - shown * shown)
    })
}

const BLOOM_STEP: usize = 4;
const BLUR_KEEP: f32 = 0.55;
const MAX_POLYGON_EDGES: usize = 8;
const DITHER: [f32; 16] = [
    0.0, 8.0, 2.0, 10.0, 12.0, 4.0, 14.0, 6.0, 3.0, 11.0, 1.0, 9.0, 15.0, 7.0, 13.0, 5.0,
];

/// A position on screen, in pixels.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct P2 {
    pub x: f32,
    pub y: f32,
}

impl P2 {
    #[must_use]
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

/// How new light meets what is already there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Blend {
    /// Replace it, for solid things.
    Over,
    /// Add to it, for things that glow.
    Add,
}

#[derive(Debug, Clone, Copy, Default)]
struct Edge {
    normal_x: f32,
    normal_y: f32,
    offset: f32,
}

impl Edge {
    fn between(from: P2, to: P2, clockwise: bool) -> Self {
        let (dx, dy) = (to.x - from.x, to.y - from.y);
        let length = (dx * dx + dy * dy).sqrt().max(1e-4);
        let sign = if clockwise { 1.0 } else { -1.0 };
        let (normal_x, normal_y) = (-dy / length * sign, dx / length * sign);
        Self {
            normal_x,
            normal_y,
            offset: -(normal_x * from.x + normal_y * from.y),
        }
    }

    fn distance(&self, x: f32, y: f32) -> f32 {
        self.normal_x * x + self.normal_y * y + self.offset
    }
}

#[derive(Debug, Clone, Copy)]
struct Bounds {
    left: usize,
    top: usize,
    right: usize,
    bottom: usize,
}

/// A floating-point framebuffer: games add light to it, bloom it and
/// tone-map it down to the bytes a terminal shows.
#[derive(Debug, Clone)]
pub struct Light {
    width: usize,
    height: usize,
    pixel_aspect: f32,
    pixels: Vec<Rgb>,
    bloom: Vec<Rgb>,
}

impl Light {
    /// A black buffer whose pixels are `pixel_aspect` as wide as they are
    /// tall.
    #[must_use]
    pub fn new(width: u32, height: u32, pixel_aspect: f32) -> Self {
        let (width, height) = (width.max(1) as usize, height.max(1) as usize);
        let (bloom_width, bloom_height) = bloom_size(width, height);
        Self {
            width,
            height,
            pixel_aspect,
            pixels: vec![[0.0; 3]; width * height],
            bloom: vec![[0.0; 3]; bloom_width * bloom_height],
        }
    }

    #[must_use]
    pub fn width(&self) -> u32 {
        self.width as u32
    }

    #[must_use]
    pub fn height(&self) -> u32 {
        self.height as u32
    }

    /// One pixel's width over its height.
    #[must_use]
    pub fn pixel_aspect(&self) -> f32 {
        self.pixel_aspect
    }

    /// Become a copy of `other`, the same size as this one.
    pub fn copy_from(&mut self, other: &Self) {
        self.pixels.clone_from(&other.pixels);
    }

    /// Fill every pixel with `shade` of `x` and `y`, for baked backdrops.
    pub fn paint(&mut self, shade: impl Fn(usize, usize) -> Rgb) {
        for (y, row) in self.pixels.chunks_exact_mut(self.width).enumerate() {
            for (x, pixel) in row.iter_mut().enumerate() {
                *pixel = shade(x, y);
            }
        }
    }

    /// Multiply all light by `factor`.
    pub fn dim(&mut self, factor: f32) {
        for pixel in &mut self.pixels {
            *pixel = scaled(*pixel, factor);
        }
    }

    fn index(&self, x: u32, y: u32) -> Option<usize> {
        let (x, y) = (x as usize, y as usize);
        (x < self.width && y < self.height).then_some(y * self.width + x)
    }

    /// The light at `x`, `y`, or `None` outside the buffer.
    #[must_use]
    pub fn pixel(&self, x: u32, y: u32) -> Option<Rgb> {
        self.pixels.get(self.index(x, y)?).copied()
    }

    /// Set the light at `x`, `y`; a spot outside the buffer is ignored.
    pub fn set_pixel(&mut self, x: u32, y: u32, color: Rgb) {
        if let Some(pixel) = self.index(x, y).and_then(|at| self.pixels.get_mut(at)) {
            *pixel = color;
        }
    }

    /// Draw `image` with its top-left at `at`, one image pixel per buffer
    /// pixel, over what is there by its alpha. The image's colors are
    /// sRGB, so they show as they are; the parts off the buffer are cut.
    pub fn image(&mut self, at: P2, image: &Surface) {
        self.stamp(at, image, |[red, green, blue, _]| srgb(red, green, blue));
    }

    /// Lay `image` over the buffer, its pixels colored by `ink` and covering
    /// by their alpha.
    fn stamp(&mut self, at: P2, image: &Surface, ink: impl Fn([u8; 4]) -> Rgb) {
        let at = P2::new(at.x.round(), at.y.round());
        let far = P2::new(at.x + image.width() as f32, at.y + image.height() as f32);
        let Some(bounds) = self.bounds(at, far) else {
            return;
        };
        let (pixels, row_width) = (image.pixels(), image.width() as usize);
        self.for_each_pixel(bounds, |x, y, pixel| {
            let start = ((y - at.y) as usize * row_width + (x - at.x) as usize) * 4;
            let rgba = pixels.get(start..start + 4).map(<[u8; 4]>::try_from);
            let Some(Ok(source)) = rgba else {
                return;
            };
            let [.., alpha] = source;
            if alpha > 0 {
                mix(pixel, ink(source), f32::from(alpha) / 255.0, Blend::Over);
            }
        });
    }

    /// Draw `text` in the bundled UI font, `size` buffer pixels tall, with
    /// its top-left at `at`, over what is there. It renders smooth and
    /// proportional, for a sharp HUD beside the pixel font's retro look.
    pub fn ui_text(&mut self, at: P2, text: &str, size: f32, color: Rgb) {
        let Some(drawn) = with_ui_layout(text, size, rasterized).flatten() else {
            return;
        };
        let wide = (drawn.width() as f32 / self.pixel_aspect).round().max(1.0) as u32;
        let fitted = if wide == drawn.width() {
            Some(drawn)
        } else {
            drawn.scaled(wide, drawn.height())
        };
        if let Some(fitted) = fitted {
            self.stamp(at, &fitted, |_| color);
        }
    }

    /// How many pixels wide `text` is in the UI font at `size`.
    #[must_use]
    pub fn ui_text_width(&self, text: &str, size: f32) -> f32 {
        with_ui_layout(text, size, |layout| layout.size().0).unwrap_or(0.0) / self.pixel_aspect
    }

    fn bounds(&self, low: P2, high: P2) -> Option<Bounds> {
        let bounds = Bounds {
            left: low.x.floor() as usize,
            top: low.y.floor() as usize,
            right: (high.x.ceil().max(0.0) as usize).min(self.width),
            bottom: (high.y.ceil().max(0.0) as usize).min(self.height),
        };
        (bounds.left < bounds.right && bounds.top < bounds.bottom).then_some(bounds)
    }

    fn for_each_pixel(&mut self, bounds: Bounds, mut visit: impl FnMut(f32, f32, &mut Rgb)) {
        let rows = self
            .pixels
            .chunks_exact_mut(self.width)
            .enumerate()
            .take(bounds.bottom)
            .skip(bounds.top);
        for (y, row) in rows {
            let span = row.iter_mut().enumerate().take(bounds.right);
            for (x, pixel) in span.skip(bounds.left) {
                visit(x as f32 + 0.5, y as f32 + 0.5, pixel);
            }
        }
    }

    /// Fill an axis-aligned rectangle.
    pub fn rect(&mut self, at: P2, width: f32, height: f32, color: Rgb, blend: Blend) {
        if let Some(bounds) = self.bounds(at, P2::new(at.x + width, at.y + height)) {
            self.for_each_pixel(bounds, |_, _, pixel| mix(pixel, color, 1.0, blend));
        }
    }

    /// Fill the convex polygon through `points`, with smooth edges.
    pub fn polygon(&mut self, points: &[P2], color: Rgb, blend: Blend) {
        let clockwise = signed_area(points) > 0.0;
        let mut edges = [Edge::default(); MAX_POLYGON_EDGES];
        let count = points.len().min(MAX_POLYGON_EDGES);
        let ends = points.iter().cycle().skip(1);
        for (edge, (from, to)) in edges.iter_mut().zip(points.iter().zip(ends)) {
            *edge = Edge::between(*from, *to, clockwise);
        }
        let Some(bounds) = polygon_box(points).and_then(|(low, high)| self.bounds(low, high))
        else {
            return;
        };
        self.for_each_pixel(bounds, |x, y, pixel| {
            let inside = edges
                .iter()
                .take(count)
                .fold(1.0_f32, |least, edge| least.min(edge.distance(x, y) + 0.5));
            if inside > 0.0 {
                mix(pixel, color, inside, blend);
            }
        });
    }

    /// Add a smooth line `width` pixels wide.
    pub fn line(&mut self, from: P2, to: P2, width: f32, color: Rgb) {
        let reach = width / 2.0 + 1.0;
        let low = P2::new(from.x.min(to.x) - reach, from.y.min(to.y) - reach);
        let high = P2::new(from.x.max(to.x) + reach, from.y.max(to.y) + reach);
        let Some(bounds) = self.bounds(low, high) else {
            return;
        };
        let (dx, dy) = (to.x - from.x, to.y - from.y);
        let length_squared = (dx * dx + dy * dy).max(1e-4);
        self.for_each_pixel(bounds, |x, y, pixel| {
            let along = (((x - from.x) * dx + (y - from.y) * dy) / length_squared).clamp(0.0, 1.0);
            let (off_x, off_y) = (x - from.x - dx * along, y - from.y - dy * along);
            let coverage = reach - 0.5 - (off_x * off_x + off_y * off_y).sqrt();
            if coverage > 0.0 {
                mix(pixel, color, coverage.min(1.0), Blend::Add);
            }
        });
    }

    fn radial_box(&self, center: P2, radius: f32) -> Option<Bounds> {
        let reach_x = radius / self.pixel_aspect;
        self.bounds(
            P2::new(center.x - reach_x, center.y - radius),
            P2::new(center.x + reach_x, center.y + radius),
        )
    }

    /// Add a soft round glow.
    pub fn glow(&mut self, center: P2, radius: f32, color: Rgb) {
        let Some(bounds) = self.radial_box(center, radius) else {
            return;
        };
        let aspect = self.pixel_aspect;
        let inverse = 1.0 / (radius * radius).max(1e-3);
        self.for_each_pixel(bounds, |x, y, pixel| {
            let (dx, dy) = ((x - center.x) * aspect, y - center.y);
            let falloff = 1.0 - (dx * dx + dy * dy) * inverse;
            if falloff > 0.0 {
                mix(pixel, color, falloff * falloff, Blend::Add);
            }
        });
    }

    /// Add a soft ring.
    pub fn ring(&mut self, center: P2, radius: f32, width: f32, color: Rgb) {
        let Some(bounds) = self.radial_box(center, radius + width) else {
            return;
        };
        let aspect = self.pixel_aspect;
        self.for_each_pixel(bounds, |x, y, pixel| {
            let (dx, dy) = ((x - center.x) * aspect, y - center.y);
            let off = 1.0 - ((dx * dx + dy * dy).sqrt() - radius).abs() / width.max(0.5);
            if off > 0.0 {
                mix(pixel, color, off * off, Blend::Add);
            }
        });
    }

    /// Add `text` with its top-left at `at`, glyph pixels `scale` tall.
    pub fn text(&mut self, at: P2, text: &str, scale: u32, color: Rgb) {
        self.blended_text(at, text, scale, color, Blend::Add);
    }

    /// Draw `text` like [`Light::text`], meeting what is there with `blend`.
    pub fn blended_text(&mut self, at: P2, text: &str, scale: u32, color: Rgb, blend: Blend) {
        // We start on a whole pixel because a fractional start smears glyphs together.
        let at = P2::new(at.x.round(), at.y.round());
        let tall = scale.max(1) as f32;
        let wide = (tall / self.pixel_aspect).round().max(1.0);
        for (index, letter) in text.chars().enumerate() {
            let left = at.x + (index as u32 * ADVANCE) as f32 * wide;
            let Some(rows) = glyph(letter) else {
                continue;
            };
            for (row, bits) in rows.iter().enumerate() {
                let top = at.y + row as f32 * tall;
                for column in (0..5_u8).filter(|column| bits >> (4 - column) & 1 == 1) {
                    let spot = P2::new(left + f32::from(column) * wide, top);
                    self.rect(spot, wide, tall, color, blend);
                }
            }
        }
    }

    /// Add `text` centered on `center_x`.
    pub fn centered_text(&mut self, center_x: f32, y: f32, text: &str, scale: u32, color: Rgb) {
        let width = self.text_width(text, scale);
        self.text(P2::new(center_x - width / 2.0, y), text, scale, color);
    }

    /// How many pixels wide `text` is.
    #[must_use]
    pub fn text_width(&self, text: &str, scale: u32) -> f32 {
        let wide = (scale.max(1) as f32 / self.pixel_aspect).round().max(1.0);
        (text.chars().count() as u32 * ADVANCE) as f32 * wide - wide
    }

    /// How many pixels tall a line of text is.
    #[must_use]
    pub fn text_height(scale: u32) -> f32 {
        (GLYPH_ROWS * scale.max(1)) as f32
    }

    /// Spread light brighter than `threshold` into its surroundings.
    pub fn bloom(&mut self, threshold: f32, strength: f32) {
        self.gather_bright(threshold);
        self.blur_bloom();
        self.add_bloom(strength);
    }

    fn bloom_width(&self) -> usize {
        bloom_size(self.width, self.height).0
    }

    fn gather_bright(&mut self, threshold: f32) {
        let small_width = self.bloom_width();
        let sampled = self.pixels.chunks_exact(self.width).step_by(BLOOM_STEP);
        for (small, row) in self.bloom.chunks_exact_mut(small_width).zip(sampled) {
            for (target, block) in small.iter_mut().zip(row.chunks(BLOOM_STEP)) {
                let sum = block
                    .iter()
                    .fold([0.0; 3], |sum, pixel| add3(sum, bright(*pixel, threshold)));
                *target = sum.map(|channel| channel / BLOOM_STEP as f32);
            }
        }
    }

    fn blur_bloom(&mut self) {
        let small_width = self.bloom_width();
        for row in self.bloom.chunks_exact_mut(small_width) {
            smooth_forward(row.iter_mut());
            smooth_forward(row.iter_mut().rev());
        }
        let mut carried = vec![[0.0; 3]; small_width];
        for row in self.bloom.chunks_exact_mut(small_width) {
            smooth_line(&mut carried, row);
        }
        carried.fill([0.0; 3]);
        for row in self.bloom.chunks_exact_mut(small_width).rev() {
            smooth_line(&mut carried, row);
        }
    }

    fn add_bloom(&mut self, strength: f32) {
        let small_width = self.bloom_width();
        let bloom = &self.bloom;
        for (y, row) in self.pixels.chunks_exact_mut(self.width).enumerate() {
            let line = y / BLOOM_STEP * small_width;
            let (Some(upper), Some(lower)) = (
                bloom.get(line..line + small_width),
                bloom.get(line + small_width..line + small_width * 2),
            ) else {
                continue;
            };
            let down = (y % BLOOM_STEP) as f32 / BLOOM_STEP as f32;
            let pairs = upper.windows(2).zip(lower.windows(2));
            for (block, (top, bottom)) in row.chunks_mut(BLOOM_STEP).zip(pairs) {
                add_block(block, top, bottom, down, strength);
            }
        }
    }

    /// Tone-map to the bytes a terminal shows. The dither pattern depends
    /// only on position, so unchanged areas stay byte-identical and cost
    /// nothing to present.
    #[must_use]
    pub fn to_surface(&self) -> Surface {
        let mut bytes = Vec::with_capacity(self.pixels.len() * 4);
        for (y, row) in self.pixels.chunks_exact(self.width).enumerate() {
            for (x, pixel) in row.iter().enumerate() {
                let threshold = DITHER.get((x & 3) | ((y & 3) << 2)).copied().unwrap_or(0.0);
                let nudge = threshold / 16.0;
                bytes.extend(pixel.map(|channel| tone_mapped(channel, nudge)));
                bytes.push(255);
            }
        }
        Surface::from_rgba(self.width as u32, self.height as u32, bytes)
            .unwrap_or_else(|| Surface::new(self.width as u32, self.height as u32))
    }
}

fn tone_mapped(channel: f32, nudge: f32) -> u8 {
    let linear = channel.max(0.0);
    ((linear / (1.0 + linear)).sqrt() * 255.0 + nudge).min(255.0) as u8
}

fn mix(pixel: &mut Rgb, color: Rgb, coverage: f32, blend: Blend) {
    let coverage = coverage.min(1.0);
    for (channel, new) in pixel.iter_mut().zip(color) {
        match blend {
            Blend::Add => *channel += new * coverage,
            Blend::Over => *channel += (new - *channel) * coverage,
        }
    }
}

fn add3(sum: Rgb, other: Rgb) -> Rgb {
    let [red, green, blue] = sum;
    let [more_red, more_green, more_blue] = other;
    [red + more_red, green + more_green, blue + more_blue]
}

fn bright(pixel: Rgb, threshold: f32) -> Rgb {
    pixel.map(|channel| (channel - threshold).max(0.0))
}

fn bloom_size(width: usize, height: usize) -> (usize, usize) {
    (width / BLOOM_STEP + 1, height / BLOOM_STEP + 1)
}

fn smooth_forward<'a>(pixels: impl Iterator<Item = &'a mut Rgb>) {
    let mut carried = [0.0_f32; 3];
    for pixel in pixels {
        blur_step(&mut carried, pixel);
    }
}

fn smooth_line(carried: &mut [Rgb], row: &mut [Rgb]) {
    for (held, pixel) in carried.iter_mut().zip(row) {
        blur_step(held, pixel);
    }
}

fn blur_step(carried: &mut Rgb, pixel: &mut Rgb) {
    *carried = add3(scaled(*carried, BLUR_KEEP), scaled(*pixel, 1.0 - BLUR_KEEP));
    *pixel = *carried;
}

fn add_block(block: &mut [Rgb], top: &[Rgb], bottom: &[Rgb], down: f32, strength: f32) {
    let (&[top_left, top_right], &[bottom_left, bottom_right]) = (
        <&[Rgb; 2]>::try_from(top).unwrap_or(&[[0.0; 3]; 2]),
        <&[Rgb; 2]>::try_from(bottom).unwrap_or(&[[0.0; 3]; 2]),
    );
    if is_dark(&[top_left, top_right, bottom_left, bottom_right]) {
        return;
    }
    let left = lerp3(top_left, bottom_left, down);
    let right = lerp3(top_right, bottom_right, down);
    for (index, pixel) in block.iter_mut().enumerate() {
        let across = index as f32 / BLOOM_STEP as f32;
        let bloom = lerp3(left, right, across);
        *pixel = add3(*pixel, bloom.map(|channel| channel * strength));
    }
}

fn is_dark(corners: &[Rgb; 4]) -> bool {
    corners
        .iter()
        .all(|corner| corner.iter().all(|channel| *channel < 0.002))
}

/// The color `amount` of the way from `from` to `to`.
#[must_use]
pub fn lerp3(from: Rgb, to: Rgb, amount: f32) -> Rgb {
    let mut mixed = from;
    mix(&mut mixed, to, amount, Blend::Over);
    mixed
}

fn signed_area(points: &[P2]) -> f32 {
    points
        .iter()
        .zip(points.iter().cycle().skip(1))
        .map(|(from, to)| from.x * to.y - to.x * from.y)
        .sum()
}

fn polygon_box(points: &[P2]) -> Option<(P2, P2)> {
    let first = *points.first()?;
    Some(points.iter().fold((first, first), |(low, high), point| {
        (
            P2::new(low.x.min(point.x), low.y.min(point.y)),
            P2::new(high.x.max(point.x), high.y.max(point.y)),
        )
    }))
}

#[cfg(test)]
mod tests {
    use katui_scene::surface::{PixelPos, Surface};

    use super::{Blend, Light, P2, srgb};

    fn red_at(light: &Light, x: u32, y: u32) -> u8 {
        light
            .to_surface()
            .get(PixelPos::new(x, y))
            .expect("the pixel is inside")
            .r
    }

    #[test]
    fn a_polygon_covers_its_inside_whatever_its_winding() {
        let square = [
            P2::new(2.0, 2.0),
            P2::new(8.0, 2.0),
            P2::new(8.0, 8.0),
            P2::new(2.0, 8.0),
        ];
        let reversed = [square[3], square[2], square[1], square[0]];
        for points in [square, reversed] {
            let mut light = Light::new(10, 10, 1.0);
            light.polygon(&points, [1.0, 0.0, 0.0], Blend::Over);
            assert!(red_at(&light, 5, 5) > 100, "the center is filled");
            assert_eq!(red_at(&light, 0, 0), 0, "the corner stays dark");
        }
    }

    #[test]
    fn shapes_off_screen_are_ignored() {
        let mut light = Light::new(10, 10, 1.0);
        light.line(P2::new(-50.0, -50.0), P2::new(-40.0, -40.0), 2.0, [1.0; 3]);
        light.glow(P2::new(500.0, 500.0), 5.0, [1.0; 3]);
        light.text(P2::new(500.0, 500.0), "A", 1, [1.0; 3]);
        assert_eq!(red_at(&light, 5, 5), 0);
    }

    #[test]
    fn bloom_spreads_only_bright_light() {
        let mut dim = Light::new(64, 64, 1.0);
        dim.rect(P2::new(30.0, 30.0), 4.0, 4.0, [0.3; 3], Blend::Add);
        dim.bloom(1.0, 1.0);
        assert_eq!(red_at(&dim, 24, 31), 0);

        let mut hot = Light::new(64, 64, 1.0);
        hot.rect(P2::new(30.0, 30.0), 4.0, 4.0, [40.0; 3], Blend::Add);
        hot.bloom(1.0, 1.0);
        assert!(red_at(&hot, 24, 31) > 0, "light reaches past the block");
    }

    #[test]
    fn an_unchanged_buffer_tone_maps_to_the_same_bytes() {
        let mut light = Light::new(32, 32, 0.5);
        light.glow(P2::new(16.0, 16.0), 12.0, [0.7, 0.2, 0.4]);
        assert_eq!(light.to_surface().pixels(), light.to_surface().pixels());
    }

    #[test]
    fn an_srgb_color_tone_maps_back_to_itself() {
        for value in [0_u8, 1, 17, 128, 200, 254] {
            let mut light = Light::new(4, 4, 1.0);
            light.rect(
                P2::new(0.0, 0.0),
                4.0,
                4.0,
                srgb(value, value, value),
                Blend::Over,
            );
            for y in 0..4 {
                for x in 0..4 {
                    assert_eq!(red_at(&light, x, y), value, "{value} at {x},{y}");
                }
            }
        }
    }

    fn sprite() -> Surface {
        let opaque_red = [255, 0, 0, 255];
        let half_blue = [0, 0, 255, 128];
        let clear = [0, 0, 0, 0];
        let pixels = [opaque_red, half_blue, clear, opaque_red].concat();
        Surface::from_rgba(2, 2, pixels).expect("2 by 2 pixels of 4 bytes")
    }

    #[test]
    fn an_image_blends_by_its_alpha_and_clips_at_the_edges() {
        let mut light = Light::new(4, 4, 1.0);
        light.rect(P2::new(0.0, 0.0), 4.0, 4.0, srgb(0, 255, 0), Blend::Over);
        light.image(P2::new(1.0, 1.0), &sprite());
        assert_eq!(red_at(&light, 1, 1), 255, "an opaque pixel covers");
        let half = light.to_surface().get(PixelPos::new(2, 1)).expect("inside");
        assert!(half.b > 100 && half.g > 100, "a half-clear pixel mixes");
        assert_eq!(
            light.pixel(1, 2),
            light.pixel(0, 0),
            "a clear pixel leaves it"
        );
        light.image(P2::new(2.0, -1.0), &sprite());
        assert_eq!(red_at(&light, 3, 0), 255, "the part on the buffer lands");
        light.image(P2::new(-9.0, 9.0), &sprite());
    }

    #[test]
    fn a_pixel_reads_back_what_was_set_and_nothing_outside() {
        let mut light = Light::new(3, 2, 1.0);
        light.set_pixel(2, 1, [0.5, 0.25, 1.0]);
        light.set_pixel(3, 0, [9.0; 3]);
        assert_eq!(light.pixel(2, 1), Some([0.5, 0.25, 1.0]));
        assert_eq!(light.pixel(0, 0), Some([0.0; 3]));
        assert_eq!(light.pixel(3, 0), None);
    }

    #[test]
    fn ui_text_draws_smooth_glyphs_as_wide_as_it_measures() {
        let mut light = Light::new(120, 30, 1.0);
        let width = light.ui_text_width("Score 42", 20.0);
        assert!(width > 40.0 && width < 120.0, "{width}");
        light.ui_text(P2::new(2.0, 2.0), "Score 42", 20.0, [1.0; 3]);
        let lit = |x: u32| (0..30).any(|y| red_at(&light, x, y) > 0);
        assert!((2..2 + width as u32).any(lit));
        assert!(!lit(width as u32 + 6), "nothing past its width");
        let wide = Light::new(10, 10, 0.5).ui_text_width("Score 42", 20.0);
        assert!(
            (wide - width * 2.0).abs() < 1.0,
            "narrow pixels take twice as many"
        );
    }

    #[test]
    fn writes_the_text_and_image_drawing_as_a_png() {
        let mut light = Light::new(320, 90, 1.0);
        light.text(
            P2::new(4.0, 4.0),
            "ABC abc 0-9 !?.,:;'\"()[]{}<>/*+=_#%&@$^~|",
            1,
            [1.0; 3],
        );
        light.text(
            P2::new(4.0, 16.0),
            "Pixel font, scale 2",
            2,
            srgb(255, 200, 80),
        );
        light.ui_text(
            P2::new(4.0, 36.0),
            "UI font: Score 1,234",
            22.0,
            srgb(120, 200, 255),
        );
        let big = sprite().scaled(16, 16).expect("16 by 16 is a valid size");
        light.image(P2::new(290.0, 40.0), &big);
        let surface = light.to_surface();
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../target/screenshots");
        std::fs::create_dir_all(dir).expect("the target directory is writable");
        image::save_buffer(
            format!("{dir}/light-text.png"),
            surface.pixels(),
            surface.width(),
            surface.height(),
            image::ColorType::Rgba8,
        )
        .expect("the png writer saves under target");
    }

    #[test]
    fn text_lights_its_glyph_pixels() {
        let mut light = Light::new(40, 10, 1.0);
        light.text(P2::new(0.0, 0.0), "T", 1, [1.0, 1.0, 1.0]);
        assert!(red_at(&light, 2, 3) > 100, "the stem of the T");
        assert_eq!(red_at(&light, 0, 3), 0, "beside the stem");
    }
}
