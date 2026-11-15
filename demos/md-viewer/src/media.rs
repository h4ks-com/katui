//! The pictures a page shows: still images, SVG drawings, animated GIF
//! and WebP files, and videos played through ffmpeg.

use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use image::codecs::gif::GifDecoder;
use image::codecs::webp::WebPDecoder;
use image::{AnimationDecoder, Frames, RgbaImage};
use katui::rt::media::VideoStream;
use katui::scene::surface::Surface;
use resvg::{tiny_skia, usvg};

/// The width we draw an SVG at, so it stays sharp when the page scales it.
const SVG_WIDTH: f32 = 512.0;
const VIDEO_SIZE: (u32, u32) = (800, 450);
const VIDEO_FRAMES_PER_SECOND: u32 = 24;
const SHORTEST_FRAME: Duration = Duration::from_millis(20);

pub(crate) enum Media {
    Still(Arc<Surface>),
    Animation(Animation),
    Video(Video),
}

impl Media {
    pub(crate) fn open(path: &Path) -> Option<Self> {
        let extension = path.extension()?.to_string_lossy().to_lowercase();
        match extension.as_str() {
            "svg" => svg(path).map(Self::Still),
            "gif" | "webp" => Animation::open(path).map_or_else(
                || still(path).map(Self::Still),
                |animation| Some(Self::Animation(animation)),
            ),
            "mp4" | "webm" | "mov" | "mkv" | "m4v" => Video::open(path).map(Self::Video),
            _ => still(path).map(Self::Still),
        }
    }

    pub(crate) fn current(&self) -> Option<Arc<Surface>> {
        match self {
            Self::Still(surface) => Some(surface.clone()),
            Self::Animation(animation) => Some(animation.current.clone()),
            Self::Video(video) => video.current.clone(),
        }
    }

    /// Move to the frame due at `now`; `true` when the picture changed.
    pub(crate) fn advance(&mut self, now: Instant) -> bool {
        match self {
            Self::Still(_) => false,
            Self::Animation(animation) => animation.advance(now),
            Self::Video(video) => video.advance(),
        }
    }
}

fn still(path: &Path) -> Option<Arc<Surface>> {
    surface(image::open(path).ok()?.to_rgba8())
}

fn surface(rgba: RgbaImage) -> Option<Arc<Surface>> {
    Surface::from_rgba(rgba.width(), rgba.height(), rgba.into_raw()).map(Arc::new)
}

fn svg(path: &Path) -> Option<Arc<Surface>> {
    let data = std::fs::read(path).ok()?;
    let tree = usvg::Tree::from_data(&data, &usvg::Options::default()).ok()?;
    let size = tree.size();
    let scale = SVG_WIDTH / size.width();
    let mut pixmap = tiny_skia::Pixmap::new(
        whole_pixels(size.width() * scale),
        whole_pixels(size.height() * scale),
    )?;
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    let rgba = pixmap
        .pixels()
        .iter()
        .flat_map(|pixel| {
            let color = pixel.demultiply();
            [color.red(), color.green(), color.blue(), color.alpha()]
        })
        .collect();
    Surface::from_rgba(pixmap.width(), pixmap.height(), rgba).map(Arc::new)
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // clamped to a drawable size first
fn whole_pixels(length: f32) -> u32 {
    length.ceil().clamp(1.0, 8192.0) as u32
}

/// An animated GIF or WebP, decoded one frame at a time so a long one
/// never sits in memory whole.
pub(crate) struct Animation {
    path: PathBuf,
    frames: Frames<'static>,
    current: Arc<Surface>,
    due: Instant,
}

impl Animation {
    /// The animation in `path`, or `None` for a picture of one frame.
    fn open(path: &Path) -> Option<Self> {
        let mut frames = frames(path)?;
        let (first, delay) = decode(frames.next()?)?;
        frames.next()?.ok()?;
        let mut frames = self::frames(path)?;
        frames.next();
        Some(Self {
            path: path.to_path_buf(),
            frames,
            current: first,
            due: Instant::now() + delay,
        })
    }

    fn advance(&mut self, now: Instant) -> bool {
        if now < self.due {
            return false;
        }
        let next = self.frames.next().or_else(|| {
            self.frames = frames(&self.path)?;
            self.frames.next()
        });
        let Some((surface, delay)) = next.and_then(decode) else {
            self.due = now + Duration::from_secs(1);
            return false;
        };
        self.current = surface;
        self.due = now + delay;
        true
    }
}

fn frames(path: &Path) -> Option<Frames<'static>> {
    let reader = BufReader::new(File::open(path).ok()?);
    let is_gif = path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("gif"));
    if is_gif {
        return Some(GifDecoder::new(reader).ok()?.into_frames());
    }
    let decoder = WebPDecoder::new(reader).ok()?;
    decoder.has_animation().then(|| decoder.into_frames())
}

fn decode(frame: image::ImageResult<image::Frame>) -> Option<(Arc<Surface>, Duration)> {
    let frame = frame.ok()?;
    let (numerator, denominator) = frame.delay().numer_denom_ms();
    let delay = Duration::from_millis(u64::from(numerator / denominator.max(1)));
    Some((surface(frame.into_buffer())?, delay.max(SHORTEST_FRAME)))
}

/// A video decoded by ffmpeg, looping, without sound.
pub(crate) struct Video {
    stream: VideoStream,
    current: Option<Arc<Surface>>,
}

impl Video {
    fn open(path: &Path) -> Option<Self> {
        let input = [
            "-stream_loop".as_ref(),
            "-1".as_ref(),
            "-i".as_ref(),
            path.as_os_str(),
        ];
        let stream = VideoStream::ffmpeg(&input, VIDEO_SIZE, VIDEO_FRAMES_PER_SECOND).ok()?;
        Some(Self {
            stream,
            current: None,
        })
    }

    fn advance(&mut self) -> bool {
        let Some(rgba) = self.stream.latest() else {
            return false;
        };
        self.current = Surface::from_rgba(VIDEO_SIZE.0, VIDEO_SIZE.1, rgba).map(Arc::new);
        true
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::time::{Duration, Instant};

    use super::Media;

    fn asset(path: &str) -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join(path)
    }

    #[test]
    fn an_svg_draws_sharp_at_its_own_aspect() {
        let logo = Media::open(&asset("assets/logo.svg")).expect("the logo draws");
        let surface = logo.current().expect("a still has pixels");
        assert_eq!((surface.width(), surface.height()), (512, 512));
        assert!(matches!(logo, Media::Still(_)));
    }

    #[test]
    fn an_animated_webp_moves_on_when_its_frame_is_due() {
        let mut demo = Media::open(&asset("assets/demos/html.webp")).expect("the demo decodes");
        assert!(matches!(demo, Media::Animation(_)));
        let first = demo.current().expect("the first frame");
        assert!(!demo.advance(Instant::now()), "the first frame still shows");
        assert!(demo.advance(Instant::now() + Duration::from_secs(5)));
        assert!(!std::sync::Arc::ptr_eq(
            &first,
            &demo.current().expect("a frame")
        ));
    }
}

#[cfg(test)]
mod video_tests {
    use std::process::Command;
    use std::time::{Duration, Instant};

    use super::Media;

    #[test]
    #[ignore = "needs ffmpeg"]
    fn a_video_plays_its_frames() {
        let clip = std::env::temp_dir().join("md-viewer-clip.mp4");
        let made = Command::new("ffmpeg")
            .args([
                "-y",
                "-loglevel",
                "error",
                "-f",
                "lavfi",
                "-i",
                "testsrc=size=320x180:rate=24",
            ])
            .args(["-t", "2"])
            .arg(&clip)
            .status()
            .expect("ffmpeg runs");
        assert!(made.success());
        let mut video = Media::open(&clip).expect("ffmpeg starts");
        let deadline = Instant::now() + Duration::from_secs(5);
        while !video.advance(Instant::now()) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        let frame = video.current().expect("a frame arrives");
        assert_eq!((frame.width(), frame.height()), (800, 450));
    }
}
