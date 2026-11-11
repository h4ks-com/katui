//! The OSC 52 clipboard write: text in, the bytes that put it on the
//! system clipboard of the terminal's host.

use base64::Engine as _;

/// The sequence that copies `text` to the clipboard. Inside a multiplexer
/// we wrap it in a passthrough command, because tmux drops a bare OSC 52
/// unless the user turned `set-clipboard` on.
#[must_use]
pub fn copy(text: &str, multiplexed: bool) -> Vec<u8> {
    let encoded = base64::engine::general_purpose::STANDARD.encode(text);
    let osc = format!("\x1b]52;c;{encoded}\x07");
    if multiplexed {
        format!("\x1bPtmux;\x1b{osc}\x1b\\").into_bytes()
    } else {
        osc.into_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::copy;

    #[test]
    fn text_goes_out_base64_encoded() {
        assert_eq!(copy("hi", false), b"\x1b]52;c;aGk=\x07");
    }

    #[test]
    fn a_multiplexer_gets_it_through_passthrough() {
        assert_eq!(copy("hi", true), b"\x1bPtmux;\x1b\x1b]52;c;aGk=\x07\x1b\\");
    }
}
