//! An RGBA pixel surface: a scene plane holding real pixels, plus the
//! tile damage that keeps full-frame retransmits honest until patched
//! presenters arrive.

/// One opaque or transparent pixel color.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgba {
    /// Red.
    pub r: u8,
    /// Green.
    pub g: u8,
    /// Blue.
    pub b: u8,
    /// Alpha, 255 opaque.
    pub a: u8,
}

impl Rgba {
    /// A solid color.
    pub const fn solid(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }
}

/// A pixel coordinate: `x` counts pixels from the left, `y` from the
/// top. The pixel plane's answer to the cell plane's
/// [`Position`](crate::cell::Position).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PixelPos {
    /// Column.
    pub x: u32,
    /// Row.
    pub y: u32,
}

impl PixelPos {
    /// A point at (`x`, `y`).
    #[must_use]
    pub const fn new(x: u32, y: u32) -> Self {
        Self { x, y }
    }
}

/// A rectangle of pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PixelRect {
    /// Left edge.
    pub x: u32,
    /// Top edge.
    pub y: u32,
    /// Width.
    pub width: u32,
    /// Height.
    pub height: u32,
}

impl PixelRect {
    /// Whether `x` and `y` fall inside.
    pub fn contains(&self, x: u32, y: u32) -> bool {
        x >= self.x && y >= self.y && x < self.x + self.width && y < self.y + self.height
    }
}

/// A plane of RGBA pixels, row-major, origin top-left.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Surface {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

impl Surface {
    /// A transparent black surface of `width` x `height` pixels.
    #[must_use]
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            pixels: vec![0; (width as usize) * (height as usize) * 4],
        }
    }

    /// A surface from RGBA bytes; `None` when `pixels` does not hold
    /// exactly `4 * width * height` bytes.
    #[must_use]
    pub fn from_rgba(width: u32, height: u32, pixels: Vec<u8>) -> Option<Self> {
        if pixels.len() == (width as usize) * (height as usize) * 4 {
            Some(Self {
                width,
                height,
                pixels,
            })
        } else {
            None
        }
    }

    /// The width in pixels.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// The height in pixels.
    pub fn height(&self) -> u32 {
        self.height
    }

    /// The RGBA bytes.
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    /// Read one pixel; `None` outside.
    #[must_use]
    pub fn get(&self, at: PixelPos) -> Option<Rgba> {
        let base = self.index(at.x, at.y)?;
        Some(Rgba {
            r: *self.pixels.get(base)?,
            g: *self.pixels.get(base + 1)?,
            b: *self.pixels.get(base + 2)?,
            a: *self.pixels.get(base + 3)?,
        })
    }

    /// Write one pixel; `false` and no change outside.
    pub fn set(&mut self, at: PixelPos, color: Rgba) -> bool {
        match self.index(at.x, at.y) {
            Some(base) => {
                for (offset, channel) in
                    [color.r, color.g, color.b, color.a].into_iter().enumerate()
                {
                    if let Some(slot) = self.pixels.get_mut(base + offset) {
                        *slot = channel;
                    }
                }
                true
            }
            None => false,
        }
    }

    /// Fill a pixel rectangle, clipped to the surface.
    pub fn fill_rect(&mut self, rect: PixelRect, color: Rgba) {
        for y in rect.y..rect.y.saturating_add(rect.height) {
            for x in rect.x..rect.x.saturating_add(rect.width) {
                self.set(PixelPos::new(x, y), color);
            }
        }
    }

    /// Copy every pixel of `source` onto this surface at (`x`, `y`),
    /// clipped to the destination. Overwrites, no blending: the cheap
    /// path for stamping a precomputed backdrop under a frame.
    pub fn blit(&mut self, source: &Surface, x: u32, y: u32) {
        for row in 0..source.height() {
            for column in 0..source.width() {
                if let Some(color) = source.get(PixelPos::new(column, row)) {
                    self.set(
                        PixelPos::new(x.saturating_add(column), y.saturating_add(row)),
                        color,
                    );
                }
            }
        }
    }

    fn index(&self, x: u32, y: u32) -> Option<usize> {
        if x < self.width && y < self.height {
            Some((y as usize) * (self.width as usize) * 4 + (x as usize) * 4)
        } else {
            None
        }
    }
}

/// The changed tiles between two surfaces, cut on a `tile`-pixel grid:
/// every tile whose content differs, in row-major order. Surfaces of
/// different sizes damage as one full-surface tile.
pub fn diff_tiles(prev: &Surface, next: &Surface, tile: u32) -> Vec<PixelRect> {
    let tile = tile.max(1);
    if prev.width != next.width || prev.height != next.height {
        return vec![PixelRect {
            x: 0,
            y: 0,
            width: next.width,
            height: next.height,
        }];
    }
    let mut damaged = Vec::new();
    let mut y = 0;
    while y < next.height {
        let mut x = 0;
        while x < next.width {
            if tile_differs(prev, next, x, y, tile) {
                damaged.push(PixelRect {
                    x,
                    y,
                    width: tile.min(next.width - x),
                    height: tile.min(next.height - y),
                });
            }
            x += tile;
        }
        y += tile;
    }
    damaged
}

fn tile_differs(prev: &Surface, next: &Surface, x: u32, y: u32, tile: u32) -> bool {
    let mut row = y;
    while row < y.saturating_add(tile).min(next.height) {
        let mut column = x;
        while column < x.saturating_add(tile).min(next.width) {
            if prev.get(PixelPos::new(column, row)) != next.get(PixelPos::new(column, row)) {
                return true;
            }
            column += 1;
        }
        row += 1;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::{PixelPos, PixelRect, Rgba, Surface, diff_tiles};

    const RED: Rgba = Rgba::solid(255, 0, 0);
    const BLUE: Rgba = Rgba::solid(0, 0, 255);

    #[test]
    fn a_new_surface_is_transparent_black() {
        let surface = Surface::new(2, 2);
        assert_eq!(surface.pixels(), &[0; 16]);
        assert_eq!(
            surface.get(PixelPos::new(0, 0)),
            Some(Rgba {
                r: 0,
                g: 0,
                b: 0,
                a: 0
            })
        );
    }

    #[test]
    fn from_rgba_rejects_short_pixel_buffers() {
        assert!(Surface::from_rgba(2, 2, vec![0; 15]).is_none());
        assert!(Surface::from_rgba(2, 2, vec![0; 16]).is_some());
    }

    #[test]
    fn writes_clip_to_the_surface() {
        let mut surface = Surface::new(2, 2);
        assert!(surface.set(PixelPos::new(1, 1), RED));
        assert!(!surface.set(PixelPos::new(2, 1), RED));
        assert!(!surface.set(PixelPos::new(1, 2), RED));
        assert_eq!(surface.get(PixelPos::new(1, 1)), Some(RED));
    }

    #[test]
    fn fill_rect_covers_only_its_rectangle() {
        let mut surface = Surface::new(4, 4);
        surface.fill_rect(
            PixelRect {
                x: 1,
                y: 1,
                width: 2,
                height: 1,
            },
            BLUE,
        );
        for y in 0..4 {
            for x in 0..4 {
                let expected = if (1..3).contains(&x) && y == 1 {
                    BLUE
                } else {
                    Rgba {
                        r: 0,
                        g: 0,
                        b: 0,
                        a: 0,
                    }
                };
                assert_eq!(surface.get(PixelPos::new(x, y)), Some(expected), "{x},{y}");
            }
        }
    }

    #[test]
    fn blit_stamps_a_source_and_clips_at_the_edges() {
        let stamp = Surface::from_rgba(2, 2, [255, 0, 0, 255].repeat(4)).expect("16 bytes");
        let mut surface = Surface::new(4, 4);
        surface.blit(&stamp, 1, 1);
        assert_eq!(
            surface.get(PixelPos::new(1, 1)),
            Some(Rgba::solid(255, 0, 0))
        );
        assert_eq!(
            surface.get(PixelPos::new(2, 2)),
            Some(Rgba::solid(255, 0, 0))
        );
        assert_eq!(
            surface.get(PixelPos::new(0, 0)),
            Some(Rgba {
                r: 0,
                g: 0,
                b: 0,
                a: 0
            })
        );

        let mut corner = Surface::new(4, 4);
        corner.blit(&stamp, 3, 3);
        assert_eq!(
            corner.get(PixelPos::new(3, 3)),
            Some(Rgba::solid(255, 0, 0))
        );
        assert_eq!(
            corner.get(PixelPos::new(0, 3)),
            Some(Rgba {
                r: 0,
                g: 0,
                b: 0,
                a: 0
            })
        );
    }

    #[test]
    fn identical_surfaces_carry_no_damage() {
        let surface = Surface::new(8, 8);
        assert!(diff_tiles(&surface, &surface, 4).is_empty());
    }

    #[test]
    fn one_changed_pixel_damages_only_its_tile() {
        let prev = Surface::new(8, 8);
        let mut next = prev.clone();
        next.set(PixelPos::new(6, 5), RED);
        assert_eq!(
            diff_tiles(&prev, &next, 4),
            vec![PixelRect {
                x: 4,
                y: 4,
                width: 4,
                height: 4
            }]
        );
    }

    #[test]
    fn a_size_change_damages_everything_as_one_rect() {
        let prev = Surface::new(4, 4);
        let next = Surface::new(8, 8);
        assert_eq!(
            diff_tiles(&prev, &next, 4),
            vec![PixelRect {
                x: 0,
                y: 0,
                width: 8,
                height: 8
            }]
        );
    }

    #[test]
    fn edge_tiles_clip_to_the_surface() {
        let prev = Surface::new(6, 6);
        let mut next = prev.clone();
        next.set(PixelPos::new(5, 5), RED);
        assert_eq!(
            diff_tiles(&prev, &next, 4),
            vec![PixelRect {
                x: 4,
                y: 4,
                width: 2,
                height: 2
            }]
        );
    }
}
