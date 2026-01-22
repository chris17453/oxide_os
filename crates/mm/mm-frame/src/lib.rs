//! OXIDE Frame Allocator
//!
//! Physical frame allocation using a bitmap allocator.

#![no_std]

mod bitmap;

pub use bitmap::BitmapFrameAllocator;

use core::sync::atomic::{AtomicPtr, Ordering};
use os_core::PhysAddr;

/// Global frame allocator pointer
static GLOBAL_ALLOCATOR: AtomicPtr<BitmapFrameAllocator> = AtomicPtr::new(core::ptr::null_mut());

/// Initialize the global frame allocator reference
///
/// # Safety
/// Must be called once during kernel initialization with a reference
/// to a static allocator that will outlive all uses.
pub unsafe fn init_global_allocator(allocator: &'static BitmapFrameAllocator) {
    GLOBAL_ALLOCATOR.store(allocator as *const _ as *mut _, Ordering::Release);
}

/// Get a reference to the global frame allocator
///
/// Returns None if the allocator hasn't been initialized.
pub fn frame_allocator() -> &'static BitmapFrameAllocator {
    let ptr = GLOBAL_ALLOCATOR.load(Ordering::Acquire);
    if ptr.is_null() {
        panic!("Frame allocator not initialized");
    }
    unsafe { &*ptr }
}

/// Size of a physical frame (4KB)
pub const FRAME_SIZE: usize = 4096;

/// A physical memory frame
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PhysFrame {
    addr: PhysAddr,
}

impl PhysFrame {
    /// Create a frame from a physical address (must be frame-aligned)
    pub const fn from_addr(addr: PhysAddr) -> Self {
        Self { addr }
    }

    /// Create a frame containing the given address
    pub const fn containing(addr: PhysAddr) -> Self {
        Self {
            addr: addr.page_align_down(),
        }
    }

    /// Get the start address of this frame
    pub const fn start_addr(&self) -> PhysAddr {
        self.addr
    }

    /// Get the frame number (address / frame_size)
    pub const fn number(&self) -> usize {
        self.addr.as_usize() / FRAME_SIZE
    }

    /// Create a frame from a frame number
    pub const fn from_number(n: usize) -> Self {
        Self {
            addr: PhysAddr::new((n * FRAME_SIZE) as u64),
        }
    }
}

/// A memory region descriptor
#[derive(Debug, Clone, Copy)]
pub struct MemoryRegion {
    /// Start address of the region
    pub start: PhysAddr,
    /// Length in bytes
    pub len: u64,
    /// Whether this region is usable RAM
    pub usable: bool,
}

impl MemoryRegion {
    /// Create a new memory region
    pub const fn new(start: PhysAddr, len: u64, usable: bool) -> Self {
        Self { start, len, usable }
    }

    /// Get the end address (exclusive)
    pub const fn end(&self) -> PhysAddr {
        PhysAddr::new(self.start.as_u64() + self.len)
    }
}
