//! OXIDE Kernel Heap Allocator
//!
//! A simple linked-list allocator for the kernel heap.

#![no_std]

mod linked_list;

pub use linked_list::LinkedListAllocator;

use core::alloc::{GlobalAlloc, Layout};
use spin::Mutex;

/// Global kernel heap allocator
pub struct LockedHeap {
    inner: Mutex<LinkedListAllocator>,
}

impl LockedHeap {
    /// Create a new empty locked heap
    pub const fn empty() -> Self {
        Self {
            inner: Mutex::new(LinkedListAllocator::empty()),
        }
    }

    /// Initialize the heap with a memory region
    ///
    /// # Safety
    /// The caller must ensure that the given memory region is valid, unused,
    /// and not used for anything else.
    pub unsafe fn init(&self, heap_start: usize, heap_size: usize) {
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
