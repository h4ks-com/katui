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

#[cfg(test)]
mod tests {
    use crate::fuzz::XorShift;
    use crate::kitty::{ImageId, PlacementId, Response, parse_responses};

    #[test]
    fn random_byte_streams_parse_without_panicking() {
        let mut rng = XorShift::new(0x9e37_79b9_7f4a_7c15);
        for _ in 0..3000 {
            let len = (rng.next() % 128) as usize;
            // half the streams stay pure noise, half carry real frame
            // markers so the parser walks its deepest paths too
            let mut bytes: Vec<u8> = (0..len).map(|_| rng.byte()).collect();
            if rng.next().is_multiple_of(2) {
                let at = (rng.next() as usize) % (len + 1);
                bytes.splice(at..at, b"\x1b_G".iter().copied());
            }
            for response in parse_responses(&bytes) {
                assert!(
                    matches!(response, Response::Ok { .. } | Response::Error { .. }),
                    "only the two answer shapes exist"
                );
            }
        }
    }

    #[test]
    fn a_stray_frame_start_cannot_swallow_the_next_terminator() {
        let input = b"\x1b_G\x1b_Gi=1;OK\x1b\\";
        assert_eq!(
            super::parse_responses(input),
            vec![Response::Ok {
                image: ImageId(1),
                placement: None
            }]
        );
    }

    #[test]
    fn parses_ok_and_error_answers() {
        let input = b"\x1b_Gi=31;OK\x1b\\\x1b_Gi=7,p=2;ENOENT:missing\x1b\\";
        assert_eq!(
            super::parse_responses(input),
            vec![
                Response::Ok {
                    image: ImageId(31),
                    placement: None
                },
                Response::Error {
                    image: ImageId(7),
                    placement: Some(PlacementId(2)),
                    message: "ENOENT:missing".to_owned()
                }
            ]
        );
    }

    #[test]
    fn parsing_ignores_interleaved_junk_and_unterminated_frames() {
        let input = b"x\x1b[?u\x1b_Gi=1;OK\x1b\\key\x1b_Gi=2;OK";
        assert_eq!(
            super::parse_responses(input),
            vec![Response::Ok {
                image: ImageId(1),
                placement: None
            }]
        );
        assert!(super::parse_responses(b"plain typing, no frames").is_empty());
        assert!(super::parse_responses(b"\x1b_Ggarbage").is_empty());
        assert!(super::parse_responses(&[0xff, 0xfe, 0x1b, 0x47, 0x00, 0x1b, 0x5c]).is_empty());
    }

    #[test]
    fn parsing_skips_frames_without_an_image_id() {
        assert!(super::parse_responses(b"\x1b_Gp=2;OK\x1b\\").is_empty());
    }
}
