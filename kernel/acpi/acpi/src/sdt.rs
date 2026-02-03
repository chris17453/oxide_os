//! ACPI System Description Table (SDT) header and RSDT/XSDT traversal
//!
//! RSDT contains an array of 32-bit physical pointers to other SDTs.
//! XSDT contains an array of 64-bit physical pointers (preferred on x86_64).
//!
//! — SableWire: table-walking the firmware maze

/// Common SDT header (shared by all ACPI tables)
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct SdtHeader {
    pub signature: [u8; 4],
    pub length: u32,
    pub revision: u8,
    pub checksum: u8,
    pub oem_id: [u8; 6],
    pub oem_table_id: [u8; 8],
    pub oem_revision: u32,
    pub creator_id: u32,
    pub creator_revision: u32,
}

impl SdtHeader {
    /// Validate the table checksum
    ///
    /// # Safety
    /// The header must point to a contiguous region of `length` bytes.
    pub unsafe fn validate(&self) -> bool {
        let ptr = self as *const SdtHeader as *const u8;
        let len = self.length as usize;
        let mut sum: u8 = 0;
        for i in 0..len {
            // Safety: caller guarantees contiguous region of `length` bytes
            sum = sum.wrapping_add(unsafe { *ptr.add(i) });
        }
        sum == 0
    }
}

/// Find a table with the given 4-byte signature in the RSDT or XSDT.
///
/// `phys_map_base` is the virtual base at which all physical memory is
/// identity-mapped (e.g. 0xFFFF_8000_0000_0000).
///
/// # Safety
/// The RSDP and all referenced tables must be mapped through `phys_map_base`.
pub unsafe fn find_table(
    phys_map_base: u64,
    xsdt_phys: u64,
    rsdt_phys: u32,
    signature: &[u8; 4],
) -> Option<u64> {
    // Prefer XSDT (64-bit pointers)
    if xsdt_phys != 0 {
        // Safety: caller guarantees tables are mapped through phys_map_base
        if let Some(addr) = unsafe { find_in_xsdt(phys_map_base, xsdt_phys, signature) } {
            return Some(addr);
        }
    }

    // Fall back to RSDT (32-bit pointers)
    if rsdt_phys != 0 {
        return unsafe { find_in_rsdt(phys_map_base, rsdt_phys as u64, signature) };
    }

    None
}

unsafe fn find_in_xsdt(phys_map_base: u64, xsdt_phys: u64, signature: &[u8; 4]) -> Option<u64> {
    let xsdt_virt = (phys_map_base + xsdt_phys) as *const u8;
    // Safety: caller guarantees XSDT is mapped
    let header = unsafe { &*(xsdt_virt as *const SdtHeader) };

    if !unsafe { header.validate() } {
        return None;
    }

    // Entries start after the header (36 bytes), each is 8 bytes (u64)
    let header_size = core::mem::size_of::<SdtHeader>();
    let entries_size = (header.length as usize).saturating_sub(header_size);
    let num_entries = entries_size / 8;

    // Safety: pointer arithmetic within validated table bounds
    let entries_ptr = unsafe { xsdt_virt.add(header_size) } as *const u64;

    for i in 0..num_entries {
        // Safety: i < num_entries, within table bounds; unaligned read for packed table
        let entry_phys = unsafe { core::ptr::read_unaligned(entries_ptr.add(i)) };
        if entry_phys == 0 {
            continue;
        }

        let entry_virt = (phys_map_base + entry_phys) as *const SdtHeader;
        // Safety: entry is mapped through phys_map_base
        let entry_header = unsafe { &*entry_virt };

        if &entry_header.signature == signature {
            return Some(entry_phys);
        }
    }

    None
}

unsafe fn find_in_rsdt(phys_map_base: u64, rsdt_phys: u64, signature: &[u8; 4]) -> Option<u64> {
    let rsdt_virt = (phys_map_base + rsdt_phys) as *const u8;
    // Safety: caller guarantees RSDT is mapped
    let header = unsafe { &*(rsdt_virt as *const SdtHeader) };

    if !unsafe { header.validate() } {
        return None;
    }

    // Entries start after the header (36 bytes), each is 4 bytes (u32)
    let header_size = core::mem::size_of::<SdtHeader>();
    let entries_size = (header.length as usize).saturating_sub(header_size);
    let num_entries = entries_size / 4;

    // Safety: pointer arithmetic within validated table bounds
    let entries_ptr = unsafe { rsdt_virt.add(header_size) } as *const u32;

    for i in 0..num_entries {
        // Safety: i < num_entries, within table bounds
        let entry_phys = unsafe { core::ptr::read_unaligned(entries_ptr.add(i)) } as u64;
        if entry_phys == 0 {
            continue;
        }

        let entry_virt = (phys_map_base + entry_phys) as *const SdtHeader;
        // Safety: entry is mapped through phys_map_base
        let entry_header = unsafe { &*entry_virt };

        if &entry_header.signature == signature {
            return Some(entry_phys);
        }
    }

    None
}
