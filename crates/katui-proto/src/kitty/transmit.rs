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
    /// Where the pixels travel: in the escape code, or in a file the
    /// terminal reads, in which case `payload` is ignored.
    pub medium: Medium,
}

/// Where a transmit's pixels travel.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Medium {
    /// Inside the escape code, base64 encoded (`t=d`).
    #[default]
    Direct,
    /// A file the terminal reads and then deletes (`t=t`). A terminal only
    /// reads such a file from a temporary directory, with
    /// `tty-graphics-protocol` in its name, and only on the same machine.
    TempFile(String),
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
            medium: Medium::Direct,
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
        let compressed = self.compressed && self.medium == Medium::Direct;
        head.push_str(match self.medium {
            Medium::Direct => ",t=d",
            Medium::TempFile(_) => ",t=t",
        });
        if compressed {
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
        let payload = match &self.medium {
            Medium::Direct => self.payload,
            Medium::TempFile(path) => path.as_bytes(),
        };
        encode_chunks(&head, payload, compressed)
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

#[cfg(test)]
pub(in crate::kitty) mod tests {

    use crate::kitty::{CHUNK, Format, ImageId, Quiet, Transmit};

    pub(in crate::kitty) fn transmit() -> Transmit<'static> {
        Transmit::new(ImageId(1), Format::Rgba, 1, 1, &[0, 0, 0, 0xff])
    }

    /// The concatenated base64 payload of every frame, terminators and
    /// control data stripped.
    pub(in crate::kitty) fn joined_payload(frames: &[Vec<u8>]) -> String {
        let mut joined = String::new();
        for frame in frames {
            let text = String::from_utf8_lossy(frame);
            let chunk = text.split(';').next_back().expect("separator");
            let chunk = chunk.strip_suffix("\x1b\\").expect("frame terminator");
            joined.push_str(chunk);
        }
        joined
    }

    #[test]
    fn transmit_encodes_one_golden_frame() {
        let frames = transmit().encode().expect("encoding cannot fail");
        assert_eq!(
            frames,
            vec![b"\x1b_Ga=t,f=32,t=d,s=1,v=1,i=1,q=2,m=0;AAAA/w==\x1b\\".to_vec()]
        );
    }

    #[test]
    fn a_temp_file_transmit_names_the_file_and_sends_no_pixels() {
        let mut command = transmit();
        command.compressed = true;
        command.medium = crate::kitty::Medium::TempFile("/tmp/tty-graphics-protocol-1".into());
        let frames = command.encode().expect("encoding cannot fail");
        assert_eq!(
            frames,
            vec![
                b"\x1b_Ga=t,f=32,t=t,s=1,v=1,i=1,q=2,m=0;L3RtcC90dHktZ3JhcGhpY3MtcHJvdG9jb2wtMQ==\x1b\\"
                    .to_vec()
            ],
            "the terminal reads the file raw, so it is never compressed"
        );
    }

    #[test]
    fn transmit_emits_the_control_data_only_on_the_first_chunk() {
        let payload = vec![0xa5; 2 * CHUNK * 3 / 4 + 1];
        let command = Transmit::new(ImageId(9), Format::Rgb, 10, 20, &payload);
        let frames = command.encode().expect("encoding cannot fail");
        assert_eq!(frames.len(), 3);
        let first = String::from_utf8_lossy(&frames[0]).into_owned();
        assert!(first.starts_with("\x1b_Ga=t,f=24,t=d,s=10,v=20,i=9,q=2,m=1;"));
        assert!(first.ends_with("\x1b\\"));
        for continuation in &frames[1..2] {
            let text = String::from_utf8_lossy(continuation);
            assert!(text.starts_with("\x1b_Gm=1;"));
            assert!(text.ends_with("\x1b\\"));
        }
        let last = String::from_utf8_lossy(&frames[2]);
        assert!(last.starts_with("\x1b_Gm=0;"));
    }

    #[test]
    fn chunks_respect_the_size_and_multiple_of_four_rules() {
        for size in [0, 1, 2, 3, 3071, 3072, 3073, 8192] {
            let payload = vec![0u8; size];
            let frames = Transmit::new(ImageId(2), Format::Rgba, size as u32, 1, &payload)
                .encode()
                .expect("encoding cannot fail in tests");
            let expected = size.div_ceil(3) * 4;
            assert_eq!(
                frames.len(),
                expected.div_ceil(CHUNK).max(1),
                "chunk count for {size} raw bytes"
            );
            for frame in &frames {
                let text = String::from_utf8_lossy(frame);
                let payload = text.split(';').next_back().expect("separator");
                let payload = payload.strip_suffix("\x1b\\").unwrap_or(payload);
                assert!(payload.len() <= CHUNK);
            }
            for frame in &frames[..frames.len() - 1] {
                let text = String::from_utf8_lossy(frame);
                let payload = text.split(';').next_back().expect("separator");
                let payload = payload.strip_suffix("\x1b\\").unwrap_or(payload);
                assert_eq!(payload.len() % 4, 0, "non-final chunks are a multiple of 4");
            }
        }
    }

    #[test]
    fn a_compressed_transmit_round_trips_through_zlib() {
        use base64::Engine as _;
        use flate2::read::ZlibDecoder;
        use std::io::Read as _;

        let payload: Vec<u8> = (0..600u32).map(|i| (i % 7) as u8).collect();
        let mut command = Transmit::new(ImageId(5), Format::Rgba, 10, 20, &payload);
        command.compressed = true;
        let frames = command.encode().expect("encoding cannot fail");
        let first = String::from_utf8_lossy(&frames[0]).into_owned();
        assert!(first.contains("o=z"), "the wire declares compression");
        let joined = joined_payload(&frames);
        let wire = base64::engine::general_purpose::STANDARD
            .decode(&joined)
            .expect("the chunks are valid base64");
        let mut restored = Vec::new();
        ZlibDecoder::new(wire.as_slice())
            .read_to_end(&mut restored)
            .expect("the wire is valid zlib");
        assert_eq!(restored, payload);
        assert!(
            joined.len()
                < base64::engine::general_purpose::STANDARD
                    .encode(&payload)
                    .len(),
            "repetitive pixels compress below their raw size"
        );
    }

    #[test]
    fn png_omits_dimensions_and_quiet_controls_the_q_key() {
        let mut command = Transmit::new(ImageId(3), Format::Png, 0, 0, b"png");
        command.quiet = Quiet::Normal;
        let text =
            String::from_utf8_lossy(&command.encode().expect("encoding cannot fail in tests")[0])
                .into_owned();
        assert!(text.contains("f=100"));
        assert!(!text.contains("s="));
        assert!(!text.contains("v="));
        assert!(text.contains("q=0"));
    }

    #[test]
    fn chunk_payloads_round_trip_to_the_original_bytes() {
        use base64::Engine as _;

        let payload: Vec<u8> = (0..5000u32).map(|i| (i % 256) as u8).collect();
        let frames = Transmit::new(ImageId(4), Format::Rgba, 100, 25, &payload)
            .encode()
            .expect("encoding cannot fail in tests");
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(joined_payload(&frames))
            .expect("concatenated chunks are valid base64");
        assert_eq!(decoded, payload);
    }
}
