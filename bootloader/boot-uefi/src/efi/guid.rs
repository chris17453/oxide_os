//! Well-known UEFI GUIDs — the magic numbers that unlock firmware protocols.
//! Each one is a 128-bit skeleton key to a different piece of the firmware.
//!
//! — SableWire: memorize these and you can talk to any UEFI system. Forget them and you're blind.

use super::types::EfiGuid;

/// Graphics Output Protocol — the gateway to pixels
/// — NeonVale: without this GUID, the screen stays dark
pub const EFI_GRAPHICS_OUTPUT_PROTOCOL_GUID: EfiGuid = EfiGuid {
    data1: 0x9042A9DE,
    data2: 0x23DC,
    data3: 0x4A38,
    data4: [0x96, 0xFB, 0x7A, 0xDE, 0xD0, 0x80, 0x51, 0x6A],
};

/// Simple File System Protocol — FAT32 access for the ESP
/// — SableWire: the key to the kingdom of \EFI\OXIDE\
pub const EFI_SIMPLE_FILE_SYSTEM_PROTOCOL_GUID: EfiGuid = EfiGuid {
    data1: 0x0964E5B22,
    data2: 0x6459,
    data3: 0x11D2,
    data4: [0x8E, 0x39, 0x00, 0xA0, 0xC9, 0x69, 0x72, 0x3B],
};

/// File Info GUID — for querying file metadata
pub const EFI_FILE_INFO_ID: EfiGuid = EfiGuid {
    data1: 0x09576E92,
    data2: 0x6D3F,
    data3: 0x11D2,
    data4: [0x8E, 0x39, 0x00, 0xA0, 0xC9, 0x69, 0x72, 0x3B],
};

/// ACPI 2.0 Table GUID — the modern ACPI anchor
/// — SableWire: XSDP lives here — 64-bit pointers to the hardware description tables
pub const ACPI_20_TABLE_GUID: EfiGuid = EfiGuid {
    data1: 0x8868E871,
    data2: 0xE4F1,
    data3: 0x11D3,
    data4: [0xBC, 0x22, 0x00, 0x80, 0xC7, 0x3C, 0x88, 0x81],
};

/// ACPI 1.0 Table GUID — legacy fallback
/// — SableWire: RSDP lives here — the original 32-bit ACPI root pointer
pub const ACPI_TABLE_GUID: EfiGuid = EfiGuid {
    data1: 0xEB9D2D30,
    data2: 0x2D88,
    data3: 0x11D3,
    data4: [0x9A, 0x16, 0x00, 0x90, 0x27, 0x3F, 0xC1, 0x4D],
};
