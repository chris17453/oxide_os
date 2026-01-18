//! EFFLUX Memory Management Traits
//!
//! Defines interfaces for memory allocators and page table operations.

#![no_std]

use efflux_core::PhysAddr;

/// Frame allocator trait
///
/// Uses `&self` to support implementations with interior mutability (locks).
pub trait FrameAllocator {
    /// Allocate a single physical frame
    fn alloc_frame(&self) -> Option<PhysAddr>;

    /// Free a physical frame
    fn free_frame(&self, addr: PhysAddr);

    /// Allocate contiguous frames
    fn alloc_frames(&self, count: usize) -> Option<PhysAddr>;

    /// Free contiguous frames
    fn free_frames(&self, addr: PhysAddr, count: usize);
}
