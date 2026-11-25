//! Opening a link or a file in the program this machine picks for it.

use std::io;
use std::process::{Command, Stdio};

/// The program this machine hands links to.
const OPENER: &str = if cfg!(target_os = "macos") {
    "open"
} else if cfg!(windows) {
    "explorer"
} else {
    "xdg-open"
};

/// Open `target`, a URL or a path, in the program this machine picks for
/// it. We start that program and leave it running; it never touches the
/// terminal.
pub fn open_link(target: &str) -> io::Result<()> {
    Command::new(OPENER)
        .arg(target)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(drop)
}
