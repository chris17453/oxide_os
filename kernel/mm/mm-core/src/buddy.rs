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
use crate::{FRAME_SHIFT, FRAME_SIZE, MAX_ORDER, MmError, MmResult};
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

/// Free block header stored in the first 24 bytes of a free block — BlackLatch
/// Doubly-linked list for O(1) removal (the Linux way, not that amateur singly-linked bullshit)
/// Now with corruption detection canary — GraveShift: Trust but verify
#[repr(C)]
struct FreeBlock {
    magic: u64, // Canary value 0xFREEBL0C (0x4652454542304C) — corruption detector
    next: u64,  // Frame number of next free block, or 0 if none
    prev: u64,  // Frame number of previous free block, or 0 if head — TorqueJax
}

const FREE_BLOCK_MAGIC: u64 = 0x4652454542304C; // "FREEBL0C" in hex — SableWire

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
            zone.stats.free_pages[order].fetch_add(1, core::sync::atomic::Ordering::Relaxed);
            self.stats.free_bytes.fetch_add(
                pages * FRAME_SIZE as u64,
                core::sync::atomic::Ordering::Relaxed,
            );
            self.stats.total_bytes.fetch_add(
                pages * FRAME_SIZE as u64,
                core::sync::atomic::Ordering::Relaxed,
            );

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

        // [TRACE] Log adds in target range AND check existing magic — ColdCipher
        if addr >= 0xc400000 && addr <= 0xc500000 {
            // Read existing value BEFORE we write the magic
            let existing_magic = unsafe { core::ptr::read_volatile(virt as *const u64) };

            unsafe {
                use arch_x86_64 as arch;
                let msg = b"[ADD-FREE] 0x";
                for &byte in msg.iter() {
                    while arch::inb(0x3FD) & 0x20 == 0 {}
                    arch::outb(0x3F8, byte);
                }
                for i in (0..16).rev() {
                    let nibble = ((addr >> (i * 4)) & 0xF) as u8;
                    let hex_char = if nibble < 10 { b'0' + nibble } else { b'a' + nibble - 10 };
                    arch::outb(0x3F8, hex_char);
                }
                let msg2 = b" order=";
                for &byte in msg2.iter() {
                    while arch::inb(0x3FD) & 0x20 == 0 {}
                    arch::outb(0x3F8, byte);
                }
                arch::outb(0x3F8, b'0' + order as u8);
                let msg3 = b" old_magic=0x";
                for &byte in msg3.iter() {
                    while arch::inb(0x3FD) & 0x20 == 0 {}
                    arch::outb(0x3F8, byte);
                }
                for i in (0..16).rev() {
                    let nibble = ((existing_magic >> (i * 4)) & 0xF) as u8;
                    let hex_char = if nibble < 10 { b'0' + nibble } else { b'a' + nibble - 10 };
                    arch::outb(0x3F8, hex_char);
                }
                let msg4 = b"\r\n";
                for &byte in msg4.iter() {
                    while arch::inb(0x3FD) & 0x20 == 0 {}
                    arch::outb(0x3F8, byte);
                }
            }
        }

        // Set canary — GraveShift: Mark this as a valid free block
        block.magic = FREE_BLOCK_MAGIC;
        // Insert at head of free list — Doubly-linked style, update prev pointers
        block.next = old_head;
        block.prev = 0; // New head has no predecessor

        // Update old head's prev pointer to point back to new head — SableWire
        if old_head != 0 {
            let old_head_addr = old_head << FRAME_SHIFT;
            let old_head_virt = phys_to_virt(PhysAddr::new(old_head_addr));
            let old_head_block = unsafe { &mut *(old_head_virt as *mut FreeBlock) };

            // VALIDATE — BlackLatch: Check old head's canary before writing
            if old_head_block.magic != FREE_BLOCK_MAGIC {
                unsafe {
                    use arch_x86_64 as arch;
                    let msg = b"[BUDDY-FATAL] Old head corrupted! magic=0x";
                    for &byte in msg.iter() {
                        while arch::inb(0x3FD) & 0x20 == 0 {}
                        arch::outb(0x3F8, byte);
                    }
                    for i in (0..16).rev() {
                        let nibble = ((old_head_block.magic >> (i * 4)) & 0xF) as u8;
                        let hex_char = if nibble < 10 { b'0' + nibble } else { b'a' + nibble - 10 };
                        arch::outb(0x3F8, hex_char);
                    }
                    let msg2 = b", expected=0x";
                    for &byte in msg2.iter() {
                        while arch::inb(0x3FD) & 0x20 == 0 {}
                        arch::outb(0x3F8, byte);
                    }
                    for i in (0..16).rev() {
                        let nibble = ((FREE_BLOCK_MAGIC >> (i * 4)) & 0xF) as u8;
                        let hex_char = if nibble < 10 { b'0' + nibble } else { b'a' + nibble - 10 };
                        arch::outb(0x3F8, hex_char);
                    }
                    let msg3 = b" - GPF\r\n";
                    for &byte in msg3.iter() {
                        while arch::inb(0x3FD) & 0x20 == 0 {}
                        arch::outb(0x3F8, byte);
                    }
                    core::ptr::write_volatile(0xBADBAD as *mut u64, old_head_block.magic);
                }
            }

            old_head_block.prev = frame_num;
        }

        zone.free_lists[order].head = frame_num; // Store frame number
        let old_count = zone.free_lists[order].count;
        zone.free_lists[order].count += 1;
        let new_count = zone.free_lists[order].count;

        // TRACE EVERY COUNT CHANGE — GraveShift
        unsafe {
            use arch_x86_64 as arch;
            let msg = b"[COUNT+] order=";
            for &byte in msg.iter() {
                while arch::inb(0x3FD) & 0x20 == 0 {}
                arch::outb(0x3F8, byte);
            }
            arch::outb(0x3F8, b'0' + order as u8);
            let msg2 = b", ";
            for &byte in msg2.iter() {
                while arch::inb(0x3FD) & 0x20 == 0 {}
                arch::outb(0x3F8, byte);
            }
            arch::outb(0x3F8, b'0' + (old_count as u8));
            arch::outb(0x3F8, b'-');
            arch::outb(0x3F8, b'>');
            arch::outb(0x3F8, b'0' + (new_count as u8));
            let msg3 = b"\r\n";
            for &byte in msg3.iter() {
                while arch::inb(0x3FD) & 0x20 == 0 {}
                arch::outb(0x3F8, byte);
            }
        }
    }

    /// Remove and return a free block from a zone's free list
    ///
    /// # Safety
    /// The zone must have been properly initialized.
    unsafe fn pop_free_block(&self, zone: &mut MemoryZone, order: usize) -> Option<u64> {
        let initial_count = zone.free_lists[order].count;

        if initial_count == 0 {
            return None;
        }

        let frame_num = zone.free_lists[order].head;
        let addr = frame_num << FRAME_SHIFT;

        // [MONITOR] Log ALL pops of order >= 4 — ColdCipher
        if order >= 4 {
            unsafe {
                use arch_x86_64 as arch;
                let msg = b"[POP-O";
                for &byte in msg.iter() {
                    while arch::inb(0x3FD) & 0x20 == 0 {}
                    arch::outb(0x3F8, byte);
                }
                arch::outb(0x3F8, b'0' + order as u8);
                let msg2 = b"] 0x";
                for &byte in msg2.iter() {
                    while arch::inb(0x3FD) & 0x20 == 0 {}
                    arch::outb(0x3F8, byte);
                }
                for i in (0..16).rev() {
                    let nibble = ((addr >> (i * 4)) & 0xF) as u8;
                    let hex_char = if nibble < 10 { b'0' + nibble } else { b'a' + nibble - 10 };
                    arch::outb(0x3F8, hex_char);
                }
                let msg3 = b"\r\n";
                for &byte in msg3.iter() {
                    while arch::inb(0x3FD) & 0x20 == 0 {}
                    arch::outb(0x3F8, byte);
                }
            }
        }

        let virt = phys_to_virt(PhysAddr::new(addr));

        // TRACE — GraveShift: About to access block memory
        unsafe {
            use arch_x86_64 as arch;
            let msg = b"[POP-TRACE] Accessing block\r\n";
            for &byte in msg.iter() {
                while arch::inb(0x3FD) & 0x20 == 0 {}
                arch::outb(0x3F8, byte);
            }
        }

        // SAFETY: Frame was previously added to free list so memory is valid
        let block = unsafe { &mut *(virt as *mut FreeBlock) };

        // VALIDATE CANARY — GraveShift: Check magic before trusting this block
        if block.magic != FREE_BLOCK_MAGIC {
            unsafe {
                use arch_x86_64 as arch;
                let msg = b"[BUDDY-FATAL] Block corrupted at pop! magic=0x";
                for &byte in msg.iter() {
                    while arch::inb(0x3FD) & 0x20 == 0 {}
                    arch::outb(0x3F8, byte);
                }
                for i in (0..16).rev() {
                    let nibble = ((block.magic >> (i * 4)) & 0xF) as u8;
                    let hex_char = if nibble < 10 { b'0' + nibble } else { b'a' + nibble - 10 };
                    arch::outb(0x3F8, hex_char);
                }
                let msg2 = b", addr=0x";
                for &byte in msg2.iter() {
                    while arch::inb(0x3FD) & 0x20 == 0 {}
                    arch::outb(0x3F8, byte);
                }
                for i in (0..16).rev() {
                    let nibble = ((addr >> (i * 4)) & 0xF) as u8;
                    let hex_char = if nibble < 10 { b'0' + nibble } else { b'a' + nibble - 10 };
                    arch::outb(0x3F8, hex_char);
                }
                let msg3 = b", order=";
                for &byte in msg3.iter() {
                    while arch::inb(0x3FD) & 0x20 == 0 {}
                    arch::outb(0x3F8, byte);
                }
                arch::outb(0x3F8, b'0' + order as u8);
                let msg4 = b" - GPF\r\n";
                for &byte in msg4.iter() {
                    while arch::inb(0x3FD) & 0x20 == 0 {}
                    arch::outb(0x3F8, byte);
                }
                // Trigger GPF with aligned address
                core::ptr::write_volatile(0xDEAD0000 as *mut u64, block.magic);
            }
        }

        let next_frame = block.next;

        // TRACE — TorqueJax: Read next successfully
        unsafe {
            use arch_x86_64 as arch;
            let msg = b"[POP-TRACE] Read next OK\r\n";
            for &byte in msg.iter() {
                while arch::inb(0x3FD) & 0x20 == 0 {}
                arch::outb(0x3F8, byte);
            }
        }

        // [TRACE] Return marker — BlackLatch: Verify we return from pop
        unsafe {
            use arch_x86_64 as arch;
            let msg = b"[POP-RETURN]\r\n";
            for &byte in msg.iter() {
                while arch::inb(0x3FD) & 0x20 == 0 {}
                arch::outb(0x3F8, byte);
            }

            // [MONITOR] Check target block's magic — ColdCipher: Track when it corrupts
            let target_addr = 0x0c480000u64;
            let target_virt = phys_to_virt(PhysAddr::new(target_addr));
            let target_magic = core::ptr::read_volatile(target_virt as *const u64);
            if target_magic != FREE_BLOCK_MAGIC && target_magic != 0 {
                let msg = b"[WATCH] 0xc480000 magic=0x";
                for &byte in msg.iter() {
                    while arch::inb(0x3FD) & 0x20 == 0 {}
                    arch::outb(0x3F8, byte);
                }
                for i in (0..16).rev() {
                    let nibble = ((target_magic >> (i * 4)) & 0xF) as u8;
                    let hex_char = if nibble < 10 { b'0' + nibble } else { b'a' + nibble - 10 };
                    arch::outb(0x3F8, hex_char);
                }
                let msg2 = b"\r\n";
                for &byte in msg2.iter() {
                    while arch::inb(0x3FD) & 0x20 == 0 {}
                    arch::outb(0x3F8, byte);
                }
            }
        }

        // Clear the popped block's canary and pointers — WireSaint: Prevent use-after-free detection
        // [TRACE] Log every magic clear to catch double-clears — ColdCipher
        if addr >= 0xc400000 && addr <= 0xc500000 {
            unsafe {
                use arch_x86_64 as arch;
                let msg = b"[CLEAR-MAGIC] 0x";
                for &byte in msg.iter() {
                    while arch::inb(0x3FD) & 0x20 == 0 {}
                    arch::outb(0x3F8, byte);
                }
                for i in (0..16).rev() {
                    let nibble = ((addr >> (i * 4)) & 0xF) as u8;
                    let hex_char = if nibble < 10 { b'0' + nibble } else { b'a' + nibble - 10 };
                    arch::outb(0x3F8, hex_char);
                }
                let msg2 = b"\r\n";
                for &byte in msg2.iter() {
                    while arch::inb(0x3FD) & 0x20 == 0 {}
                    arch::outb(0x3F8, byte);
                }
            }
        }
        block.magic = 0; // Invalidate canary — any future access will detect corruption
        block.next = 0;
        block.prev = 0;

        // Update head and fix new head's prev pointer — BlackLatch: Clean up the doubly-linked chain
        zone.free_lists[order].head = next_frame;
        if next_frame != 0 {
            let next_addr = next_frame << FRAME_SHIFT;

            // VALIDATE — GraveShift: Check if next_frame is sane (< 512MB for 512M RAM)
            // Frame numbers above 0x20000 (512MB / 4KB) are invalid
            if next_frame > 0x20000 {
                unsafe {
                    use arch_x86_64 as arch;
                    let msg = b"[BUDDY-FATAL] Corrupted next_frame=0x";
                    for &byte in msg.iter() {
                        while arch::inb(0x3FD) & 0x20 == 0 {}
                        arch::outb(0x3F8, byte);
                    }
                    for i in (0..16).rev() {
                        let nibble = ((next_frame >> (i * 4)) & 0xF) as u8;
                        let hex_char = if nibble < 10 { b'0' + nibble } else { b'a' + nibble - 10 };
                        arch::outb(0x3F8, hex_char);
                    }
                    let msg2 = b" - TRIGGERING GPF\r\n";
                    for &byte in msg2.iter() {
                        while arch::inb(0x3FD) & 0x20 == 0 {}
                        arch::outb(0x3F8, byte);
                    }
                    // Trigger GPF — Die screaming
                    core::ptr::write_volatile(0xBADBAD as *mut u64, next_frame);
                }
            }

            let next_virt = phys_to_virt(PhysAddr::new(next_addr));

            // TRACE — BlackLatch: About to dereference pointer
            unsafe {
                use arch_x86_64 as arch;
                let msg = b"[POP-TRACE] About to access next_virt=0x";
                for &byte in msg.iter() {
                    while arch::inb(0x3FD) & 0x20 == 0 {}
                    arch::outb(0x3F8, byte);
                }
                let virt_addr = next_virt as u64;
                for i in (0..16).rev() {
                    let nibble = ((virt_addr >> (i * 4)) & 0xF) as u8;
                    let hex_char = if nibble < 10 { b'0' + nibble } else { b'a' + nibble - 10 };
                    arch::outb(0x3F8, hex_char);
                }
                let msg2 = b"\r\n";
                for &byte in msg2.iter() {
                    while arch::inb(0x3FD) & 0x20 == 0 {}
                    arch::outb(0x3F8, byte);
                }
            }

            let next_block = unsafe { &mut *(next_virt as *mut FreeBlock) };

            // TRACE — TorqueJax: Successfully accessed next block
            unsafe {
                use arch_x86_64 as arch;
                let msg = b"[POP-TRACE] Accessed next block OK\r\n";
                for &byte in msg.iter() {
                    while arch::inb(0x3FD) & 0x20 == 0 {}
                    arch::outb(0x3F8, byte);
                }
            }

            next_block.prev = 0; // New head has no predecessor

            // TRACE — SableWire: Set prev = 0 successfully
            unsafe {
                use arch_x86_64 as arch;
                let msg = b"[POP-TRACE] Set prev=0 OK\r\n";
                for &byte in msg.iter() {
                    while arch::inb(0x3FD) & 0x20 == 0 {}
                    arch::outb(0x3F8, byte);
                }
            }
        }

        // Re-read count to detect corruption — GraveShift
        let current_count = zone.free_lists[order].count;
        if current_count != initial_count {
            // Count changed between read and now! Memory corruption! — SableWire
            // TRIGGER GPF — This is unrecoverable corruption
            unsafe {
                use arch_x86_64 as arch;
                let msg = b"[BUDDY-FATAL] Count corrupted! initial=";
                for &byte in msg.iter() {
                    while arch::inb(0x3FD) & 0x20 == 0 {}
                    arch::outb(0x3F8, byte);
                }
                arch::outb(0x3F8, b'0' + (initial_count as u8));
                let msg2 = b", current=";
                for &byte in msg2.iter() {
                    while arch::inb(0x3FD) & 0x20 == 0 {}
                    arch::outb(0x3F8, byte);
                }
                arch::outb(0x3F8, b'0' + (current_count as u8));
                let msg3 = b", order=";
                for &byte in msg3.iter() {
                    while arch::inb(0x3FD) & 0x20 == 0 {}
                    arch::outb(0x3F8, byte);
                }
                arch::outb(0x3F8, b'0' + order as u8);
                let msg4 = b" - TRIGGERING GPF\r\n";
                for &byte in msg4.iter() {
                    while arch::inb(0x3FD) & 0x20 == 0 {}
                    arch::outb(0x3F8, byte);
                }

                // Trigger GPF by accessing invalid memory — GraveShift: Die screaming
                core::ptr::write_volatile(0xDEADBEEF as *mut u64, 0xCAFEBABE);
            }
        }

        // Check for underflow — BlackLatch: Better to crash than corrupt silently
        if current_count == 0 {
            // Count is 0 but we're about to decrement! MEMORY CORRUPTION!
            unsafe {
                use arch_x86_64 as arch;
                let msg = b"[BUDDY-FATAL] Count underflow! order=";
                for &byte in msg.iter() {
                    while arch::inb(0x3FD) & 0x20 == 0 {}
                    arch::outb(0x3F8, byte);
                }
                arch::outb(0x3F8, b'0' + order as u8);
                let msg2 = b" - TRIGGERING GPF\r\n";
                for &byte in msg2.iter() {
                    while arch::inb(0x3FD) & 0x20 == 0 {}
                    arch::outb(0x3F8, byte);
                }

                // Trigger GPF — TorqueJax: Fail loud not silent
                core::ptr::write_volatile(0xDEADBEEF as *mut u64, 0xDEADC0DE);
            }
        }

        let old_count_before_dec = zone.free_lists[order].count;
        zone.free_lists[order].count -= 1;
        let new_count_after_dec = zone.free_lists[order].count;

        // TRACE EVERY COUNT CHANGE — BlackLatch
        unsafe {
            use arch_x86_64 as arch;
            let msg = b"[COUNT-] order=";
            for &byte in msg.iter() {
                while arch::inb(0x3FD) & 0x20 == 0 {}
                arch::outb(0x3F8, byte);
            }
            arch::outb(0x3F8, b'0' + order as u8);
            let msg2 = b", ";
            for &byte in msg2.iter() {
                while arch::inb(0x3FD) & 0x20 == 0 {}
                arch::outb(0x3F8, byte);
            }
            arch::outb(0x3F8, b'0' + (old_count_before_dec as u8));
            arch::outb(0x3F8, b'-');
            arch::outb(0x3F8, b'>');
            arch::outb(0x3F8, b'0' + (new_count_after_dec as u8));
            let msg3 = b" (pop)\r\n";
            for &byte in msg3.iter() {
                while arch::inb(0x3FD) & 0x20 == 0 {}
                arch::outb(0x3F8, byte);
            }
        }

        Some(addr)
    }

    /// Allocate memory with the given request
    pub fn alloc(&self, request: &AllocRequest) -> MmResult<PhysAddr> {
        // [TRACE] Entry marker — BlackLatch
        unsafe {
            use arch_x86_64 as arch;
            arch::outb(0x3F8, b'[');
        }

        if request.order > MAX_ORDER {
            return Err(MmError::InvalidOrder);
        }

        // Try preferred zone first, then fall back to others
        let zone_order = match request.zone {
            Some(ZoneType::Dma) => [0, 1, 2], // DMA only, then Normal, then High
            Some(ZoneType::Normal) => [1, 2, 0], // Normal preferred
            Some(ZoneType::High) => [2, 1, 0], // High preferred
            None => [1, 2, 0],                // Default: Normal, High, DMA
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

                // [TRACE] Log allocation details — BlackLatch: Track all allocs including PT frames
                unsafe {
                    use arch_x86_64 as arch;
                    arch::outb(0x3F8, b'A');
                    // Only log detailed info for non-page table allocations (order 0)
                    // to reduce spam, but log ALL order > 0 allocations
                    if request.order > 0 {
                        let msg = b"[ALLOC-O";
                        for &byte in msg.iter() {
                            while arch::inb(0x3FD) & 0x20 == 0 {}
                            arch::outb(0x3F8, byte);
                        }
                        arch::outb(0x3F8, b'0' + request.order as u8);
                        let msg2 = b"] 0x";
                        for &byte in msg2.iter() {
                            while arch::inb(0x3FD) & 0x20 == 0 {}
                            arch::outb(0x3F8, byte);
                        }
                        for i in (0..16).rev() {
                            let nibble = ((addr >> (i * 4)) & 0xF) as u8;
                            let hex_char = if nibble < 10 { b'0' + nibble } else { b'a' + nibble - 10 };
                            arch::outb(0x3F8, hex_char);
                        }
                        let msg3 = b"\r\n";
                        for &byte in msg3.iter() {
                            while arch::inb(0x3FD) & 0x20 == 0 {}
                            arch::outb(0x3F8, byte);
                        }
                    }
                }

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
                // Found a block, may need to split — SAFETY: Zone is initialized
                let addr = unsafe { self.pop_free_block(zone, current_order)? };

                // Split larger blocks down to requested size — TorqueJax
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
            if buddy_addr < zone.base.as_u64() || buddy_addr >= zone.base.as_u64() + zone.size {
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
        zone.stats.free_pages[current_order].fetch_add(1, core::sync::atomic::Ordering::Relaxed);
    }

    /// Try to remove a specific block from a free list — GraveShift: O(1) removal, no traversal needed
    ///
    /// # Safety
    /// Zone must be initialized and the address must be valid.
    unsafe fn remove_from_free_list(&self, zone: &mut MemoryZone, order: usize, addr: u64) -> bool {
        let target_frame = addr >> FRAME_SHIFT;

        if zone.free_lists[order].count == 0 {
            return false;
        }

        // Check if target is the head — fast path
        if zone.free_lists[order].head == target_frame {
            // SAFETY: Zone is initialized
            unsafe { self.pop_free_block(zone, order) };
            return true;
        }

        // O(1) removal using doubly-linked list — BlackLatch: The Linux way, motherfucker
        // Get the target block directly (no traversal!)
        let target_virt = phys_to_virt(PhysAddr::new(addr));
        let target_block = unsafe { &mut *(target_virt as *mut FreeBlock) };

        // Verify block is actually in the free list — SableWire: Paranoid but necessary
        // If prev is 0, target should be head (already checked above)
        // If next is 0 and prev is 0, block was cleared (not in list)
        if target_block.prev == 0 || (target_block.prev == 0 && target_block.next == 0) {
            return false;
        }

        // Verify the chain integrity — TorqueJax: Check predecessor points to us
        let prev_addr = target_block.prev << FRAME_SHIFT;
        let prev_virt = phys_to_virt(PhysAddr::new(prev_addr));
        let prev_block = unsafe { &mut *(prev_virt as *mut FreeBlock) };

        if prev_block.next != target_frame {
            // Chain is broken, block not actually in list — WireSaint
            return false;
        }

        // Update predecessor's next pointer
        prev_block.next = target_block.next;

        // Update successor's prev pointer (if exists)
        if target_block.next != 0 {
            let next_addr = target_block.next << FRAME_SHIFT;
            let next_virt = phys_to_virt(PhysAddr::new(next_addr));
            let next_block = unsafe { &mut *(next_virt as *mut FreeBlock) };
            next_block.prev = target_block.prev;
        }

        // Clear removed block's pointers — GraveShift: Poison the corpse
        target_block.next = 0;
        target_block.prev = 0;

        let old_count_remove = zone.free_lists[order].count;
        zone.free_lists[order].count -= 1;
        let new_count_remove = zone.free_lists[order].count;

        // TRACE EVERY COUNT CHANGE — TorqueJax
        unsafe {
            use arch_x86_64 as arch;
            let msg = b"[COUNT-] order=";
            for &byte in msg.iter() {
                while arch::inb(0x3FD) & 0x20 == 0 {}
                arch::outb(0x3F8, byte);
            }
            arch::outb(0x3F8, b'0' + order as u8);
            let msg2 = b", ";
            for &byte in msg2.iter() {
                while arch::inb(0x3FD) & 0x20 == 0 {}
                arch::outb(0x3F8, byte);
            }
            arch::outb(0x3F8, b'0' + (old_count_remove as u8));
            arch::outb(0x3F8, b'-');
            arch::outb(0x3F8, b'>');
            arch::outb(0x3F8, b'0' + (new_count_remove as u8));
            let msg3 = b" (remove)\r\n";
            for &byte in msg3.iter() {
                while arch::inb(0x3FD) & 0x20 == 0 {}
                arch::outb(0x3F8, byte);
            }
        }

        true
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
                    self.stats.free_bytes.fetch_sub(
                        pages * FRAME_SIZE as u64,
                        core::sync::atomic::Ordering::Relaxed,
                    );
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
        assert_eq!(ZoneType::for_address(PhysAddr::new(0x1000)), ZoneType::Dma);
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
