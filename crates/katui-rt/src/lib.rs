//! The katui runtime: the one impure layer. It owns the controlling
//! terminal, the raw-mode lifecycle around a probe, and, later, the event
//! pump and frame clock. Everything pure lives in the crates below.

pub mod clock;
pub mod info;
pub mod session;

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
