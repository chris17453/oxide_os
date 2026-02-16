//! VirtIO Virtqueue Management
//!
//! Shared split virtqueue implementation (descriptor table + avail/used rings).
//! Eliminates ~500 LOC duplication per VirtIO driver.
//! — WireSaint: one ring to manage them all, in the darkness bind them

use core::sync::atomic::Ordering;
use mm_manager::mm;
use mm_traits::FrameAllocator;

use crate::{phys_to_virt, virt_to_phys};

/// Virtqueue descriptor flags
pub mod desc_flags {
    /// Descriptor is chained (next field valid)
    pub const NEXT: u16 = 1;
    /// Buffer is write-only (device writes, driver reads)
    pub const WRITE: u16 = 2;
    /// Buffer contains list of descriptors
    pub const INDIRECT: u16 = 4;
}

/// VirtIO virtqueue descriptor (§2.7.5)
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct VirtqDesc {
    /// Physical address of buffer
    pub addr: u64,
    /// Length of buffer
    pub len: u32,
    /// Flags (NEXT, WRITE, INDIRECT)
    pub flags: u16,
    /// Next descriptor index (if NEXT flag set)
    pub next: u16,
}

/// VirtIO available ring (§2.7.6)
#[repr(C)]
pub struct VirtqAvail {
    pub flags: u16,
    pub idx: u16,
    pub ring: [u16; 256],
    pub used_event: u16,
}

/// VirtIO used ring element (§2.7.8)
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct VirtqUsedElem {
    /// Descriptor chain head index
    pub id: u32,
    /// Number of bytes written
    pub len: u32,
}

/// VirtIO used ring (§2.7.8)
#[repr(C)]
pub struct VirtqUsed {
    pub flags: u16,
    pub idx: u16,
    pub ring: [VirtqUsedElem; 256],
    pub avail_event: u16,
}

/// Maximum queue size (VirtIO spec allows up to 32768, we use 256 for simplicity)
pub const MAX_QUEUE_SIZE: usize = 256;

/// Virtqueue management structure
///
/// Manages a split virtqueue with descriptor table, available ring, and used ring.
/// Handles descriptor allocation, chain management, and completion detection.
pub struct Virtqueue {
    /// Descriptor table (physical address)
    desc_phys: u64,
    /// Descriptor table (virtual address for kernel access)
    desc: *mut VirtqDesc,
    /// Available ring (physical address)
    avail_phys: u64,
    /// Available ring (virtual address)
    avail: *mut VirtqAvail,
    /// Used ring (physical address)
    used_phys: u64,
    /// Used ring (virtual address)
    used: *mut VirtqUsed,
    /// Number of descriptors
    num: u16,
    /// Free descriptor list head
    free_head: u16,
    /// Number of free descriptors
    num_free: u16,
    /// Last used index we processed
    last_used_idx: u16,
}

impl Virtqueue {
    /// Allocate and initialize a new virtqueue for modern VirtIO (PCI/MMIO)
    ///
    /// Uses the frame allocator for DMA-safe physical addresses. The kernel heap
    /// lives at 0xFFFF_FFFF_8xxx_xxxx (kernel text region) — virt_to_phys() on
    /// heap pointers yields ~128TB bogus addresses. Frame allocator gives us
    /// memory in the PHYS_MAP region where virt_to_phys() actually works.
    /// — WireSaint: the heap was never meant for DMA. Stack pointers lie too.
    ///
    /// # Safety
    /// Requires a working memory manager (call mm_manager::init_global first).
    pub unsafe fn new(num: u16) -> Option<Self> {
        // Calculate sizes
        // Descriptors: 16 bytes each
        let desc_size = (num as usize) * core::mem::size_of::<VirtqDesc>();
        // Available ring: 2 + 2 + 2*num + 2 (flags, idx, ring, used_event)
        let avail_size = 6 + 2 * (num as usize);
        // Used ring: 2 + 2 + 8*num + 2 (flags, idx, ring, avail_event)
        let used_size = 6 + 8 * (num as usize);

        // Used ring needs 4-byte alignment, round up
        let used_offset = ((desc_size + avail_size) + 3) & !3;
        let total_size = used_offset + used_size;
        let num_pages = (total_size + 4095) / 4096;

        // — WireSaint: frame allocator gives physical frames with known addresses.
        // DMA devices need real physical addresses, not kernel virtual illusions.
        let phys_addr = mm().alloc_contiguous(num_pages).ok()?;
        let phys_base = phys_addr.as_u64();

        // Access through the physical map where virt_to_phys() is an identity op
        let virt_base = phys_to_virt(phys_base);
        let ptr = virt_base as *mut u8;

        // Zero the memory
        unsafe {
            core::ptr::write_bytes(ptr, 0, num_pages * 4096);
        }

        // Calculate addresses within the allocation
        let desc = ptr as *mut VirtqDesc;
        let avail = unsafe { ptr.add(desc_size) } as *mut VirtqAvail;
        let used = unsafe { ptr.add(used_offset) } as *mut VirtqUsed;

        // Physical addresses are straightforward — we allocated physical frames
        let desc_phys = phys_base;
        let avail_phys = phys_base + desc_size as u64;
        let used_phys = phys_base + used_offset as u64;

        // Initialize free list - chain all descriptors together
        for i in 0..num {
            // SAFETY: desc is valid and i is within bounds
            unsafe { (*desc.add(i as usize)).next = i + 1 };
        }

        Some(Virtqueue {
            desc_phys,
            desc,
            avail_phys,
            avail,
            used_phys,
            used,
            num,
            free_head: 0,
            num_free: num,
            last_used_idx: 0,
        })
    }

    /// Allocate and initialize a new virtqueue for legacy VirtIO PCI
    ///
    /// Legacy VirtIO requires all three rings in a single contiguous region with
    /// the used ring starting at a page boundary. Uses the frame allocator for
    /// guaranteed physical contiguity and known physical addresses.
    ///
    /// Layout:
    /// - Descriptor table: at offset 0, size = 16 * num
    /// - Available ring: immediately after descriptors, size = 6 + 2 * num
    /// - Padding to next page boundary
    /// - Used ring: at page-aligned offset, size = 6 + 8 * num
    ///
    /// # Safety
    /// Requires a working memory manager (call mm_manager::init_global first).
    pub unsafe fn new_legacy(num: u16) -> Option<Self> {
        // Calculate sizes
        let desc_size = (num as usize) * core::mem::size_of::<VirtqDesc>();
        let avail_size = 6 + 2 * (num as usize);
        let used_size = 6 + 8 * (num as usize);

        // For legacy, used ring must be at page boundary
        // Calculate offset: round up (desc_size + avail_size) to page boundary
        let avail_end = desc_size + avail_size;
        let used_offset = (avail_end + 4095) & !4095; // Round up to 4096

        let total_size = used_offset + used_size;
        let num_pages = (total_size + 4095) / 4096;

        // Allocate physical frames using the frame allocator
        // This gives us a known physical address that we can use for DMA
        let phys_addr = mm().alloc_contiguous(num_pages).ok()?;
        let phys_base = phys_addr.as_u64();

        // Access the physical memory through the physical map
        let virt_base = phys_to_virt(phys_base);
        let ptr = virt_base as *mut u8;

        // Zero the memory
        unsafe {
            core::ptr::write_bytes(ptr, 0, num_pages * 4096);
        }

        // Calculate addresses within the allocation
        let desc = ptr as *mut VirtqDesc;
        let avail = unsafe { ptr.add(desc_size) } as *mut VirtqAvail;
        let used = unsafe { ptr.add(used_offset) } as *mut VirtqUsed;

        // Physical addresses are straightforward since we allocated physical frames
        let desc_phys = phys_base;
        let avail_phys = phys_base + desc_size as u64;
        let used_phys = phys_base + used_offset as u64;

        // Initialize free list - chain all descriptors together
        for i in 0..num {
            unsafe { (*desc.add(i as usize)).next = i + 1 };
        }

        Some(Virtqueue {
            desc_phys,
            desc,
            avail_phys,
            avail,
            used_phys,
            used,
            num,
            free_head: 0,
            num_free: num,
            last_used_idx: 0,
        })
    }

    /// Get number of descriptors in queue
    pub fn size(&self) -> u16 {
        self.num
    }

    /// Get number of free descriptors (for diagnostics)
    pub fn num_free(&self) -> u16 {
        self.num_free
    }

    /// Get physical addresses for queue setup (descriptor, avail, used)
    pub fn physical_addresses(&self) -> (u64, u64, u64) {
        (self.desc_phys, self.avail_phys, self.used_phys)
    }

    /// Allocate a descriptor from the free list
    pub fn alloc_desc(&mut self) -> Option<u16> {
        if self.num_free == 0 {
            return None;
        }
        let idx = self.free_head;
        unsafe {
            self.free_head = (*self.desc.add(idx as usize)).next;
        }
        self.num_free -= 1;
        Some(idx)
    }

    /// Free a descriptor chain starting at idx
    pub fn free_chain(&mut self, mut idx: u16) {
        loop {
            unsafe {
                let desc = &mut *self.desc.add(idx as usize);
                let next = desc.next;
                let has_next = desc.flags & desc_flags::NEXT != 0;

                // Add to free list
                desc.next = self.free_head;
                self.free_head = idx;
                self.num_free += 1;

                if !has_next {
                    break;
                }
                idx = next;
            }
        }
    }

    /// Write descriptor at index
    ///
    /// # Safety
    /// Index must be a valid allocated descriptor.
    pub unsafe fn write_desc(&mut self, idx: u16, addr: u64, len: u32, flags: u16, next: u16) {
        let desc = &mut *self.desc.add(idx as usize);
        desc.addr = addr;
        desc.len = len;
        desc.flags = flags;
        desc.next = next;
    }

    /// Add a descriptor chain to the available ring
    pub fn add_available(&mut self, head: u16) {
        unsafe {
            let avail = &mut *self.avail;
            let idx = avail.idx as usize % self.num as usize;
            avail.ring[idx] = head;
            // Memory barrier to ensure descriptor is visible before idx update
            core::sync::atomic::fence(Ordering::Release);
            avail.idx = avail.idx.wrapping_add(1);
        }
    }

    /// Check if there are completed requests in the used ring
    pub fn has_completed(&self) -> bool {
        unsafe {
            let used = &*self.used;
            // Memory barrier to ensure we see the latest idx
            core::sync::atomic::fence(Ordering::Acquire);
            used.idx != self.last_used_idx
        }
    }

    /// Get next completed request (descriptor chain head and length)
    pub fn pop_used(&mut self) -> Option<(u16, u32)> {
        unsafe {
            let used = &*self.used;
            core::sync::atomic::fence(Ordering::Acquire);
            if used.idx == self.last_used_idx {
                return None;
            }

            let idx = self.last_used_idx as usize % self.num as usize;
            let elem = used.ring[idx];
            self.last_used_idx = self.last_used_idx.wrapping_add(1);

            Some((elem.id as u16, elem.len))
        }
    }
}

// SAFETY: Virtqueue uses raw pointers but they're only accessed through &mut methods
unsafe impl Send for Virtqueue {}
