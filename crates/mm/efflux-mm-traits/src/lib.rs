//! EFFLUX Memory Management Traits
//!
//! Defines interfaces for memory allocators and page table operations.

#![no_std]

use efflux_core::PhysAddr;

/// Frame allocator trait
pub trait FrameAllocator {
    /// Allocate a single physical frame
    fn alloc_frame(&mut self) -> Option<PhysAddr>;

    /// Free a physical frame
    fn free_frame(&mut self, addr: PhysAddr);

    /// Allocate contiguous frames
    fn alloc_frames(&mut self, count: usize) -> Option<PhysAddr>;

    /// Free contiguous frames
    fn free_frames(&mut self, addr: PhysAddr, count: usize);
}
