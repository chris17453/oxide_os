//! Virtual Memory Area tracking for OXIDE OS
//!
//! — NeonRoot: Finally, the kernel knows what lives where. No more blind page
//! table walks hoping for the best. Every mapped region gets a VMA or it doesn't
//! exist. Binary search, sorted invariant, no overlaps. The way Torvalds intended.

#![no_std]

extern crate alloc;

use alloc::vec::Vec;
use bitflags::bitflags;

bitflags! {
    /// — NeonRoot: Protection and behavior flags for virtual memory regions.
    /// One bitfield to describe what you can do and what happens when you try.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct VmFlags: u32 {
        const READ      = 1 << 0;
        const WRITE     = 1 << 1;
        const EXEC      = 1 << 2;
        const SHARED    = 1 << 3;
        const GROWSDOWN = 1 << 4;
        const STACK     = 1 << 5;
        const DONTCOPY  = 1 << 6;
    }
}

/// — NeonRoot: What kind of mapping this is. The page tables don't care,
/// but procfs, fault handlers, and your sanity sure do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VmType {
    Text,
    Data,
    Bss,
    Stack,
    Heap,
    Anon,
    FileBacked,
    Tls,
    Guard,
}

/// — NeonRoot: A single contiguous virtual memory region.
/// Fixed-size name avoids heap alloc per VMA. 32 bytes is enough for
/// "[stack]", "[heap]", "/bin/sh", and whatever else you throw at it.
#[derive(Debug, Clone)]
pub struct VmArea {
    /// Page-aligned start address (inclusive)
    pub start: u64,
    /// Page-aligned end address (exclusive)
    pub end: u64,
    /// Protection and behavior flags
    pub flags: VmFlags,
    /// Region type — semantic classification for fault handling and /proc
    pub vm_type: VmType,
    /// Fixed-size name — no heap alloc per VMA
    name: [u8; 32],
    /// Length of the name within the fixed buffer
    name_len: u8,
}

impl VmArea {
    /// Create a new VMA with no name.
    ///
    /// — NeonRoot: Callers must ensure start < end and both page-aligned.
    /// We don't panic on violation — bad VMA metadata is annoying but not
    /// fatal. The page tables are the real authority. Log and carry on.
    pub fn new(start: u64, end: u64, flags: VmFlags, vm_type: VmType) -> Self {
        Self {
            start,
            end,
            flags,
            vm_type,
            name: [0u8; 32],
            name_len: 0,
        }
    }

    /// Create a new VMA with a name (e.g., "[stack]", "[heap]")
    pub fn new_named(start: u64, end: u64, flags: VmFlags, vm_type: VmType, label: &[u8]) -> Self {
        let mut vma = Self::new(start, end, flags, vm_type);
        let copy_len = label.len().min(32);
        vma.name[..copy_len].copy_from_slice(&label[..copy_len]);
        vma.name_len = copy_len as u8;
        vma
    }

    /// Get the name as a byte slice
    pub fn name(&self) -> &[u8] {
        &self.name[..self.name_len as usize]
    }

    /// Size in bytes
    pub fn size(&self) -> u64 {
        self.end - self.start
    }

    /// Check if an address falls within this VMA
    pub fn contains(&self, addr: u64) -> bool {
        addr >= self.start && addr < self.end
    }

    /// Check if this VMA overlaps with a given range [start, end)
    pub fn overlaps(&self, start: u64, end: u64) -> bool {
        self.start < end && start < self.end
    }
}

/// — NeonRoot: Errors from VMA operations. Overlap is the big one —
/// two VMAs claiming the same address is the memory subsystem equivalent
/// of two people showing up to the same apartment with different keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VmAreaError {
    /// Requested region overlaps with an existing VMA
    Overlap,
    /// Region not found
    NotFound,
    /// Invalid arguments (e.g., start >= end, not page-aligned)
    InvalidArgs,
    /// No free region of the requested size found
    NoSpace,
}

/// — NeonRoot: Sorted, non-overlapping list of VMAs for a process.
/// Vec with binary search — O(log n) find, O(n) insert. Typical process
/// has 5-15 VMAs. Better cache locality than a tree at this scale. When
/// we hit 200+ VMAs, revisit. Until then, KISS wins.
#[derive(Debug, Clone)]
pub struct VmAreaList {
    /// VMAs sorted by start address, guaranteed non-overlapping
    areas: Vec<VmArea>,
}

impl VmAreaList {
    /// Create an empty VMA list
    pub fn new() -> Self {
        Self {
            areas: Vec::new(),
        }
    }

    /// Find the VMA containing a given address — binary search, O(log n)
    pub fn find(&self, addr: u64) -> Option<&VmArea> {
        // — NeonRoot: Binary search for the VMA whose range includes addr.
        // We search for the rightmost VMA with start <= addr, then check
        // if addr < end. Classic interval search on a sorted array.
        let idx = match self.areas.binary_search_by(|vma| {
            if addr < vma.start {
                core::cmp::Ordering::Greater
            } else if addr >= vma.end {
                core::cmp::Ordering::Less
            } else {
                core::cmp::Ordering::Equal
            }
        }) {
            Ok(i) => i,
            Err(_) => return None,
        };
        Some(&self.areas[idx])
    }

    /// Find mutable reference to VMA containing a given address
    pub fn find_mut(&mut self, addr: u64) -> Option<&mut VmArea> {
        let idx = match self.areas.binary_search_by(|vma| {
            if addr < vma.start {
                core::cmp::Ordering::Greater
            } else if addr >= vma.end {
                core::cmp::Ordering::Less
            } else {
                core::cmp::Ordering::Equal
            }
        }) {
            Ok(i) => i,
            Err(_) => return None,
        };
        Some(&mut self.areas[idx])
    }

    /// Insert a VMA, maintaining sorted order and no-overlap invariant.
    /// Returns Err(Overlap) if the new VMA conflicts with an existing one.
    /// Zero-size VMAs (start >= end) are silently rejected — not an error,
    /// just nothing to track (e.g., empty heap before first brk call).
    pub fn insert(&mut self, vma: VmArea) -> Result<(), VmAreaError> {
        if vma.start >= vma.end {
            return Ok(()); // — NeonRoot: Nothing to insert. Move along.
        }
        if vma.start & 0xFFF != 0 || vma.end & 0xFFF != 0 {
            return Err(VmAreaError::InvalidArgs);
        }

        // — NeonRoot: Find insertion point. Binary search for first VMA
        // with start > vma.start. Then check neighbors for overlap.
        let pos = self.areas.partition_point(|a| a.start < vma.start);

        // Check overlap with predecessor (if any)
        if pos > 0 && self.areas[pos - 1].end > vma.start {
            return Err(VmAreaError::Overlap);
        }

        // Check overlap with successor (if any)
        if pos < self.areas.len() && vma.end > self.areas[pos].start {
            return Err(VmAreaError::Overlap);
        }

        self.areas.insert(pos, vma);

        #[cfg(feature = "debug-vma")]
        {
            let inserted = &self.areas[pos];
            unsafe {
                os_log::write_str_raw("[VMA] insert ");
                os_log::write_u64_hex_raw(inserted.start);
                os_log::write_str_raw("-");
                os_log::write_u64_hex_raw(inserted.end);
                os_log::write_str_raw("\n");
            }
        }

        Ok(())
    }

    /// Remove all VMAs fully or partially within [start, end).
    /// Handles splitting: if a VMA is partially covered, the uncovered
    /// portion(s) remain. Returns the removed/trimmed VMAs.
    pub fn remove(&mut self, start: u64, end: u64) -> Result<Vec<VmArea>, VmAreaError> {
        if start >= end || start & 0xFFF != 0 || end & 0xFFF != 0 {
            return Err(VmAreaError::InvalidArgs);
        }

        let mut removed = Vec::new();
        let mut new_areas = Vec::new();

        for vma in self.areas.drain(..) {
            if vma.end <= start || vma.start >= end {
                // — NeonRoot: Completely outside the removal range. Keep it.
                new_areas.push(vma);
            } else if vma.start >= start && vma.end <= end {
                // — NeonRoot: Completely inside. Remove entirely.
                removed.push(vma);
            } else if vma.start < start && vma.end > end {
                // — NeonRoot: VMA straddles both sides — split into two.
                // Left remnant: [vma.start, start)
                // Right remnant: [end, vma.end)
                let left = VmArea {
                    start: vma.start,
                    end: start,
                    flags: vma.flags,
                    vm_type: vma.vm_type,
                    name: vma.name,
                    name_len: vma.name_len,
                };
                let right = VmArea {
                    start: end,
                    end: vma.end,
                    flags: vma.flags,
                    vm_type: vma.vm_type,
                    name: vma.name,
                    name_len: vma.name_len,
                };
                removed.push(VmArea {
                    start,
                    end,
                    flags: vma.flags,
                    vm_type: vma.vm_type,
                    name: vma.name,
                    name_len: vma.name_len,
                });
                new_areas.push(left);
                new_areas.push(right);
            } else if vma.start < start {
                // — NeonRoot: Overlaps on the left — trim the tail.
                let trimmed = VmArea {
                    start,
                    end: vma.end.min(end),
                    flags: vma.flags,
                    vm_type: vma.vm_type,
                    name: vma.name,
                    name_len: vma.name_len,
                };
                removed.push(trimmed);
                new_areas.push(VmArea {
                    start: vma.start,
                    end: start,
                    flags: vma.flags,
                    vm_type: vma.vm_type,
                    name: vma.name,
                    name_len: vma.name_len,
                });
            } else {
                // vma.end > end — overlaps on the right — trim the head.
                let trimmed = VmArea {
                    start: vma.start,
                    end: end.min(vma.end),
                    flags: vma.flags,
                    vm_type: vma.vm_type,
                    name: vma.name,
                    name_len: vma.name_len,
                };
                removed.push(trimmed);
                new_areas.push(VmArea {
                    start: end,
                    end: vma.end,
                    flags: vma.flags,
                    vm_type: vma.vm_type,
                    name: vma.name,
                    name_len: vma.name_len,
                });
            }
        }

        self.areas = new_areas;

        #[cfg(feature = "debug-vma")]
        unsafe {
            os_log::write_str_raw("[VMA] remove ");
            os_log::write_u64_hex_raw(start);
            os_log::write_str_raw("-");
            os_log::write_u64_hex_raw(end);
            os_log::write_str_raw(" removed=");
            os_log::write_u32_raw(removed.len() as u32);
            os_log::write_str_raw("\n");
        }

        Ok(removed)
    }

    /// Find a free region of at least `size` bytes, searching downward from `max`.
    /// Used by mmap(NULL) to find an unmapped address range.
    ///
    /// — NeonRoot: Scans gaps between VMAs from high to low. Returns the
    /// highest address that fits. This matches Linux's top-down mmap behavior
    /// — libraries load from the top of the address space downward.
    pub fn find_free_region(&self, size: u64, hint: u64, max: u64) -> Option<u64> {
        if size == 0 || size & 0xFFF != 0 {
            return None;
        }

        // — NeonRoot: If hint is provided, try it first.
        let hint_aligned = hint & !0xFFF;
        if hint_aligned != 0 && hint_aligned + size <= max {
            let hint_end = hint_aligned + size;
            let conflicts = self.areas.iter().any(|vma| vma.overlaps(hint_aligned, hint_end));
            if !conflicts {
                return Some(hint_aligned);
            }
        }

        // — NeonRoot: Top-down scan. Walk gaps between VMAs from high to low.
        // The gap between [last VMA end, max) is checked first, then gaps
        // between adjacent VMAs, then [min_addr, first VMA start).
        let min_addr: u64 = 0x1000; // MMAP_MIN_ADDR

        if self.areas.is_empty() {
            // — NeonRoot: No VMAs at all. The whole address space is free.
            if max >= size + min_addr {
                return Some((max - size) & !0xFFF);
            }
            return None;
        }

        // Check gap above the last VMA
        let last = &self.areas[self.areas.len() - 1];
        if last.end + size <= max {
            let candidate = (max - size) & !0xFFF;
            if candidate >= last.end {
                return Some(candidate);
            }
        }

        // Check gaps between VMAs (walking from top to bottom)
        for i in (1..self.areas.len()).rev() {
            let gap_start = self.areas[i - 1].end;
            let gap_end = self.areas[i].start;
            if gap_end - gap_start >= size {
                let candidate = (gap_end - size) & !0xFFF;
                if candidate >= gap_start {
                    return Some(candidate);
                }
            }
        }

        // Check gap below the first VMA
        let first = &self.areas[0];
        if first.start >= min_addr + size {
            let candidate = (first.start - size) & !0xFFF;
            if candidate >= min_addr {
                return Some(candidate);
            }
        }

        None
    }

    /// Clone all VMA metadata for fork — O(n) Vec clone.
    /// The page table walk handles the actual COW marking; this is metadata only.
    pub fn clone_for_fork(&self) -> Self {
        Self {
            areas: self.areas.clone(),
        }
    }

    /// Iterate over all VMAs
    pub fn iter(&self) -> impl Iterator<Item = &VmArea> {
        self.areas.iter()
    }

    /// Clear all VMAs (used during exec to wipe the old address space metadata)
    pub fn clear(&mut self) {
        self.areas.clear();
    }

    /// Number of VMAs
    pub fn len(&self) -> usize {
        self.areas.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.areas.is_empty()
    }
}

impl Default for VmAreaList {
    fn default() -> Self {
        Self::new()
    }
}
