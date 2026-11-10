//! The katui video demo: plays a file or stream URL through ffmpeg, its
//! sound through ffplay, and a beeping test pattern when given nothing.
//! Space pauses, the arrows seek, m mutes. Run with `just demo-video`.
#![cfg(unix)] // the runner drives /dev/tty through katui-rt's unix layer
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]

use std::error::Error;
use std::io::{self, Read};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{Receiver, TryRecvError, sync_channel};
use std::thread;
use std::time::{Duration, Instant};

use katui_game::Key;
use katui_present::surface::{PatchedKitty, PixelScreen};
use katui_proto::input::InputEvent;
use katui_proto::probe::{Capabilities, CellPixels};
use katui_rt::app::{App, AppEvent, Control, run_app};
use katui_rt::events::Waker;
use katui_scene::CellSize;
use katui_scene::surface::Surface;

const FRAMES_PER_SECOND: u32 = 30;
const SEEK_SECONDS: f32 = 5.0;
const MOST_PIXELS: (u32, u32) = (960, 540);
/// A multiplexer carries graphics far more slowly than a terminal does.
const MULTIPLEXED_SHRINK: u32 = 2;
const BASE_IMAGE: u32 = 900;
const TILE_PIXELS: u32 = 64;
const OVERLAY_SECONDS: f32 = 2.0;
const TEST_SECONDS: f32 = 60.0;

fn main() -> Result<(), Box<dyn Error>> {
    let clip = match std::env::args().nth(1) {
        Some(path) => Clip::File(path),
        None => Clip::Test,
    };
    let duration = clip.duration()?;
    let caps = katui_rt::tty::probe_default();
    let mut player = Player::new(clip, duration, caps.clone());
    run_app(&mut player, io::stdout(), &caps, FRAMES_PER_SECOND)?;
    Ok(())
}

enum Clip {
    File(String),
    Test,
}

impl Clip {
    fn duration(&self) -> Result<Option<f32>, Box<dyn Error>> {
        let Self::File(path) = self else {
            return Ok(Some(TEST_SECONDS));
        };
        let probe = Command::new("ffprobe")
            .args([
                "-v",
                "error",
                "-show_entries",
                "format=duration",
                "-of",
                "csv=p=0",
            ])
            .arg(path)
            .stdin(Stdio::null())
            .output()
            .map_err(|error| format!("run ffprobe from ffmpeg: {error}"))?;
        if !probe.status.success() {
            return Err(format!(
                "open {path}: {}",
                String::from_utf8_lossy(&probe.stderr).trim()
            )
            .into());
        }
        Ok(String::from_utf8_lossy(&probe.stdout).trim().parse().ok())
    }

    /// We trim the test sources inside their graphs because lavfi inputs do not seek.
    fn video_input(&self, at: f32) -> Vec<String> {
        match self {
            Self::File(path) => strings(&["-ss", &at.to_string(), "-i", path]),
            Self::Test => strings(&[
                "-f",
                "lavfi",
                "-i",
                &format!(
                    "testsrc2=size=1280x720:rate=30:duration={TEST_SECONDS},trim=start={at},setpts=PTS-STARTPTS"
                ),
            ]),
        }
    }

    fn audio_input(&self, at: f32) -> Vec<String> {
        match self {
            Self::File(path) => strings(&["-ss", &at.to_string(), path]),
            Self::Test => strings(&[
                "-f",
                "lavfi",
                &format!(
                    "sine=frequency=440:beep_factor=4:duration={TEST_SECONDS},atrim=start={at},asetpts=PTS-STARTPTS"
                ),
            ]),
        }
    }
}

fn strings(parts: &[&str]) -> Vec<String> {
    parts.iter().map(ToString::to_string).collect()
}

/// The ffmpeg filter that fits the clip inside `width` x `height` square
/// pixels, letterboxed, at our frame rate.
fn fit_filter(width: u32, height: u32) -> String {
    format!(
        "fps={FRAMES_PER_SECOND},scale={width}:{height}:force_original_aspect_ratio=decrease,pad={width}:{height}:-1:-1:black"
    )
}

/// The pixels across and down each cell gets: the most that stay within
/// what the terminal takes thirty times a second, in whole numbers so
/// every tile a multiplexer gets covers whole cells.
fn density(columns: u16, rows: u16, cell: CellPixels, multiplexed: bool) -> (u32, u32) {
    let shrink = if multiplexed { MULTIPLEXED_SHRINK } else { 1 };
    let (cell_width, cell_height) = (u32::from(cell.width).max(1), u32::from(cell.height).max(1));
    let down_for = |across: u32| ((across * cell_height + cell_width / 2) / cell_width).max(1);
    let fits = |across: &u32| {
        u32::from(columns) * across <= MOST_PIXELS.0 / shrink
            && u32::from(rows) * down_for(*across) <= MOST_PIXELS.1 / shrink
    };
    let across = (1..=cell_width).rev().find(fits).unwrap_or(1);
    (across, down_for(across))
}

/// A tile side that spans whole cells both ways and is at least `TILE_PIXELS`.
fn tile_side((across, down): (u32, u32)) -> u32 {
    let mut both = across.max(down);
    while both % across != 0 || both % down != 0 {
        both += across.max(down);
    }
    both * TILE_PIXELS.div_ceil(both)
}

struct Killed(Child);

impl Drop for Killed {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

struct Decoder {
    frames: Receiver<Vec<u8>>,
    _process: Killed,
}

impl Decoder {
    fn spawn(clip: &Clip, at: f32, (width, height): (u32, u32)) -> io::Result<Self> {
        let mut child = Command::new("ffmpeg")
            .args(["-hide_banner", "-loglevel", "error", "-nostdin"])
            .args(clip.video_input(at))
            .args(["-an", "-vf", &fit_filter(width, height)])
            .args(["-f", "rawvideo", "-pix_fmt", "rgba", "-"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let mut stdout = child
            .stdout
            .take()
            .ok_or_else(|| io::Error::other("ffmpeg gave no output pipe"))?;
        let (send, frames) = sync_channel(FRAMES_PER_SECOND as usize / 4);
        let frame_bytes = width as usize * height as usize * 4;
        thread::spawn(move || {
            loop {
                let mut frame = vec![0; frame_bytes];
                if stdout.read_exact(&mut frame).is_err() || send.send(frame).is_err() {
                    break;
                }
            }
        });
        Ok(Self {
            frames,
            _process: Killed(child),
        })
    }
}

fn spawn_audio(clip: &Clip, at: f32) -> io::Result<Killed> {
    Command::new("ffplay")
        .args(["-nodisp", "-autoexit", "-loglevel", "quiet"])
        .args(clip.audio_input(at))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(Killed)
}

struct Player {
    clip: Clip,
    duration: Option<f32>,
    caps: Capabilities,
    screen: PixelScreen,
    fitted: Option<CellSize>,
    size: (u32, u32),
    decoder: Option<Decoder>,
    audio: Option<Killed>,
    muted: bool,
    started: Instant,
    offset: f32,
    paused: Option<f32>,
    shown: u32,
    picture: Vec<u8>,
    overlay_until: Instant,
}

impl Player {
    fn new(clip: Clip, duration: Option<f32>, caps: Capabilities) -> Self {
        Self {
            clip,
            duration,
            caps,
            screen: PixelScreen::default(),
            fitted: None,
            size: (2, 2),
            decoder: None,
            audio: None,
            muted: false,
            started: Instant::now(),
            offset: 0.0,
            paused: None,
            shown: 0,
            picture: vec![0; 16],
            overlay_until: Instant::now(),
        }
    }

    fn position(&self) -> f32 {
        self.paused
            .unwrap_or_else(|| self.offset + self.started.elapsed().as_secs_f32())
    }

    fn play_from(&mut self, at: f32) -> io::Result<()> {
        let at = self.duration.map_or(at, |end| at.min(end)).max(0.0);
        self.decoder = None;
        self.audio = None;
        self.decoder = Some(Decoder::spawn(&self.clip, at, self.size)?);
        self.audio = if self.muted {
            None
        } else {
            Some(spawn_audio(&self.clip, at)?)
        };
        self.started = Instant::now();
        self.offset = at;
        self.paused = None;
        self.shown = 0;
        Ok(())
    }

    fn pause_at(&mut self, at: f32) {
        self.decoder = None;
        self.audio = None;
        self.paused = Some(at);
    }

    fn press(&mut self, key: Key) -> io::Result<()> {
        self.overlay_until = Instant::now() + Duration::from_secs_f32(OVERLAY_SECONDS);
        let at = self.position();
        match (key, self.paused) {
            (Key::Char(' '), None) => self.pause_at(at),
            (Key::Char(' '), Some(_)) => self.play_from(at)?,
            (Key::Left, None) => self.play_from(at - SEEK_SECONDS)?,
            (Key::Right, None) => self.play_from(at + SEEK_SECONDS)?,
            (Key::Left, Some(_)) => self.paused = Some((at - SEEK_SECONDS).max(0.0)),
            (Key::Right, Some(_)) => {
                self.paused = Some(
                    self.duration
                        .map_or(at + SEEK_SECONDS, |end| (at + SEEK_SECONDS).min(end)),
                );
            }
            (Key::Char('m'), None) => {
                self.muted = !self.muted;
                self.audio = if self.muted {
                    None
                } else {
                    Some(spawn_audio(&self.clip, at)?)
                };
            }
            (Key::Char('m'), Some(_)) => self.muted = !self.muted,
            _ => {}
        }
        Ok(())
    }

    fn fit(&mut self, out: &mut Vec<u8>, size: CellSize) -> io::Result<()> {
        if self.fitted == Some(size) {
            return Ok(());
        }
        let cell = katui_rt::tty::cell_size(&self.caps);
        let rows = PixelScreen::picture(&self.caps, cell).rows_shown(size.rows);
        let (across, down) = density(size.columns, rows, cell, self.caps.multiplexed());
        self.screen.free(out)?;
        self.screen = PixelScreen::new(
            &self.caps,
            cell,
            PatchedKitty::with_tiles(BASE_IMAGE, cell, tile_side((across, down)), 0),
        );
        self.size = (u32::from(size.columns) * across, u32::from(rows) * down);
        self.picture = vec![0; self.size.0 as usize * self.size.1 as usize * 4];
        self.fitted = Some(size);
        let at = self.position();
        match self.paused {
            Some(_) => Ok(()),
            None => self.play_from(at),
        }
    }

    /// We take every frame the clock has passed and keep the newest, so a
    /// slow terminal drops frames and the picture stays in step with the sound.
    fn catch_up(&mut self) {
        let Some(decoder) = &self.decoder else {
            return;
        };
        let due = (self.started.elapsed().as_secs_f32() * FRAMES_PER_SECOND as f32) as u32 + 1;
        let mut ended = false;
        while self.shown < due {
            match decoder.frames.try_recv() {
                Ok(frame) => {
                    self.picture = frame;
                    self.shown += 1;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    ended = true;
                    break;
                }
            }
        }
        if ended {
            let end = self.offset + self.shown as f32 / FRAMES_PER_SECOND as f32;
            self.pause_at(end);
            self.overlay_until = Instant::now() + Duration::from_secs(3600);
        }
    }
}

/// The progress bar along the bottom and, when paused, two bars in the middle.
fn draw_overlay(
    pixels: &mut [u8],
    (width, height): (u32, u32),
    progress: Option<f32>,
    paused: bool,
) {
    let bar = (height / 40).max(2);
    let mut fill = |left: u32, top: u32, right: u32, bottom: u32, color: [u8; 4]| {
        for y in top..bottom.min(height) {
            for x in left..right.min(width) {
                let at = (y * width + x) as usize * 4;
                if let Some(pixel) = pixels.get_mut(at..at + 4) {
                    pixel.copy_from_slice(&color);
                }
            }
        }
    };
    if let Some(progress) = progress {
        let done = (width as f32 * progress.clamp(0.0, 1.0)) as u32;
        fill(0, height - bar, width, height, [40, 40, 48, 255]);
        fill(0, height - bar, done, height, [236, 72, 96, 255]);
    }
    if paused {
        let (side, middle_x, middle_y) = (height / 6, width / 2, height / 2);
        let (top, bottom, thick) = (middle_y - side / 2, middle_y + side / 2, side / 3);
        fill(
            middle_x.saturating_sub(thick * 3 / 2),
            top,
            middle_x.saturating_sub(thick / 2),
            bottom,
            [255; 4],
        );
        fill(
            middle_x + thick / 2,
            top,
            middle_x + thick * 3 / 2,
            bottom,
            [255; 4],
        );
    }
}

impl App for Player {
    type Message = ();

    fn start(&mut self, _waker: Waker<()>) {}

    fn update(&mut self, event: AppEvent<()>) -> Control {
        match event {
            AppEvent::Input(input) if input.is_quit_key() => return Control::Quit,
            AppEvent::Input(InputEvent::Key(key, _)) => {
                if self.press(key).is_err() {
                    return Control::Quit;
                }
            }
            AppEvent::Repaint => self.screen.repaint(),
            _ => {}
        }
        Control::Animate
    }

    fn draw(&mut self, out: &mut Vec<u8>, size: CellSize) -> io::Result<Control> {
        self.fit(out, size)?;
        self.catch_up();
        let mut pixels = self.picture.clone();
        if self.paused.is_some() || Instant::now() < self.overlay_until {
            let progress = self.duration.map(|end| self.position() / end.max(0.01));
            draw_overlay(&mut pixels, self.size, progress, self.paused.is_some());
        }
        let surface = Surface::from_rgba(self.size.0, self.size.1, pixels)
            .unwrap_or_else(|| Surface::new(self.size.0, self.size.1));
        let rows = self.screen.rows_shown(size.rows);
        self.screen
            .render_frame(out, &surface, size.columns, rows)?;
        Ok(Control::Animate)
    }
}

#[cfg(test)]
mod tests {
    use katui_proto::probe::CellPixels;

    use super::{Clip, density, draw_overlay, fit_filter, tile_side};

    const CELL: CellPixels = CellPixels {
        width: 8,
        height: 17,
    };

    #[test]
    fn cells_get_whole_pixels_within_the_budget_and_fewer_under_a_multiplexer() {
        assert_eq!(density(120, 30, CELL, false), (8, 17));
        assert_eq!(density(120, 40, CELL, false), (6, 13));
        assert_eq!(density(120, 40, CELL, true), (3, 6));
        assert_eq!(density(2000, 900, CELL, false), (1, 2));
    }

    #[test]
    fn tiles_span_whole_cells_both_ways() {
        for density in [(8, 17), (6, 13), (3, 6), (1, 2)] {
            let side = tile_side(density);
            assert!(side >= 64 && side % density.0 == 0 && side % density.1 == 0);
        }
    }

    #[test]
    fn the_filter_letterboxes_to_the_frame_at_the_frame_rate() {
        assert_eq!(
            fit_filter(640, 360),
            "fps=30,scale=640:360:force_original_aspect_ratio=decrease,pad=640:360:-1:-1:black"
        );
    }

    #[test]
    fn a_file_seeks_before_its_input_and_the_test_pattern_trims_its_graph() {
        let file = Clip::File("clip.mp4".into());
        assert_eq!(file.video_input(5.0), ["-ss", "5", "-i", "clip.mp4"]);
        assert_eq!(file.audio_input(5.0), ["-ss", "5", "clip.mp4"]);
        assert!(Clip::Test.video_input(5.0)[3].contains("trim=start=5,"));
        assert!(Clip::Test.audio_input(5.0)[2].contains("atrim=start=5,"));
    }

    #[test]
    fn the_overlay_draws_progress_and_the_pause_sign() {
        let (width, height) = (160, 90);
        let mut pixels = vec![0; width * height * 4];
        draw_overlay(&mut pixels, (width as u32, height as u32), Some(0.5), true);
        let at = |x: usize, y: usize| &pixels[(y * width + x) * 4..(y * width + x) * 4 + 3];
        assert_eq!(at(10, height - 1), [236, 72, 96], "played part");
        assert_eq!(at(150, height - 1), [40, 40, 48], "part still to play");
        assert_eq!(at(width / 2 - 4, height / 2), [255, 255, 255], "pause sign");
        assert_eq!(at(width / 2, height / 2), [0, 0, 0], "gap between its bars");
        image::save_buffer(
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../target/katui-video-overlay.png"
            ),
            &pixels,
            width as u32,
            height as u32,
            image::ColorType::Rgba8,
        )
        .expect("the png writer saves under target");
    }
}
