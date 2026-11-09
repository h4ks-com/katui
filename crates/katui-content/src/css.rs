//! The CSS subset: rules from `<style>` blocks and `style` attributes,
//! and the values the HTML renderer understands.

use katui_draw::Rgba;

/// One `property: value` pair, lowercased and trimmed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Declaration {
    pub(crate) property: String,
    pub(crate) value: String,
}

/// One rule: its selector list as written and its declarations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Rule {
    pub(crate) selectors: String,
    pub(crate) declarations: Vec<Declaration>,
}

/// The rules of a stylesheet, comments and at-rules dropped.
pub(crate) fn rules(sheet: &str) -> Vec<Rule> {
    let sheet = without_comments(sheet);
    let mut rules = Vec::new();
    let mut rest = sheet.as_str();
    while let Some(open) = rest.find('{') {
        let before = rest.get(..open).unwrap_or_default();
        // a statement at-rule like @import ends in a semicolon before the
        // next selector, so we start the selector after it
        let selectors = before.rsplit(';').next().unwrap_or(before).trim();
        let after = rest.get(open + 1..).unwrap_or_default();
        let close = block_end(after);
        if !selectors.starts_with('@') && !selectors.is_empty() {
            rules.push(Rule {
                selectors: selectors.to_owned(),
                declarations: declarations(after.get(..close).unwrap_or_default()),
            });
        }
        rest = after.get(close + 1..).unwrap_or_default();
    }
    rules
}

/// Where the block that `text` opens ends, nested blocks included.
fn block_end(text: &str) -> usize {
    let mut depth = 0_usize;
    for (at, character) in text.char_indices() {
        match character {
            '{' => depth += 1,
            '}' if depth == 0 => return at,
            '}' => depth -= 1,
            _ => {}
        }
    }
    text.len()
}

/// The declarations of a block or a `style` attribute.
pub(crate) fn declarations(block: &str) -> Vec<Declaration> {
    block
        .split(';')
        .filter_map(|pair| {
            let (property, value) = pair.split_once(':')?;
            let value = value.trim().trim_end_matches("!important").trim();
            Some(Declaration {
                property: property.trim().to_ascii_lowercase(),
                value: value.to_ascii_lowercase(),
            })
        })
        .filter(|declaration| !declaration.property.is_empty() && !declaration.value.is_empty())
        .collect()
}

fn without_comments(sheet: &str) -> String {
    let mut out = String::with_capacity(sheet.len());
    let mut rest = sheet;
    while let Some(start) = rest.find("/*") {
        out.push_str(rest.get(..start).unwrap_or_default());
        rest = rest
            .get(start + 2..)
            .and_then(|tail| tail.find("*/").and_then(|end| tail.get(end + 2..)))
            .unwrap_or_default();
    }
    out.push_str(rest);
    out
}

/// A length in pixels, from `px`, `em` and `rem` against `font_size`, or
/// a bare number of pixels.
pub(crate) fn length(value: &str, font_size: f32, root_size: f32) -> Option<f32> {
    let value = value.trim();
    let (number, scale) = if let Some(number) = value.strip_suffix("rem") {
        (number, root_size)
    } else if let Some(number) = value.strip_suffix("em") {
        (number, font_size)
    } else if let Some(number) = value.strip_suffix("px") {
        (number, 1.0)
    } else {
        (value, 1.0)
    };
    number
        .trim()
        .parse::<f32>()
        .ok()
        .map(|number| number * scale)
}

/// The four sides of a `padding`, `margin` or similar shorthand.
pub(crate) fn sides(value: &str, font_size: f32, root_size: f32) -> Option<[f32; 4]> {
    let parts: Option<Vec<f32>> = value
        .split_whitespace()
        .map(|part| length(part, font_size, root_size))
        .collect();
    match parts?.as_slice() {
        [all] => Some([*all; 4]),
        [vertical, horizontal] => Some([*vertical, *horizontal, *vertical, *horizontal]),
        [top, horizontal, bottom] => Some([*top, *horizontal, *bottom, *horizontal]),
        [top, right, bottom, left] => Some([*top, *right, *bottom, *left]),
        _ => None,
    }
}

/// A color from a name, `#rgb`, `#rrggbb`, `#rrggbbaa`, `rgb()` or
/// `rgba()`.
pub(crate) fn color(value: &str) -> Option<Rgba> {
    let value = value.trim();
    if let Some(hex) = value.strip_prefix('#') {
        return hex_color(hex);
    }
    if let Some(inner) = value
        .strip_prefix("rgba(")
        .or_else(|| value.strip_prefix("rgb("))
        .and_then(|rest| rest.strip_suffix(')'))
    {
        return functional_color(inner);
    }
    named_color(value)
}

fn hex_color(hex: &str) -> Option<Rgba> {
    let digit = |index: usize| {
        hex.get(index..=index)
            .and_then(|digit| u8::from_str_radix(digit, 16).ok())
    };
    let pair = |index: usize| {
        hex.get(index..index + 2)
            .and_then(|pair| u8::from_str_radix(pair, 16).ok())
    };
    match hex.len() {
        3 => Some(Rgba::solid(digit(0)? * 17, digit(1)? * 17, digit(2)? * 17)),
        6 => Some(Rgba::solid(pair(0)?, pair(2)?, pair(4)?)),
        8 => Some(Rgba {
            r: pair(0)?,
            g: pair(2)?,
            b: pair(4)?,
            a: pair(6)?,
        }),
        _ => None,
    }
}

fn functional_color(inner: &str) -> Option<Rgba> {
    let parts: Vec<&str> = inner
        .split([',', ' ', '/'])
        .filter(|part| !part.is_empty())
        .collect();
    let channel = |index: usize| -> Option<u8> {
        let part = parts.get(index)?;
        let number: f32 = part.trim_end_matches('%').parse().ok()?;
        let scaled = if part.ends_with('%') {
            number * 2.55
        } else {
            number
        };
        Some(byte(scaled))
    };
    let alpha = parts.get(3).map_or(Some(255), |part| {
        let number: f32 = part.trim_end_matches('%').parse().ok()?;
        let fraction = if part.ends_with('%') {
            number / 100.0
        } else {
            number
        };
        Some(byte(fraction * 255.0))
    })?;
    Some(Rgba {
        r: channel(0)?,
        g: channel(1)?,
        b: channel(2)?,
        a: alpha,
    })
}

/// `value` rounded into a color channel.
fn byte(value: f32) -> u8 {
    u8::try_from(value.clamp(0.0, 255.0).round() as i32).unwrap_or(u8::MAX)
}

fn named_color(name: &str) -> Option<Rgba> {
    let rgb = match name {
        "black" => (0, 0, 0),
        "white" => (255, 255, 255),
        "red" => (255, 0, 0),
        "green" => (0, 128, 0),
        "lime" => (0, 255, 0),
        "blue" => (0, 0, 255),
        "navy" => (0, 0, 128),
        "yellow" => (255, 255, 0),
        "orange" => (255, 165, 0),
        "purple" => (128, 0, 128),
        "fuchsia" | "magenta" => (255, 0, 255),
        "aqua" | "cyan" => (0, 255, 255),
        "teal" => (0, 128, 128),
        "olive" => (128, 128, 0),
        "maroon" => (128, 0, 0),
        "silver" => (192, 192, 192),
        "gray" | "grey" => (128, 128, 128),
        "transparent" => {
            return Some(Rgba {
                r: 0,
                g: 0,
                b: 0,
                a: 0,
            });
        }
        _ => return None,
    };
    Some(Rgba::solid(rgb.0, rgb.1, rgb.2))
}

#[cfg(test)]
mod tests {
    use katui_draw::Rgba;

    use super::{color, length, rules, sides};

    #[test]
    fn a_stylesheet_splits_into_rules_without_comments_or_at_rules() {
        let sheet = "@charset \"utf-8\";\n/* intro */ h1, .big { color: Red; font-size: 2em !important }\n@media print { p { color: black } }\n#main{padding:4px}";
        let parsed = rules(sheet);
        let names: Vec<&str> = parsed.iter().map(|rule| rule.selectors.as_str()).collect();
        assert!(names.contains(&"h1, .big"));
        assert!(names.contains(&"#main"));
        assert!(!names.iter().any(|name| name.starts_with('@')));
        assert_eq!(parsed[0].declarations[0].value, "red");
        assert_eq!(parsed[0].declarations[1].value, "2em");
    }

    #[test]
    fn lengths_colors_and_sides_parse() {
        assert_eq!(length("2em", 10.0, 16.0), Some(20.0));
        assert_eq!(length("1.5rem", 10.0, 16.0), Some(24.0));
        assert_eq!(length("7px", 10.0, 16.0), Some(7.0));
        assert_eq!(length("auto", 10.0, 16.0), None);
        assert_eq!(sides("1px 2px", 16.0, 16.0), Some([1.0, 2.0, 1.0, 2.0]));
        assert_eq!(color("#f80"), Some(Rgba::solid(255, 136, 0)));
        assert_eq!(color("#102030"), Some(Rgba::solid(16, 32, 48)));
        assert_eq!(color("rgb(1, 2, 3)"), Some(Rgba::solid(1, 2, 3)));
        assert_eq!(color("rgba(0,0,0,0.5)").map(|rgba| rgba.a), Some(128));
        assert_eq!(color("teal"), Some(Rgba::solid(0, 128, 128)));
        assert_eq!(color("nonsense"), None);
    }
}
