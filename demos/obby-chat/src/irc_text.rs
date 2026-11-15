//! mIRC formatting, turned into styled spans in the demo's palette.

use katui_draw::Rgba;
use katui_ui::TextSpan;
use obby_client::proto::{Colour, Span, parse_spans, strip_formatting};

use crate::theme::{ASH, INK_ON_LIGHT};

/// Below this brightness a color with no background is unreadable on the
/// dark interface.
const MIN_BRIGHTNESS: u32 = 90;

/// The 99 standard colors; code 99 means the default color.
const PALETTE: [u32; 99] = [
    0xffffff, 0x000000, 0x00007f, 0x009300, 0xff0000, 0x7f0000, 0x9c009c, 0xfc7f00, 0xffff00,
    0x00fc00, 0x009393, 0x00ffff, 0x0000fc, 0xff00ff, 0x7f7f7f, 0xd2d2d2, 0x470000, 0x472100,
    0x474700, 0x324700, 0x004700, 0x00472c, 0x004747, 0x002747, 0x000047, 0x2e0047, 0x470047,
    0x47002a, 0x740000, 0x743a00, 0x747400, 0x517400, 0x007400, 0x007449, 0x007474, 0x004074,
    0x000074, 0x4b0074, 0x740074, 0x740045, 0xb50000, 0xb56300, 0xb5b500, 0x7db500, 0x00b500,
    0x00b571, 0x00b5b5, 0x0063b5, 0x0000b5, 0x7500b5, 0xb500b5, 0xb5006b, 0xff0000, 0xff8c00,
    0xffff00, 0xb2ff00, 0x00ff00, 0x00ffa0, 0x00ffff, 0x008cff, 0x0000ff, 0xa500ff, 0xff00ff,
    0xff0098, 0xff5959, 0xffb459, 0xffff71, 0xcfff60, 0x6fff6f, 0x65ffc9, 0x6dffff, 0x59b4ff,
    0x5959ff, 0xc459ff, 0xff66ff, 0xff59bc, 0xff9c9c, 0xffd39c, 0xffff9c, 0xe2ff9c, 0x9cff9c,
    0x9cffdb, 0x9cffff, 0x9cd3ff, 0x9c9cff, 0xdc9cff, 0xff9cff, 0xff94d3, 0x000000, 0x131313,
    0x282828, 0x363636, 0x4d4d4d, 0x656565, 0x818181, 0x9f9f9f, 0xbcbcbc, 0xe2e2e2, 0xffffff,
];

pub fn strip(text: &str) -> String {
    strip_formatting(text)
}

pub fn parse(text: &str) -> Vec<TextSpan> {
    parse_spans(text).into_iter().map(styled).collect()
}

fn styled(span: Span) -> TextSpan {
    let emphasis = span.style.emphasis;
    let foreground = span.style.foreground.and_then(color_of);
    let background = span.style.background.and_then(color_of);
    let (color, background) = if emphasis.reverse() {
        (
            Some(background.unwrap_or(INK_ON_LIGHT)),
            Some(foreground.unwrap_or(ASH)),
        )
    } else {
        (foreground, background)
    };
    TextSpan {
        text: span.text,
        color: readable(color, background),
        background,
        bold: emphasis.bold(),
        italic: emphasis.italic(),
        underline: emphasis.underline(),
        strike: emphasis.strikethrough(),
        mono: false,
    }
}

fn color_of(colour: Colour) -> Option<Rgba> {
    match colour {
        Colour::Numbered(number) => PALETTE.get(usize::from(number)).copied().map(rgb),
        Colour::Hex(red, green, blue) => Some(Rgba::solid(red, green, blue)),
    }
}

/// `color`, lightened when it would vanish into the dark interface.
fn readable(color: Option<Rgba>, background: Option<Rgba>) -> Option<Rgba> {
    let ink = color?;
    if background.is_some() {
        return Some(ink);
    }
    let brightness = (u32::from(ink.r) * 3 + u32::from(ink.g) * 6 + u32::from(ink.b)) / 10;
    if brightness >= MIN_BRIGHTNESS {
        return Some(ink);
    }
    let lift = |channel: u8| channel / 2 + 110;
    Some(Rgba::solid(lift(ink.r), lift(ink.g), lift(ink.b)))
}

fn rgb(value: u32) -> Rgba {
    Rgba::solid((value >> 16) as u8, (value >> 8) as u8, value as u8)
}

#[cfg(test)]
mod tests {
    use super::{parse, rgb, strip};
    use katui_draw::Rgba;

    #[test]
    fn plain_text_is_one_unstyled_span() {
        let spans = parse("hello");
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].text, "hello");
        assert_eq!(spans[0].color, None);
    }

    #[test]
    fn a_color_code_is_consumed_and_colors_what_follows() {
        let spans = parse("\u{3}42assfd");
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].text, "assfd");
        assert_eq!(spans[0].color, Some(rgb(0xb5b500)));
    }

    #[test]
    fn foreground_and_background_both_apply() {
        let spans = parse("\u{3}4,8warn\u{3} ok");
        assert_eq!(spans[0].color, Some(rgb(0xff0000)));
        assert_eq!(spans[0].background, Some(rgb(0xffff00)));
        assert_eq!(spans[1].text, " ok");
        assert_eq!(spans[1].color, None, "a bare code ends the color");
    }

    #[test]
    fn a_comma_with_no_number_stays_text() {
        let spans = parse("\u{3}4,hi");
        assert_eq!(spans[0].text, ",hi");
    }

    #[test]
    fn toggles_and_reset_work_in_order() {
        let spans = parse("\u{2}a\u{1d}b\u{2}c\u{f}d");
        assert!(spans[0].bold && !spans[0].italic);
        assert!(spans[1].bold && spans[1].italic);
        assert!(!spans[2].bold && spans[2].italic);
        assert!(!spans[3].bold && !spans[3].italic);
    }

    #[test]
    fn hex_colors_parse() {
        let spans = parse("\u{4}ff8000orange");
        assert_eq!(spans[0].text, "orange");
        assert_eq!(spans[0].color, Some(rgb(0xff8000)));
    }

    #[test]
    fn reverse_swaps_the_colors() {
        let spans = parse("\u{3}4,8\u{16}x");
        assert_eq!(spans[0].color, Some(rgb(0xffff00)));
        assert_eq!(spans[0].background, Some(rgb(0xff0000)));
    }

    #[test]
    fn black_text_on_no_background_stays_readable() {
        let color = parse("\u{3}1dark")[0].color.expect("the code sets a color");
        assert_ne!(color, Rgba::solid(0, 0, 0));
        assert!(color.r > 90);
        let on_white = parse("\u{3}1,0dark")[0].color;
        assert_eq!(
            on_white,
            Some(Rgba::solid(0, 0, 0)),
            "a background keeps it"
        );
    }

    #[test]
    fn strip_leaves_only_the_words() {
        assert_eq!(strip("\u{3}4,8a\u{2}b\u{f}c"), "abc");
    }

    #[test]
    fn multibyte_text_survives() {
        assert_eq!(strip("\u{3}4héllo 🔥"), "héllo 🔥");
    }
}
