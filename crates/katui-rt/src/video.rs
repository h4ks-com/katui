//! Moving pictures at a steady rate: the box a video gets, the frame size
//! a terminal keeps up with, the presenter that sends whole frames, and a
//! governor that lowers the render scale when frames arrive late.

use std::time::{Duration, Instant};

use katui_present::surface::{PatchedKitty, PixelScreen};
use katui_proto::probe::{Capabilities, CellPixels};
use katui_scene::placement::CellRect;

use crate::stash::local_stash;

/// The most pixels each way we ask a terminal to take per frame.
pub const MOST_PIXELS: (u32, u32) = (960, 540);

/// How many times fewer pixels to send each way when the frames cross a
/// multiplexer inline, which carries them far more slowly.
const MULTIPLEXED_SHRINK: f64 = 2.0;

/// Tiles this many pixels on a side; a video changes nearly every tile,
/// so they only matter for the rare still frame.
const TILE_SIDE: u32 = 64;

/// The largest box of shape `aspect` (width, height) that fits in `area`,
/// centred in it, for cells of `cell` pixels.
#[must_use]
pub fn fit(area: CellRect, cell: CellPixels, aspect: (u32, u32)) -> CellRect {
    let (wide, high) = (u32::from(area.columns), u32::from(area.rows));
    let (cell_w, cell_h) = (u32::from(cell.width).max(1), u32::from(cell.height).max(1));
    let (aspect_w, aspect_h) = (aspect.0.max(1), aspect.1.max(1));
    let rows_at_full_width = wide * cell_w * aspect_h / (aspect_w * cell_h);
    let (columns, rows) = if rows_at_full_width <= high {
        (wide, rows_at_full_width)
    } else {
        (high * cell_h * aspect_w / (aspect_h * cell_w), high)
    };
    let (columns, rows) = (columns.clamp(1, wide.max(1)), rows.clamp(1, high.max(1)));
    let offset = |room: u32, used: u32| u16::try_from(room.saturating_sub(used) / 2).unwrap_or(0);
    CellRect {
        x: area.x + offset(wide, columns),
        y: area.y + offset(high, rows),
        columns: u16::try_from(columns).unwrap_or(u16::MAX),
        rows: u16::try_from(rows).unwrap_or(u16::MAX),
    }
}

/// The picture size for an area of `area` pixels: the area's shape,
/// shrunk to [`MOST_PIXELS`] (halved again when frames cross a
/// multiplexer inline) and to `scale`, with whole even sides.
#[must_use]
#[allow(
    clippy::cast_sign_loss,
    clippy::cast_possible_truncation,
    reason = "a pixel count is positive and far below u32::MAX"
)]
pub fn frame_size(area: (u32, u32), caps: &Capabilities, scale: f64) -> (u32, u32) {
    let inline = caps.multiplexed() && !(caps.shared_memory || caps.local_files);
    let shrink = if inline { MULTIPLEXED_SHRINK } else { 1.0 };
    let (across, down) = (f64::from(area.0.max(1)), f64::from(area.1.max(1)));
    let fit = (f64::from(MOST_PIXELS.0) / shrink / across)
        .min(f64::from(MOST_PIXELS.1) / shrink / down)
        .min(1.0);
    let side = |pixels: f64| ((pixels * fit * scale).round() as u32).max(2) & !1;
    (side(across), side(down))
}

/// The screen for video on `caps`: whole kitty frames from image id
/// `first_image` on, through a local stash when the terminal reads one,
/// and sixel or quarter blocks elsewhere.
#[must_use]
pub fn screen(caps: &Capabilities, cell: CellPixels, first_image: u32) -> PixelScreen {
    let mut presenter = PatchedKitty::with_tiles(first_image, cell, TILE_SIDE, 0);
    if let Some(stash) = local_stash(caps) {
        presenter = presenter.with_stash(stash);
    }
    PixelScreen::new(caps, cell, presenter)
}

/// How many delivered frames we judge at once.
const SAMPLES: usize = 30;
/// How long a scale that proved too slow stays out of reach, so cheap
/// frames in a quiet moment do not grow us straight back into it.
const REMEMBER_TOO_SLOW: Duration = Duration::from_secs(30);
/// The share of the frame time we aim to spend delivering a frame.
const AIM: f64 = 0.6;
/// Above this share the terminal falls behind and we shrink.
const TOO_SLOW: f64 = 0.85;
/// Below this share there is room to grow.
const ROOM: f64 = 0.3;
/// The finest step between scales, so small swings do not resize the picture.
const STEP: f64 = 0.05;

/// Picks the render scale whose frames reach the terminal within the
/// frame budget, from how long recent frames took. A frame's cost grows
/// with the square of the scale.
#[derive(Debug, Clone)]
pub struct ScaleGovernor {
    frame_time: Duration,
    scale: f64,
    min: f64,
    max: f64,
    samples: Vec<Duration>,
    too_slow: Option<(f64, Instant)>,
}

impl ScaleGovernor {
    /// A governor for `frame_time` that starts at `max` and stays within
    /// `min..=max`.
    #[must_use]
    pub fn new(frame_time: Duration, min: f64, max: f64) -> Self {
        Self {
            frame_time,
            scale: max,
            min,
            max,
            samples: Vec::with_capacity(SAMPLES),
            too_slow: None,
        }
    }

    /// The scale to render at now.
    #[must_use]
    pub fn scale(&self) -> f64 {
        self.scale
    }

    /// The largest scale we may grow to at `now`.
    fn ceiling(&self, now: Instant) -> f64 {
        match self.too_slow {
            Some((scale, at)) if now.duration_since(at) < REMEMBER_TOO_SLOW => scale - STEP,
            _ => self.max,
        }
    }

    /// Record that one frame took `took` to reach the terminal, at `now`;
    /// the new scale when the frames so far call for one.
    pub fn delivered(&mut self, took: Duration, now: Instant) -> Option<f64> {
        self.samples.push(took);
        if self.samples.len() < SAMPLES {
            return None;
        }
        self.samples.sort_unstable();
        let median = self.samples.get(SAMPLES / 2).copied().unwrap_or_default();
        self.samples.clear();
        let share = median.as_secs_f64() / self.frame_time.as_secs_f64();
        if share <= TOO_SLOW && (share >= ROOM || self.scale >= self.max) {
            return None;
        }
        let wanted = self.scale * (AIM / share.max(0.01)).sqrt();
        let mut next = ((wanted / STEP).round() * STEP).clamp(self.min, self.max);
        if share > TOO_SLOW {
            self.too_slow = Some((self.scale, now));
        } else {
            next = next.min(self.ceiling(now)).max(self.scale);
        }
        if (next - self.scale).abs() < STEP / 2.0 {
            return None;
        }
        self.scale = next;
        Some(next)
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use katui_proto::probe::{Capabilities, CellPixels, Identity};
    use katui_scene::placement::CellRect;

    use super::{REMEMBER_TOO_SLOW, SAMPLES, ScaleGovernor, fit, frame_size};

    const FRAME: Duration = Duration::from_micros(16_667);

    fn feed(governor: &mut ScaleGovernor, took: Duration, now: Instant) -> Option<f64> {
        (0..SAMPLES)
            .map(|_| governor.delivered(took, now))
            .last()
            .flatten()
    }

    fn cell() -> CellPixels {
        CellPixels {
            width: 8,
            height: 16,
        }
    }

    #[test]
    fn a_wide_area_fits_the_box_by_height_and_centres_it() {
        let area = CellRect {
            x: 0,
            y: 2,
            columns: 200,
            rows: 40,
        };
        let video = fit(area, cell(), (16, 9));
        assert_eq!((video.columns, video.rows), (142, 40));
        assert_eq!((video.x, video.y), (29, 2));
    }

    #[test]
    fn a_tall_area_fits_the_box_by_width() {
        let video = fit(CellRect::full(80, 60), cell(), (16, 9));
        assert_eq!((video.columns, video.rows), (80, 22));
        assert_eq!((video.x, video.y), (0, 19));
    }

    #[test]
    fn frames_shrink_to_what_the_terminal_takes_and_keep_even_sides() {
        let caps = Capabilities::default();
        assert_eq!(frame_size((800, 400), &caps, 1.0), (800, 400));
        assert_eq!(frame_size((1920, 1080), &caps, 1.0), (960, 540));
        assert_eq!(frame_size((1920, 1080), &caps, 0.5), (480, 270));
        let (width, height) = frame_size((301, 99), &caps, 1.0);
        assert_eq!((width % 2, height % 2), (0, 0));
    }

    #[test]
    fn frames_crossing_a_multiplexer_inline_shrink_again() {
        let mut caps = Capabilities {
            identity: Some(Identity {
                name: "tmux".to_owned(),
                version: None,
            }),
            ..Capabilities::default()
        };
        assert_eq!(frame_size((1920, 1080), &caps, 1.0), (480, 270));
        caps.local_files = true;
        assert_eq!(
            frame_size((1920, 1080), &caps, 1.0),
            (960, 540),
            "a local stash skips the multiplexer"
        );
    }

    #[test]
    fn a_slow_terminal_shrinks_the_scale_to_fit_the_budget() {
        let now = Instant::now();
        let mut governor = ScaleGovernor::new(FRAME, 0.25, 1.0);
        let shrunk = feed(&mut governor, FRAME * 4, now).expect("four frame times is too slow");
        assert!((0.35..=0.45).contains(&shrunk), "{shrunk}");
        assert_eq!(
            feed(&mut governor, FRAME / 2, now),
            None,
            "half the budget fits"
        );
    }

    #[test]
    fn a_scale_that_was_too_slow_stays_out_of_reach_for_a_while() {
        let now = Instant::now();
        let mut governor = ScaleGovernor::new(FRAME, 0.25, 1.0);
        let shrunk = feed(&mut governor, FRAME * 2, now).expect("two frame times is too slow");
        let held = feed(&mut governor, FRAME / 20, now).expect("plenty of room");
        assert!(
            held < 1.0 && held > shrunk,
            "grows, but not into 1.0: {held}"
        );
        let later = now + REMEMBER_TOO_SLOW;
        assert_eq!(feed(&mut governor, FRAME / 20, later), Some(1.0));
    }

    #[test]
    fn the_scale_stays_within_its_range() {
        let now = Instant::now();
        let mut governor = ScaleGovernor::new(FRAME, 0.25, 0.5);
        feed(&mut governor, FRAME * 8, now);
        assert_eq!(governor.scale(), 0.25, "never below the minimum");
        let later = now + REMEMBER_TOO_SLOW;
        assert_eq!(feed(&mut governor, FRAME / 20, later), Some(0.5));
        assert_eq!(feed(&mut governor, FRAME / 20, later), None);
    }
}
