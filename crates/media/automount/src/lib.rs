//! Automount Daemon for OXIDE OS
//!
//! Automatic mounting of external media with security policies.

#![no_std]

extern crate alloc;

pub mod config;
pub mod daemon;
pub mod mount;

pub use config::*;
pub use daemon::*;
pub use mount::*;
