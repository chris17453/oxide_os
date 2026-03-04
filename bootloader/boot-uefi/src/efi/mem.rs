//! UEFI Memory Descriptor — the firmware's memory map entry type.
//! UEFI 2.10 Section 7.2.
//!
//! — SableWire: each descriptor is a confession from the firmware about what it did with your RAM

use super::types::*;

/// UEFI Memory Descriptor — one entry in the firmware's memory map
/// — SableWire: the firmware's ledger — every page of RAM accounted for, allegedly
#[repr(C)]
#[derive(Clone, Copy)]
pub struct EfiMemoryDescriptor {
    /// Type of memory region
    pub memory_type: u32,
    /// Padding to align physical_start to 8 bytes
    pub _pad: u32,
    /// Physical address of the start of the region
    pub physical_start: EfiPhysicalAddress,
    /// Virtual address of the start of the region
    pub virtual_start: EfiVirtualAddress,
    /// Number of 4KB pages in the region
    pub number_of_pages: u64,
    /// Attribute bits (cacheability, etc.)
    pub attribute: u64,
}
