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

/// The image ids one program owns on a terminal it shares with others:
/// its own high byte over 24-bit local ids. The high byte fits the
/// placeholder diacritic, so anchored ids keep working.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IdSpace(u8);

impl IdSpace {
    /// The space of the running process, from its process id.
    // Two processes whose ids match modulo 255 share a space, a rare clash we accept.
    #[must_use]
    pub fn of_process() -> Self {
        Self(u8::try_from(std::process::id() % 255).unwrap_or(0) + 1)
    }

    /// The id `local` takes inside this space.
    #[must_use]
    pub fn image(self, local: u32) -> ImageId {
        ImageId(u32::from(self.0) << 24 | (local & 0x00ff_ffff))
    }
}

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
fn encode_chunks(head: &str, payload: &[u8], compressed: bool) -> std::io::Result<Vec<Vec<u8>>> {
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
        // base64 output is ascii by construction
        encoded
            .as_bytes()
            .chunks(CHUNK)
            .map(|c| match std::str::from_utf8(c) {
                Ok(text) => text,
                Err(_) => unreachable!("base64 output is ASCII"),
            })
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
    /// A virtual placement that Unicode placeholder text shows (`U=1`).
    pub virtual_placement: bool,
    /// The placement this one sits relative to (`P`, `Q`), offset by
    /// whole cells (`H`, `V`).
    pub parent: Option<Relative>,
}

/// Where a placement sits relative to another placement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Relative {
    /// The parent's image.
    pub image: ImageId,
    /// The parent's placement.
    pub placement: PlacementId,
    /// Cells to the right of the parent's top-left cell.
    pub columns: i32,
    /// Cells below the parent's top-left cell.
    pub rows: i32,
}

impl Place {
    /// Build a silent placement holding the cursor, the presenter default.
    #[must_use]
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
            virtual_placement: false,
            parent: None,
        }
    }

    /// Encode as one APC frame.
    #[must_use]
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
        if self.virtual_placement {
            command.push_str(",U=1");
        }
        if let Some(parent) = self.parent {
            let _ = write!(
                command,
                ",P={},Q={},H={},V={}",
                parent.image.0, parent.placement.0, parent.columns, parent.rows
            );
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
    /// One image's placements, keeping its data for re-display (`d=i`).
    Image(ImageId),
    /// One image and its stored data (`d=I`).
    Free(ImageId),
    /// One placement of one image (`d=i` with `p`).
    Placement(ImageId, PlacementId),
    /// Every image with an id in this inclusive range, data included
    /// (`d=R`).
    FreeRange(ImageId, ImageId),
}

/// A kitty query carrying `id`: the terminal parses it in stream order,
/// so its answer proves every earlier byte has been consumed.
#[must_use]
pub fn barrier(id: ImageId) -> Vec<u8> {
    apc(&format!("i={},s=1,v=1,a=q,t=d,f=24;AAAA", id.0)).into_bytes()
}

/// The canvas a new animation frame composes onto.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Canvas {
    /// Blank, the default: transparent black.
    #[default]
    Blank,
    /// The pixels of an earlier frame, 1-based, so a frame can carry
    /// only the rectangles that changed since it.
    Frame(u32),
}

/// One animation frame's data (`a=f`): an append when `edit` is `None`,
/// an in-place edit of the frame `edit` names otherwise. The payload
/// covers `rect` of the image (the whole image when `None`); the canvas
/// fills the rest. `gap_ms` paces playback: the number of milliseconds
/// the terminal waits on this frame before showing the next.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameTransmit<'a> {
    /// The animated image this frame belongs to.
    pub id: ImageId,
    /// Edit this existing frame instead of appending.
    pub edit: Option<u32>,
    /// The canvas the frame's data composes onto.
    pub canvas: Canvas,
    /// The part of the image the payload covers, in pixels.
    pub rect: Option<PixelRect>,
    /// Payload width in pixels, required by RGBA data.
    pub width: u32,
    /// Payload height in pixels.
    pub height: u32,
    /// Raw RGBA bytes.
    pub payload: &'a [u8],
    /// Zlib-compress the payload (`o=z`).
    pub compressed: bool,
    /// Playback gap in milliseconds (`z`).
    pub gap_ms: Option<i32>,
}

impl FrameTransmit<'_> {
    /// Encode as APC frames, one per chunk, sharing the transmit
    /// chunking. Opaque pixels make the composition a replacement. A
    /// partial frame's rectangle keys already name the payload size:
    /// the data covers that rectangle one to one, so `s` and `v` are
    /// never emitted twice.
    pub fn encode(&self) -> std::io::Result<Vec<Vec<u8>>> {
        let mut head = String::from("a=f");
        let _ = write!(head, ",i={}", self.id.0);
        if let Some(frame) = self.edit {
            let _ = write!(head, ",r={frame}");
        }
        match self.canvas {
            Canvas::Blank => {}
            Canvas::Frame(frame) => {
                let _ = write!(head, ",c={frame}");
            }
        }
        if let Some(rect) = self.rect {
            let _ = write!(
                head,
                ",x={},y={},s={},v={}",
                rect.x, rect.y, rect.width, rect.height
            );
        } else {
            let _ = write!(head, ",s={},v={}", self.width, self.height);
        }
        let _ = write!(head, ",f={},t=d", Format::Rgba.key());
        if self.compressed {
            head.push_str(",o=z");
        }
        head.push_str(",q=2");
        if let Some(gap) = self.gap_ms {
            let _ = write!(head, ",z={gap}");
        }
        encode_chunks(&head, self.payload, self.compressed)
    }
}

/// A compressed RGBA upload as commands of at most `band_bytes` raw
/// pixels each: a blank image of the full size, then one edit of its root
/// frame per band of rows. Each command finishes its own chunked load, so
/// a writer can pause between them, which tmux needs because it discards
/// its whole output buffer once that grows past eight bytes per screen
/// cell.
pub fn banded_upload(
    id: ImageId,
    width: u32,
    height: u32,
    pixels: &[u8],
    band_bytes: usize,
) -> std::io::Result<Vec<Vec<u8>>> {
    let row_bytes = (width as usize).saturating_mul(4).max(1);
    if pixels.len() <= band_bytes || height <= 1 {
        let mut whole = Transmit::new(id, Format::Rgba, width, height, pixels);
        whole.compressed = true;
        return whole.encode();
    }
    let blank = vec![0; pixels.len()];
    let mut root = Transmit::new(id, Format::Rgba, width, height, &blank);
    root.compressed = true;
    let mut frames = root.encode()?;
    let band_rows = (band_bytes / row_bytes).max(1);
    for (index, band) in pixels.chunks(band_rows * row_bytes).enumerate() {
        let rows = u32::try_from(band.len() / row_bytes).unwrap_or(height);
        let top = u32::try_from(index * band_rows).unwrap_or(height);
        let edit = FrameTransmit {
            id,
            edit: Some(1),
            canvas: Canvas::Blank,
            rect: Some(PixelRect {
                x: 0,
                y: top,
                width,
                height: rows,
            }),
            width,
            height: rows,
            payload: band,
            compressed: true,
            gap_ms: None,
        };
        frames.extend(edit.encode()?);
    }
    Ok(frames)
}

/// Animation control (`a=a`): start, stop or pace an animation. In
/// loading mode the terminal plays the frames it holds and waits for
/// more instead of looping, so the wire becomes a buffer and the
/// compositor's clock drives the motion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Animate {
    /// The animated image to control.
    pub id: ImageId,
    /// Stop (`s=1`), play-and-wait (`s=2`), play-and-loop (`s=3`).
    pub state: Option<AnimationState>,
    /// Set the gap of this frame, 1-based, in milliseconds.
    pub frame_gap: Option<(u32, i32)>,
}

/// What an animation does at its last frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnimationState {
    /// Stop on the current frame.
    Stop,
    /// Play, and wait for more frames instead of looping.
    Loading,
    /// Play, looping back to the first frame.
    Loop,
}

impl Animate {
    /// Encode as one APC frame.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut command = format!("a=a,i={}", self.id.0);
        if let Some(state) = self.state {
            let key = match state {
                AnimationState::Stop => 1,
                AnimationState::Loading => 2,
                AnimationState::Loop => 3,
            };
            let _ = write!(command, ",s={key}");
        }
        if let Some((frame, gap)) = self.frame_gap {
            let _ = write!(command, ",r={frame},z={gap}");
        }
        apc(&command).into_bytes()
    }
}

impl Delete {
    /// Encode as one APC frame.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut command = match self {
            Self::AllVisible => String::from("a=d,d=a"),
            Self::Free(_) => String::from("a=d,d=I"),
            Self::Image(_) | Self::Placement(..) => String::from("a=d,d=i"),
            Self::FreeRange(first, last) => format!("a=d,d=R,x={},y={}", first.0, last.0),
        };
        if let Self::Image(id) | Self::Free(id) | Self::Placement(id, _) = self
            && id.0 > 0
        {
            let _ = write!(command, ",i={}", id.0);
        }
        if let Self::Placement(_, placement) = self
            && placement.0 > 0
        {
            let _ = write!(command, ",p={}", placement.0);
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
/// always writes `m` last among the control keys.
fn upload_continues(command: &[u8]) -> bool {
    command
        .split(|byte| *byte == b';')
        .next()
        .is_some_and(|keys| keys.ends_with(b"m=1"))
}

const PASSTHROUGH_COMMAND_START: &[u8] = b"\x1b_G";
const STRING_END: &[u8] = b"\x1b\\";

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
    use super::parse_responses;
    use super::{CHUNK, Delete, Format, ImageId, Place, PlacementId, Quiet, Response, Transmit};
    use crate::fuzz::XorShift;

    fn transmit() -> Transmit<'static> {
        Transmit::new(ImageId(1), Format::Rgba, 1, 1, &[0, 0, 0, 0xff])
    }

    #[test]
    fn commands_are_wrapped_with_their_escapes_doubled_and_text_kept() {
        let wrapped = super::wrap_for_passthrough(b"ab\x1b_Ga=t;AAAA\x1b\\cd\x1b[2J");
        assert_eq!(
            wrapped,
            b"ab\x1bPtmux;\x1b\x1b_Ga=t;AAAA\x1b\x1b\\\x1b\\cd\x1b[2J".to_vec()
        );
    }

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

    /// The concatenated base64 payload of every frame, terminators and
    /// control data stripped.
    fn joined_payload(frames: &[Vec<u8>]) -> String {
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
        let mut wire = frames.concat();
        wire.extend_from_slice(b"\x1b_Ga=p,i=9\x1b\\");
        let parts = super::split_at_commands(&wire, 16);
        assert_eq!(parts.concat(), wire);
        assert_eq!(parts.len(), 2, "the upload, then the placement");
        assert_eq!(parts[0], frames.concat().as_slice());
    }

    #[test]
    fn an_id_space_puts_its_byte_above_the_local_id() {
        let space = super::IdSpace::of_process();
        let image = space.image(0x0100_0007);
        assert_eq!(image.0 & 0x00ff_ffff, 7, "local ids keep 24 bits");
        assert_ne!(image.0 >> 24, 0, "every process space is nonzero");
        assert_eq!(space.image(7), image);
    }

    #[test]
    fn a_big_upload_goes_up_as_a_blank_image_and_bands_of_rows() {
        let pixels = vec![0x40; 4 * 8 * 10];
        let frames = super::banded_upload(ImageId(5), 8, 10, &pixels, 4 * 8 * 4)
            .expect("encoding cannot fail");
        let text: Vec<String> = frames
            .iter()
            .map(|frame| String::from_utf8_lossy(frame).into_owned())
            .collect();
        assert!(text[0].starts_with("\x1b_Ga=t,f=32,t=d,o=z,s=8,v=10,i=5"));
        let bands: Vec<&String> = text.iter().filter(|frame| frame.contains("a=f")).collect();
        assert_eq!(bands.len(), 3, "rows 0..4, 4..8 and 8..10");
        assert!(bands[0].contains("r=1,x=0,y=0,s=8,v=4"));
        assert!(bands[2].contains("r=1,x=0,y=8,s=8,v=2"));
        let small = super::banded_upload(ImageId(5), 8, 10, &pixels, pixels.len())
            .expect("encoding cannot fail");
        assert_eq!(small.len(), 1, "a small upload stays one transmit");
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
    fn a_relative_placement_names_its_parent_and_offset() {
        let mut place = Place::new(ImageId(7));
        place.placement = PlacementId(3);
        place.parent = Some(super::Relative {
            image: ImageId(9),
            placement: PlacementId(1),
            columns: 4,
            rows: 2,
        });
        assert_eq!(
            place.encode(),
            b"\x1b_Ga=p,i=7,p=3,q=2,C=1,P=9,Q=1,H=4,V=2\x1b\\".to_vec()
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
        assert_eq!(
            Delete::Free(ImageId(10)).encode(),
            b"\x1b_Ga=d,d=I,i=10,q=2\x1b\\".to_vec()
        );
        assert_eq!(
            Delete::FreeRange(ImageId(5), ImageId(9)).encode(),
            b"\x1b_Ga=d,d=R,x=5,y=9,q=2\x1b\\".to_vec()
        );
        assert_eq!(
            super::barrier(ImageId(7)),
            b"\x1b_Gi=7,s=1,v=1,a=q,t=d,f=24;AAAA\x1b\\".to_vec()
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
        let frames = Transmit::new(ImageId(4), Format::Rgba, 100, 25, &payload)
            .encode()
            .expect("encoding cannot fail in tests");
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(joined_payload(&frames))
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

    #[test]
    fn frame_transmits_carry_canvas_rect_and_gap() {
        let pixels = [0u8, 0, 0, 255];
        let root = super::FrameTransmit {
            id: super::ImageId(3),
            edit: None,
            canvas: super::Canvas::Blank,
            rect: None,
            width: 1,
            height: 1,
            payload: &pixels,
            compressed: true,
            gap_ms: Some(50),
        };
        let frames = root.encode().expect("encoding cannot fail");
        let text = String::from_utf8_lossy(&frames[0]).into_owned();
        assert!(text.starts_with("\x1b_Ga=f,i=3,s=1,v=1,f=32,t=d,o=z,q=2,z=50,m=0;"));
        assert!(text.ends_with("\x1b\\"));
        // a frame declares its size exactly once: rect keys and payload
        // keys are the same keys
        assert_eq!(text.matches(",s=").count(), 1);
        assert_eq!(text.matches(",v=").count(), 1);

        let edit = super::FrameTransmit {
            id: super::ImageId(3),
            edit: Some(7),
            canvas: super::Canvas::Frame(6),
            rect: Some(super::PixelRect {
                x: 10,
                y: 5,
                width: 4,
                height: 2,
            }),
            width: 4,
            height: 2,
            payload: &[0u8; 32],
            compressed: false,
            gap_ms: None,
        };
        let frames = edit.encode().expect("encoding cannot fail");
        let text = String::from_utf8_lossy(&frames[0]).into_owned();
        assert!(text.contains("a=f,i=3,r=7,c=6,x=10,y=5,s=4,v=2,f=32,t=d"));
        assert!(!text.contains("o=z"), "uncompressed carries no marker");
    }

    #[test]
    fn animation_control_encodes_state_and_gap() {
        let run = super::Animate {
            id: super::ImageId(3),
            state: Some(super::AnimationState::Loading),
            frame_gap: None,
        };
        assert_eq!(run.encode(), b"\x1b_Ga=a,i=3,s=2\x1b\\".to_vec());
        let gap = super::Animate {
            id: super::ImageId(3),
            state: None,
            frame_gap: Some((1, 48)),
        };
        assert_eq!(gap.encode(), b"\x1b_Ga=a,i=3,r=1,z=48\x1b\\".to_vec());
        let stop = super::Animate {
            id: super::ImageId(3),
            state: Some(super::AnimationState::Stop),
            frame_gap: None,
        };
        assert_eq!(stop.encode(), b"\x1b_Ga=a,i=3,s=1\x1b\\".to_vec());
    }
}
