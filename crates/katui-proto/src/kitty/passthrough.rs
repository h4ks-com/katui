//! Commands carried through a terminal multiplexer: split at command
//! boundaries and wrapped for tmux passthrough.

const PASSTHROUGH_COMMAND_START: &[u8] = b"\x1b_G";
const STRING_END: &[u8] = b"\x1b\\";

/// Cut encoded `frame` into runs of at most `limit` bytes where possible,
/// only between complete commands: any graphics command inside a chunked
/// upload would abort that upload.
#[must_use]
pub fn split_at_commands(frame: &[u8], limit: usize) -> Vec<&[u8]> {
    let mut parts = Vec::new();
    let (mut start, mut safe, mut cursor) = (0, 0, 0);
    while let Some(offset) = frame
        .get(cursor..)
        .and_then(|rest| rest.windows(2).position(|pair| pair == b"\x1b\\"))
    {
        let end = cursor + offset + 2;
        let command = frame.get(cursor..end).unwrap_or_default();
        cursor = end;
        if upload_continues(command) {
            continue;
        }
        if end - start > limit && safe > start {
            parts.extend(frame.get(start..safe));
            start = safe;
        }
        safe = end;
    }
    if start < frame.len() {
        parts.extend(frame.get(start..));
    }
    parts
}

/// Whether more chunks of this command's upload follow it. The encoder
/// always writes `m` last among the control keys. Text before the command
/// may hold `;` of its own, so we read the keys from the command start.
fn upload_continues(command: &[u8]) -> bool {
    let keys = find(command, PASSTHROUGH_COMMAND_START)
        .and_then(|start| command.get(start + PASSTHROUGH_COMMAND_START.len()..))
        .unwrap_or_default();
    keys.split(|byte| *byte == b';')
        .next()
        .is_some_and(|keys| keys.ends_with(b"m=1"))
}

/// `bytes` with every graphics command wrapped for passthrough, which is
/// the only way tmux forwards one to the terminal. Other bytes pass as
/// they are.
#[must_use]
pub fn wrap_for_passthrough(bytes: &[u8]) -> Vec<u8> {
    let mut wrapped = Vec::with_capacity(bytes.len() + bytes.len() / 64 + 16);
    let mut rest = bytes;
    while let Some(start) = find(rest, PASSTHROUGH_COMMAND_START) {
        let after_start = rest
            .get(start + PASSTHROUGH_COMMAND_START.len()..)
            .unwrap_or_default();
        let Some(end) = find(after_start, STRING_END) else {
            break;
        };
        let command_end = start + PASSTHROUGH_COMMAND_START.len() + end + STRING_END.len();
        wrapped.extend_from_slice(rest.get(..start).unwrap_or_default());
        wrapped.extend_from_slice(b"\x1bPtmux;");
        for byte in rest.get(start..command_end).unwrap_or_default() {
            if *byte == 0x1b {
                wrapped.push(0x1b);
            }
            wrapped.push(*byte);
        }
        wrapped.extend_from_slice(STRING_END);
        rest = rest.get(command_end..).unwrap_or_default();
    }
    wrapped.extend_from_slice(rest);
    wrapped
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

#[cfg(test)]
mod tests {

    use crate::kitty::{CHUNK, Format, ImageId, Transmit};

    #[test]
    fn commands_are_wrapped_with_their_escapes_doubled_and_text_kept() {
        let wrapped = super::wrap_for_passthrough(b"ab\x1b_Ga=t;AAAA\x1b\\cd\x1b[2J");
        assert_eq!(
            wrapped,
            b"ab\x1bPtmux;\x1b\x1b_Ga=t;AAAA\x1b\x1b\\\x1b\\cd\x1b[2J".to_vec()
        );
    }

    #[test]
    fn splitting_respects_the_limit_between_complete_commands() {
        let frame = b"\x1b_Ga=t,i=1,m=0;aaaa\x1b\\\x1b_Ga=t,i=2,m=0;bbbb\x1b\\\x1b_Ga=p,i=2\x1b\\";
        let parts = super::split_at_commands(frame, 24);
        assert_eq!(parts.concat(), frame, "nothing is lost");
        assert!(parts.len() > 1, "the frame is split");
        assert!(parts.iter().all(|part| part.ends_with(b"\x1b\\")));
    }

    #[test]
    fn splitting_never_divides_an_encoded_chunked_upload() {
        let payload = vec![0xa5; 2 * CHUNK * 3 / 4 + 1];
        let frames = Transmit::new(ImageId(9), Format::Rgb, 10, 20, &payload)
            .encode()
            .expect("encoding cannot fail");
        let mut upload = b"\x1b[3;5H\x1b[38;2;1;2;3m".to_vec();
        upload.extend(frames.concat());
        let mut wire = upload.clone();
        wire.extend_from_slice(b"\x1b_Ga=p,i=9\x1b\\");
        let parts = super::split_at_commands(&wire, 16);
        assert_eq!(parts.concat(), wire);
        assert_eq!(parts.len(), 2, "the upload, then the placement");
        assert_eq!(parts[0], upload.as_slice());
    }
}
