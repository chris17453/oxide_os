//! — ByteRiot: GlobalAlloc for System using oxide_rt's mmap-backed allocator.
use crate::alloc::{GlobalAlloc, Layout, System};

#[stable(feature = "alloc_system_type", since = "1.28.0")]
unsafe impl GlobalAlloc for System {
    #[inline]
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // Use oxide_rt's mmap-backed allocator
        unsafe { oxide_rt::alloc::mmap(0, layout.size().max(layout.align()), 0x3, 0x22, -1, 0) }
    }

    #[inline]
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { oxide_rt::alloc::munmap(ptr, layout.size()); }
    }
}
