//! Terminal session sequences: alternate screen and cursor state, one
//! enter, one leave, nothing between.

use crate::Sink;
use katui_proto::kitty;
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
/// unambiguous keys, then ask whether pixel coordinates took effect.
pub fn enable_input<W: Sink>(out: &mut W) -> io::Result<()> {
    out.write_all(b"\x1b[?1003h\x1b[?1006h\x1b[?1016h\x1b[?1004h\x1b[?2004h\x1b[>1u\x1b[?1016$p")
}

/// Undo [`enable_input`].
pub fn disable_input<W: Sink>(out: &mut W) -> io::Result<()> {
    out.write_all(b"\x1b[<u\x1b[?2004l\x1b[?1004l\x1b[?1016l\x1b[?1006l\x1b[?1003l")
}

/// Ask the terminal to answer under `id` once it has consumed everything
/// written before this.
pub fn terminal_query<W: Sink>(out: &mut W, id: u32) -> io::Result<()> {
    out.write_all(&kitty::barrier(kitty::ImageId(id)))
}

#[cfg(test)]
mod tests {
    use super::{disable_input, enable_input, enter, leave};

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
