//! UEFI Simple Text I/O protocols — console input and output before we have a real terminal.
//! UEFI 2.10 Chapter 12.
//!
//! — InputShade: the keyboard driver that predates your kernel's keyboard driver

use super::types::*;

/// EFI Input Key — raw scan code + unicode character pair
/// — InputShade: the firmware's idea of a keystroke — two fields, maximum confusion
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct EfiInputKey {
    /// Scan code for special keys (arrows, function keys, etc.)
    /// Zero if this is a regular character
    pub scan_code: u16,
    /// Unicode character value
    /// Zero if this is a special key
    pub unicode_char: Char16,
}

// ── Scan code constants ──
// — InputShade: the numeric names for the keys that don't have letters

pub const SCAN_NULL: u16 = 0x00;
pub const SCAN_UP: u16 = 0x01;
pub const SCAN_DOWN: u16 = 0x02;
pub const SCAN_RIGHT: u16 = 0x03;
pub const SCAN_LEFT: u16 = 0x04;
pub const SCAN_HOME: u16 = 0x05;
pub const SCAN_END: u16 = 0x06;
pub const SCAN_INSERT: u16 = 0x07;
pub const SCAN_DELETE: u16 = 0x08;
pub const SCAN_PAGE_UP: u16 = 0x09;
pub const SCAN_PAGE_DOWN: u16 = 0x0A;
pub const SCAN_F1: u16 = 0x0B;
pub const SCAN_F2: u16 = 0x0C;
pub const SCAN_F10: u16 = 0x14;
pub const SCAN_ESC: u16 = 0x17;

/// Simple Text Input Protocol — UEFI 2.10 Section 12.3
/// — InputShade: the firmware's stdin — WaitForKey blocks, ReadKeyStroke polls
#[repr(C)]
pub struct SimpleTextInputProtocol {
    /// Reset(This, ExtendedVerification) -> Status
    pub reset: unsafe extern "efiapi" fn(
        *mut SimpleTextInputProtocol,
        bool,
    ) -> EfiStatus,

    /// ReadKeyStroke(This, *mut Key) -> Status
    pub read_key_stroke: unsafe extern "efiapi" fn(
        *mut SimpleTextInputProtocol,
        *mut EfiInputKey,
    ) -> EfiStatus,

    /// WaitForKey event handle
    pub wait_for_key: EfiEvent,
}

/// Simple Text Output Protocol — UEFI 2.10 Section 12.4
/// — NeonVale: the firmware's stdout — unicode strings only, no raw bytes
#[repr(C)]
pub struct SimpleTextOutputProtocol {
    /// Reset(This, ExtendedVerification) -> Status
    pub reset: unsafe extern "efiapi" fn(
        *mut SimpleTextOutputProtocol,
        bool,
    ) -> EfiStatus,

    /// OutputString(This, *const Char16) -> Status
    pub output_string: unsafe extern "efiapi" fn(
        *mut SimpleTextOutputProtocol,
        *const Char16,
    ) -> EfiStatus,

    /// TestString(This, *const Char16) -> Status
    pub test_string: usize,

    /// QueryMode(This, ModeNumber, *mut Columns, *mut Rows) -> Status
    pub query_mode: usize,

    /// SetMode(This, ModeNumber) -> Status
    pub set_mode: usize,

    /// SetAttribute(This, Attribute) -> Status
    pub set_attribute: usize,

    /// ClearScreen(This) -> Status
    pub clear_screen: unsafe extern "efiapi" fn(
        *mut SimpleTextOutputProtocol,
    ) -> EfiStatus,

    /// SetCursorPosition(This, Column, Row) -> Status
    pub set_cursor_position: usize,

    /// EnableCursor(This, Visible) -> Status
    pub enable_cursor: usize,

    /// Mode pointer
    pub mode: *mut core::ffi::c_void,
}
