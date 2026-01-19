//! EFFLUX Kernel Heap Allocator
//!
//! A simple linked-list allocator for the kernel heap.

#![no_std]

mod linked_list;

pub use linked_list::LinkedListAllocator;

use core::alloc::{GlobalAlloc, Layout};
use spin::Mutex;

// Serial port debug output
fn heap_lib_debug(s: &str) {
    const SERIAL: u16 = 0x3F8;
    for b in s.bytes() {
        unsafe {
            loop {
                let status: u8;
                core::arch::asm!("in al, dx", out("al") status, in("dx") SERIAL + 5, options(nomem, nostack));
                if status & 0x20 != 0 { break; }
            }
            core::arch::asm!("out dx, al", in("al") b, in("dx") SERIAL, options(nomem, nostack));
        }
    }
}

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

fn print_hex(val: usize) {
    const HEX: &[u8] = b"0123456789abcdef";
    const SERIAL: u16 = 0x3F8;
    heap_lib_debug("0x");
    for i in (0..16).rev() {
        let nibble = ((val >> (i * 4)) & 0xF) as usize;
        let c = HEX[nibble];
        unsafe {
            loop {
                let status: u8;
                core::arch::asm!("in al, dx", out("al") status, in("dx") SERIAL + 5, options(nomem, nostack));
                if status & 0x20 != 0 { break; }
            }
            core::arch::asm!("out dx, al", in("al") c, in("dx") SERIAL, options(nomem, nostack));
        }
    }
}

unsafe impl GlobalAlloc for LockedHeap {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        heap_lib_debug("[ALLOC] enter sz=");
        print_hex(layout.size());
        heap_lib_debug("\n");
        heap_lib_debug("[ALLOC] locking\n");
        let mut guard = self.inner.lock();
        heap_lib_debug("[ALLOC] locked\n");
        let result = guard.allocate(layout);
        heap_lib_debug("[ALLOC] ptr=");
        print_hex(result as usize);
        heap_lib_debug("\n");
        drop(guard);
        heap_lib_debug("[ALLOC] returning\n");
        result
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        self.inner.lock().deallocate(ptr, layout);
    }
}
