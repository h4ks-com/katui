//! Animation: frames composed onto earlier ones, and the controls that
//! play them.

use std::fmt::Write as _;

use super::place::PixelRect;
use super::transmit::{Transmit, encode_chunks};
use super::{Format, ImageId, apc};

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

#[cfg(test)]
mod tests {

    use crate::kitty::ImageId;

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
