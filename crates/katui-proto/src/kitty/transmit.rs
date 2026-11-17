//! Image transmission (`a=t`): one image's pixels, chunked and optionally
//! compressed.

use std::fmt::Write as _;
use std::io::Write as _;

use base64::Engine as _;
use flate2::write::ZlibEncoder;
use flate2::{Compress, Compression, FlushCompress, Status};

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
    /// Mark the image as short-lived (`N=1`), so a terminal that knows the
    /// hint keeps it out of its disk cache. Others ignore the key.
    pub transient: bool,
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
    /// A POSIX shared memory object the terminal reads `size` bytes from
    /// and then unlinks (`t=s`), on the same machine only. We send the
    /// size since macOS rounds the object up to a whole page.
    SharedMemory { name: String, size: usize },
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
            transient: false,
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
        match &self.medium {
            Medium::Direct => head.push_str(",t=d"),
            Medium::TempFile(_) => head.push_str(",t=t"),
            Medium::SharedMemory { size, .. } => {
                let _ = write!(head, ",t=s,S={size}");
            }
        }
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
        if self.transient {
            head.push_str(",N=1");
        }
        let payload = match &self.medium {
            Medium::Direct => self.payload,
            Medium::TempFile(path) => path.as_bytes(),
            Medium::SharedMemory { name, .. } => name.as_bytes(),
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
        zlib(payload)?
    } else {
        payload.to_vec()
    };
    let encoded = base64(&raw);
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

/// Payloads below this many bytes compress and encode on one thread, where
/// starting threads costs more than it saves.
const PARALLEL_FROM: usize = 256 * 1024;

/// How many slices a payload of `length` bytes is cut into, one per core.
fn slices_for(length: usize) -> usize {
    let cores = std::thread::available_parallelism().map_or(1, std::num::NonZero::get);
    (length / PARALLEL_FROM).clamp(1, cores)
}

/// `payload` as one zlib stream. A large payload deflates in slices on
/// every core, like pigz: each slice but the last ends on a sync flush, so
/// the slices join into one valid stream under one header and checksum.
fn zlib(payload: &[u8]) -> std::io::Result<Vec<u8>> {
    let slices = slices_for(payload.len());
    if slices == 1 {
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::fast());
        encoder.write_all(payload)?;
        return encoder.finish();
    }
    let size = payload.len().div_ceil(slices);
    let parts: Vec<&[u8]> = payload.chunks(size).collect();
    let last = parts.len() - 1;
    let (deflated, checksum) = std::thread::scope(|scope| {
        let workers: Vec<_> = parts
            .iter()
            .enumerate()
            .map(|(index, part)| scope.spawn(move || deflate(part, index == last)))
            .collect();
        let mut adler = simd_adler32::Adler32::new();
        adler.write(payload);
        let deflated: Vec<_> = workers
            .into_iter()
            .map(|worker| {
                worker
                    .join()
                    .unwrap_or_else(|_| Err(std::io::Error::other("a compression thread panicked")))
            })
            .collect();
        (deflated, adler.finish())
    });
    // 0x78 0x01: a 32 KB window at the fastest level
    let mut stream = vec![0x78, 0x01];
    for part in deflated {
        stream.extend_from_slice(&part?);
    }
    stream.extend_from_slice(&checksum.to_be_bytes());
    Ok(stream)
}

/// One slice as raw deflate: closed by a sync flush, or by the final
/// block when it is the `last` slice.
fn deflate(slice: &[u8], last: bool) -> std::io::Result<Vec<u8>> {
    let mut compress = Compress::new(Compression::fast(), false);
    let flush = if last {
        FlushCompress::Finish
    } else {
        FlushCompress::Sync
    };
    let mut out = Vec::with_capacity(slice.len() / 2 + 1024);
    loop {
        let consumed = usize::try_from(compress.total_in()).unwrap_or(slice.len());
        let status = compress
            .compress_vec(slice.get(consumed..).unwrap_or_default(), &mut out, flush)
            .map_err(std::io::Error::other)?;
        let all_in = usize::try_from(compress.total_in()).unwrap_or(0) == slice.len();
        let done = if last {
            status == Status::StreamEnd
        } else {
            all_in && out.len() < out.capacity()
        };
        if done {
            return Ok(out);
        }
        out.reserve(out.capacity().max(1024));
    }
}

/// `raw` in base64, a large payload in slices on every core. Each slice
/// but the last is a whole number of three-byte groups, so the encoded
/// slices join with no padding between them.
fn base64(raw: &[u8]) -> String {
    let slices = slices_for(raw.len());
    if slices == 1 {
        return base64::engine::general_purpose::STANDARD.encode(raw);
    }
    let size = raw.len().div_ceil(slices).div_ceil(3) * 3;
    std::thread::scope(|scope| {
        let workers: Vec<_> = raw
            .chunks(size)
            .map(|part| scope.spawn(move || base64::engine::general_purpose::STANDARD.encode(part)))
            .collect();
        workers
            .into_iter()
            .map(|worker| worker.join().unwrap_or_default())
            .collect()
    })
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
    fn a_shared_memory_transmit_names_the_object_its_size_and_the_transient_hint() {
        let mut command = transmit();
        command.compressed = true;
        command.transient = true;
        command.medium = crate::kitty::Medium::SharedMemory {
            name: "/kt1-0".into(),
            size: 4,
        };
        let frames = command.encode().expect("encoding cannot fail");
        assert_eq!(
            frames,
            vec![b"\x1b_Ga=t,f=32,t=s,S=4,s=1,v=1,i=1,q=2,N=1,m=0;L2t0MS0w\x1b\\".to_vec()]
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
    fn a_frame_sized_payload_compresses_in_slices_into_one_valid_stream() {
        use base64::Engine as _;
        use flate2::read::ZlibDecoder;
        use std::io::Read as _;

        let payload: Vec<u8> = (0..3 * 1024 * 1024u32)
            .map(|i| ((i / 7) ^ (i % 251)) as u8)
            .collect();
        let mut command = Transmit::new(ImageId(6), Format::Rgba, 1024, 768, &payload);
        command.compressed = true;
        let frames = command.encode().expect("encoding cannot fail");
        let wire = base64::engine::general_purpose::STANDARD
            .decode(joined_payload(&frames))
            .expect("the slices join into valid base64");
        let mut restored = Vec::new();
        ZlibDecoder::new(wire.as_slice())
            .read_to_end(&mut restored)
            .expect("the slices join into one valid zlib stream");
        assert_eq!(restored, payload);
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
