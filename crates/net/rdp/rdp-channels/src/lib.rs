//! RDP Virtual Channel Implementations
//!
//! This crate provides virtual channel implementations for RDP:
//! - cliprdr: Clipboard redirection channel

#![no_std]

extern crate alloc;

pub mod cliprdr;

pub use cliprdr::ClipboardChannel;
