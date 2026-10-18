//! The blocks fallback: pixels drawn as half-block and quarter-block
//! glyphs over SGR truecolor, the one pixel path every color terminal
//! understands.

use std::fmt::Write as _;

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

/// Render one terminal line of a pixel image into `out`: every cell
/// classifies the 2x2 pixel block under it, a trailing odd column becomes
/// a half block. SGR truecolor only; palette degradation belongs to the
/// fidelity ladders. Rows of different lengths stop at the shorter one.
pub fn render_line<W: std::io::Write>(
    out: &mut W,
    top: &[Rgb],
    bottom: &[Rgb],
) -> std::io::Result<()> {
    let mut text = String::new();
    for (top_pair, bottom_pair) in top.chunks(2).zip(bottom.chunks(2)) {
        let block = match (top_pair, bottom_pair) {
            ([tl, tr], [bl, br]) => quarter([[*tl, *tr], [*bl, *br]]),
            (odd_top, odd_bottom) => match (odd_top.first(), odd_bottom.first()) {
                (Some(top), Some(bottom)) => half(*top, *bottom),
                _ => break,
            },
        };
        let _ = write!(
            text,
            "\x1b[38;2;{r};{g};{b}m\x1b[48;2;{r2};{g2};{b2}m{glyph}\x1b[0m",
            r = block.fg.r,
            g = block.fg.g,
            b = block.fg.b,
            r2 = block.bg.r,
            g2 = block.bg.g,
            b2 = block.bg.b,
            glyph = block.glyph,
        );
    }
    out.write_all(text.as_bytes())
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

    #[test]
    fn render_line_writes_golden_sgr_bytes() {
        let mut out = Vec::new();
        super::render_line(&mut out, &[WHITE, BLACK], &[BLACK, WHITE])
            .expect("writing to a Vec cannot fail");
        assert_eq!(
            String::from_utf8_lossy(&out),
            "\x1b[38;2;255;255;255m\x1b[48;2;0;0;0m▚\x1b[0m"
        );
    }

    #[test]
    fn a_trailing_odd_column_renders_a_half_block() {
        let mut out = Vec::new();
        super::render_line(&mut out, &[WHITE, BLACK, WHITE], &[BLACK, WHITE, BLACK])
            .expect("writing to a Vec cannot fail");
        let text = String::from_utf8_lossy(&out);
        assert!(text.contains('▀'), "the odd column becomes a half block");
        assert_eq!(text.chars().filter(|c| *c == '▀').count(), 1);
    }

    #[test]
    fn empty_rows_render_nothing() {
        let mut out = Vec::new();
        super::render_line(&mut out, &[], &[]).expect("writing to a Vec cannot fail");
        assert!(out.is_empty());
    }
}
