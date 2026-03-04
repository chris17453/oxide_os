//! Core UEFI types — the raw primitives that everything else is built on.
//! Matching UEFI 2.10 spec Chapter 2 exactly, byte for byte.
//!
//! — SableWire: these types ARE the ABI contract with firmware — get one wrong and you triple-fault

/// Opaque handle to a UEFI object — firmware gives these out like candy, revokes them like debts
pub type EfiHandle = *mut core::ffi::c_void;

/// UEFI status code — zero is success, everything else is a flavor of pain
pub type EfiStatus = usize;

/// UEFI physical address — just a u64 with a fancier name
pub type EfiPhysicalAddress = u64;

/// UEFI virtual address
pub type EfiVirtualAddress = u64;

/// UEFI event handle
pub type EfiEvent = *mut core::ffi::c_void;

/// TPL (Task Priority Level)
pub type EfiTpl = usize;

/// UTF-16 character — because UEFI decided ASCII was too simple
pub type Char16 = u16;

/// 128-bit GUID — the firmware's favorite way to identify things
/// — SableWire: if you thought UUIDs were annoying in userspace, wait till you meet them in ring -2
#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct EfiGuid {
    pub data1: u32,
    pub data2: u16,
    pub data3: u16,
    pub data4: [u8; 8],
}

// ── Status codes ──
// — SableWire: UEFI error taxonomy — each one a unique flavor of "no"

/// Success — the one status code you actually want to see
pub const EFI_SUCCESS: EfiStatus = 0;

/// High bit mask for error codes — because UEFI uses the sign bit as an error flag
const ERROR_BIT: usize = 1usize << (core::mem::size_of::<usize>() * 8 - 1);

pub const EFI_LOAD_ERROR: EfiStatus = ERROR_BIT | 1;
pub const EFI_INVALID_PARAMETER: EfiStatus = ERROR_BIT | 2;
pub const EFI_UNSUPPORTED: EfiStatus = ERROR_BIT | 3;
pub const EFI_BAD_BUFFER_SIZE: EfiStatus = ERROR_BIT | 4;
pub const EFI_BUFFER_TOO_SMALL: EfiStatus = ERROR_BIT | 5;
pub const EFI_NOT_READY: EfiStatus = ERROR_BIT | 6;
pub const EFI_DEVICE_ERROR: EfiStatus = ERROR_BIT | 7;
pub const EFI_NOT_FOUND: EfiStatus = ERROR_BIT | 14;

/// Check if an EFI status represents an error
/// — SableWire: it's an error if the high bit is set — simple, elegant, infuriating
#[inline]
pub fn efi_error(status: EfiStatus) -> bool {
    (status & ERROR_BIT) != 0
}
