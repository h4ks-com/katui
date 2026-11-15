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
