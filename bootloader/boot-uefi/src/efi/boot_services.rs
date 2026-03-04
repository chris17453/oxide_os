//! EFI Boot Services Table — the firmware's API surface, available only until
//! exit_boot_services burns the bridge.
//!
//! UEFI 2.10 Table 4.4 — ~44 function pointers, most of which we'll never call.
//! Unused slots are `usize` padding to keep the offsets correct.
//!
//! — SableWire: this is the firmware's gift shop — browse carefully, because after
//! exit_boot_services these pointers become landmines

use super::types::*;
use super::system_table::EfiTableHeader;
use super::mem::EfiMemoryDescriptor;

/// Memory allocation type — where to put the pages
/// — SableWire: AllocateAnyPages is the "dealer's choice" of memory allocation
pub const ALLOCATE_ANY_PAGES: u32 = 0;
pub const ALLOCATE_MAX_ADDRESS: u32 = 1;
pub const ALLOCATE_ADDRESS: u32 = 2;

/// UEFI Memory type constants
/// — SableWire: the firmware's memory taxonomy — each type has different post-ExitBootServices rules
pub const EFI_RESERVED_MEMORY_TYPE: u32 = 0;
pub const EFI_LOADER_CODE: u32 = 1;
pub const EFI_LOADER_DATA: u32 = 2;
pub const EFI_BOOT_SERVICES_CODE: u32 = 3;
pub const EFI_BOOT_SERVICES_DATA: u32 = 4;
pub const EFI_RUNTIME_SERVICES_CODE: u32 = 5;
pub const EFI_RUNTIME_SERVICES_DATA: u32 = 6;
pub const EFI_CONVENTIONAL_MEMORY: u32 = 7;
pub const EFI_UNUSABLE_MEMORY: u32 = 8;
pub const EFI_ACPI_RECLAIM_MEMORY: u32 = 9;
pub const EFI_ACPI_MEMORY_NVS: u32 = 10;
pub const EFI_MEMORY_MAPPED_IO: u32 = 11;
pub const EFI_MEMORY_MAPPED_IO_PORT_SPACE: u32 = 12;
pub const EFI_PAL_CODE: u32 = 13;
pub const EFI_PERSISTENT_MEMORY: u32 = 14;

/// Locate search type for locate_handle
pub const BY_PROTOCOL: u32 = 2;

/// EFI Boot Services Table — UEFI 2.10 Table 4.4
/// — SableWire: the mother of all vtables — 44 function pointers that ARE the firmware API
///
/// Function pointer layout matches the UEFI spec exactly.
/// Unused functions are typed as `usize` to maintain correct struct offsets.
#[repr(C)]
pub struct EfiBootServices {
    pub hdr: EfiTableHeader,

    // ── Task Priority Services ──
    pub raise_tpl: usize,
    pub restore_tpl: usize,

    // ── Memory Services ──
    /// AllocatePages(Type, MemoryType, Pages, *mut PhysicalAddress) -> Status
    pub allocate_pages: unsafe extern "efiapi" fn(
        u32,            // AllocateType
        u32,            // MemoryType
        usize,          // Pages
        *mut EfiPhysicalAddress,
    ) -> EfiStatus,

    /// FreePages(Memory, Pages) -> Status
    pub free_pages: unsafe extern "efiapi" fn(EfiPhysicalAddress, usize) -> EfiStatus,

    /// GetMemoryMap(*mut MapSize, *mut MemDesc, *mut MapKey, *mut DescSize, *mut DescVersion) -> Status
    pub get_memory_map: unsafe extern "efiapi" fn(
        *mut usize,                 // MemoryMapSize
        *mut EfiMemoryDescriptor,   // MemoryMap buffer
        *mut usize,                 // MapKey
        *mut usize,                 // DescriptorSize
        *mut u32,                   // DescriptorVersion
    ) -> EfiStatus,

    /// AllocatePool(PoolType, Size, *mut *mut u8) -> Status
    pub allocate_pool: usize,

    /// FreePool(*mut u8) -> Status
    pub free_pool: usize,

    // ── Event & Timer Services ──
    pub create_event: usize,
    pub set_timer: usize,

    /// WaitForEvent(NumberOfEvents, *const Event, *mut usize) -> Status
    pub wait_for_event: usize,

    pub signal_event: usize,
    pub close_event: usize,
    pub check_event: usize,

    // ── Protocol Handler Services ──
    pub install_protocol_interface: usize,
    pub reinstall_protocol_interface: usize,
    pub uninstall_protocol_interface: usize,

    /// HandleProtocol(Handle, *const Guid, *mut *mut c_void) -> Status
    pub handle_protocol: unsafe extern "efiapi" fn(
        EfiHandle,
        *const EfiGuid,
        *mut *mut core::ffi::c_void,
    ) -> EfiStatus,

    pub reserved: usize,
    pub register_protocol_notify: usize,

    /// LocateHandle(SearchType, *const Guid, SearchKey, *mut BufSize, *mut Handle) -> Status
    pub locate_handle: unsafe extern "efiapi" fn(
        u32,                        // SearchType
        *const EfiGuid,             // Protocol (optional)
        *mut core::ffi::c_void,     // SearchKey (optional)
        *mut usize,                 // BufferSize
        *mut EfiHandle,             // Buffer
    ) -> EfiStatus,

    pub locate_device_path: usize,
    pub install_configuration_table: usize,

    // ── Image Services ──
    pub load_image: usize,
    pub start_image: usize,
    pub exit: usize,
    pub unload_image: usize,

    /// ExitBootServices(ImageHandle, MapKey) -> Status
    pub exit_boot_services: unsafe extern "efiapi" fn(
        EfiHandle,  // ImageHandle
        usize,      // MapKey
    ) -> EfiStatus,

    // ── Miscellaneous Services ──
    pub get_next_monotonic_count: usize,

    /// Stall(Microseconds) -> Status
    pub stall: unsafe extern "efiapi" fn(usize) -> EfiStatus,

    pub set_watchdog_timer: usize,

    // ── DriverSupport Services ──
    pub connect_controller: usize,
    pub disconnect_controller: usize,

    // ── Open and Close Protocol Services ──
    /// OpenProtocol(Handle, *const Guid, *mut *mut c_void, AgentHandle, ControllerHandle, Attributes) -> Status
    pub open_protocol: unsafe extern "efiapi" fn(
        EfiHandle,
        *const EfiGuid,
        *mut *mut core::ffi::c_void,
        EfiHandle,
        EfiHandle,
        u32,
    ) -> EfiStatus,

    pub close_protocol: usize,
    pub open_protocol_information: usize,

    // ── Library Services ──
    pub protocols_per_handle: usize,
    pub locate_handle_buffer: usize,

    /// LocateProtocol(*const Guid, Registration, *mut *mut c_void) -> Status
    pub locate_protocol: unsafe extern "efiapi" fn(
        *const EfiGuid,
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> EfiStatus,

    pub install_multiple_protocol_interfaces: usize,
    pub uninstall_multiple_protocol_interfaces: usize,

    // ── 32-bit CRC Services ──
    pub calculate_crc32: usize,

    // ── Miscellaneous Services (cont.) ──
    pub copy_mem: usize,
    pub set_mem: usize,
    pub create_event_ex: usize,
}

/// Open protocol attributes
pub const EFI_OPEN_PROTOCOL_BY_HANDLE_PROTOCOL: u32 = 0x00000001;
pub const EFI_OPEN_PROTOCOL_GET_PROTOCOL: u32 = 0x00000002;
pub const EFI_OPEN_PROTOCOL_EXCLUSIVE: u32 = 0x00000020;
