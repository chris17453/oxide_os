//! Buddy allocator for physical frame allocation
//!
//! The buddy system provides O(1) allocation and deallocation with automatic
//! coalescing of adjacent free blocks. Memory is organized into power-of-2
//! sized blocks from 4KB (order 0) to 4MB (order 10).
//!
//! When allocating:
//! 1. Find a free block of the requested order
//! 2. If none available, split a larger block recursively
//! 3. Mark the block as allocated
//!
//! When freeing:
//! 1. Mark the block as free
//! 2. Check if the buddy block is also free
//! 3. If so, merge them and recursively check the next level

use crate::stats::MemoryStats;
use crate::zone::{AllocRequest, MemoryZone, ZoneType};
use crate::{MmError, MmResult, FRAME_SHIFT, FRAME_SIZE, MAX_ORDER};
use mm_traits::FrameAllocator;
use os_core::PhysAddr;
use spin::Mutex;

/// Physical memory map base for direct access (same as mm-paging)
const PHYS_MAP_BASE: u64 = 0xFFFF_8000_0000_0000;

/// Convert physical address to virtual address in direct map
#[inline]
fn phys_to_virt(phys: PhysAddr) -> *mut u64 {
    PHYS_MAP_BASE.wrapping_add(phys.as_u64()) as *mut u64
}

/// Free block header stored in the first 8 bytes of a free block
/// Contains the frame number of the next free block at this order
#[repr(C)]
struct FreeBlock {
    next: u64, // Frame number of next free block, or 0 if none
}

/// Buddy allocator managing multiple memory zones
pub struct BuddyAllocator {
    /// Memory zones (DMA, Normal, High)
    zones: [Mutex<MemoryZone>; 3],
    /// Global memory statistics
    stats: MemoryStats,
    /// Whether the allocator is initialized
    initialized: bool,
}

impl BuddyAllocator {
    /// Create a new uninitialized buddy allocator
    pub const fn new() -> Self {
        Self {
            zones: [
                Mutex::new(MemoryZone::new(ZoneType::Dma)),
                Mutex::new(MemoryZone::new(ZoneType::Normal)),
                Mutex::new(MemoryZone::new(ZoneType::High)),
            ],
            stats: MemoryStats::new(),
            initialized: false,
        }
    }

    /// Initialize the allocator with memory regions
    ///
    /// # Safety
    /// Must only be called once during boot with valid memory regions.
    pub unsafe fn init(&mut self, regions: &[(PhysAddr, u64, bool)]) {
        // Process each usable region
        for &(start, len, usable) in regions {
            if !usable || len == 0 {
                continue;
            }

            // Align start up and end down to page boundaries
            let start_aligned = start.page_align_up();
            let end = PhysAddr::new(start.as_u64() + len);
            let end_aligned = end.page_align_down();

            if end_aligned.as_u64() <= start_aligned.as_u64() {
                continue;
            }

            let size = end_aligned.as_u64() - start_aligned.as_u64();
            // SAFETY: We're in an unsafe fn, caller guarantees regions are valid
            unsafe { self.add_region(start_aligned, size) };
        }

        self.initialized = true;
    }

    /// Add a memory region to the appropriate zone(s)
    unsafe fn add_region(&mut self, start: PhysAddr, size: u64) {
        // A region might span multiple zones, so process it zone by zone
        let mut current = start.as_u64();
        let end = start.as_u64() + size;

        while current < end {
            let zone_type = ZoneType::for_address(PhysAddr::new(current));
            let zone_end = zone_type.end().min(end);
            let region_size = zone_end - current;

            if region_size >= FRAME_SIZE as u64 {
                // SAFETY: We're in an unsafe fn, caller guarantees memory is valid
                unsafe { self.add_to_zone(zone_type, PhysAddr::new(current), region_size) };
            }

            current = zone_end;
        }
    }

    /// Add a region to a specific zone
    unsafe fn add_to_zone(&mut self, zone_type: ZoneType, start: PhysAddr, size: u64) {
        let mut zone = self.zones[zone_type.index()].lock();

        // Initialize zone if empty
        if zone.is_empty() {
            zone.init(start, size);
        } else {
            // Extend zone size
            let new_end = (start.as_u64() + size).max(zone.base.as_u64() + zone.size);
            let new_base = start.as_u64().min(zone.base.as_u64());
            zone.base = PhysAddr::new(new_base);
            zone.size = new_end - new_base;
        }

        // Add free blocks to appropriate order lists
        let mut addr = start.as_u64();
        let mut remaining = size;

        while remaining >= FRAME_SIZE as u64 {
            // Find the largest order that fits and is properly aligned
            let frame_num = addr >> FRAME_SHIFT;
            let mut order = MAX_ORDER;

            // Find the largest order that:
            // 1. Has size <= remaining
            // 2. Has base address properly aligned (must be 2^order page aligned)
            while order > 0 {
                let block_size = (1u64 << order) * FRAME_SIZE as u64;
                let alignment = 1u64 << order;
                if block_size <= remaining && (frame_num & (alignment - 1)) == 0 {
                    break;
                }
                order -= 1;
            }

            let block_size = (1u64 << order) * FRAME_SIZE as u64;

            // Add block to free list
            // SAFETY: We're in an unsafe fn, memory is valid
            unsafe { self.add_free_block(&mut zone, order, addr) };

            // Update statistics
            let pages = 1u64 << order;
            zone.stats
                .free_pages[order]
                .fetch_add(1, core::sync::atomic::Ordering::Relaxed);
            self.stats
                .free_bytes
                .fetch_add(pages * FRAME_SIZE as u64, core::sync::atomic::Ordering::Relaxed);
            self.stats
                .total_bytes
                .fetch_add(pages * FRAME_SIZE as u64, core::sync::atomic::Ordering::Relaxed);

            addr += block_size;
            remaining -= block_size;
        }
    }

    /// Add a free block to a zone's free list at the given order
    ///
    /// # Safety
    /// The address must point to valid physical memory that is part of this zone.
    unsafe fn add_free_block(&self, zone: &mut MemoryZone, order: usize, addr: u64) {
        let virt = phys_to_virt(PhysAddr::new(addr));
        // SAFETY: Caller guarantees addr is valid physical memory
        let block = unsafe { &mut *(virt as *mut FreeBlock) };

        let old_head = zone.free_lists[order].head;
        let frame_num = addr >> FRAME_SHIFT;

        // Insert at head of free list
        block.next = old_head;
        zone.free_lists[order].head = frame_num; // Store frame number
        zone.free_lists[order].count += 1;
    }

    /// Remove and return a free block from a zone's free list
    ///
    /// # Safety
    /// The zone must have been properly initialized.
    unsafe fn pop_free_block(&self, zone: &mut MemoryZone, order: usize) -> Option<u64> {
        if zone.free_lists[order].count == 0 {
            return None;
        }

        let frame_num = zone.free_lists[order].head;
        let addr = frame_num << FRAME_SHIFT;

        let virt = phys_to_virt(PhysAddr::new(addr));
        // SAFETY: Frame was previously added to free list so memory is valid
        let block = unsafe { &*(virt as *const FreeBlock) };
        let next_frame = block.next;

        // Read next pointer and update head
        zone.free_lists[order].head = next_frame;
        zone.free_lists[order].count -= 1;

        Some(addr)
    }

    /// Allocate memory with the given request
    pub fn alloc(&self, request: &AllocRequest) -> MmResult<PhysAddr> {
        if request.order > MAX_ORDER {
            return Err(MmError::InvalidOrder);
        }

        // Try preferred zone first, then fall back to others
        let zone_order = match request.zone {
            Some(ZoneType::Dma) => [0, 1, 2], // DMA only, then Normal, then High
            Some(ZoneType::Normal) => [1, 2, 0], // Normal preferred
            Some(ZoneType::High) => [2, 1, 0], // High preferred
            None => [1, 2, 0],                 // Default: Normal, High, DMA
        };

        for zone_idx in zone_order {
            let mut zone = self.zones[zone_idx].lock();
            if zone.is_empty() {
                continue;
            }

            // SAFETY: Zone is initialized and locked
            if let Some(addr) = unsafe { self.alloc_from_zone(&mut zone, request.order) } {
                let pages = 1u64 << request.order;
                self.stats.record_alloc(pages * FRAME_SIZE as u64);
                return Ok(PhysAddr::new(addr));
            }
        }

        self.stats.record_failure();
        Err(MmError::OutOfMemory)
    }

    /// Allocate from a specific zone
    ///
    /// # Safety
    /// Zone must be properly initialized.
    unsafe fn alloc_from_zone(&self, zone: &mut MemoryZone, order: usize) -> Option<u64> {
        // Try to find a free block at the requested order or higher
        for current_order in order..=MAX_ORDER {
            if zone.free_lists[current_order].count > 0 {
                // Found a block, may need to split
                // SAFETY: Zone is initialized
                let addr = unsafe { self.pop_free_block(zone, current_order)? };

                // Split larger blocks down to requested size
                // When splitting order N to get order M (where N > M):
                // - We keep the low half at each level
                // - We add the high half (buddy) to the free list
                // The buddy at split level S is at: addr + (2^S * page_size)
                for split_order in (order..current_order).rev() {
                    let buddy_addr = addr + ((1u64 << split_order) << FRAME_SHIFT);
                    // SAFETY: Buddy address is valid as it comes from splitting a larger valid block
                    unsafe { self.add_free_block(zone, split_order, buddy_addr) };
                    zone.stats.free_pages[split_order]
                        .fetch_add(1, core::sync::atomic::Ordering::Relaxed);
                }

                zone.stats.free_pages[current_order]
                    .fetch_sub(1, core::sync::atomic::Ordering::Relaxed);

                return Some(addr);
            }
        }

        None
    }

    /// Free a block of memory
    pub fn free(&self, addr: PhysAddr, order: usize) -> MmResult<()> {
        if order > MAX_ORDER {
            return Err(MmError::InvalidOrder);
        }

        if !addr.is_aligned(FRAME_SIZE as u64) {
            return Err(MmError::NotAligned);
        }

        let zone_type = ZoneType::for_address(addr);
        let mut zone = self.zones[zone_type.index()].lock();

        if zone.is_empty() {
            return Err(MmError::ZoneNotFound);
        }

        // SAFETY: Caller guarantees addr was previously allocated
        unsafe {
            self.free_to_zone(&mut zone, addr.as_u64(), order);
        }

        let pages = 1u64 << order;
        self.stats.record_free(pages * FRAME_SIZE as u64);
        Ok(())
    }

    /// Free a block back to a zone with buddy coalescing
    ///
    /// # Safety
    /// Address must have been previously allocated from this zone.
    unsafe fn free_to_zone(&self, zone: &mut MemoryZone, addr: u64, order: usize) {
        let mut current_addr = addr;
        let mut current_order = order;

        // Try to coalesce with buddy blocks up the orders
        while current_order < MAX_ORDER {
            let frame_num = current_addr >> FRAME_SHIFT;
            let buddy_frame = frame_num ^ (1 << current_order);
            let buddy_addr = buddy_frame << FRAME_SHIFT;

            // Check if buddy is in the same zone
            if buddy_addr < zone.base.as_u64()
                || buddy_addr >= zone.base.as_u64() + zone.size
            {
                break;
            }

            // Try to find and remove buddy from free list
            // SAFETY: Zone is initialized
            if !unsafe { self.remove_from_free_list(zone, current_order, buddy_addr) } {
                break;
            }

            zone.stats.free_pages[current_order]
                .fetch_sub(1, core::sync::atomic::Ordering::Relaxed);

            // Merge: use the lower address as the combined block
            current_addr = current_addr.min(buddy_addr);
            current_order += 1;
        }

        // Add the (possibly merged) block to the free list
        // SAFETY: Address is valid as it was just freed
        unsafe { self.add_free_block(zone, current_order, current_addr) };
        zone.stats.free_pages[current_order]
            .fetch_add(1, core::sync::atomic::Ordering::Relaxed);
    }

    /// Try to remove a specific block from a free list
    ///
    /// # Safety
    /// Zone must be initialized and the address must be valid.
    unsafe fn remove_from_free_list(
        &self,
        zone: &mut MemoryZone,
        order: usize,
        addr: u64,
    ) -> bool {
        let target_frame = addr >> FRAME_SHIFT;

        if zone.free_lists[order].count == 0 {
            return false;
        }

        // Check if target is the head
        if zone.free_lists[order].head == target_frame {
            // SAFETY: Zone is initialized
            unsafe { self.pop_free_block(zone, order) };
            return true;
        }

        // Search through the list
        let mut current_frame = zone.free_lists[order].head;
        while current_frame != 0 {
            let current_addr = current_frame << FRAME_SHIFT;
            let virt = phys_to_virt(PhysAddr::new(current_addr));
            // SAFETY: Frame is in free list so memory is valid
            let block = unsafe { &mut *(virt as *mut FreeBlock) };

            if block.next == target_frame {
                // Found it - remove from list
                let target_virt = phys_to_virt(PhysAddr::new(addr));
                // SAFETY: Target frame is in free list so memory is valid
                let target_block = unsafe { &*(target_virt as *const FreeBlock) };
                block.next = target_block.next;
                zone.free_lists[order].count -= 1;
                return true;
            }

            current_frame = block.next;
        }

        false
    }

    /// Get memory statistics
    pub fn stats(&self) -> &MemoryStats {
        &self.stats
    }

    /// Get total free memory in bytes
    pub fn free_bytes(&self) -> u64 {
        self.stats.free()
    }

    /// Get total memory in bytes
    pub fn total_bytes(&self) -> u64 {
        self.stats.total()
    }

    /// Get free pages at a specific order across all zones
    pub fn free_at_order(&self, order: usize) -> u64 {
        if order > MAX_ORDER {
            return 0;
        }
        let mut total = 0u64;
        for zone in &self.zones {
            total += zone.lock().free_lists[order].count;
        }
        total
    }

    /// Check if the allocator is initialized
    pub fn is_initialized(&self) -> bool {
        self.initialized
    }

    /// Mark a region as used (for kernel, bootloader, etc.)
    pub fn mark_used(&self, start: PhysAddr, len: usize) {
        // This is used during early boot to mark regions that shouldn't be allocated
        // The actual implementation removes pages from free lists if they're present
        let start_frame = start.page_align_down().as_usize() / FRAME_SIZE;
        let end_frame = (start.as_usize() + len + FRAME_SIZE - 1) / FRAME_SIZE;

        for frame in start_frame..end_frame {
            let addr = PhysAddr::new((frame * FRAME_SIZE) as u64);
            let zone_type = ZoneType::for_address(addr);
            let mut zone = self.zones[zone_type.index()].lock();

            // Try to remove from each order's free list
            for order in 0..=MAX_ORDER {
                let block_start_frame = frame & !((1 << order) - 1);
                let block_addr = (block_start_frame * FRAME_SIZE) as u64;
                // SAFETY: Zone is initialized if not empty
                if unsafe { self.remove_from_free_list(&mut zone, order, block_addr) } {
                    zone.stats.free_pages[order]
                        .fetch_sub(1, core::sync::atomic::Ordering::Relaxed);
                    let pages = 1u64 << order;
                    self.stats
                        .free_bytes
                        .fetch_sub(pages * FRAME_SIZE as u64, core::sync::atomic::Ordering::Relaxed);
                    break;
                }
            }
        }
    }
}

impl Default for BuddyAllocator {
    fn default() -> Self {
        Self::new()
    }
}

/// Implement the FrameAllocator trait for compatibility
impl FrameAllocator for BuddyAllocator {
    fn alloc_frame(&self) -> Option<PhysAddr> {
        self.alloc(&AllocRequest::new(0)).ok()
    }

    fn free_frame(&self, addr: PhysAddr) {
        let _ = self.free(addr, 0);
    }

    fn alloc_frames(&self, count: usize) -> Option<PhysAddr> {
        if count == 0 {
            return None;
        }
        // Round up to power of 2
        let order = count.next_power_of_two().trailing_zeros() as usize;
        self.alloc(&AllocRequest::new(order)).ok()
    }

    fn free_frames(&self, addr: PhysAddr, count: usize) {
        if count == 0 {
            return;
        }
        let order = count.next_power_of_two().trailing_zeros() as usize;
        let _ = self.free(addr, order);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zone_type_for_address() {
        assert_eq!(
            ZoneType::for_address(PhysAddr::new(0x1000)),
            ZoneType::Dma
        );
        assert_eq!(
            ZoneType::for_address(PhysAddr::new(0x100_0000)), // 16MB
            ZoneType::Normal
        );
        assert_eq!(
            ZoneType::for_address(PhysAddr::new(0x1_0000_0000)), // 4GB
            ZoneType::High
        );
    }

    #[test]
    fn test_pages_to_order() {
        assert_eq!(crate::zone::pages_to_order(1), 0);
        assert_eq!(crate::zone::pages_to_order(2), 1);
        assert_eq!(crate::zone::pages_to_order(3), 2);
        assert_eq!(crate::zone::pages_to_order(4), 2);
        assert_eq!(crate::zone::pages_to_order(5), 3);
    }
}
