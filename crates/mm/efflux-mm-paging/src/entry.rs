//! Page table entry

use bitflags::bitflags;
use efflux_core::PhysAddr;

bitflags! {
    /// Page table entry flags
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct PageTableFlags: u64 {
        /// Page is present in memory
        const PRESENT = 1 << 0;
        /// Page is writable
        const WRITABLE = 1 << 1;
        /// Page is accessible from user mode
        const USER = 1 << 2;
        /// Write-through caching
        const WRITE_THROUGH = 1 << 3;
        /// Disable caching
        const NO_CACHE = 1 << 4;
        /// Page has been accessed
        const ACCESSED = 1 << 5;
        /// Page has been written to (dirty)
        const DIRTY = 1 << 6;
        /// Huge page (2MB at PD level, 1GB at PDPT level)
        const HUGE_PAGE = 1 << 7;
        /// Global page (not flushed on CR3 switch)
        const GLOBAL = 1 << 8;
        /// No execute (requires NX bit in EFER)
        const NO_EXECUTE = 1 << 63;
    }
}

/// Mask for the physical address portion of an entry
const ADDR_MASK: u64 = 0x000F_FFFF_FFFF_F000;

/// A page table entry (64-bit)
#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct PageTableEntry(u64);

impl PageTableEntry {
    /// Create an empty (not present) entry
    #[inline]
    pub const fn empty() -> Self {
        Self(0)
    }

    /// Create an entry pointing to a frame with given flags
    #[inline]
    pub const fn new(addr: PhysAddr, flags: PageTableFlags) -> Self {
        Self((addr.as_u64() & ADDR_MASK) | flags.bits())
    }

    /// Get the raw entry value
    #[inline]
    pub const fn raw(&self) -> u64 {
        self.0
    }

    /// Check if entry is present
    #[inline]
    pub const fn is_present(&self) -> bool {
        self.0 & PageTableFlags::PRESENT.bits() != 0
    }

    /// Check if entry is a huge page
    #[inline]
    pub const fn is_huge(&self) -> bool {
        self.0 & PageTableFlags::HUGE_PAGE.bits() != 0
    }

    /// Get the flags
    #[inline]
    pub const fn flags(&self) -> PageTableFlags {
        PageTableFlags::from_bits_truncate(self.0)
    }

    /// Get the physical address this entry points to
    #[inline]
    pub const fn addr(&self) -> PhysAddr {
        PhysAddr::new(self.0 & ADDR_MASK)
    }

    /// Set the entry to a new address and flags
    #[inline]
    pub fn set(&mut self, addr: PhysAddr, flags: PageTableFlags) {
        self.0 = (addr.as_u64() & ADDR_MASK) | flags.bits();
    }

    /// Clear the entry
    #[inline]
    pub fn clear(&mut self) {
        self.0 = 0;
    }

    /// Set flags (preserving address)
    #[inline]
    pub fn set_flags(&mut self, flags: PageTableFlags) {
        self.0 = (self.0 & ADDR_MASK) | flags.bits();
    }

    /// Add flags to existing flags
    #[inline]
    pub fn add_flags(&mut self, flags: PageTableFlags) {
        self.0 |= flags.bits();
    }

    /// Remove flags from existing flags
    #[inline]
    pub fn remove_flags(&mut self, flags: PageTableFlags) {
        self.0 &= !flags.bits();
    }
}

impl core::fmt::Debug for PageTableEntry {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("PageTableEntry")
            .field("addr", &self.addr())
            .field("flags", &self.flags())
            .finish()
    }
}
