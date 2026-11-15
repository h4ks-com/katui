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
    let mut upload = b"\x1b[3;5H\x1b[38;2;1;2;3m".to_vec();
    upload.extend(frames.concat());
    let mut wire = upload.clone();
    wire.extend_from_slice(b"\x1b_Ga=p,i=9\x1b\\");
    let parts = super::split_at_commands(&wire, 16);
    assert_eq!(parts.concat(), wire);
    assert_eq!(parts.len(), 2, "the upload, then the placement");
    assert_eq!(parts[0], upload.as_slice());
}

#[test]
fn an_id_space_puts_its_byte_above_the_local_id() {
    let space = super::IdSpace::of_process(4242);
    let image = space.image(0x0100_0007);
    assert_eq!(image.0 & 0x00ff_ffff, 7, "local ids keep 24 bits");
    assert_ne!(image.0 >> 24, 0, "every process space is nonzero");
    assert_eq!(space.image(7), image);
}

#[test]
fn a_big_upload_goes_up_as_a_blank_image_and_bands_of_rows() {
    let pixels = vec![0x40; 4 * 8 * 10];
    let frames =
        super::banded_upload(ImageId(5), 8, 10, &pixels, 4 * 8 * 4).expect("encoding cannot fail");
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
