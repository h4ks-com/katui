//! Image transmission (`a=t`): one image's pixels, chunked and optionally
//! compressed.

use std::fmt::Write as _;
use std::io::Write as _;

use base64::Engine as _;
use flate2::Compression;
use flate2::write::ZlibEncoder;

use super::{CHUNK, Format, ImageId, Quiet, apc};

/// Transmit-only command (`a=t`): upload payload bytes under an id without
/// displaying anything.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transmit<'a> {
    /// Id the terminal stores the image under.
    pub id: ImageId,
    /// Pixel format of `payload`.
    pub format: Format,
    /// Width in pixels (`s`); PNG reads it from the data and ignores it.
    /// Rgb and Rgba require a nonzero width: the terminal rejects the
    /// command without it.
    pub width: u32,
    /// Height in pixels (`v`), required the same way as the width.
    pub height: u32,
    /// Raw pixel or PNG bytes.
    pub payload: &'a [u8],
    /// Answer suppression, [`Quiet::Silent`] from the constructor.
    pub quiet: Quiet,
    /// Zlib-compress the payload before base64 (`o=z`), the default for
    /// anything frame-sized.
    pub compressed: bool,
}

impl<'a> Transmit<'a> {
    /// Build a silent transmit command.
    #[must_use]
    pub fn new(id: ImageId, format: Format, width: u32, height: u32, payload: &'a [u8]) -> Self {
        Self {
            id,
            format,
            width,
            height,
            payload,
            quiet: Quiet::Silent,
            compressed: false,
        }
    }

    /// Encode as APC frames, one per chunk. Compression failures
    /// propagate: a frame declaring `o=z` never carries raw bytes.
    ///
    /// The payload is base64 encoded and cut into chunks of at most
    /// [`CHUNK`] characters. Base64 length is a multiple of four, so every
    /// chunk except the last meets the protocol's multiple-of-four rule by
    /// construction. Only the first frame carries control data; the rest
    /// carry the `m` continuation flag alone.
    pub fn encode(&self) -> std::io::Result<Vec<Vec<u8>>> {
        let mut head = String::from("a=t");
        let _ = write!(head, ",f={}", self.format.key());
        head.push_str(",t=d");
        if self.compressed {
            head.push_str(",o=z");
        }
        if self.width > 0 {
            let _ = write!(head, ",s={}", self.width);
        }
        if self.height > 0 {
            let _ = write!(head, ",v={}", self.height);
        }
        if self.id.0 > 0 {
            let _ = write!(head, ",i={}", self.id.0);
        }
        let _ = write!(head, ",q={}", self.quiet.key());
        encode_chunks(&head, self.payload, self.compressed)
    }
}

/// Cut `payload` into APC frames under an existing `head` of control
/// keys: compress when asked, base64, chunk at [`CHUNK`] characters, and
/// carry the control data on the first frame alone. Compression runs
/// at the fast level: frames are a realtime stream, and the CPU spent
/// on tighter ratios starves the framerate.
pub(super) fn encode_chunks(
    head: &str,
    payload: &[u8],
    compressed: bool,
) -> std::io::Result<Vec<Vec<u8>>> {
    let raw = if compressed {
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::fast());
        encoder.write_all(payload)?;
        encoder.finish()?
    } else {
        payload.to_vec()
    };
    let encoded = base64::engine::general_purpose::STANDARD.encode(raw);
    let chunks: Vec<&str> = if encoded.is_empty() {
        vec![""]
    } else {
        encoded
            .as_bytes()
            .chunks(CHUNK)
            .map(|chunk| std::str::from_utf8(chunk).unwrap_or_default())
            .collect()
    };
    let last = chunks.len() - 1;
    let frames = chunks
        .iter()
        .enumerate()
        .map(|(index, chunk)| {
            let more = u8::from(index < last);
            if index == 0 {
                apc(&format!("{head},m={more};{chunk}")).into_bytes()
            } else {
                apc(&format!("m={more};{chunk}")).into_bytes()
            }
        })
        .collect();
    Ok(frames)
}
