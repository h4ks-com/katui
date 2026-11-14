//! Image decode and fit: bytes in, `draw_image`-ready RGBA out, with a
//! cache that decodes off the drawing thread. Scaling
//! follows the readability policy of keeping small images crisp: pixel
//! replication up, resampling down.

use std::collections::{HashMap, HashSet};
use std::hash::Hash;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};

use image::DynamicImage;
use katui_proto::probe::CellPixels;
use katui_scene::placement::CellRect;

type Finished<K> = (K, Option<Arc<DynamicImage>>);

/// Images keyed by the caller, decoded on worker threads once per key and
/// fitted once per size, so a redraw never decodes or scales again.
#[derive(Debug)]
pub struct ImageCache<K> {
    sender: Sender<Finished<K>>,
    finished: Receiver<Finished<K>>,
    pending: HashSet<K>,
    decoded: HashMap<K, Option<Arc<DynamicImage>>>,
    fitted: HashMap<(K, u16, u16, CellPixels), Arc<Vec<u8>>>,
}

impl<K: Eq + Hash + Clone + Send + 'static> Default for ImageCache<K> {
    fn default() -> Self {
        let (sender, finished) = mpsc::channel();
        Self {
            sender,
            finished,
            pending: HashSet::new(),
            decoded: HashMap::new(),
            fitted: HashMap::new(),
        }
    }
}

impl<K: Eq + Hash + Clone + Send + 'static> ImageCache<K> {
    /// Decode `bytes` under `key` on a worker thread, unless that key is
    /// already decoded or on its way.
    pub fn request(&mut self, key: K, bytes: Vec<u8>) {
        if self.decoded.contains_key(&key) || !self.pending.insert(key.clone()) {
            return;
        }
        let sender = self.sender.clone();
        // we spawn one thread per new image; a caller showing hundreds at
        // once would want a bounded pool here
        std::thread::spawn(move || {
            let decoded = decode(&bytes).ok().map(Arc::new);
            let _ = sender.send((key, decoded));
        });
    }

    /// Collect the decodes that finished and return their keys, so the
    /// caller knows to draw again.
    pub fn poll(&mut self) -> Vec<K> {
        let mut ready = Vec::new();
        while let Ok((key, decoded)) = self.finished.try_recv() {
            self.pending.remove(&key);
            self.decoded.insert(key.clone(), decoded);
            ready.push(key);
        }
        ready
    }

    /// Whether `key` failed to decode.
    pub fn failed(&self, key: &K) -> bool {
        matches!(self.decoded.get(key), Some(None))
    }

    /// The RGBA bytes of `key` fitted to `area`, scaled once per size;
    /// `None` until its decode finished, or when it failed.
    pub fn fitted(&mut self, key: &K, area: CellRect, cell: CellPixels) -> Option<Arc<Vec<u8>>> {
        let size_key = (key.clone(), area.columns, area.rows, cell);
        if let Some(bytes) = self.fitted.get(&size_key) {
            return Some(Arc::clone(bytes));
        }
        let decoded = self.decoded.get(key)?.as_ref()?;
        let bytes = Arc::new(fit_to_area(decoded, area, cell));
        self.fitted.insert(size_key, Arc::clone(&bytes));
        Some(bytes)
    }
}

/// Decode PNG, JPEG, GIF or WebP bytes.
pub fn decode(bytes: &[u8]) -> Result<DynamicImage, DecodeError> {
    image::load_from_memory(bytes).map_err(DecodeError)
}

/// A decode failure, wrapping the decoder's own error.
#[derive(Debug, thiserror::Error)]
#[error("decoding the image failed: {0}")]
pub struct DecodeError(#[from] image::ImageError);

/// The pixel size an image fits into when drawn at `area` cells.
#[must_use]
pub fn pixel_area(area: CellRect, cell: CellPixels) -> (u32, u32) {
    (
        u32::from(area.columns) * u32::from(cell.width),
        u32::from(area.rows) * u32::from(cell.height),
    )
}

/// Scale `decoded` to the pixel size of `area` and return its RGBA bytes:
/// nearest-neighbor when the image already fits (small images keep sharp
/// pixels), triangle resampling when it must shrink.
#[must_use]
pub fn fit_to_area(decoded: &DynamicImage, area: CellRect, cell: CellPixels) -> Vec<u8> {
    let (target_w, target_h) = pixel_area(area, cell);
    let (width, height) = (decoded.width(), decoded.height());
    let fits = width <= target_w && height <= target_h;
    let filter = if fits {
        image::imageops::FilterType::Nearest
    } else {
        image::imageops::FilterType::Triangle
    };
    decoded
        .resize_exact(target_w, target_h, filter)
        .to_rgba8()
        .into_raw()
}

/// `decoded` scaled to fit inside the pixel size of `area` with its shape
/// kept, centered, with transparent bars on the sides it does not fill:
/// RGBA bytes of exactly the area's pixel size.
#[must_use]
pub fn contain_in_area(decoded: &DynamicImage, area: CellRect, cell: CellPixels) -> Vec<u8> {
    let (target_w, target_h) = pixel_area(area, cell);
    let mut canvas = image::RgbaImage::new(target_w, target_h);
    let (width, height) = (decoded.width().max(1), decoded.height().max(1));
    let scale =
        (f64::from(target_w) / f64::from(width)).min(f64::from(target_h) / f64::from(height));
    let fitted_w = (f64::from(width) * scale)
        .round()
        .clamp(1.0, f64::from(target_w.max(1)));
    let fitted_h = (f64::from(height) * scale)
        .round()
        .clamp(1.0, f64::from(target_h.max(1)));
    let (fitted_w, fitted_h) = (whole(fitted_w), whole(fitted_h));
    let filter = if scale >= 1.0 {
        image::imageops::FilterType::Nearest
    } else {
        image::imageops::FilterType::Triangle
    };
    let fitted = decoded.resize_exact(fitted_w, fitted_h, filter).to_rgba8();
    image::imageops::overlay(
        &mut canvas,
        &fitted,
        i64::from((target_w - fitted_w) / 2),
        i64::from((target_h - fitted_h) / 2),
    );
    canvas.into_raw()
}

/// A whole number of pixels from a rounded value already clamped into
/// `u32` range.
fn whole(value: f64) -> u32 {
    u32::try_from(value as i64).unwrap_or(1)
}

#[cfg(test)]
mod tests {
    use super::{contain_in_area, decode, fit_to_area, pixel_area};
    use image::{DynamicImage, RgbImage};
    use katui_proto::probe::CellPixels;
    use katui_scene::placement::CellRect;

    fn area(columns: u16, rows: u16) -> CellRect {
        CellRect {
            x: 0,
            y: 0,
            columns,
            rows,
        }
    }

    fn cell() -> CellPixels {
        CellPixels {
            width: 10,
            height: 20,
        }
    }

    #[test]
    fn the_pixel_area_multiplies_cells_by_cell_pixels() {
        assert_eq!(pixel_area(area(4, 2), cell()), (40, 40));
    }

    #[test]
    fn fitting_returns_exact_area_rgba_bytes() {
        let decoded = DynamicImage::ImageRgb8(RgbImage::from_pixel(3, 3, image::Rgb([10, 20, 30])));
        let bytes = fit_to_area(&decoded, area(4, 2), cell());
        assert_eq!(bytes.len(), 40 * 40 * 4, "width * height * rgba");
    }

    #[test]
    fn containing_keeps_the_shape_and_pads_with_transparency() {
        let wide = DynamicImage::ImageRgb8(RgbImage::from_pixel(40, 10, image::Rgb([200, 0, 0])));
        let cell = CellPixels {
            width: 10,
            height: 10,
        };
        let rgba = contain_in_area(&wide, area(4, 4), cell);
        assert_eq!(rgba.len(), 40 * 40 * 4, "the bytes cover the whole area");
        let alpha = |x: usize, y: usize| rgba[(y * 40 + x) * 4 + 3];
        assert_eq!(alpha(20, 0), 0, "the bar above is transparent");
        assert_eq!(alpha(20, 20), 255, "the middle holds the picture");
        assert_eq!(alpha(20, 39), 0, "the bar below is transparent");
    }

    #[test]
    fn a_small_image_scales_up_with_its_own_colors() {
        let decoded =
            DynamicImage::ImageRgb8(RgbImage::from_pixel(1, 1, image::Rgb([200, 30, 30])));
        let bytes = fit_to_area(&decoded, area(2, 1), cell());
        assert!(bytes.len() >= 4);
        assert_eq!(&bytes[..4], &[200, 30, 30, 255]);
    }

    fn png(width: u32, height: u32) -> Vec<u8> {
        let decoded =
            DynamicImage::ImageRgb8(RgbImage::from_pixel(width, height, image::Rgb([9, 8, 7])));
        let mut png = Vec::new();
        decoded
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .expect("encoding a png cannot fail");
        png
    }

    fn ready_keys(cache: &mut super::ImageCache<&'static str>, wanted: usize) -> Vec<&'static str> {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let mut ready = Vec::new();
        while ready.len() < wanted && std::time::Instant::now() < deadline {
            ready.extend(cache.poll());
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        ready
    }

    #[test]
    fn the_cache_decodes_each_key_once_off_thread_and_fits_each_size_once() {
        let mut cache = super::ImageCache::default();
        cache.request("logo", png(2, 2));
        cache.request("logo", png(2, 2));
        cache.request("junk", b"not an image".to_vec());
        assert_eq!(
            cache.fitted(&"logo", area(1, 1), cell()),
            None,
            "not decoded yet"
        );
        let mut ready = ready_keys(&mut cache, 2);
        ready.sort_unstable();
        assert_eq!(ready, ["junk", "logo"], "each key finishes once");
        assert!(cache.failed(&"junk"));
        let first = cache.fitted(&"logo", area(2, 1), cell()).expect("decoded");
        let again = cache.fitted(&"logo", area(2, 1), cell()).expect("decoded");
        assert!(std::sync::Arc::ptr_eq(&first, &again), "one fit per size");
        assert_eq!(first.len(), 20 * 20 * 4);
        cache.request("logo", png(2, 2));
        assert!(cache.poll().is_empty(), "a decoded key never decodes again");
    }

    #[test]
    fn decoding_rejects_bytes_that_are_not_an_image() {
        assert!(decode(b"not an image").is_err());
    }

    #[test]
    fn decoding_accepts_a_generated_png() {
        let decoded = DynamicImage::ImageRgb8(RgbImage::from_pixel(2, 2, image::Rgb([1, 2, 3])));
        let mut png = Vec::new();
        decoded
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .expect("encoding a png cannot fail");
        let decoded = decode(&png).expect("the generated png decodes");
        assert_eq!((decoded.width(), decoded.height()), (2, 2));
    }
}
