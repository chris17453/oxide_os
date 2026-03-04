//! EFI System Table — the root of all firmware interaction.
//! UEFI 2.10 spec Table 4.3. Every protocol, every service, every byte of
//! firmware data flows through this single pointer.
//!
//! — SableWire: the system table is the umbilical cord — cut it at exit_boot_services and you're on your own

use super::types::{Char16, EfiGuid};
use super::boot_services::EfiBootServices;
use super::runtime::EfiRuntimeServices;
use super::text::{SimpleTextInputProtocol, SimpleTextOutputProtocol};

/// UEFI table header — present at the top of every major table
/// — SableWire: signature + revision + checksum — the firmware's handshake
#[repr(C)]
pub struct EfiTableHeader {
    pub signature: u64,
    pub revision: u32,
    pub header_size: u32,
    pub crc32: u32,
    pub reserved: u32,
}

/// UEFI System Table — UEFI 2.10 Table 4.3
/// — SableWire: this is THE table — lose this pointer and you're flying blind
#[repr(C)]
pub struct EfiSystemTable {
    pub hdr: EfiTableHeader,
    pub firmware_vendor: *const Char16,
    pub firmware_revision: u32,
    pub console_in_handle: *mut core::ffi::c_void,
    pub con_in: *mut SimpleTextInputProtocol,
    pub console_out_handle: *mut core::ffi::c_void,
    pub con_out: *mut SimpleTextOutputProtocol,
    pub standard_error_handle: *mut core::ffi::c_void,
    pub std_err: *mut SimpleTextOutputProtocol,
    pub runtime_services: *mut EfiRuntimeServices,
    pub boot_services: *mut EfiBootServices,
    pub number_of_table_entries: usize,
    pub configuration_table: *const EfiConfigurationTable,
}

/// UEFI Configuration Table entry — a GUID + pointer pair
/// — SableWire: the firmware's key-value store for ACPI roots and other treasures
#[repr(C)]
pub struct EfiConfigurationTable {
    pub vendor_guid: EfiGuid,
    pub vendor_table: *const core::ffi::c_void,
}
