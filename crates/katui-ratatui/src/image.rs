//! Image decode and fit: bytes in, `draw_image`-ready RGBA out. Scaling
//! follows the readability policy of keeping small images crisp: pixel
//! replication up, resampling down.

use image::DynamicImage;
use katui_proto::probe::CellPixels;
use katui_scene::placement::CellRect;

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

#[cfg(test)]
mod tests {
    use super::{decode, fit_to_area, pixel_area};
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
    fn a_small_image_scales_up_with_its_own_colors() {
        let decoded =
            DynamicImage::ImageRgb8(RgbImage::from_pixel(1, 1, image::Rgb([200, 30, 30])));
        let bytes = fit_to_area(&decoded, area(2, 1), cell());
        assert!(bytes.len() >= 4);
        assert_eq!(&bytes[..4], &[200, 30, 30, 255]);
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
