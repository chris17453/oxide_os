//! OXIDE Paging
//!
//! Page table structures and mapping operations for x86_64.

#![no_std]

mod entry;
mod table;
mod mapper;

pub use entry::{PageTableEntry, PageTableFlags};
pub use table::PageTable;
#[cfg(target_arch = "x86_64")]
pub use mapper::{flush_tlb, flush_tlb_all, read_cr3, write_cr3, PageMapper, MapError};
#[cfg(not(target_arch = "x86_64"))]
pub use mapper::{PageMapper, MapError};

use os_core::{PhysAddr, VirtAddr};

/// Page size (4KB)
pub const PAGE_SIZE: usize = 4096;

/// Number of entries per page table
pub const ENTRIES_PER_TABLE: usize = 512;

/// Base of direct physical memory map
pub const PHYS_MAP_BASE: u64 = 0xFFFF_8000_0000_0000;

/// Convert a physical address to its direct-mapped virtual address
#[inline]
pub const fn phys_to_virt(phys: PhysAddr) -> VirtAddr {
    VirtAddr::new(phys.as_u64() + PHYS_MAP_BASE)
}

/// Convert a direct-mapped virtual address back to physical
#[inline]
pub const fn virt_to_phys(virt: VirtAddr) -> PhysAddr {
    PhysAddr::new(virt.as_u64() - PHYS_MAP_BASE)
}

/// Page table level (4 = PML4, 3 = PDPT, 2 = PD, 1 = PT)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageLevel {
    Pml4 = 4,
    Pdpt = 3,
    Pd = 2,
    Pt = 1,
}

impl PageLevel {
    /// Get the next lower level
    pub const fn next(self) -> Option<Self> {
        match self {
            Self::Pml4 => Some(Self::Pdpt),
            Self::Pdpt => Some(Self::Pd),
            Self::Pd => Some(Self::Pt),
            Self::Pt => None,
        }
    }

    /// Get the index into this level's table for an address
    pub const fn index(self, addr: VirtAddr) -> usize {
        let shift = 12 + (self as usize - 1) * 9;
        ((addr.as_u64() >> shift) & 0x1FF) as usize
    }
}
