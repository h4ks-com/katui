//! Raw video from a child process: a decoder writes RGBA frames to its
//! standard output, a reader thread queues them, and the stream hands out
//! the newest frame its clock has reached.

use std::ffi::OsStr;
use std::io::{self, Read as _};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{Receiver, TryRecvError, sync_channel};
use std::thread;
use std::time::Instant;

/// A running decoder and the frames it produced. Dropping it kills the
/// decoder.
#[derive(Debug)]
pub struct VideoStream {
    frames: Receiver<Vec<u8>>,
    decoder: Child,
    frames_per_second: u32,
    started: Instant,
    shown: u32,
    ended: bool,
}

impl VideoStream {
    /// Run `command`, which must write `width` x `height` RGBA frames
    /// back to back on its standard output at `frames_per_second`.
    pub fn spawn(
        mut command: Command,
        (width, height): (u32, u32),
        frames_per_second: u32,
    ) -> io::Result<Self> {
        let mut decoder = command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let mut output = decoder
            .stdout
            .take()
            .ok_or_else(|| io::Error::other("the decoder gave no output pipe"))?;
        let (send, frames) = sync_channel(frames_per_second.max(4) as usize / 4);
        let frame_bytes = width as usize * height as usize * 4;
        thread::spawn(move || {
            loop {
                let mut frame = vec![0; frame_bytes];
                if output.read_exact(&mut frame).is_err() || send.send(frame).is_err() {
                    break;
                }
            }
        });
        Ok(Self {
            frames,
            decoder,
            frames_per_second: frames_per_second.max(1),
            started: Instant::now(),
            shown: 0,
            ended: false,
        })
    }

    /// Decode with ffmpeg. `input` holds ffmpeg's input arguments, such as
    /// `["-ss", "12", "-i", "clip.mp4"]`. We letterbox the picture to
    /// `size` and drop the sound.
    pub fn ffmpeg<S: AsRef<OsStr>>(
        input: &[S],
        size: (u32, u32),
        frames_per_second: u32,
    ) -> io::Result<Self> {
        let mut command = Command::new("ffmpeg");
        command
            .args(["-hide_banner", "-loglevel", "error", "-nostdin"])
            .args(input)
            .args(["-an", "-vf", &fit_filter(size, frames_per_second)])
            .args(["-f", "rawvideo", "-pix_fmt", "rgba", "-"]);
        Self::spawn(command, size, frames_per_second)
    }

    /// The newest frame the clock has passed since the last call, or
    /// `None` when no new frame is due or decoded yet. We skip the frames
    /// in between, so a slow terminal drops frames and the picture keeps
    /// pace with the clock.
    pub fn latest(&mut self) -> Option<Vec<u8>> {
        let passed = self.started.elapsed().as_millis() * u128::from(self.frames_per_second) / 1000;
        let due = u32::try_from(passed).unwrap_or(u32::MAX).saturating_add(1);
        let mut newest = None;
        while self.shown < due {
            match self.frames.try_recv() {
                Ok(frame) => {
                    newest = Some(frame);
                    self.shown += 1;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.ended = true;
                    break;
                }
            }
        }
        newest
    }

    /// How many frames the stream has handed out or skipped.
    #[must_use]
    pub fn shown(&self) -> u32 {
        self.shown
    }

    /// Whether the decoder finished and every frame went out.
    #[must_use]
    pub fn ended(&self) -> bool {
        self.ended
    }
}

impl Drop for VideoStream {
    fn drop(&mut self) {
        let _ = self.decoder.kill();
        let _ = self.decoder.wait();
    }
}

/// The ffmpeg filter that fits a clip inside `width` x `height` square
/// pixels, letterboxed, at `frames_per_second`.
fn fit_filter((width, height): (u32, u32), frames_per_second: u32) -> String {
    format!(
        "fps={frames_per_second},scale={width}:{height}:force_original_aspect_ratio=decrease,pad={width}:{height}:-1:-1:black"
    )
}

#[cfg(test)]
mod tests {
    use std::process::Command;
    use std::time::{Duration, Instant};

    use super::{VideoStream, fit_filter};

    #[test]
    fn the_filter_letterboxes_to_the_frame_at_the_frame_rate() {
        assert_eq!(
            fit_filter((320, 180), 30),
            "fps=30,scale=320:180:force_original_aspect_ratio=decrease,pad=320:180:-1:-1:black"
        );
    }

    #[test]
    fn a_stream_hands_out_the_newest_due_frame_then_ends() {
        let mut command = Command::new("sh");
        command.args(["-c", "printf 'aaaabbbbccccdddd'"]);
        let mut stream = VideoStream::spawn(command, (2, 1), 1000).expect("sh runs");
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut last = None;
        while !stream.ended() && Instant::now() < deadline {
            if let Some(frame) = stream.latest() {
                last = Some(frame);
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(stream.ended(), "the decoder exited");
        assert_eq!(stream.shown(), 2);
        assert_eq!(last.as_deref(), Some(&b"ccccdddd"[..]));
    }
}
