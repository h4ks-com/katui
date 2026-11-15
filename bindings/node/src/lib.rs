//! The native module behind the `katui` npm package.
#![cfg(unix)]
// napi passes JavaScript values as raw handles, and reading one as a class
// instance is an unsafe call even after the type is checked
#![allow(unsafe_code)]
// napi hands every argument over as an owned value and takes `&Env`
// everywhere, so these two pedantic lints flag the shape napi requires
#![allow(clippy::needless_pass_by_value, clippy::trivially_copy_pass_by_ref)]
// other languages call these functions, where `#[must_use]` means nothing
#![allow(clippy::must_use_candidate)]

#[macro_use]
extern crate napi_derive;

pub mod app;
mod convert;
pub mod element;
pub mod event;
pub mod game;
pub mod html;
mod message;
mod reclaim;
pub mod state;
pub mod testing;
pub mod theme;
pub mod widgets;

/// The package version, the workspace version of the Rust crates.
#[napi]
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
