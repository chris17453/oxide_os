//! Memory Mapping (mmap) Implementation
//!
//! Provides mmap/munmap syscall handling for self-hosting support.

#![no_std]
#![allow(unused_imports)]
#![allow(unsafe_attr_outside_unsafe)]
#![allow(unsafe_op_in_unsafe_fn)]

extern crate alloc;

pub mod vma;
pub mod anonymous;
pub mod file;

use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::ffi::c_int;
use spin::Mutex;

pub use vma::{VirtualMemoryArea, VmaFlags, VmaType};
pub use anonymous::AnonymousMapping;
pub use file::FileMapping;

/// Protection flags
pub mod prot {
    pub const PROT_NONE: i32 = 0x0;
    pub const PROT_READ: i32 = 0x1;
    pub const PROT_WRITE: i32 = 0x2;
    pub const PROT_EXEC: i32 = 0x4;
}

/// Map flags
pub mod flags {
    pub const MAP_SHARED: i32 = 0x01;
    pub const MAP_PRIVATE: i32 = 0x02;
    pub const MAP_FIXED: i32 = 0x10;
    pub const MAP_ANONYMOUS: i32 = 0x20;
    pub const MAP_ANON: i32 = MAP_ANONYMOUS;
    pub const MAP_GROWSDOWN: i32 = 0x0100;
    pub const MAP_DENYWRITE: i32 = 0x0800;
    pub const MAP_EXECUTABLE: i32 = 0x1000;
    pub const MAP_LOCKED: i32 = 0x2000;
    pub const MAP_NORESERVE: i32 = 0x4000;
    pub const MAP_POPULATE: i32 = 0x8000;
    pub const MAP_NONBLOCK: i32 = 0x10000;
    pub const MAP_STACK: i32 = 0x20000;
    pub const MAP_HUGETLB: i32 = 0x40000;
}

/// msync flags
pub mod msync {
    pub const MS_ASYNC: i32 = 1;
    pub const MS_INVALIDATE: i32 = 2;
    pub const MS_SYNC: i32 = 4;
}

/// madvise advice values
pub mod madvise {
    pub const MADV_NORMAL: i32 = 0;
    pub const MADV_RANDOM: i32 = 1;
    pub const MADV_SEQUENTIAL: i32 = 2;
    pub const MADV_WILLNEED: i32 = 3;
    pub const MADV_DONTNEED: i32 = 4;
    pub const MADV_FREE: i32 = 8;
    pub const MADV_REMOVE: i32 = 9;
    pub const MADV_DONTFORK: i32 = 10;
    pub const MADV_DOFORK: i32 = 11;
    pub const MADV_MERGEABLE: i32 = 12;
    pub const MADV_UNMERGEABLE: i32 = 13;
    pub const MADV_HUGEPAGE: i32 = 14;
    pub const MADV_NOHUGEPAGE: i32 = 15;
    pub const MADV_DONTDUMP: i32 = 16;
    pub const MADV_DODUMP: i32 = 17;
}

/// mremap flags
pub mod mremap {
    pub const MREMAP_MAYMOVE: i32 = 1;
    pub const MREMAP_FIXED: i32 = 2;
    pub const MREMAP_DONTUNMAP: i32 = 4;
}

/// Error codes
pub const ESUCCESS: c_int = 0;
pub const EINVAL: c_int = 22;
pub const ENOMEM: c_int = 12;
pub const EACCES: c_int = 13;
pub const EBADF: c_int = 9;

/// Failed mmap result
pub const MAP_FAILED: *mut u8 = usize::MAX as *mut u8;

/// Memory mapping manager for a process
pub struct MmapManager {
    /// Virtual memory areas, keyed by start address
    vmas: BTreeMap<usize, VirtualMemoryArea>,
    /// Next address hint for allocation
    next_addr: usize,
    /// Process address space bounds
    mmap_min_addr: usize,
    mmap_max_addr: usize,
}

impl MmapManager {
    /// Create new mmap manager
    pub fn new() -> Self {
        MmapManager {
            vmas: BTreeMap::new(),
            next_addr: 0x0000_7000_0000_0000, // User space hint
            mmap_min_addr: 0x0000_1000,       // 4KB minimum
            mmap_max_addr: 0x0000_7FFF_FFFF_F000, // User space maximum
        }
    }

    /// Perform mmap operation
    pub fn mmap(
        &mut self,
        addr: usize,
        len: usize,
        prot: i32,
        map_flags: i32,
        fd: i32,
        offset: i64,
    ) -> Result<usize, c_int> {
        // Validate length
        if len == 0 {
            return Err(EINVAL);
        }

        // Page-align length
        let len = (len + 0xFFF) & !0xFFF;

        // Determine mapping address
        let addr = if map_flags & flags::MAP_FIXED != 0 {
            // Fixed address - must be page aligned
            if addr & 0xFFF != 0 {
                return Err(EINVAL);
            }
            if addr < self.mmap_min_addr || addr + len > self.mmap_max_addr {
                return Err(ENOMEM);
            }
            // Unmap any existing mappings in the range
            self.unmap_range(addr, len);
            addr
        } else if addr != 0 {
            // Hint address
            let aligned = (addr + 0xFFF) & !0xFFF;
            if self.find_free_region(aligned, len).is_some() {
                aligned
            } else {
                self.find_free_region(self.next_addr, len).ok_or(ENOMEM)?
            }
        } else {
            // No hint - find free region
            self.find_free_region(self.next_addr, len).ok_or(ENOMEM)?
        };

        // Create VMA
        let vma_flags = VmaFlags::from_prot_and_flags(prot, map_flags);
        let vma_type = if map_flags & flags::MAP_ANONYMOUS != 0 {
            VmaType::Anonymous
        } else {
            VmaType::FileBacked { fd, offset: offset as u64 }
        };

        let vma = VirtualMemoryArea {
            start: addr,
            end: addr + len,
            flags: vma_flags,
            vma_type,
        };

        self.vmas.insert(addr, vma);

        // Update next hint
        self.next_addr = addr + len;
        if self.next_addr > self.mmap_max_addr {
            self.next_addr = self.mmap_min_addr;
        }

        Ok(addr)
    }

    /// Perform munmap operation
    pub fn munmap(&mut self, addr: usize, len: usize) -> Result<(), c_int> {
        if addr & 0xFFF != 0 {
            return Err(EINVAL);
        }
        if len == 0 {
            return Err(EINVAL);
        }

        let len = (len + 0xFFF) & !0xFFF;
        self.unmap_range(addr, len);
        Ok(())
    }

    /// Perform mprotect operation
    pub fn mprotect(&mut self, addr: usize, len: usize, prot: i32) -> Result<(), c_int> {
        if addr & 0xFFF != 0 {
            return Err(EINVAL);
        }
        if len == 0 {
            return Ok(());
        }

        let len = (len + 0xFFF) & !0xFFF;
        let end = addr + len;

        // Find all VMAs that overlap with the range
        let overlapping: Vec<usize> = self.vmas.iter()
            .filter(|(_, vma)| vma.start < end && vma.end > addr)
            .map(|(&k, _)| k)
            .collect();

        if overlapping.is_empty() {
            return Err(ENOMEM);
        }

        // Update protection for each overlapping VMA
        for key in overlapping {
            if let Some(vma) = self.vmas.get_mut(&key) {
                vma.flags = VmaFlags::from_prot(prot);
            }
        }

        Ok(())
    }

    /// Perform mremap operation
    pub fn mremap(
        &mut self,
        old_addr: usize,
        old_size: usize,
        new_size: usize,
        mremap_flags: i32,
    ) -> Result<usize, c_int> {
        if old_addr & 0xFFF != 0 {
            return Err(EINVAL);
        }

        let old_size = (old_size + 0xFFF) & !0xFFF;
        let new_size = (new_size + 0xFFF) & !0xFFF;

        // Find the VMA
        let vma = self.vmas.get(&old_addr).ok_or(EINVAL)?;
        if vma.end != old_addr + old_size {
            return Err(EINVAL);
        }

        if new_size <= old_size {
            // Shrinking - just update the VMA
            if let Some(vma) = self.vmas.get_mut(&old_addr) {
                vma.end = old_addr + new_size;
            }
            return Ok(old_addr);
        }

        // Growing
        let can_extend = self.vmas.range((old_addr + old_size)..)
            .next()
            .map(|(_, next)| next.start >= old_addr + new_size)
            .unwrap_or(true);

        if can_extend {
            // Extend in place
            if let Some(vma) = self.vmas.get_mut(&old_addr) {
                vma.end = old_addr + new_size;
            }
            return Ok(old_addr);
        }

        // Need to move
        if mremap_flags & mremap::MREMAP_MAYMOVE == 0 {
            return Err(ENOMEM);
        }

        // Find new location
        let new_addr = self.find_free_region(self.next_addr, new_size).ok_or(ENOMEM)?;

        // Copy VMA to new location
        if let Some(mut vma) = self.vmas.remove(&old_addr) {
            vma.start = new_addr;
            vma.end = new_addr + new_size;
            self.vmas.insert(new_addr, vma);
        }

        self.next_addr = new_addr + new_size;

        Ok(new_addr)
    }

    /// Find a free region of given size
    fn find_free_region(&self, hint: usize, size: usize) -> Option<usize> {
        let mut addr = (hint + 0xFFF) & !0xFFF;

        // Try from hint
        loop {
            if addr + size > self.mmap_max_addr {
                break;
            }

            // Check if region is free
            let conflicts = self.vmas.iter()
                .any(|(_, vma)| vma.start < addr + size && vma.end > addr);

            if !conflicts {
                return Some(addr);
            }

            // Skip to end of conflicting VMA
            if let Some((_, vma)) = self.vmas.iter().find(|(_, vma)| vma.start < addr + size && vma.end > addr) {
                addr = (vma.end + 0xFFF) & !0xFFF;
            } else {
                addr += 0x1000;
            }
        }

        // Wrap around and try from minimum
        addr = self.mmap_min_addr;
        while addr < hint && addr + size <= self.mmap_max_addr {
            let conflicts = self.vmas.iter()
                .any(|(_, vma)| vma.start < addr + size && vma.end > addr);

            if !conflicts {
                return Some(addr);
            }

            if let Some((_, vma)) = self.vmas.iter().find(|(_, vma)| vma.start < addr + size && vma.end > addr) {
                addr = (vma.end + 0xFFF) & !0xFFF;
            } else {
                addr += 0x1000;
            }
        }

        None
    }

    /// Unmap a range, potentially splitting VMAs
    fn unmap_range(&mut self, addr: usize, len: usize) {
        let end = addr + len;

        // Collect VMAs to remove or modify
        let overlapping: Vec<(usize, VirtualMemoryArea)> = self.vmas.iter()
            .filter(|(_, vma)| vma.start < end && vma.end > addr)
            .map(|(&k, v)| (k, v.clone()))
            .collect();

        for (key, vma) in overlapping {
            self.vmas.remove(&key);

            // Re-add parts that weren't unmapped
            if vma.start < addr {
                // Keep the beginning
                let mut new_vma = vma.clone();
                new_vma.end = addr;
                self.vmas.insert(vma.start, new_vma);
            }
            if vma.end > end {
                // Keep the end
                let mut new_vma = vma.clone();
                new_vma.start = end;
                self.vmas.insert(end, new_vma);
            }
        }
    }

    /// Get VMA containing address
    pub fn find_vma(&self, addr: usize) -> Option<&VirtualMemoryArea> {
        self.vmas.iter()
            .find(|(_, vma)| vma.start <= addr && vma.end > addr)
            .map(|(_, vma)| vma)
    }
}

impl Default for MmapManager {
    fn default() -> Self {
        Self::new()
    }
}
