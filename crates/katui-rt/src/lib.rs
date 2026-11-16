//! The katui runtime: the one impure layer. It owns the controlling
//! terminal, the raw-mode lifecycle, the frame clock, the event pump and
//! the interactive loop. Everything pure lives in the crates below.

pub mod clock;
#[cfg(feature = "gamepad")]
pub mod gamepad;
pub mod info;
pub mod media;
pub mod session;
pub mod stash;

pub mod app;
pub mod events;
pub mod headless;
pub mod pacing;
pub mod pane;
mod stop;
pub mod tty;

/// Failures of the terminal lifecycle: the crate's one error enum.
#[derive(Debug, thiserror::Error)]
pub enum RtError {
    /// An open, read or write on the terminal failed: no controlling
    /// terminal exists, or the stream broke mid-exchange.
    #[error("terminal io failed: {0}")]
    Io(#[from] std::io::Error),
    /// A termios or poll operation failed mid-exchange.
    #[cfg(unix)]
    #[error("terminal control failed: {0}")]
    Control(#[from] rustix::io::Errno),
}
