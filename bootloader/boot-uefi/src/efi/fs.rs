//! UEFI Simple File System & File Protocol — FAT32 access to the ESP.
//! UEFI 2.10 Chapter 13.
//!
//! — SableWire: the firmware's filesystem API — three function calls between you and a kernel image

use super::types::*;

/// File open modes
pub const EFI_FILE_MODE_READ: u64 = 0x0000000000000001;
pub const EFI_FILE_MODE_WRITE: u64 = 0x0000000000000002;
pub const EFI_FILE_MODE_CREATE: u64 = 0x8000000000000000;

/// File attributes
pub const EFI_FILE_READ_ONLY: u64 = 0x0000000000000001;
pub const EFI_FILE_DIRECTORY: u64 = 0x0000000000000010;

/// Simple File System Protocol — UEFI 2.10 Section 13.4
/// — SableWire: the gateway to FAT32 — one function: open_volume, that's it
#[repr(C)]
pub struct EfiSimpleFileSystemProtocol {
    /// Revision
    pub revision: u64,

    /// OpenVolume(This, *mut *mut EfiFileProtocol) -> Status
    pub open_volume: unsafe extern "efiapi" fn(
        *mut EfiSimpleFileSystemProtocol,
        *mut *mut EfiFileProtocol,
    ) -> EfiStatus,
}

/// File Protocol — UEFI 2.10 Section 13.5
/// — SableWire: the file handle — open, read, close, forget — the eternal lifecycle
#[repr(C)]
pub struct EfiFileProtocol {
    /// Revision
    pub revision: u64,

    /// Open(This, *mut *mut NewHandle, *const FileName, OpenMode, Attributes) -> Status
    pub open: unsafe extern "efiapi" fn(
        *mut EfiFileProtocol,
        *mut *mut EfiFileProtocol,
        *const Char16,
        u64,    // OpenMode
        u64,    // Attributes
    ) -> EfiStatus,

    /// Close(This) -> Status
    pub close: unsafe extern "efiapi" fn(*mut EfiFileProtocol) -> EfiStatus,

    /// Delete(This) -> Status
    pub delete: usize,

    /// Read(This, *mut BufferSize, *mut Buffer) -> Status
    pub read: unsafe extern "efiapi" fn(
        *mut EfiFileProtocol,
        *mut usize,
        *mut u8,
    ) -> EfiStatus,

    /// Write — not needed
    pub write: usize,

    /// GetPosition — not needed
    pub get_position: usize,

    /// SetPosition(This, Position) -> Status
    pub set_position: unsafe extern "efiapi" fn(
        *mut EfiFileProtocol,
        u64,
    ) -> EfiStatus,

    /// GetInfo(This, *const InfoType, *mut BufferSize, *mut Buffer) -> Status
    pub get_info: unsafe extern "efiapi" fn(
        *mut EfiFileProtocol,
        *const EfiGuid,
        *mut usize,
        *mut u8,
    ) -> EfiStatus,
}

/// File Info structure — returned by GetInfo with EFI_FILE_INFO_ID
/// — SableWire: the metadata payload — variable length because the filename trails
#[repr(C)]
pub struct EfiFileInfo {
    pub size: u64,
    pub file_size: u64,
    pub physical_size: u64,
    pub create_time: EfiTime,
    pub last_access_time: EfiTime,
    pub modification_time: EfiTime,
    pub attribute: u64,
    // file_name: [Char16] follows — variable length, null-terminated
}

/// UEFI Time structure
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct EfiTime {
    pub year: u16,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
    pub pad1: u8,
    pub nanosecond: u32,
    pub time_zone: i16,
    pub daylight: u8,
    pub pad2: u8,
}
