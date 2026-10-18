//! The blocks fallback: pixels drawn as half-block and quarter-block
//! glyphs over SGR truecolor, the one pixel path every color terminal
//! understands.

/// An opaque color, the pixel the classifiers work with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb {
    /// Red.
    pub r: u8,
    /// Green.
    pub g: u8,
    /// Blue.
    pub b: u8,
}

impl Rgb {
    fn distance_squared(self, other: Rgb) -> u32 {
        let dr = u32::from(self.r.abs_diff(other.r));
        let dg = u32::from(self.g.abs_diff(other.g));
        let db = u32::from(self.b.abs_diff(other.b));
        dr * dr + dg * dg + db * db
    }

    fn luminance(self) -> u32 {
        2_126 * u32::from(self.r) + 7_152 * u32::from(self.g) + 722 * u32::from(self.b)
    }
}

/// One classified cell: the glyph plus the two colors it draws with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Block {
    /// The glyph, one of the half or quarter block characters.
    pub glyph: char,
    /// The glyph's foreground color.
    pub fg: Rgb,
    /// The glyph's background color.
    pub bg: Rgb,
}

/// The sixteen quarter-block glyphs, indexed by a four-bit mask:
/// bit 1 top-left, bit 2 top-right, bit 4 bottom-left, bit 8 bottom-right.
const QUARTERS: [char; 16] = [
    ' ', '▘', '▝', '▀', '▖', '▌', '▞', '▛', '▗', '▚', '▐', '▜', '▄', '▙', '▟', '█',
];

/// Classify a 2x2 pixel block into one quarter-block cell.
///
/// The most distant pair of the four pixels becomes the cell's two colors,
/// the more luminant one as the foreground; every quadrant joins whichever
/// of the two it sits nearer to. Four equal pixels collapse to a full
/// block.
pub fn quarter(pixels: [[Rgb; 2]; 2]) -> Block {
    let [[top_left, top_right], [bottom_left, bottom_right]] = pixels;
    let corners = [
        (top_left, 1u8),
        (top_right, 2),
        (bottom_left, 4),
        (bottom_right, 8),
    ];
    let mut best: Option<(Rgb, Rgb, u32)> = None;
    for (first, (pixel, _)) in corners.iter().enumerate() {
        for (other, _) in corners.iter().skip(first + 1) {
            let distance = pixel.distance_squared(*other);
            let improved = best.is_none_or(|(_, _, best_distance)| distance > best_distance);
            if improved {
                best = Some((*pixel, *other, distance));
            }
        }
    }
    let (left, right) = match best {
        Some((first, second, _)) => (first, second),
        None => (top_left, top_right),
    };
    let (fg, bg) = if left.luminance() >= right.luminance() {
        (left, right)
    } else {
        (right, left)
    };
    let mut mask = 0u8;
    for (pixel, bit) in corners {
        if pixel.distance_squared(fg) <= pixel.distance_squared(bg) {
            mask |= bit;
        }
    }
    Block {
        glyph: QUARTERS.get(usize::from(mask)).copied().unwrap_or(' '),
        fg,
        bg,
    }
}

/// Classify two vertically stacked pixels into one half-block cell.
pub fn half(top: Rgb, bottom: Rgb) -> Block {
    if top == bottom {
        return Block {
            glyph: ' ',
            fg: top,
            bg: top,
        };
    }
    Block {
        glyph: '▀',
        fg: top,
        bg: bottom,
    }
}

#[cfg(test)]
mod tests {
    use super::{Block, Rgb, half, quarter};

    const WHITE: Rgb = Rgb {
        r: 255,
        g: 255,
        b: 255,
    };
    const BLACK: Rgb = Rgb { r: 0, g: 0, b: 0 };

    #[test]
    fn four_equal_pixels_collapse_to_a_full_block() {
        let red = Rgb {
            r: 200,
            g: 30,
            b: 30,
        };
        assert_eq!(
            quarter([[red; 2]; 2]),
            Block {
                glyph: '█',
                fg: red,
                bg: red
            }
        );
    }

    #[test]
    fn a_split_row_makes_the_upper_half() {
        let cell = quarter([[WHITE, WHITE], [BLACK, BLACK]]);
        assert_eq!(
            cell,
            Block {
                glyph: '▀',
                fg: WHITE,
                bg: BLACK
            }
        );
    }

    #[test]
    fn the_darker_pair_member_takes_the_background() {
        let cell = quarter([[BLACK, BLACK], [WHITE, WHITE]]);
        assert_eq!(cell.glyph, '▄');
        assert_eq!(cell.fg, WHITE);
        assert_eq!(cell.bg, BLACK);
    }

    #[test]
    fn a_checkerboard_picks_the_diagonal_glyph() {
        let cell = quarter([[WHITE, BLACK], [BLACK, WHITE]]);
        assert_eq!(cell.glyph, '▚');
        assert_eq!(cell.fg, WHITE);
        assert_eq!(cell.bg, BLACK);
    }

    #[test]
    fn the_other_diagonal_picks_its_own_glyph() {
        let cell = quarter([[BLACK, WHITE], [WHITE, BLACK]]);
        assert_eq!(cell.glyph, '▞');
        assert_eq!(cell.fg, WHITE);
        assert_eq!(cell.bg, BLACK);
    }

    #[test]
    fn a_left_column_makes_the_left_half() {
        let cell = quarter([[WHITE, BLACK], [WHITE, BLACK]]);
        assert_eq!(cell.glyph, '▌');
    }

    #[test]
    fn each_quadrant_joins_its_nearer_color() {
        let near_black = Rgb { r: 5, g: 5, b: 5 };
        let cell = quarter([[WHITE, near_black], [near_black, WHITE]]);
        assert_eq!(cell.glyph, '▚');
        assert_eq!(cell.fg, WHITE);
        assert_eq!(cell.bg, near_black);
    }

    #[test]
    fn half_blocks_split_or_fill() {
        assert_eq!(
            half(WHITE, BLACK),
            Block {
                glyph: '▀',
                fg: WHITE,
                bg: BLACK
            }
        );
        assert_eq!(half(BLACK, BLACK).glyph, ' ');
    }
}
