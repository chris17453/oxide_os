//! RSDP (Root System Description Pointer) parsing
//!
//! The RSDP is the entry point into the ACPI table hierarchy.
//! UEFI firmware provides its physical address via the EFI config tables.
//!
//! — SableWire: ACPI root anchor

/// RSDP signature: "RSD PTR "
const RSDP_SIGNATURE: [u8; 8] = *b"RSD PTR ";

/// RSDP v1 structure (ACPI 1.0)
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct RsdpV1 {
    pub signature: [u8; 8],
    pub checksum: u8,
    pub oem_id: [u8; 6],
    pub revision: u8,
    pub rsdt_address: u32,
}

/// RSDP v2 structure (ACPI 2.0+)
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct RsdpV2 {
    pub v1: RsdpV1,
    pub length: u32,
    pub xsdt_address: u64,
    pub extended_checksum: u8,
    pub reserved: [u8; 3],
}

/// Parsed RSDP information
#[derive(Debug, Clone, Copy)]
pub struct Rsdp {
    /// ACPI revision (0 = v1, 2+ = v2)
    pub revision: u8,
    /// Physical address of RSDT (32-bit, always present)
    pub rsdt_address: u32,
    /// Physical address of XSDT (64-bit, v2+ only, 0 if absent)
    pub xsdt_address: u64,
}

impl Rsdp {
    /// Parse the RSDP from a physical address mapped through phys_map_base.
    ///
    /// # Safety
    /// `virt_ptr` must point to a valid RSDP structure in accessible memory.
    pub unsafe fn parse(virt_ptr: *const u8) -> Option<Self> {
        // Safety: caller guarantees virt_ptr points to a valid RSDP
        let v1 = unsafe { &*(virt_ptr as *const RsdpV1) };

        if v1.signature != RSDP_SIGNATURE {
            return None;
        }

        // Validate v1 checksum (sum of first 20 bytes must be 0 mod 256)
        let mut sum: u8 = 0;
        for i in 0..20 {
            sum = sum.wrapping_add(unsafe { *virt_ptr.add(i) });
        }
        if sum != 0 {
            return None;
        }

        let revision = v1.revision;
        let rsdt_address = v1.rsdt_address;

        let xsdt_address = if revision >= 2 {
            // Safety: revision >= 2 means the v2 extension is present
            let v2 = unsafe { &*(virt_ptr as *const RsdpV2) };
            let len = v2.length as usize;
            let mut sum2: u8 = 0;
            for i in 0..len {
                sum2 = sum2.wrapping_add(unsafe { *virt_ptr.add(i) });
            }
            if sum2 != 0 {
                0 // Checksum failed — fall back to RSDT
            } else {
                v2.xsdt_address
            }
        } else {
            0
        };

        Some(Rsdp {
            revision,
            rsdt_address,
            xsdt_address,
        })
    }
}
