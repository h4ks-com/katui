//! Terminal session sequences: alternate screen and cursor state, one
//! enter, one leave, nothing between.

use crate::Sink;
use katui_proto::{clipboard, kitty};
use std::io;

/// Enter the alternate screen with the cursor hidden; everything drawn
/// afterwards leaves the user's scrollback untouched.
pub fn enter<W: Sink>(out: &mut W) -> io::Result<()> {
    out.write_all(b"\x1b[?1049h\x1b[?25l")
}

/// Leave the alternate screen with the cursor back.
pub fn leave<W: Sink>(out: &mut W) -> io::Result<()> {
    out.write_all(b"\x1b[?25h\x1b[?1049l")
}

/// Turn on what an interactive application reads: all mouse motion in SGR
/// form with pixel coordinates, focus changes, bracketed paste and
/// unambiguous keys with their base layout key, then ask whether pixel
/// coordinates took effect.
pub fn enable_input<W: Sink>(out: &mut W) -> io::Result<()> {
    out.write_all(b"\x1b[?1003h\x1b[?1006h\x1b[?1016h\x1b[?1004h\x1b[?2004h\x1b[>5u\x1b[?1016$p")
}

/// Undo [`enable_input`] and give the pointer its default shape back.
pub fn disable_input<W: Sink>(out: &mut W) -> io::Result<()> {
    out.write_all(
        b"\x1b[=0;1u\x1b[<u\x1b[?2004l\x1b[?1004l\x1b[?1016l\x1b[?1006l\x1b[?1003l\x1b]22;\x1b\\",
    )
}

/// A mouse pointer shape, as the kitty pointer shape protocol names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerShape {
    Default,
    /// The hand over a link.
    Pointer,
    Text,
    Crosshair,
    Move,
    Wait,
    Progress,
    Help,
    NotAllowed,
    Grab,
    Grabbing,
    ColResize,
    RowResize,
    /// No pointer, where the terminal supports hiding it.
    Hidden,
}

impl PointerShape {
    /// The name OSC 22 carries.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Pointer => "pointer",
            Self::Text => "text",
            Self::Crosshair => "crosshair",
            Self::Move => "move",
            Self::Wait => "wait",
            Self::Progress => "progress",
            Self::Help => "help",
            Self::NotAllowed => "not-allowed",
            Self::Grab => "grab",
            Self::Grabbing => "grabbing",
            Self::ColResize => "ew-resize",
            Self::RowResize => "ns-resize",
            Self::Hidden => "none",
        }
    }
}

/// Give the mouse pointer `shape` over the terminal window. Terminals
/// without OSC 22 ignore it.
pub fn set_pointer_shape<W: Sink>(out: &mut W, shape: PointerShape) -> io::Result<()> {
    write!(out, "\x1b]22;{}\x1b\\", shape.name())
}

/// Also report key repeats and releases, with every key as an escape
/// code so that plain letters and modifier keys report theirs too, and
/// keep the base layout key. [`disable_input`] undoes
/// this along with the rest.
pub fn report_key_releases<W: Sink>(out: &mut W) -> io::Result<()> {
    out.write_all(b"\x1b[=15;1u")
}

/// Ask the terminal to answer under `id` once it has consumed everything
/// written before this.
pub fn terminal_query<W: Sink>(out: &mut W, id: u32) -> io::Result<()> {
    out.write_all(&kitty::barrier(kitty::ImageId(id)))
}

/// Put `text` on the system clipboard; `multiplexed` routes it through a
/// tmux or rmux pane.
pub fn copy_to_clipboard<W: Sink>(out: &mut W, text: &str, multiplexed: bool) -> io::Result<()> {
    out.write_all(&clipboard::copy(text, multiplexed))
}

#[cfg(test)]
mod tests {
    use super::{
        PointerShape, disable_input, enable_input, enter, leave, report_key_releases,
        set_pointer_shape,
    };

    #[test]
    fn disabling_input_undoes_the_key_release_flags() {
        let (mut flags, mut off) = (Vec::new(), Vec::new());
        report_key_releases(&mut flags).expect("writing to a Vec cannot fail");
        disable_input(&mut off).expect("writing to a Vec cannot fail");
        assert_eq!(flags, b"\x1b[=15;1u");
        assert!(
            off.starts_with(b"\x1b[=0;1u\x1b[<u"),
            "clearing the flags before popping covers terminals that keep them"
        );
    }

    #[test]
    fn disabling_input_turns_off_every_mode_enabling_set() {
        let (mut on, mut off) = (Vec::new(), Vec::new());
        enable_input(&mut on).expect("writing to a Vec cannot fail");
        disable_input(&mut off).expect("writing to a Vec cannot fail");
        let (on, off) = (
            String::from_utf8_lossy(&on).into_owned(),
            String::from_utf8_lossy(&off).into_owned(),
        );
        for mode in ["1003", "1006", "1016", "1004", "2004"] {
            assert!(on.contains(&format!("?{mode}h")), "{mode} goes on");
            assert!(off.contains(&format!("?{mode}l")), "{mode} goes off");
        }
    }

    #[test]
    fn pointer_shapes_write_osc_22_and_disabling_input_resets_the_shape() {
        let mut out = Vec::new();
        for shape in [
            PointerShape::Pointer,
            PointerShape::NotAllowed,
            PointerShape::ColResize,
            PointerShape::Hidden,
        ] {
            set_pointer_shape(&mut out, shape).expect("writing to a Vec cannot fail");
        }
        assert_eq!(
            String::from_utf8_lossy(&out),
            "\x1b]22;pointer\x1b\\\x1b]22;not-allowed\x1b\\\x1b]22;ew-resize\x1b\\\x1b]22;none\x1b\\"
        );
        let mut off = Vec::new();
        disable_input(&mut off).expect("writing to a Vec cannot fail");
        assert!(off.ends_with(b"\x1b]22;\x1b\\"));
    }

    #[test]
    fn enter_and_leave_write_their_exact_sequences() {
        let mut out = Vec::new();
        enter(&mut out).expect("writing to a Vec cannot fail");
        leave(&mut out).expect("writing to a Vec cannot fail");
        assert_eq!(
            String::from_utf8_lossy(&out),
            "\x1b[?1049h\x1b[?25l\x1b[?25h\x1b[?1049l"
        );
    }
}
