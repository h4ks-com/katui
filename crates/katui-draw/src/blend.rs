//! Pixel math on premultiplied RGBA8.

use crate::{Paint, Rgba};

type Pixel = [u8; 4];

const TRANSPARENT: Pixel = [0; 4];

#[allow(clippy::cast_precision_loss)] // Canvas coordinates stay far below 2^24, so we convert them exactly.
pub(crate) fn coordinate(value: u32) -> f32 {
    value as f32
}

fn mul255(a: u8, b: u8) -> u8 {
    let product = u32::from(a) * u32::from(b) + 128;
    ((product + (product >> 8)) >> 8) as u8
}

fn scale(pixel: Pixel, alpha: u8) -> Pixel {
    pixel.map(|channel| mul255(channel, alpha))
}

fn over(destination: &mut Pixel, source: Pixel) {
    let remaining = 255 - source[3];
    for (channel, added) in destination.iter_mut().zip(source) {
        *channel = added.saturating_add(mul255(*channel, remaining));
    }
}

fn premultiply([r, g, b, a]: Pixel) -> Pixel {
    [mul255(r, a), mul255(g, a), mul255(b, a), a]
}

pub(crate) fn demultiply(pixel: Pixel) -> Pixel {
    let [r, g, b, a] = pixel;
    match a {
        255 => pixel,
        0 => TRANSPARENT,
        _ => {
            let alpha = u32::from(a);
            let straight =
                |channel: u8| ((u32::from(channel) * 255 + alpha / 2) / alpha).min(255) as u8;
            [straight(r), straight(g), straight(b), a]
        }
    }
}

/// Writes `source` (straight alpha, `width` wide) into `target` as
/// premultiplied pixels, averaging each `reduction` x `reduction` block,
/// and returns the written dimensions.
pub(crate) fn premultiply_reduced(
    source: &[u8],
    width: u32,
    reduction: u32,
    target: &mut Vec<u8>,
) -> (u32, u32) {
    let reduction = reduction.max(1) as usize;
    let stride = width as usize * 4;
    let out_width = width as usize / reduction;
    let out_height = (source.len() / stride.max(1)) / reduction;
    target.clear();
    if reduction == 1 {
        target.resize(source.len(), 0);
        let source = source.as_chunks::<4>().0;
        for (out, pixel) in target.as_chunks_mut::<4>().0.iter_mut().zip(source) {
            *out = premultiply(*pixel);
        }
    } else {
        target.reserve(out_width * out_height * 4);
        for block in source.chunks_exact(stride * reduction) {
            for column in 0..out_width {
                target.extend(average_block(block, stride, column, reduction));
            }
        }
    }
    (out_width as u32, out_height as u32)
}

fn average_block(block: &[u8], stride: usize, column: usize, reduction: usize) -> Pixel {
    let span = column * reduction * 4..(column + 1) * reduction * 4;
    let mut sum = [0u32; 4];
    for row in block.chunks_exact(stride) {
        let pixels = row.get(span.clone()).unwrap_or(&[]).as_chunks::<4>().0;
        for pixel in pixels {
            for (total, channel) in sum.iter_mut().zip(premultiply(*pixel)) {
                *total += u32::from(channel);
            }
        }
    }
    let count = (reduction * reduction) as u32;
    sum.map(|total| ((total + count / 2) / count) as u8)
}

/// The premultiplied color text draws with.
pub(crate) struct Ink(Pixel);

impl Ink {
    pub(crate) fn new(paint: &Paint) -> Self {
        let color = match paint {
            Paint::Solid(color) => Some(*color),
            Paint::Linear { stops, .. } => stops.first().map(|stop| stop.color),
        };
        Self(color.map_or(TRANSPARENT, |color| premultiply(channels(color))))
    }

    /// Blends one glyph mask row.
    pub(crate) fn blend_mask(&self, row: &mut [Pixel], mask: &[u8]) {
        for (destination, coverage) in row.iter_mut().zip(mask) {
            over(destination, scale(self.0, *coverage));
        }
    }

    /// Blends one row of straight-alpha glyph color, keeping the glyph's
    /// own colors and taking only opacity from the paint.
    pub(crate) fn blend_color(&self, row: &mut [Pixel], colors: &[Pixel]) {
        let [.., opacity] = self.0;
        for (destination, color) in row.iter_mut().zip(colors) {
            over(destination, scale(premultiply(*color), opacity));
        }
    }
}

fn channels(color: Rgba) -> Pixel {
    [color.r, color.g, color.b, color.a]
}
