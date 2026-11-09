#![allow(clippy::cast_precision_loss, clippy::cast_sign_loss)]

use katui_scene::surface::Surface;

use crate::font::{ADVANCE, GLYPH_ROWS, glyph};

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
    /// From the left.
    pub x: f32,
    /// From the top.
    pub y: f32,
}

impl P2 {
    /// A position at (`x`, `y`).
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
        Self {
            width,
            height,
            pixel_aspect,
            pixels: vec![[0.0; 3]; width * height],
            bloom: vec![[0.0; 3]; (width / BLOOM_STEP + 1) * (height / BLOOM_STEP + 1)],
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
            *pixel = pixel.map(|channel| channel * factor);
        }
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

    fn bloom_size(&self) -> (usize, usize) {
        (self.width / BLOOM_STEP + 1, self.height / BLOOM_STEP + 1)
    }

    fn gather_bright(&mut self, threshold: f32) {
        let (small_width, _) = self.bloom_size();
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
        let (small_width, _) = self.bloom_size();
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
        let (small_width, _) = self.bloom_size();
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

fn smooth_forward<'a>(pixels: impl Iterator<Item = &'a mut Rgb>) {
    let mut carried = [0.0_f32; 3];
    for pixel in pixels {
        carried = add3(
            carried.map(|channel| channel * BLUR_KEEP),
            pixel.map(|channel| channel * (1.0 - BLUR_KEEP)),
        );
        *pixel = carried;
    }
}

fn smooth_line(carried: &mut [Rgb], row: &mut [Rgb]) {
    for (held, pixel) in carried.iter_mut().zip(row) {
        *held = add3(
            held.map(|channel| channel * BLUR_KEEP),
            pixel.map(|channel| channel * (1.0 - BLUR_KEEP)),
        );
        *pixel = *held;
    }
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
    use katui_scene::surface::PixelPos;

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

    #[test]
    fn text_lights_its_glyph_pixels() {
        let mut light = Light::new(40, 10, 1.0);
        light.text(P2::new(0.0, 0.0), "T", 1, [1.0, 1.0, 1.0]);
        assert!(red_at(&light, 2, 3) > 100, "the stem of the T");
        assert_eq!(red_at(&light, 0, 3), 0, "beside the stem");
    }
}
