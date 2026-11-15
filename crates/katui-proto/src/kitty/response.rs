//! The terminal's answers, and the barrier query that proves it read
//! everything before it.

use super::{ImageId, PlacementId, apc};

/// A kitty query carrying `id`: the terminal parses it in stream order,
/// so its answer proves every earlier byte has been consumed.
#[must_use]
pub fn barrier(id: ImageId) -> Vec<u8> {
    apc(&format!("i={},s=1,v=1,a=q,t=d,f=24;AAAA", id.0)).into_bytes()
}

/// One answer frame from the terminal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Response {
    /// The command succeeded.
    Ok {
        /// The image id the answer refers to.
        image: ImageId,
        /// The placement id, when the answer carries one.
        placement: Option<PlacementId>,
    },
    /// The command failed; the message explains why.
    Error {
        /// The image id the answer refers to.
        image: ImageId,
        /// The placement id, when the answer carries one.
        placement: Option<PlacementId>,
        /// The terminal's error text, for example `ENOENT:...`.
        message: String,
    },
}

/// Pull every graphics answer out of `input`, ignoring any other bytes
/// around them: a terminal may interleave keystrokes with answers, and a
/// half-received frame stays ignored until its terminator arrives.
///
/// Malformed frames are skipped silently; parsing never fails and never
/// panics, whatever the input.
#[must_use]
pub fn parse_responses(input: &[u8]) -> Vec<Response> {
    let mut responses = Vec::new();
    let mut cursor = 0;
    while cursor < input.len().saturating_sub(2) {
        if input.get(cursor..cursor + 3) != Some(b"\x1b_G") {
            cursor += 1;
            continue;
        }
        let Some(tail) = input.get(cursor + 3..) else {
            break;
        };
        let Some(terminator) = tail.windows(2).position(|w| w == b"\x1b\\") else {
            break;
        };
        if let Some(frame) = tail.get(..terminator)
            && let Some(response) = parse_frame(frame)
        {
            responses.push(response);
            cursor += 3 + terminator + 2;
        } else {
            // the bytes were a false start: resume inside them, so a stray
            // _G does not swallow the terminator of the real frame after it
            cursor += 3;
        }
    }
    responses
}

/// Parse one answer frame body: `i=<id>[,p=<pid>];<message>`.
fn parse_frame(frame: &[u8]) -> Option<Response> {
    let text = std::str::from_utf8(frame).ok()?;
    let (keys, message) = text.split_once(';')?;
    let mut image = None;
    let mut placement = None;
    for pair in keys.split(',') {
        let (key, value) = pair.split_once('=')?;
        match key {
            "i" => image = value.parse().ok().map(ImageId),
            "p" => placement = value.parse().ok().map(PlacementId),
            _ => {}
        }
    }
    let image = image?;
    if message == "OK" {
        return Some(Response::Ok { image, placement });
    }
    Some(Response::Error {
        image,
        placement,
        message: message.to_owned(),
    })
}
