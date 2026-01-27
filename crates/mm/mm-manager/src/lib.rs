//! OXIDE Memory Manager
//!
//! Unified facade for all memory management operations:
//! - Physical frame allocation via buddy allocator
//! - Slab caches for kernel objects (when mm-slab is integrated)
//! - Memory accounting for resource limits
//! - Statistics and monitoring

#![no_std]

pub mod account;

// Re-export commonly used types from mm-core
pub use mm_core::{
    AllocFlags, AllocRequest, MemoryStats, MmError, MmResult, ZoneType, FRAME_SIZE, MAX_ORDER,
};

use account::AccountingContext;
use core::sync::atomic::{AtomicPtr, Ordering};
use mm_core::BuddyAllocator;
use mm_traits::FrameAllocator;
use os_core::PhysAddr;

/// Global memory manager instance
static GLOBAL_MM: AtomicPtr<MemoryManager> = AtomicPtr::new(core::ptr::null_mut());

/// Initialize the global memory manager
///
/// # Safety
/// Must be called once during boot with a reference to a static MemoryManager.
pub unsafe fn init_global(mm: &'static MemoryManager) {
    GLOBAL_MM.store(mm as *const _ as *mut _, Ordering::Release);
}

/// Get a reference to the global memory manager
///
/// # Panics
/// Panics if the memory manager hasn't been initialized.
pub fn mm() -> &'static MemoryManager {
    let ptr = GLOBAL_MM.load(Ordering::Acquire);
    if ptr.is_null() {
        panic!("Memory manager not initialized");
    }
    unsafe { &*ptr }
}

/// Try to get a reference to the global memory manager
///
/// Returns None if not initialized.
pub fn try_mm() -> Option<&'static MemoryManager> {
    let ptr = GLOBAL_MM.load(Ordering::Acquire);
    if ptr.is_null() {
        None
    } else {
        Some(unsafe { &*ptr })
    }
}

/// Unified memory manager providing access to all memory subsystems
pub struct MemoryManager {
    /// Buddy allocator for physical frames
    buddy: BuddyAllocator,
}

impl MemoryManager {
    /// Create a new uninitialized memory manager
    pub const fn new() -> Self {
        Self {
            buddy: BuddyAllocator::new(),
        }
    }

    /// Initialize the memory manager with memory regions
    ///
    /// # Safety
    /// Must be called once during boot with valid memory regions.
    /// Each region is (start_addr, length, is_usable).
    pub unsafe fn init(&mut self, regions: &[(PhysAddr, u64, bool)]) {
        // SAFETY: Caller ensures regions are valid
        unsafe { self.buddy.init(regions) };
    }

    /// Allocate physical frames
    ///
    /// # Arguments
    /// * `request` - Allocation request specifying order and zone preferences
    ///
    /// # Returns
    /// Physical address of allocated memory, or error
    pub fn alloc_frames(&self, request: &AllocRequest) -> MmResult<PhysAddr> {
        self.buddy.alloc(request)
    }

    /// Allocate a single physical frame
    pub fn alloc_frame(&self) -> MmResult<PhysAddr> {
        self.buddy.alloc(&AllocRequest::new(0))
    }

    /// Allocate contiguous physical frames
    ///
    /// # Arguments
    /// * `count` - Number of frames needed (will be rounded up to power of 2)
    pub fn alloc_contiguous(&self, count: usize) -> MmResult<PhysAddr> {
        if count == 0 {
            return Err(MmError::InvalidOrder);
        }
        let order = count.next_power_of_two().trailing_zeros() as usize;
        self.buddy.alloc(&AllocRequest::new(order))
    }

    /// Allocate frames from DMA zone (below 16MB)
    pub fn alloc_dma(&self, order: usize) -> MmResult<PhysAddr> {
        self.buddy.alloc(&AllocRequest::dma(order))
    }

    /// Free physical frames
    ///
    /// # Arguments
    /// * `addr` - Physical address of memory to free
    /// * `order` - Order of the allocation (0 = 4KB, 1 = 8KB, etc.)
    pub fn free_frames(&self, addr: PhysAddr, order: usize) -> MmResult<()> {
        self.buddy.free(addr, order)
    }

    /// Free a single physical frame
    pub fn free_frame(&self, addr: PhysAddr) -> MmResult<()> {
        self.buddy.free(addr, 0)
    }

    /// Allocate frames with accounting
    ///
    /// Checks resource limits before allocating and charges the account.
    ///
    /// # Arguments
    /// * `request` - Allocation request
    /// * `account` - Accounting context for resource tracking
    pub fn alloc_frames_accounted(
        &self,
        request: &AllocRequest,
        account: &dyn AccountingContext,
    ) -> MmResult<PhysAddr> {
        let bytes = (1u64 << request.order) * FRAME_SIZE as u64;

        // Check limit before allocating
        if !account.can_charge(bytes) {
            return Err(MmError::OutOfMemory);
        }

        // Allocate memory
        let addr = self.buddy.alloc(request)?;

        // Charge the account
        if let Err(e) = account.charge(bytes) {
            // Failed to charge, free the memory and return error
            let _ = self.buddy.free(addr, request.order);
            return Err(e);
        }

        Ok(addr)
    }

    /// Free frames with accounting
    ///
    /// Returns resources to the account.
    pub fn free_frames_accounted(
        &self,
        addr: PhysAddr,
        order: usize,
        account: &dyn AccountingContext,
    ) -> MmResult<()> {
        let bytes = (1u64 << order) * FRAME_SIZE as u64;

        // Free the memory
        self.buddy.free(addr, order)?;

        // Uncharge the account
        account.uncharge(bytes);

        Ok(())
    }

    /// Mark a region as used (for kernel, bootloader, etc.)
    pub fn mark_used(&self, start: PhysAddr, len: usize) {
        self.buddy.mark_used(start, len);
    }

    /// Get memory statistics
    pub fn stats(&self) -> &MemoryStats {
        self.buddy.stats()
    }

    /// Get total physical memory in bytes
    pub fn total_bytes(&self) -> u64 {
        self.buddy.total_bytes()
    }

    /// Get free physical memory in bytes
    pub fn free_bytes(&self) -> u64 {
        self.buddy.free_bytes()
    }

    /// Get used physical memory in bytes
    pub fn used_bytes(&self) -> u64 {
        self.total_bytes() - self.free_bytes()
    }

    /// Get free blocks at a specific order across all zones
    pub fn free_at_order(&self, order: usize) -> u64 {
        self.buddy.free_at_order(order)
    }

    /// Check if the memory manager is initialized
    pub fn is_initialized(&self) -> bool {
        self.buddy.is_initialized()
    }

    /// Get a reference to the buddy allocator
    ///
    /// For direct access when needed (e.g., for FrameAllocator trait).
    pub fn buddy(&self) -> &BuddyAllocator {
        &self.buddy
    }
}

impl Default for MemoryManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Implement FrameAllocator trait for compatibility with existing code
impl FrameAllocator for MemoryManager {
    fn alloc_frame(&self) -> Option<PhysAddr> {
        MemoryManager::alloc_frame(self).ok()
    }

    fn free_frame(&self, addr: PhysAddr) {
        let _ = MemoryManager::free_frame(self, addr);
    }

    fn alloc_frames(&self, count: usize) -> Option<PhysAddr> {
        self.alloc_contiguous(count).ok()
    }

    fn free_frames(&self, addr: PhysAddr, count: usize) {
        if count == 0 {
            return;
        }
        let order = count.next_power_of_two().trailing_zeros() as usize;
        let _ = MemoryManager::free_frames(self, addr, order);
    }
}
