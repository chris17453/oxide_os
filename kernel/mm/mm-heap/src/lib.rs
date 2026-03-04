//! OXIDE Kernel Heap Allocator
//!
//! Provides heap allocation for the kernel with optional hardening features.
//!
//! # Features
//!
//! - `heap-hardening` - Enable security hardening (redzones, canaries, freed memory fill)
//! - `debug-heap` - Enable serial debug output for heap operations
//!
//! # Usage
//!
//! For production, use `LockedHeap`:
//! ```ignore
//! static HEAP: LockedHeap = LockedHeap::empty();
//! ```
//!
//! For debugging, use `LockedHardenedHeap`:
//! ```ignore
//! static HEAP: LockedHardenedHeap = LockedHardenedHeap::empty();
//! ```

#![no_std]

mod linked_list;

#[cfg(feature = "heap-hardening")]
mod hardened;

pub use linked_list::LinkedListAllocator;

#[cfg(feature = "heap-hardening")]
pub use hardened::{HardenedHeapAllocator, LockedHardenedHeap};

use core::alloc::{GlobalAlloc, Layout};
use os_core::sync::KernelMutex;

/// Global kernel heap allocator (standard version)
///
/// — GraveShift: KernelMutex wraps every alloc/dealloc with preempt_disable/enable.
/// The scheduler can't yank us mid-allocation anymore. Build 67 sends its regards.
pub struct LockedHeap {
    inner: KernelMutex<LinkedListAllocator>,
}

impl LockedHeap {
    /// Create a new empty locked heap
    pub const fn empty() -> Self {
        Self {
            inner: KernelMutex::new(LinkedListAllocator::empty()),
        }
    }

    /// Initialize the heap with a memory region
    ///
    /// # Safety
    /// The caller must ensure that the given memory region is valid, unused,
    /// and not used for anything else.
    pub unsafe fn init(&self, heap_start: usize, heap_size: usize) {
        // SAFETY: caller ensures heap region is valid
        unsafe {
            self.inner.lock().init(heap_start, heap_size);
        }
    }

    /// Get the amount of free memory in the heap
    pub fn free(&self) -> usize {
        self.inner.lock().free()
    }

    /// Get the amount of used memory in the heap
    pub fn used(&self) -> usize {
        self.inner.lock().used()
    }
}

unsafe impl GlobalAlloc for LockedHeap {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        self.inner.lock().allocate(layout)
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        self.inner.lock().deallocate(ptr, layout);
    }
}

/// Type alias for the appropriate heap type based on features
#[cfg(feature = "heap-hardening")]
pub type KernelHeap = LockedHardenedHeap;

#[cfg(not(feature = "heap-hardening"))]
pub type KernelHeap = LockedHeap;

/// Create a new kernel heap (selects based on features)
#[cfg(feature = "heap-hardening")]
pub const fn new_kernel_heap() -> KernelHeap {
    LockedHardenedHeap::empty()
}

#[cfg(not(feature = "heap-hardening"))]
pub const fn new_kernel_heap() -> KernelHeap {
    LockedHeap::empty()
}
