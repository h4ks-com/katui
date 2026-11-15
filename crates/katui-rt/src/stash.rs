//! Pixels through files a terminal on this machine reads (`t=t`), so a
//! frame skips compression, base64 and the terminal connection.

use std::io;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use katui_present::surface::Stash;
use katui_proto::kitty::{Delete, Format, ImageId, Medium, Quiet, Transmit};
use katui_proto::probe::{Capabilities, Transport};

/// The image id the local file probe uploads under.
const PROBE_IMAGE: u32 = 29;
/// How long the probe waits for the terminal to read the file.
const PROBE_WAIT: Duration = Duration::from_millis(300);

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

impl Stash for TempFileStash {
    fn put(&mut self, pixels: &[u8]) -> io::Result<Medium> {
        let path = self.path();
        std::fs::write(&path, pixels)?;
        Ok(Medium::TempFile(path.to_string_lossy().into_owned()))
    }
}

/// The stash for a terminal that reads local files, when `caps` says it
/// does.
#[must_use]
pub fn local_stash(caps: &Capabilities) -> Option<Box<dyn Stash>> {
    caps.local_files
        .then(|| Box::new(TempFileStash::new()) as Box<dyn Stash>)
}

/// Whether the terminal behind `transport` reads pixels from a local file:
/// we send it one pixel through a file and wait for its OK. Only a kitty
/// terminal outside a multiplexer and outside SSH is worth asking.
pub(crate) fn reads_local_files(transport: &mut dyn Transport, caps: &Capabilities) -> bool {
    let remote =
        std::env::var_os("SSH_CONNECTION").is_some() || std::env::var_os("SSH_TTY").is_some();
    if !caps.kitty || caps.multiplexed() || remote {
        return false;
    }
    let mut stash = TempFileStash::new();
    let Ok(medium) = stash.put(&[0, 0, 0]) else {
        return false;
    };
    let answered = ask(transport, &medium);
    if let Medium::TempFile(path) = medium {
        // the terminal deletes the file when it reads it; we clean up
        // after one that never did
        let _ = std::fs::remove_file(path);
    }
    answered
}

fn ask(transport: &mut dyn Transport, medium: &Medium) -> bool {
    let mut transmit = Transmit::new(ImageId(PROBE_IMAGE), Format::Rgb, 1, 1, &[]);
    transmit.quiet = Quiet::Normal;
    transmit.medium = medium.clone();
    let Ok(frames) = transmit.encode() else {
        return false;
    };
    let mut query: Vec<u8> = frames.concat();
    query.extend_from_slice(&Delete::Free(ImageId(PROBE_IMAGE)).encode());
    if transport.write(&query).is_err() {
        return false;
    }
    let ok = format!("i={PROBE_IMAGE};OK");
    let until = Instant::now() + PROBE_WAIT;
    let mut replies = Vec::new();
    while Instant::now() < until {
        let Some(chunk) = transport.read_timeout(until.saturating_duration_since(Instant::now()))
        else {
            break;
        };
        replies.extend_from_slice(&chunk);
        if String::from_utf8_lossy(&replies).contains(&ok) {
            return true;
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
    use katui_proto::probe::{Capabilities, Transport};

    use super::{TempFileStash, reads_local_files};

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

    #[test]
    fn a_stashed_frame_lands_in_a_file_the_terminal_may_read() {
        let mut stash = TempFileStash::new();
        let Medium::TempFile(path) = stash.put(&[1, 2, 3, 4]).expect("the temp dir is writable")
        else {
            panic!("a temp file stash hands out temp files");
        };
        assert!(path.contains("tty-graphics-protocol"));
        assert_eq!(std::fs::read(&path).expect("the file exists"), [1, 2, 3, 4]);
        std::fs::remove_file(path).expect("the file is ours to remove");
    }

    #[test]
    fn a_terminal_that_answers_ok_reads_local_files() {
        let mut terminal = Scripted {
            replies: VecDeque::from([b"\x1b_Gi=29;OK\x1b\\".to_vec()]),
            sent: Vec::new(),
        };
        assert!(reads_local_files(&mut terminal, &kitty()));
        assert!(String::from_utf8_lossy(&terminal.sent).contains("t=t"));
        let mut silent = Scripted {
            replies: VecDeque::new(),
            sent: Vec::new(),
        };
        assert!(!reads_local_files(&mut silent, &kitty()));
        assert!(!reads_local_files(&mut silent, &Capabilities::default()));
    }
}
