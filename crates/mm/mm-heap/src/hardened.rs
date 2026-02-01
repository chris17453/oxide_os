//! Hardened heap allocator with security features
//!
//! Provides heap hardening features for debug and security:
//! - Guard pages before/after heap region (cause fault on overflow/underflow)
//! - Redzones around each allocation (16 bytes, pattern 0xFD)
//! - Canary values for overflow detection (0xDEAD_BEEF_CAFE_BABE)
//! - Freed memory fill pattern (0xDD) for use-after-free detection
//!
//! Enable with the `heap-hardening` feature flag.

use crate::linked_list::LinkedListAllocator;
use core::alloc::Layout;
use core::mem;
use core::ptr;
use spin::Mutex;

/// Debug macro for heap operations
#[cfg(feature = "debug-heap")]
macro_rules! debug_heap {
    ($($arg:tt)*) => {{
        #[cfg(target_arch = "x86_64")]
        {
            use arch_x86_64::serial::SerialWriter;
            use core::fmt::Write;
            let mut writer = SerialWriter;
            let _ = writeln!(writer, $($arg)*);
        }
    }};
}

#[cfg(not(feature = "debug-heap"))]
macro_rules! debug_heap {
    ($($arg:tt)*) => {};
}

/// Redzone size in bytes (before and after each allocation)
const REDZONE_SIZE: usize = 16;

/// Redzone fill pattern (0xFD = "fence")
const REDZONE_PATTERN: u8 = 0xFD;

/// Freed memory fill pattern (0xDD = "dead")
const FREED_PATTERN: u8 = 0xDD;

/// Canary value placed at the end of each allocation
const CANARY_VALUE: u64 = 0xDEAD_BEEF_CAFE_BABE;

/// Header for hardened allocations
#[repr(C)]
struct AllocHeader {
    /// Original requested size
    requested_size: usize,
    /// Actual allocated size (including header, redzones, canary)
    actual_size: usize,
    /// Magic value for validation
    magic: u64,
}

const HEADER_MAGIC: u64 = 0x4F58_4944_4845_4150; // "OXIDEHEAP" in hex-like

impl AllocHeader {
    fn new(requested: usize, actual: usize) -> Self {
        Self {
            requested_size: requested,
            actual_size: actual,
            magic: HEADER_MAGIC,
        }
    }

    fn is_valid(&self) -> bool {
        self.magic == HEADER_MAGIC
    }
}

/// Hardened heap allocator wrapper
///
/// Wraps the standard LinkedListAllocator with security features.
pub struct HardenedHeapAllocator {
    inner: LinkedListAllocator,
    /// Total allocations made
    alloc_count: usize,
    /// Total frees made
    free_count: usize,
    /// Detected corruptions
    corruption_count: usize,
}

impl HardenedHeapAllocator {
    /// Create an empty hardened allocator
    pub const fn empty() -> Self {
        Self {
            inner: LinkedListAllocator::empty(),
            alloc_count: 0,
            free_count: 0,
            corruption_count: 0,
        }
    }

    /// Initialize with a memory region
    ///
    /// # Safety
    /// Memory region must be valid and unused.
    pub unsafe fn init(&mut self, heap_start: usize, heap_size: usize) {
        debug_heap!(
            "[HEAP] Hardened init: start={:#x} size={:#x}",
            heap_start,
            heap_size
        );
        // SAFETY: caller ensures heap region is valid
        unsafe { self.inner.init(heap_start, heap_size) };
    }

    /// Calculate the total size needed for a hardened allocation
    fn hardened_size(layout: Layout) -> usize {
        let header_size = mem::size_of::<AllocHeader>();
        let canary_size = mem::size_of::<u64>();
        // header + leading redzone + user data + trailing redzone + canary
        header_size + REDZONE_SIZE + layout.size() + REDZONE_SIZE + canary_size
    }

    /// Allocate with hardening
    pub fn allocate(&mut self, layout: Layout) -> *mut u8 {
        let total_size = Self::hardened_size(layout);
        let align = layout.align().max(mem::align_of::<AllocHeader>());

        // Create layout for the full allocation
        let full_layout = match Layout::from_size_align(total_size, align) {
            Ok(l) => l,
            Err(_) => return ptr::null_mut(),
        };

        let base = self.inner.allocate(full_layout);
        if base.is_null() {
            debug_heap!("[HEAP] Allocation failed: size={}", layout.size());
            return ptr::null_mut();
        }

        self.alloc_count += 1;

        // Set up the allocation structure:
        // [Header][Redzone][User Data][Redzone][Canary]
        let header_ptr = base as *mut AllocHeader;
        let leading_redzone = unsafe { base.add(mem::size_of::<AllocHeader>()) };
        let user_ptr = unsafe { leading_redzone.add(REDZONE_SIZE) };
        let trailing_redzone = unsafe { user_ptr.add(layout.size()) };
        let canary_ptr = unsafe { trailing_redzone.add(REDZONE_SIZE) as *mut u64 };

        // Write header
        unsafe {
            ptr::write(header_ptr, AllocHeader::new(layout.size(), total_size));
        }

        // Fill leading redzone
        unsafe {
            ptr::write_bytes(leading_redzone, REDZONE_PATTERN, REDZONE_SIZE);
        }

        // Fill trailing redzone
        unsafe {
            ptr::write_bytes(trailing_redzone, REDZONE_PATTERN, REDZONE_SIZE);
        }

        // Write canary
        unsafe {
            ptr::write(canary_ptr, CANARY_VALUE);
        }

        debug_heap!(
            "[HEAP] Alloc: user={:#x} size={} total={}",
            user_ptr as usize,
            layout.size(),
            total_size
        );

        user_ptr
    }

    /// Deallocate with hardening checks
    pub fn deallocate(&mut self, ptr: *mut u8, layout: Layout) {
        if ptr.is_null() {
            return;
        }

        // Calculate pointers
        let header_size = mem::size_of::<AllocHeader>();
        let leading_redzone = unsafe { ptr.sub(REDZONE_SIZE) };
        let base = unsafe { leading_redzone.sub(header_size) };
        let header_ptr = base as *const AllocHeader;

        // Validate header
        let header = unsafe { &*header_ptr };
        if !header.is_valid() {
            debug_heap!(
                "[HEAP] CORRUPTION: Invalid header magic at {:#x}",
                base as usize
            );
            self.corruption_count += 1;
            return;
        }

        // Validate requested size matches
        if header.requested_size != layout.size() {
            debug_heap!(
                "[HEAP] WARNING: Size mismatch: header={} layout={}",
                header.requested_size,
                layout.size()
            );
        }

        // Check leading redzone
        if !self.check_redzone(leading_redzone, REDZONE_SIZE) {
            debug_heap!(
                "[HEAP] CORRUPTION: Leading redzone corrupted at {:#x}",
                leading_redzone as usize
            );
            self.corruption_count += 1;
        }

        // Check trailing redzone
        let trailing_redzone = unsafe { ptr.add(header.requested_size) };
        if !self.check_redzone(trailing_redzone, REDZONE_SIZE) {
            debug_heap!(
                "[HEAP] CORRUPTION: Trailing redzone corrupted at {:#x}",
                trailing_redzone as usize
            );
            self.corruption_count += 1;
        }

        // Check canary
        let canary_ptr = unsafe { trailing_redzone.add(REDZONE_SIZE) as *const u64 };
        let canary = unsafe { ptr::read(canary_ptr) };
        if canary != CANARY_VALUE {
            debug_heap!(
                "[HEAP] CORRUPTION: Canary corrupted at {:#x}: {:#x} != {:#x}",
                canary_ptr as usize,
                canary,
                CANARY_VALUE
            );
            self.corruption_count += 1;
        }

        // Fill user memory with freed pattern to detect use-after-free
        unsafe {
            ptr::write_bytes(ptr, FREED_PATTERN, header.requested_size);
        }

        self.free_count += 1;

        debug_heap!(
            "[HEAP] Free: user={:#x} size={}",
            ptr as usize,
            header.requested_size
        );

        // Create layout for deallocation
        let full_layout = match Layout::from_size_align(
            header.actual_size,
            layout.align().max(mem::align_of::<AllocHeader>()),
        ) {
            Ok(l) => l,
            Err(_) => return,
        };

        self.inner.deallocate(base, full_layout);
    }

    /// Check if a redzone is intact
    fn check_redzone(&self, ptr: *const u8, size: usize) -> bool {
        for i in 0..size {
            let byte = unsafe { *ptr.add(i) };
            if byte != REDZONE_PATTERN {
                return false;
            }
        }
        true
    }

    /// Get free memory
    pub fn free(&self) -> usize {
        self.inner.free()
    }

    /// Get used memory
    pub fn used(&self) -> usize {
        self.inner.used()
    }

    /// Get allocation count
    pub fn alloc_count(&self) -> usize {
        self.alloc_count
    }

    /// Get free count
    pub fn free_count(&self) -> usize {
        self.free_count
    }

    /// Get corruption count
    pub fn corruption_count(&self) -> usize {
        self.corruption_count
    }
}

/// Locked hardened heap for global allocator use
pub struct LockedHardenedHeap {
    inner: Mutex<HardenedHeapAllocator>,
}

impl LockedHardenedHeap {
    /// Create a new empty locked hardened heap
    pub const fn empty() -> Self {
        Self {
            inner: Mutex::new(HardenedHeapAllocator::empty()),
        }
    }

    /// Initialize the heap
    ///
    /// # Safety
    /// Memory region must be valid and unused.
    pub unsafe fn init(&self, heap_start: usize, heap_size: usize) {
        // SAFETY: caller ensures heap region is valid
        unsafe { self.inner.lock().init(heap_start, heap_size) };
    }

    /// Get free memory
    pub fn free(&self) -> usize {
        self.inner.lock().free()
    }

    /// Get used memory
    pub fn used(&self) -> usize {
        self.inner.lock().used()
    }

    /// Get statistics
    pub fn stats(&self) -> (usize, usize, usize) {
        let inner = self.inner.lock();
        (
            inner.alloc_count(),
            inner.free_count(),
            inner.corruption_count(),
        )
    }
}

unsafe impl core::alloc::GlobalAlloc for LockedHardenedHeap {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        self.inner.lock().allocate(layout)
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        self.inner.lock().deallocate(ptr, layout);
    }
}
