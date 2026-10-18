//! The kitty graphics protocol codec: transmit, place, delete and the
//! answer parser, translated from the specification at
//! <https://sw.kovidgoyal.net/kitty/graphics-protocol/>.
//!
//! Pure bytes in and out. Control-data key order is canonical
//! (a, f, t, s, v, i, q, m for transmit) so golden tests can pin exact
//! output, and every command defaults to one-way emission (`q=2`): the
//! callers that want answers opt in, the ones inside host applications
//! stay silent.

use std::fmt::Write as _;

use base64::Engine as _;
use flate2::Compression;
use flate2::write::ZlibEncoder;
use std::io::Write as _;

/// The protocol's maximum chunk payload, in base64 characters.
pub const CHUNK: usize = 4096;

/// An image id in the terminal's global id namespace. Zero means "no id":
/// transmit then uploads without one and place and delete omit the key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImageId(pub u32);

/// A placement id, unique per image. Zero omits the key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlacementId(pub u32);

/// Pixel data format (the `f` key).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// 24-bit RGB, three bytes per pixel (`f=24`).
    Rgb,
    /// 32-bit RGBA, four bytes per pixel (`f=32`).
    Rgba,
    /// PNG, dimensions read from the data (`f=100`).
    Png,
}

impl Format {
    fn key(self) -> u32 {
        match self {
            Self::Rgb => 24,
            Self::Rgba => 32,
            Self::Png => 100,
        }
    }
}

/// Answer suppression (the `q` key).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Quiet {
    /// Answer every command.
    Normal,
    /// Suppress success answers, keep errors.
    ErrorsOnly,
    /// Suppress everything, the default here.
    Silent,
}

impl Quiet {
    fn key(self) -> u32 {
        match self {
            Self::Normal => 0,
            Self::ErrorsOnly => 1,
            Self::Silent => 2,
        }
    }
}

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

    /// Encode as APC frames, one per chunk.
    ///
    /// The payload is base64 encoded and cut into chunks of at most
    /// [`CHUNK`] characters. Base64 length is a multiple of four, so every
    /// chunk except the last meets the protocol's multiple-of-four rule by
    /// construction. Only the first frame carries control data; the rest
    /// carry the `m` continuation flag alone.
    pub fn encode(&self) -> Vec<Vec<u8>> {
        let raw = if self.compressed {
            let mut encoder = ZlibEncoder::new(Vec::new(), Compression::fast());
            // a ZlibEncoder over a Vec cannot fail, only its finish reports
            let _ = encoder.write_all(self.payload);
            encoder.finish().unwrap_or_else(|_| self.payload.to_vec())
        } else {
            self.payload.to_vec()
        };
        let encoded = base64::engine::general_purpose::STANDARD.encode(raw);
        let chunks: Vec<&str> = if encoded.is_empty() {
            vec![""]
        } else {
            // base64 output is ascii by construction, so this cannot fail
            encoded
                .as_bytes()
                .chunks(CHUNK)
                .map(|c| std::str::from_utf8(c).unwrap_or_default())
                .collect()
        };
        let last = chunks.len() - 1;
        chunks
            .iter()
            .enumerate()
            .map(|(index, chunk)| {
                let more = u8::from(index < last);
                let mut command = String::new();
                if index == 0 {
                    command.push_str("a=t");
                    let _ = write!(command, ",f={}", self.format.key());
                    command.push_str(",t=d");
                    if self.compressed {
                        command.push_str(",o=z");
                    }
                    if self.width > 0 {
                        let _ = write!(command, ",s={}", self.width);
                    }
                    if self.height > 0 {
                        let _ = write!(command, ",v={}", self.height);
                    }
                    if self.id.0 > 0 {
                        let _ = write!(command, ",i={}", self.id.0);
                    }
                    let _ = write!(command, ",q={}", self.quiet.key());
                    let _ = write!(command, ",m={more};{chunk}");
                } else {
                    let _ = write!(command, "m={more};{chunk}");
                }
                apc(&command).into_bytes()
            })
            .collect()
    }
}

/// A rectangle of the source image, in pixels (the `x y w h` crop keys).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PixelRect {
    /// Left edge of the crop.
    pub x: u32,
    /// Top edge of the crop.
    pub y: u32,
    /// Crop width.
    pub width: u32,
    /// Crop height.
    pub height: u32,
}

/// Display a transmitted image (`a=p`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Place {
    /// The image to display; required, a zero id makes the terminal ignore
    /// the command.
    pub image: ImageId,
    /// Placement id: re-sending the same pair moves it without flicker.
    pub placement: PlacementId,
    /// Display width in columns (`c`); with `rows` alone the terminal
    /// scales keeping the aspect ratio.
    pub columns: u32,
    /// Display height in rows (`r`).
    pub rows: u32,
    /// Z-order; negative draws under the text.
    pub z: i32,
    /// Source rectangle crop, [`None`] displays the whole image.
    pub crop: Option<PixelRect>,
    /// Answer suppression, [`Quiet::Silent`] from the constructor. A silent
    /// placement is the default because a render loop re-places every
    /// frame, and one answer per placement floods the input stream.
    pub quiet: Quiet,
    /// Keep the cursor from advancing past the image (`C=1`).
    pub hold_cursor: bool,
}

impl Place {
    /// Build a silent placement holding the cursor, the presenter default.
    pub fn new(image: ImageId) -> Self {
        Self {
            image,
            placement: PlacementId(0),
            columns: 0,
            rows: 0,
            z: 0,
            crop: None,
            quiet: Quiet::Silent,
            hold_cursor: true,
        }
    }

    /// Encode as one APC frame.
    pub fn encode(&self) -> Vec<u8> {
        let mut command = String::from("a=p");
        if self.image.0 > 0 {
            let _ = write!(command, ",i={}", self.image.0);
        }
        if self.placement.0 > 0 {
            let _ = write!(command, ",p={}", self.placement.0);
        }
        if self.columns > 0 {
            let _ = write!(command, ",c={}", self.columns);
        }
        if self.rows > 0 {
            let _ = write!(command, ",r={}", self.rows);
        }
        if self.z != 0 {
            let _ = write!(command, ",z={}", self.z);
        }
        if let Some(crop) = self.crop {
            let _ = write!(
                command,
                ",x={},y={},w={},h={}",
                crop.x, crop.y, crop.width, crop.height
            );
        }
        let _ = write!(command, ",q={}", self.quiet.key());
        if self.hold_cursor {
            command.push_str(",C=1");
        }
        apc(&command).into_bytes()
    }
}

/// Delete graphics (`a=d`). Deletes are one-way (`q=2`): nothing waits on
/// a delete, so the terminal never answers one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Delete {
    /// Every placement visible on screen (`d=a`).
    AllVisible,
    /// One image, keeping its data for re-display (`d=i`).
    Image(ImageId),
    /// One placement of one image (`d=i` with `p`).
    Placement(ImageId, PlacementId),
}

impl Delete {
    /// Encode as one APC frame.
    pub fn encode(&self) -> Vec<u8> {
        let mut command = String::from("a=d,d=i");
        match self {
            Self::AllVisible => {
                command = String::from("a=d,d=a");
            }
            Self::Image(id) => {
                if id.0 > 0 {
                    let _ = write!(command, ",i={}", id.0);
                }
            }
            Self::Placement(id, placement) => {
                if id.0 > 0 {
                    let _ = write!(command, ",i={}", id.0);
                }
                if placement.0 > 0 {
                    let _ = write!(command, ",p={}", placement.0);
                }
            }
        }
        command.push_str(",q=2");
        apc(&command).into_bytes()
    }
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

/// Frame `command` as a kitty graphics APC: `ESC _ G` + command + `ESC \`.
fn apc(command: &str) -> String {
    let mut out = String::with_capacity(command.len() + 8);
    out.push_str("\x1b_G");
    out.push_str(command);
    out.push_str("\x1b\\");
    out
}

#[cfg(test)]
mod tests {
    use super::{CHUNK, Delete, Format, ImageId, Place, PlacementId, Quiet, Response, Transmit};

    fn transmit() -> Transmit<'static> {
        Transmit::new(ImageId(1), Format::Rgba, 1, 1, &[0, 0, 0, 0xff])
    }

    #[test]
    fn transmit_encodes_one_golden_frame() {
        let frames = transmit().encode();
        assert_eq!(
            frames,
            vec![b"\x1b_Ga=t,f=32,t=d,s=1,v=1,i=1,q=2,m=0;AAAA/w==\x1b\\".to_vec()]
        );
    }

    #[test]
    fn transmit_emits_the_control_data_only_on_the_first_chunk() {
        let payload = vec![0xa5; 2 * CHUNK * 3 / 4 + 1];
        let command = Transmit::new(ImageId(9), Format::Rgb, 10, 20, &payload);
        let frames = command.encode();
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
            let frames = Transmit::new(ImageId(2), Format::Rgba, size as u32, 1, &payload).encode();
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
        let frames = command.encode();
        let first = String::from_utf8_lossy(&frames[0]).into_owned();
        assert!(first.contains("o=z"), "the wire declares compression");
        let mut joined = String::new();
        for frame in &frames {
            let text = String::from_utf8_lossy(frame);
            let chunk = text.split(';').next_back().expect("separator");
            let chunk = chunk.strip_suffix("\x1b\\").expect("frame terminator");
            joined.push_str(chunk);
        }
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
        let text = String::from_utf8_lossy(&command.encode()[0]).into_owned();
        assert!(text.contains("f=100"));
        assert!(!text.contains("s="));
        assert!(!text.contains("v="));
        assert!(text.contains("q=0"));
    }

    #[test]
    fn place_encodes_a_golden_frame() {
        let mut place = Place::new(ImageId(7));
        place.placement = PlacementId(3);
        place.columns = 4;
        place.rows = 2;
        place.z = -1;
        assert_eq!(
            place.encode(),
            b"\x1b_Ga=p,i=7,p=3,c=4,r=2,z=-1,q=2,C=1\x1b\\".to_vec()
        );
    }

    #[test]
    fn place_appends_the_crop_keys() {
        let mut place = Place::new(ImageId(7));
        place.crop = Some(super::PixelRect {
            x: 10,
            y: 5,
            width: 100,
            height: 200,
        });
        assert_eq!(
            place.encode(),
            b"\x1b_Ga=p,i=7,x=10,y=5,w=100,h=200,q=2,C=1\x1b\\".to_vec()
        );
    }

    #[test]
    fn delete_encodes_golden_frames() {
        assert_eq!(
            Delete::AllVisible.encode(),
            b"\x1b_Ga=d,d=a,q=2\x1b\\".to_vec()
        );
        assert_eq!(
            Delete::Image(ImageId(10)).encode(),
            b"\x1b_Ga=d,d=i,i=10,q=2\x1b\\".to_vec()
        );
        assert_eq!(
            Delete::Placement(ImageId(10), PlacementId(2)).encode(),
            b"\x1b_Ga=d,d=i,i=10,p=2,q=2\x1b\\".to_vec()
        );
    }

    #[test]
    fn a_zero_id_omits_the_id_keys() {
        assert_eq!(
            Delete::Image(ImageId(0)).encode(),
            b"\x1b_Ga=d,d=i,q=2\x1b\\".to_vec()
        );
        assert_eq!(
            Delete::Placement(ImageId(0), PlacementId(2)).encode(),
            b"\x1b_Ga=d,d=i,p=2,q=2\x1b\\".to_vec()
        );
    }

    #[test]
    fn chunk_payloads_round_trip_to_the_original_bytes() {
        use base64::Engine as _;

        let payload: Vec<u8> = (0..5000u32).map(|i| (i % 256) as u8).collect();
        let frames = Transmit::new(ImageId(4), Format::Rgba, 100, 25, &payload).encode();
        let mut joined = String::new();
        for frame in &frames {
            let text = String::from_utf8_lossy(frame);
            let chunk = text.split(';').next_back().expect("separator");
            let chunk = chunk.strip_suffix("\x1b\\").expect("frame terminator");
            joined.push_str(chunk);
        }
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(joined)
            .expect("concatenated chunks are valid base64");
        assert_eq!(decoded, payload);
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
