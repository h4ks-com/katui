//! The names the bindings take for alignments, lengths, splits, blends and
//! colors.

use katui_draw::Rgba;
use katui_game::Blend;
use katui_ui::{Align, Length};
use katui_widgets::Orientation;

use crate::Error;

pub fn align(name: &str) -> Result<Align, Error> {
    match name {
        "start" => Ok(Align::Start),
        "center" => Ok(Align::Center),
        "end" => Ok(Align::End),
        other => Err(Error::Align(other.to_owned())),
    }
}

/// The length named `"fill"` or `"content"`.
pub fn length(name: &str) -> Result<Length, Error> {
    match name {
        "fill" => Ok(Length::Fill),
        "content" => Ok(Length::Content),
        other => Err(Error::Length(other.to_owned())),
    }
}

pub fn orientation(name: &str) -> Result<Orientation, Error> {
    match name {
        "horizontal" => Ok(Orientation::Horizontal),
        "vertical" => Ok(Orientation::Vertical),
        other => Err(Error::Orientation(other.to_owned())),
    }
}

pub fn blend(name: &str) -> Result<Blend, Error> {
    match name {
        "over" => Ok(Blend::Over),
        "add" => Ok(Blend::Add),
        other => Err(Error::Blend(other.to_owned())),
    }
}

/// The color written `"#rrggbb"` or `"#rrggbbaa"`.
pub fn hex(text: &str) -> Result<Rgba, Error> {
    Rgba::from_hex(text).ok_or_else(|| Error::Color(text.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_parse_and_wrong_ones_say_what_to_write() {
        assert!(matches!(align("center"), Ok(Align::Center)));
        assert!(matches!(length("fill"), Ok(Length::Fill)));
        assert!(matches!(orientation("vertical"), Ok(Orientation::Vertical)));
        assert!(matches!(blend("add"), Ok(Blend::Add)));
        assert_eq!(hex("#102030").ok(), Some(Rgba::solid(16, 32, 48)));
        let wrong = align("middle").map(drop).unwrap_err().to_string();
        assert_eq!(
            wrong,
            "align to \"start\", \"center\" or \"end\", not \"middle\""
        );
    }
}
