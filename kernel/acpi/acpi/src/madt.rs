//! MADT (Multiple APIC Description Table) parsing
//!
//! The MADT enumerates all interrupt controllers in the system.
//! We care about type 0 entries (Processor Local APIC) which list
//! each CPU's APIC ID and whether it's enabled.
//!
//! — SableWire: reading the silicon census

use crate::sdt::SdtHeader;

/// MADT signature
pub const MADT_SIGNATURE: [u8; 4] = *b"APIC";

/// MADT entry types we care about
pub const ENTRY_LOCAL_APIC: u8 = 0;
pub const ENTRY_IO_APIC: u8 = 1;
pub const ENTRY_LOCAL_APIC_NMI: u8 = 4;
pub const ENTRY_LOCAL_X2APIC: u8 = 9;

/// MADT fixed fields (after the common SDT header)
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct MadtFixed {
    pub header: SdtHeader,
    pub local_apic_address: u32,
    pub flags: u32,
}

/// Processor Local APIC entry (type 0)
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct MadtLocalApic {
    pub entry_type: u8,
    pub length: u8,
    pub acpi_processor_uid: u8,
    pub apic_id: u8,
    pub flags: u32,
}

impl MadtLocalApic {
    /// Check if this LAPIC entry represents a usable CPU
    /// Bit 0: Enabled — Bit 1: Online Capable (can be enabled at runtime)
    pub fn is_usable(&self) -> bool {
        (self.flags & 0x1) != 0 || (self.flags & 0x2) != 0
    }
}

/// Processor Local x2APIC entry (type 9)
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct MadtLocalX2Apic {
    pub entry_type: u8,
    pub length: u8,
    pub reserved: u16,
    pub x2apic_id: u32,
    pub flags: u32,
    pub acpi_processor_uid: u32,
}

impl MadtLocalX2Apic {
    pub fn is_usable(&self) -> bool {
        (self.flags & 0x1) != 0 || (self.flags & 0x2) != 0
    }
}

/// Parsed MADT entry (simplified for SMP boot)
#[derive(Debug, Clone, Copy)]
pub enum MadtEntry {
    LocalApic(MadtLocalApic),
    LocalX2Apic(MadtLocalX2Apic),
}

/// Maximum CPUs we'll parse from MADT
pub const MAX_MADT_CPUS: usize = 256;

/// Parse the MADT and extract Local APIC entries.
///
/// Returns the count of entries written to `out`.
///
/// # Safety
/// `madt_phys` must point to a valid MADT mapped through `phys_map_base`.
pub unsafe fn parse_madt(
    phys_map_base: u64,
    madt_phys: u64,
    out: &mut [MadtEntry; MAX_MADT_CPUS],
) -> usize {
    let madt_virt = (phys_map_base + madt_phys) as *const u8;
    // Safety: caller guarantees MADT is mapped through phys_map_base
    let fixed = unsafe { &*(madt_virt as *const MadtFixed) };

    if fixed.header.signature != MADT_SIGNATURE {
        return 0;
    }

    let total_len = fixed.header.length as usize;
    let fixed_size = core::mem::size_of::<MadtFixed>();

    // Walk variable-length entries after the fixed portion
    let mut offset = fixed_size;
    let mut count = 0;

    while offset + 2 <= total_len && count < MAX_MADT_CPUS {
        // Safety: offset + 2 <= total_len, within MADT bounds
        let entry_type = unsafe { *madt_virt.add(offset) };
        let entry_len = unsafe { *madt_virt.add(offset + 1) } as usize;

        if entry_len < 2 {
            break; // Corrupt entry — bail out
        }

        match entry_type {
            ENTRY_LOCAL_APIC if entry_len >= 8 => {
                // Safety: entry_len validated, unaligned read for packed ACPI struct
                let lapic = unsafe {
                    core::ptr::read_unaligned(madt_virt.add(offset) as *const MadtLocalApic)
                };
                if lapic.is_usable() {
                    out[count] = MadtEntry::LocalApic(lapic);
                    count += 1;
                }
            }
            ENTRY_LOCAL_X2APIC if entry_len >= 16 => {
                // Safety: entry_len validated, unaligned read for packed ACPI struct
                let x2apic = unsafe {
                    core::ptr::read_unaligned(madt_virt.add(offset) as *const MadtLocalX2Apic)
                };
                if x2apic.is_usable() {
                    out[count] = MadtEntry::LocalX2Apic(x2apic);
                    count += 1;
                }
            }
            _ => { /* Skip IO APIC, NMI, etc. — not needed for CPU discovery */ }
        }

        offset += entry_len;
    }

    count
}
