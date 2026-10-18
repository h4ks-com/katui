//! The katui runtime: the one impure layer. It owns the controlling
//! terminal, the raw-mode lifecycle around a probe, and, later, the event
//! pump and frame clock. Everything pure lives in the crates below.

pub mod info;

#[cfg(unix)]
pub mod tty;
