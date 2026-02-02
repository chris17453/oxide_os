//! OXIDE Core - Fundamental types and utilities
//!
//! This crate provides core types used throughout the OXIDE kernel.
//! All types are `#![no_std]` compatible.

#![no_std]

pub mod addr;
pub mod creds;
pub mod sync;
pub mod time;

pub use addr::{PhysAddr, VirtAddr};
pub use creds::{current_uid_gid, register_creds_provider};
pub use sync::{Mutex, MutexGuard};
pub use time::{register_wall_clock, wall_clock_secs};
