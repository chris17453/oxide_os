//! Compression library for OXIDE OS
//!
//! Provides DEFLATE/INFLATE and TAR format support for userspace utilities.

#![no_std]

extern crate alloc;
use alloc::vec::Vec;

pub mod deflate;
pub mod tar;

/// Compression error types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompressionError {
    /// Invalid input data
    InvalidData,
    /// Buffer too small
    BufferTooSmall,
    /// Unsupported compression format
    UnsupportedFormat,
    /// Checksum mismatch
    ChecksumMismatch,
    /// Not implemented yet
    NotImplemented,
}

pub type Result<T> = core::result::Result<T, CompressionError>;

/// Compression level (0-9, where 0 = no compression, 9 = best compression)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompressionLevel(u8);

impl CompressionLevel {
    pub const NONE: Self = Self(0);
    pub const FAST: Self = Self(1);
    pub const DEFAULT: Self = Self(6);
    pub const BEST: Self = Self(9);

    pub fn new(level: u8) -> Self {
        Self(level.min(9))
    }

    pub fn value(&self) -> u8 {
        self.0
    }
}

impl Default for CompressionLevel {
    fn default() -> Self {
        Self::DEFAULT
    }
}
