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

/// Ask the terminal to answer under `id` once it has consumed everything
/// written before this.
pub fn terminal_query<W: Sink>(out: &mut W, id: u32) -> io::Result<()> {
    out.write_all(&kitty::barrier(kitty::ImageId(id)))
}

#[cfg(test)]
mod tests {
    use super::{enter, leave};

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
