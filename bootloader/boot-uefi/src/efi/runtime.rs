//! EFI Runtime Services — the subset of firmware that survives exit_boot_services.
//! We only need ResetSystem for the console `reboot` command.
//!
//! — SableWire: the firmware's afterlife — these functions work even after the bridge burns

use super::types::*;
use super::system_table::EfiTableHeader;

/// Reset type — how hard do you want to reboot?
/// — WireSaint: cold = full power cycle, warm = fast restart, shutdown = lights out
pub const EFI_RESET_COLD: u32 = 0;
pub const EFI_RESET_WARM: u32 = 1;
pub const EFI_RESET_SHUTDOWN: u32 = 2;

/// EFI Runtime Services Table — UEFI 2.10 Table 4.5
/// — SableWire: only the functions that survive the apocalypse
#[repr(C)]
pub struct EfiRuntimeServices {
    pub hdr: EfiTableHeader,

    // ── Time Services ──
    pub get_time: usize,
    pub set_time: usize,
    pub get_wakeup_time: usize,
    pub set_wakeup_time: usize,

    // ── Virtual Memory Services ──
    pub set_virtual_address_map: usize,
    pub convert_pointer: usize,

    // ── Variable Services ──
    pub get_variable: usize,
    pub get_next_variable_name: usize,
    pub set_variable: usize,

    // ── Miscellaneous Services ──
    pub get_next_high_monotonic_count: usize,

    /// ResetSystem(ResetType, Status, DataSize, *const ResetData)
    pub reset_system: unsafe extern "efiapi" fn(
        u32,                        // ResetType
        EfiStatus,                  // ResetStatus
        usize,                      // DataSize
        *const core::ffi::c_void,   // ResetData (optional)
    ) -> !,
}
