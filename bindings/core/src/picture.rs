//! Images made from RGBA bytes, encoded files and paths, and read back.

use std::path::Path;

use katui_scene::surface::Surface;

use crate::Error;

/// An image from straight-alpha RGBA bytes, four per pixel.
pub fn from_rgba(width: u32, height: u32, pixels: Vec<u8>) -> Result<Surface, Error> {
    Surface::from_rgba(width, height, pixels).ok_or(Error::ImageSize { width, height })
}

/// An image decoded from PNG, JPEG, GIF or WebP bytes.
pub fn decode(data: &[u8]) -> Result<Surface, Error> {
    let decoded = image::load_from_memory(data)
        .map_err(Error::Decode)?
        .into_rgba8();
    let (width, height) = decoded.dimensions();
    from_rgba(width, height, decoded.into_raw())
}

/// An image read from a PNG, JPEG, GIF or WebP file.
pub fn open(path: &Path) -> Result<Surface, Error> {
    let data = std::fs::read(path).map_err(|source| Error::Read {
        path: path.display().to_string(),
        source,
    })?;
    decode(&data)
}

/// Write `surface` to `path` as a PNG.
pub fn save(surface: &Surface, path: &Path) -> Result<(), Error> {
    image::save_buffer(
        path,
        surface.pixels(),
        surface.width(),
        surface.height(),
        image::ExtendedColorType::Rgba8,
    )
    .map_err(|source| Error::Write {
        path: path.display().to_string(),
        source,
    })
}

/// The color at `x`, `y` as `[r, g, b, a]`, or `None` outside the image.
#[must_use]
pub fn pixel(surface: &Surface, x: u32, y: u32) -> Option<[u8; 4]> {
    if x >= surface.width() {
        return None;
    }
    let at = (y as usize * surface.width() as usize + x as usize) * 4;
    match surface.pixels().get(at..at + 4)? {
        &[red, green, blue, alpha] => Some([red, green, blue, alpha]),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pixels_read_back_and_stop_at_the_edges() {
        let pixels = (0..16).collect();
        let surface = from_rgba(2, 2, pixels).ok();
        let surface = surface.as_ref();
        assert_eq!(
            surface.and_then(|it| pixel(it, 1, 1)),
            Some([12, 13, 14, 15])
        );
        assert_eq!(surface.and_then(|it| pixel(it, 2, 0)), None);
        assert_eq!(surface.and_then(|it| pixel(it, 0, 2)), None);
        assert!(from_rgba(2, 2, vec![0; 3]).is_err());
    }
}
