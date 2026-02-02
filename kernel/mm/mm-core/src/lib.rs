//! OXIDE Memory Management Core
//!
//! This crate provides the core memory management infrastructure:
//! - Buddy allocator for physical frame allocation
//! - Memory zones (DMA, Normal, High)
//! - Allocation statistics tracking

#![no_std]

pub mod buddy;
pub mod stats;
pub mod zone;

pub use buddy::BuddyAllocator;
pub use stats::MemoryStats;
pub use zone::{AllocFlags, AllocRequest, MemoryZone, ZoneType, pages_to_order};

/// Size of a physical frame (4KB)
pub const FRAME_SIZE: usize = 4096;
/// Log2 of frame size
pub const FRAME_SHIFT: usize = 12;

/// Maximum buddy order (4MB blocks = 2^10 pages)
pub const MAX_ORDER: usize = 10;

/// Result type for memory operations
pub type MmResult<T> = Result<T, MmError>;

/// Memory management errors
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MmError {
    /// Out of memory
    OutOfMemory,
    /// Invalid address
    InvalidAddress,
    /// Invalid order (too large)
    InvalidOrder,
    /// Zone not found
    ZoneNotFound,
    /// Address not aligned
    NotAligned,
    /// Double free detected
    DoubleFree,
}
