//! The katui runtime: the one impure layer. It owns the controlling
//! terminal, the raw-mode lifecycle, the frame clock, the event pump and
//! the interactive loop. Everything pure lives in the crates below.

pub mod clock;
pub mod info;
pub mod session;

#[cfg(unix)]
pub mod app;
#[cfg(unix)]
pub mod events;
#[cfg(unix)]
pub mod headless;
#[cfg(unix)]
pub mod pacing;
#[cfg(unix)]
pub mod pane;
#[cfg(unix)]
mod stop;
#[cfg(unix)]
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
