//! UEFI Graphics Output Protocol — the path to pixels.
//! UEFI 2.10 Section 12.9.
//!
//! — NeonVale: every pixel on the boot menu flows through these structs

use super::types::*;

/// Pixel format — how the firmware stores colors in the framebuffer
#[repr(u32)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum EfiGraphicsPixelFormat {
    PixelRedGreenBlueReserved8BitPerColor = 0,
    PixelBlueGreenRedReserved8BitPerColor = 1,
    PixelBitMask = 2,
    PixelBltOnly = 3,
}

/// GOP mode information — what we're actually looking at
/// — NeonVale: the vital stats of the current display mode
#[repr(C)]
pub struct EfiGraphicsOutputModeInformation {
    pub version: u32,
    pub horizontal_resolution: u32,
    pub vertical_resolution: u32,
    pub pixel_format: EfiGraphicsPixelFormat,
    pub pixel_information: EfiPixelBitmask,
    pub pixels_per_scan_line: u32,
}

/// Pixel bitmask for custom pixel formats (rarely used, but spec requires it)
#[repr(C)]
#[derive(Clone, Copy)]
pub struct EfiPixelBitmask {
    pub red_mask: u32,
    pub green_mask: u32,
    pub blue_mask: u32,
    pub reserved_mask: u32,
}

/// GOP mode structure — pointer to mode info + framebuffer base
#[repr(C)]
pub struct EfiGraphicsOutputProtocolMode {
    pub max_mode: u32,
    pub mode: u32,
    pub info: *const EfiGraphicsOutputModeInformation,
    pub size_of_info: usize,
    pub frame_buffer_base: EfiPhysicalAddress,
    pub frame_buffer_size: usize,
}

/// BLT pixel — 32-bit BGRX color
/// — NeonVale: the atomic unit of boot menu aesthetics
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct EfiBltPixel {
    pub blue: u8,
    pub green: u8,
    pub red: u8,
    pub reserved: u8,
}

impl EfiBltPixel {
    /// Create a new pixel from RGB values
    /// — NeonVale: the pixel constructor — deceptively simple for something that paints the void
    pub const fn new(red: u8, green: u8, blue: u8) -> Self {
        Self { blue, green, red, reserved: 0 }
    }
}

/// BLT operations — what to do with the pixels
#[repr(u32)]
#[derive(Clone, Copy)]
pub enum EfiBltOperation {
    BltVideoFill = 0,
    BltVideoToBltBuffer = 1,
    BltBufferToVideo = 2,
    BltVideoToVideo = 3,
}

/// Graphics Output Protocol — UEFI 2.10 Section 12.9
/// — NeonVale: the protocol that makes pixels happen
#[repr(C)]
pub struct EfiGraphicsOutputProtocol {
    /// QueryMode(This, ModeNumber, *mut SizeOfInfo, *mut *const ModeInfo) -> Status
    pub query_mode: unsafe extern "efiapi" fn(
        *mut EfiGraphicsOutputProtocol,
        u32,                                                // ModeNumber
        *mut usize,                                          // SizeOfInfo
        *mut *const EfiGraphicsOutputModeInformation,        // Info
    ) -> EfiStatus,

    /// SetMode(This, ModeNumber) -> Status
    pub set_mode: unsafe extern "efiapi" fn(
        *mut EfiGraphicsOutputProtocol,
        u32,
    ) -> EfiStatus,

    /// Blt(This, BltBuffer, BltOperation, SrcX, SrcY, DstX, DstY, Width, Height, Delta) -> Status
    pub blt: unsafe extern "efiapi" fn(
        *mut EfiGraphicsOutputProtocol,
        *const EfiBltPixel,     // BltBuffer (source for Fill, dest for read)
        EfiBltOperation,        // BltOperation
        usize,                  // SourceX
        usize,                  // SourceY
        usize,                  // DestinationX
        usize,                  // DestinationY
        usize,                  // Width
        usize,                  // Height
        usize,                  // Delta (0 for fill/full-width)
    ) -> EfiStatus,

    /// Mode — pointer to current mode information
    pub mode: *mut EfiGraphicsOutputProtocolMode,
}
