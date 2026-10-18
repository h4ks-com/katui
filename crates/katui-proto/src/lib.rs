//! Protocol codecs and capability probes: layer 0 of the stack in
//! ARCHITECTURE.md. Everything here is pure; the crate performs no I/O and
//! knows nothing about scenes or widgets.

/// Frame `command` as a kitty graphics APC: `ESC _ G` + command + `ESC \`.
///
/// Every kitty graphics sequence is this shape; control data, payloads and
/// chunking belong to the callers that build specific commands.
pub fn apc(command: &str) -> String {
    let mut out = String::with_capacity(command.len() + 8);
    out.push_str("\x1b_G");
    out.push_str(command);
    out.push_str("\x1b\\");
    out
}

#[cfg(test)]
mod tests {
    use super::apc;

    #[test]
    fn apc_wraps_a_command_in_the_kitty_frame() {
        assert_eq!(apc("a=q,i=31,s=1,v=1"), "\x1b_Ga=q,i=31,s=1,v=1\x1b\\");
    }
}
