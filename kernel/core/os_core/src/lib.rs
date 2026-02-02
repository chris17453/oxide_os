//! OXIDE Core - Fundamental types and utilities
//!
//! This crate provides core types used throughout the OXIDE kernel.
//! All types are `#![no_std]` compatible.

#![no_std]

pub mod addr;
pub mod sync;

pub use addr::{PhysAddr, VirtAddr};
pub use sync::{Mutex, MutexGuard};
