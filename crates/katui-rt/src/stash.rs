//! Pixels through shared memory (`t=s`) or files (`t=t`) a terminal on
//! this machine reads, so a frame skips compression, base64 and the
//! terminal connection.

#[cfg(unix)]
mod shm;

use std::io;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use katui_present::surface::Stash;
use katui_proto::kitty::{Delete, Format, ImageId, Medium, Quiet, Transmit, wrap_for_passthrough};
use katui_proto::probe::{Capabilities, Transport};

#[cfg(unix)]
pub use shm::SharedMemoryStash;

/// The image id the local probes upload under.
const PROBE_IMAGE: u32 = 29;
/// How long a probe waits for the terminal to answer.
const PROBE_WAIT: Duration = Duration::from_millis(300);
/// Below this many bytes an image travels inline: a small tile costs less
/// as a few compressed kilobytes than as an object or a file the terminal
/// opens, reads and deletes.
const INLINE_BELOW: usize = 16 * 1024;

/// Whether `pixels` are small enough to send inline.
fn inline(pixels: &[u8]) -> bool {
    pixels.len() < INLINE_BELOW
}

/// Files in the temporary directory, which the terminal deletes once it
/// has read them.
#[derive(Debug)]
pub struct TempFileStash {
    directory: PathBuf,
    prefix: String,
}

/// Numbers every stashed file in this process, so two stashes never write
/// the same name.
static NEXT_FILE: AtomicU64 = AtomicU64::new(0);

impl TempFileStash {
    /// A stash in the system's temporary directory, its files named for
    /// this process.
    #[must_use]
    pub fn new() -> Self {
        Self {
            directory: std::env::temp_dir(),
            // a terminal only reads files with this marker in their name
            prefix: format!("tty-graphics-protocol-katui-{}-", std::process::id()),
        }
    }

    fn path(&self) -> PathBuf {
        let number = NEXT_FILE.fetch_add(1, Ordering::Relaxed);
        self.directory.join(format!("{}{number}", self.prefix))
    }
}

impl Default for TempFileStash {
    fn default() -> Self {
        Self::new()
    }
}

impl TempFileStash {
    /// Write `pixels` to a fresh file, whatever their size.
    fn store(&mut self, pixels: &[u8]) -> io::Result<Medium> {
        let path = self.path();
        std::fs::write(&path, pixels)?;
        Ok(Medium::TempFile(path.to_string_lossy().into_owned()))
    }
}

impl Stash for TempFileStash {
    fn put(&mut self, pixels: &[u8]) -> io::Result<Medium> {
        if inline(pixels) {
            return Ok(Medium::Direct);
        }
        self.store(pixels)
    }
}

/// The stash for a terminal on this machine: shared memory when `caps`
/// says the terminal reads it, files when it reads those.
#[must_use]
pub fn local_stash(caps: &Capabilities) -> Option<Box<dyn Stash>> {
    #[cfg(unix)]
    if caps.shared_memory {
        return Some(Box::new(SharedMemoryStash::new()));
    }
    caps.local_files
        .then(|| Box::new(TempFileStash::new()) as Box<dyn Stash>)
}

/// Ask the terminal behind `transport` which local medium it reads,
/// shared memory first, by sending one pixel through each and waiting for
/// its OK. A multiplexer on this machine forwards the name like any other
/// graphics command, so only a bare SSH session goes unasked.
pub(crate) fn probe_local(transport: &mut dyn Transport, caps: &mut Capabilities) {
    let remote =
        std::env::var_os("SSH_CONNECTION").is_some() || std::env::var_os("SSH_TTY").is_some();
    if !caps.kitty || (remote && !caps.multiplexed()) {
        return;
    }
    caps.shared_memory = reads_shared_memory(transport, caps);
    if !caps.shared_memory {
        caps.local_files = reads_local_files(transport, caps);
    }
}

#[cfg(unix)]
fn reads_shared_memory(transport: &mut dyn Transport, caps: &Capabilities) -> bool {
    let mut stash = SharedMemoryStash::new();
    stash
        .store(&[0, 0, 0])
        .is_ok_and(|medium| ask(transport, caps, &medium))
}

#[cfg(not(unix))]
fn reads_shared_memory(_transport: &mut dyn Transport, _caps: &Capabilities) -> bool {
    false
}

fn reads_local_files(transport: &mut dyn Transport, caps: &Capabilities) -> bool {
    let mut stash = TempFileStash::new();
    let Ok(medium) = stash.store(&[0, 0, 0]) else {
        return false;
    };
    let answered = ask(transport, caps, &medium);
    if let Medium::TempFile(path) = medium {
        // the terminal deletes the file when it reads it; we clean up
        // after one that never did
        let _ = std::fs::remove_file(path);
    }
    answered
}

/// Whether the terminal answers OK to one pixel sent through `medium`. Any
/// answer ends the wait, since an error arrives as fast as an OK.
fn ask(transport: &mut dyn Transport, caps: &Capabilities, medium: &Medium) -> bool {
    let mut transmit = Transmit::new(ImageId(PROBE_IMAGE), Format::Rgb, 1, 1, &[]);
    transmit.quiet = Quiet::Normal;
    transmit.medium = medium.clone();
    let Ok(frames) = transmit.encode() else {
        return false;
    };
    let mut query: Vec<u8> = frames.concat();
    query.extend_from_slice(&Delete::Free(ImageId(PROBE_IMAGE)).encode());
    if caps.multiplexed() {
        query = wrap_for_passthrough(&query);
    }
    if transport.write(&query).is_err() {
        return false;
    }
    let answer = format!("i={PROBE_IMAGE};");
    let until = Instant::now() + PROBE_WAIT;
    let mut replies = Vec::new();
    while Instant::now() < until {
        let Some(chunk) = transport.read_timeout(until.saturating_duration_since(Instant::now()))
        else {
            break;
        };
        replies.extend_from_slice(&chunk);
        let text = String::from_utf8_lossy(&replies);
        let reply = text
            .find(&answer)
            .map(|at| &text[at + answer.len()..])
            .filter(|reply| reply.contains("\x1b\\"));
        if let Some(reply) = reply {
            return reply.starts_with("OK");
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::time::Duration;

    use katui_present::surface::Stash;
    use katui_proto::kitty::Medium;
    use katui_proto::probe::{Capabilities, Identity, Transport};

    use super::{INLINE_BELOW, TempFileStash, probe_local};

    struct Scripted {
        replies: VecDeque<Vec<u8>>,
        sent: Vec<u8>,
    }

    impl Transport for Scripted {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<()> {
            self.sent.extend_from_slice(bytes);
            Ok(())
        }

        fn read_timeout(&mut self, _wait: Duration) -> Option<Vec<u8>> {
            self.replies.pop_front()
        }
    }

    fn kitty() -> Capabilities {
        Capabilities {
            kitty: true,
            ..Capabilities::default()
        }
    }

    fn terminal(replies: &[&[u8]]) -> Scripted {
        Scripted {
            replies: replies.iter().map(|reply| reply.to_vec()).collect(),
            sent: Vec::new(),
        }
    }

    #[test]
    fn a_stashed_frame_lands_in_a_file_the_terminal_may_read() {
        let mut stash = TempFileStash::new();
        let pixels = vec![7; INLINE_BELOW];
        let Medium::TempFile(path) = stash.put(&pixels).expect("the temp dir is writable") else {
            panic!("a temp file stash hands out temp files");
        };
        assert!(path.contains("tty-graphics-protocol"));
        assert_eq!(std::fs::read(&path).expect("the file exists"), pixels);
        std::fs::remove_file(path).expect("the file is ours to remove");
    }

    #[test]
    fn a_small_image_travels_inline() {
        let mut stash = TempFileStash::new();
        assert_eq!(
            stash.put(&[1, 2, 3, 4]).expect("nothing to write"),
            Medium::Direct
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_terminal_that_reads_shared_memory_gets_it_first() {
        let mut caps = kitty();
        let mut answering = terminal(&[b"\x1b_Gi=29;OK\x1b\\"]);
        probe_local(&mut answering, &mut caps);
        assert!(caps.shared_memory);
        assert!(!caps.local_files);
        assert!(String::from_utf8_lossy(&answering.sent).contains("t=s"));
    }

    #[cfg(unix)]
    #[test]
    fn a_terminal_that_refuses_shared_memory_still_reads_files() {
        let mut caps = kitty();
        let mut answering = terminal(&[
            b"\x1b_Gi=29;EBADF:no shared memory\x1b\\",
            b"\x1b_Gi=29;OK\x1b\\",
        ]);
        probe_local(&mut answering, &mut caps);
        assert!(!caps.shared_memory);
        assert!(caps.local_files);
        assert!(String::from_utf8_lossy(&answering.sent).contains("t=t"));
    }

    #[test]
    fn a_silent_terminal_reads_nothing_local() {
        let mut caps = kitty();
        probe_local(&mut terminal(&[]), &mut caps);
        assert!(!caps.shared_memory && !caps.local_files);
        let mut plain = Capabilities::default();
        let mut answering = terminal(&[b"\x1b_Gi=29;OK\x1b\\"]);
        probe_local(&mut answering, &mut plain);
        assert!(answering.sent.is_empty(), "only a kitty terminal is asked");
    }

    #[test]
    fn under_tmux_the_probe_is_wrapped_for_passthrough() {
        let mut caps = Capabilities {
            identity: Some(Identity {
                name: "tmux".into(),
                version: None,
            }),
            ..kitty()
        };
        let mut answering = terminal(&[b"\x1b_Gi=29;OK\x1b\\"]);
        probe_local(&mut answering, &mut caps);
        assert!(caps.shared_memory || caps.local_files);
        assert!(answering.sent.starts_with(b"\x1bPtmux;"));
    }

    #[test]
    fn a_reply_split_across_reads_is_read_whole() {
        let mut caps = kitty();
        let mut answering = terminal(&[b"\x1b_Gi=29;O", b"K\x1b\\"]);
        probe_local(&mut answering, &mut caps);
        assert!(caps.shared_memory || caps.local_files);
    }
}
