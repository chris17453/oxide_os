//! Automount Daemon for EFFLUX OS
//!
//! Automatic mounting of external media with security policies.

#![no_std]

extern crate alloc;

pub mod daemon;
pub mod config;
pub mod mount;

pub use daemon::*;
pub use config::*;
pub use mount::*;
