//! The sixel encoder: RGBA pixels to one DCS sixel image with a palette of
//! its own colors. We bucket colors at five bits a channel and drop a bit
//! at a time until the buckets fit the palette, so a UI tile keeps its
//! colors and a gradient keeps its shading. We round instead of dithering:
//! dithering puts many colors in every band, and a sixel band costs a line
//! of columns per color, so dithered frames run about ten times larger.

use std::fmt::Write as _;

/// The most palette entries an image uses.
const COLORS: usize = 256;
/// The first character of a sixel, which carries six vertical bits.
const SIXEL_BASE: u8 = 0x3f;

/// Encode `pixels`, `width` x `height` RGBA, as one sixel image drawn at
/// the cursor. Pixels whose alpha is below half stay transparent, and so
/// do the rows a final band covers below `height`. `None` when `pixels`
/// does not hold exactly `4 * width * height` bytes.
#[must_use]
pub fn encode(pixels: &[u8], width: u32, height: u32) -> Option<Vec<u8>> {
    let (columns, rows) = (width as usize, height as usize);
    if pixels.len() != columns.checked_mul(rows)?.checked_mul(4)? {
        return None;
    }
    let (palette, indices) = (2..=5)
        .rev()
        .find_map(|bits| quantize(pixels, bits))
        .unwrap_or_default();
    let mut text = format!("\x1bP0;1;0q\"1;1;{width};{height}");
    write_palette(&mut text, &palette, &indices);
    let mut band = Band::new(columns);
    for (band_index, band_rows) in indices.chunks(columns * 6).enumerate() {
        if band_index > 0 {
            text.push('-');
        }
        band.fill(band_rows);
        band.write(&mut text);
    }
    text.push_str("\x1b\\");
    Some(text.into_bytes())
}

/// The palette and every pixel's index into it, `None` where a pixel is
/// transparent.
type Quantized = (Vec<[u8; 3]>, Vec<Option<u8>>);

fn opaque([red, green, blue, alpha]: [u8; 4]) -> Option<[u8; 3]> {
    (alpha >= 128).then_some([red, green, blue])
}

/// Bucket the colors at `bits` a channel, each bucket shown as the mean
/// of its pixels; `None` when there are more buckets than palette entries.
fn quantize(pixels: &[u8], bits: u8) -> Option<Quantized> {
    let mut slot = vec![None::<u8>; 1 << (3 * bits)];
    let mut sums: Vec<([u32; 3], u32)> = Vec::new();
    let mut indices = Vec::with_capacity(pixels.len() / 4);
    for pixel in pixels.as_chunks::<4>().0 {
        let Some(rgb) = opaque(*pixel) else {
            indices.push(None);
            continue;
        };
        let bucket = rgb.iter().fold(0usize, |key, channel| {
            key << bits | usize::from(channel >> (8 - bits))
        });
        let index = if let Some(index) = slot.get(bucket).copied().flatten() {
            index
        } else {
            let index = u8::try_from(sums.len()).ok()?;
            sums.push(([0; 3], 0));
            if let Some(entry) = slot.get_mut(bucket) {
                *entry = Some(index);
            }
            index
        };
        if let Some((sum, count)) = sums.get_mut(usize::from(index)) {
            for (total, channel) in sum.iter_mut().zip(rgb) {
                *total += u32::from(channel);
            }
            *count += 1;
        }
        indices.push(Some(index));
    }
    let palette = sums
        .iter()
        .map(|(sum, count)| sum.map(|total| u8::try_from(total / (*count).max(1)).unwrap_or(255)))
        .collect();
    Some((palette, indices))
}

/// Define the colors `indices` use, so a small tile carries a small
/// palette.
fn write_palette(text: &mut String, palette: &[[u8; 3]], indices: &[Option<u8>]) {
    let mut used = [false; COLORS];
    for index in indices.iter().flatten() {
        if let Some(slot) = used.get_mut(usize::from(*index)) {
            *slot = true;
        }
    }
    let percent = |channel: u8| (u16::from(channel) * 100 + 127) / 255;
    for (index, [red, green, blue]) in palette.iter().enumerate() {
        if used.get(index) == Some(&true) {
            let _ = write!(
                text,
                "#{index};2;{};{};{}",
                percent(*red),
                percent(*green),
                percent(*blue)
            );
        }
    }
}

/// One band of six pixel rows, as the columns each color lights.
struct Band {
    columns: usize,
    bits: Vec<u8>,
    used: [bool; COLORS],
}

impl Band {
    fn new(columns: usize) -> Self {
        Self {
            columns,
            bits: vec![0; columns * COLORS],
            used: [false; COLORS],
        }
    }

    fn fill(&mut self, rows: &[Option<u8>]) {
        self.bits.fill(0);
        self.used = [false; COLORS];
        for (offset, color) in rows.iter().enumerate() {
            let Some(color) = color.map(usize::from) else {
                continue;
            };
            let (row, column) = (offset / self.columns, offset % self.columns);
            if let Some(cell) = self.bits.get_mut(color * self.columns + column) {
                *cell |= 1 << row;
            }
            if let Some(used) = self.used.get_mut(color) {
                *used = true;
            }
        }
    }

    fn write(&self, text: &mut String) {
        let mut first = true;
        for (color, used) in self.used.iter().enumerate() {
            if !used {
                continue;
            }
            if !first {
                text.push('$');
            }
            first = false;
            let _ = write!(text, "#{color}");
            let start = color * self.columns;
            write_runs(
                text,
                self.bits
                    .get(start..start + self.columns)
                    .unwrap_or_default(),
            );
        }
    }
}

/// One color's columns, repeats folded into `!count` runs.
fn write_runs(text: &mut String, columns: &[u8]) {
    let mut index = 0;
    while let Some(&bits) = columns.get(index) {
        let run = columns.get(index..).map_or(1, |rest| {
            rest.iter().take_while(|next| **next == bits).count()
        });
        let symbol = char::from(SIXEL_BASE + bits);
        if run > 3 {
            let _ = write!(text, "!{run}{symbol}");
        } else {
            for _ in 0..run {
                text.push(symbol);
            }
        }
        index += run;
    }
}

#[cfg(test)]
mod tests {
    use super::encode;

    fn solid(width: u32, height: u32, rgba: [u8; 4]) -> Vec<u8> {
        rgba.repeat((width * height) as usize)
    }

    #[test]
    fn a_solid_block_is_one_color_run_per_band() {
        let sixel = encode(&solid(8, 12, [255, 0, 0, 255]), 8, 12).expect("exact pixels");
        let text = String::from_utf8(sixel).expect("sixel is ascii");
        assert!(
            text.starts_with("\x1bP0;1;0q\"1;1;8;12#0;2;100;0;0#0"),
            "only red is defined: {text}"
        );
        assert!(
            text.ends_with("#0!8~-#0!8~\x1b\\"),
            "two full bands of pure red: {text}"
        );
    }

    #[test]
    fn a_short_band_lights_only_the_rows_it_has_and_alpha_stays_transparent() {
        let mut pixels = solid(2, 2, [0, 0, 255, 255]);
        pixels[4..8].copy_from_slice(&[0, 0, 0, 0]);
        let text = String::from_utf8(encode(&pixels, 2, 2).expect("exact pixels")).expect("ascii");
        assert!(
            text.ends_with("#0BA\x1b\\"),
            "column one has rows 0 and 1, column two row 1: {text}"
        );
    }

    /// A minimal decoder for the subset we emit: palette, colors,
    /// repeats, carriage returns and band ends.
    fn decode(text: &str, width: usize, height: usize) -> Vec<Option<[u8; 3]>> {
        let body = text.split_once('q').expect("a sixel header").1;
        let body = body.strip_suffix("\x1b\\").expect("a string end");
        let mut palette = vec![[0u8; 3]; 256];
        let mut image = vec![None; width * height];
        let (mut color, mut x, mut band) = (0usize, 0usize, 0usize);
        let mut chars = body.chars().peekable();
        let number = |chars: &mut std::iter::Peekable<std::str::Chars<'_>>| {
            let mut value = 0usize;
            while let Some(digit) = chars.peek().and_then(|c| c.to_digit(10)) {
                value = value * 10 + digit as usize;
                chars.next();
            }
            value
        };
        while let Some(c) = chars.next() {
            match c {
                '"' => {
                    for _ in 0..4 {
                        number(&mut chars);
                        if chars.peek() == Some(&';') {
                            chars.next();
                        }
                    }
                }
                '#' => {
                    color = number(&mut chars);
                    if chars.peek() == Some(&';') {
                        chars.next();
                        let mut fields = [0usize; 4];
                        for field in &mut fields {
                            *field = number(&mut chars);
                            if chars.peek() == Some(&';') {
                                chars.next();
                            }
                        }
                        palette[color] = [1, 2, 3].map(|i| (fields[i] * 255 / 100) as u8);
                    }
                }
                '$' => x = 0,
                '-' => {
                    x = 0;
                    band += 1;
                }
                '!' => {
                    let count = number(&mut chars);
                    let symbol = chars.next().expect("a repeated sixel");
                    for _ in 0..count {
                        paint(&mut image, width, height, x, band, symbol, palette[color]);
                        x += 1;
                    }
                }
                symbol => {
                    paint(&mut image, width, height, x, band, symbol, palette[color]);
                    x += 1;
                }
            }
        }
        image
    }

    fn paint(
        image: &mut [Option<[u8; 3]>],
        width: usize,
        height: usize,
        x: usize,
        band: usize,
        symbol: char,
        rgb: [u8; 3],
    ) {
        let bits = symbol as u8 - 0x3f;
        for row in 0..6 {
            let y = band * 6 + row;
            if bits & (1 << row) != 0 {
                assert!(x < width && y < height, "no pixel lands outside the image");
                image[y * width + x] = Some(rgb);
            }
        }
    }

    #[test]
    fn a_gradient_survives_a_round_trip_within_one_bucket() {
        let (width, height) = (37, 13);
        let mut pixels = Vec::new();
        for y in 0..height {
            for x in 0..width {
                pixels.extend([(x * 7) as u8, (y * 19) as u8, 200, 255]);
            }
        }
        let text =
            String::from_utf8(encode(&pixels, width, height).expect("exact")).expect("ascii");
        let image = decode(&text, width as usize, height as usize);
        for (index, rgba) in pixels.as_chunks::<4>().0.iter().enumerate() {
            let shown = image[index].expect("every opaque pixel is drawn");
            for channel in 0..3 {
                let error = (i16::from(shown[channel]) - i16::from(rgba[channel])).abs();
                assert!(
                    error <= 16,
                    "pixel {index} channel {channel} is off by {error}"
                );
            }
        }
    }

    #[test]
    fn a_wrong_payload_size_encodes_nothing() {
        assert_eq!(encode(&[0; 7], 1, 2), None);
    }

    #[test]
    fn a_dark_theme_color_keeps_its_exact_value() {
        let text = String::from_utf8(encode(&solid(4, 4, [26, 22, 38, 255]), 4, 4).expect("exact"))
            .expect("ascii");
        assert!(
            text.contains("#0;2;10;9;15"),
            "the color is defined as it is: {text}"
        );
    }

    #[test]
    fn a_busy_image_drops_bits_until_its_colors_fit_the_palette() {
        let pixels: Vec<u8> = (0..300u32)
            .flat_map(|index| [(index * 8 % 256) as u8, (index / 32 * 8) as u8, 0, 255])
            .collect();
        let text = String::from_utf8(encode(&pixels, 300, 1).expect("exact")).expect("ascii");
        let image = decode(&text, 300, 1);
        for (index, rgba) in pixels.as_chunks::<4>().0.iter().enumerate() {
            let shown = image[index].expect("every opaque pixel is drawn");
            for channel in 0..3 {
                let error = (i16::from(shown[channel]) - i16::from(rgba[channel])).abs();
                assert!(
                    error <= 16,
                    "pixel {index} channel {channel} is off by {error}"
                );
            }
        }
    }
}
