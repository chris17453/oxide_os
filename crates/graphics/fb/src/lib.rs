//! Framebuffer Graphics for OXIDE OS
//!
//! Provides framebuffer abstraction, text console, and font rendering.

#![no_std]

extern crate alloc;

pub mod color;
pub mod framebuffer;
pub mod console;
pub mod font;

pub use color::{Color, PixelFormat};
pub use framebuffer::{Framebuffer, FramebufferInfo, LinearFramebuffer};
pub use console::{FbConsole, Cell};
pub use font::{Font, Glyph, PSF2_FONT};

use alloc::sync::Arc;
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use spin::Mutex;

/// Global framebuffer instance
static FRAMEBUFFER: Mutex<Option<Arc<dyn Framebuffer>>> = Mutex::new(None);

/// Global console instance
static CONSOLE: Mutex<Option<FbConsole>> = Mutex::new(None);

/// Initialization flag
static INITIALIZED: AtomicBool = AtomicBool::new(false);

/// Physical base address of framebuffer (for /dev/fb0)
static FB_PHYS_BASE: Mutex<u64> = Mutex::new(0);

/// Video mode storage
static VIDEO_MODES: Mutex<Option<boot_proto::VideoModeList>> = Mutex::new(None);

/// Current video mode index
static CURRENT_MODE: AtomicU32 = AtomicU32::new(0);

/// Initialize framebuffer from boot info
pub fn init(info: FramebufferInfo) {
    let fb = Arc::new(LinearFramebuffer::new(info));
    *FRAMEBUFFER.lock() = Some(fb.clone());

    // Initialize console
    let console = FbConsole::new(fb);
    *CONSOLE.lock() = Some(console);

    INITIALIZED.store(true, Ordering::SeqCst);
}

/// Initialize framebuffer from boot_proto::FramebufferInfo
///
/// Converts boot protocol framebuffer info to internal format and initializes.
/// The `phys_map_base` is used to convert physical addresses to virtual addresses.
/// If `video_modes` is provided, it stores them for mode enumeration.
pub fn init_from_boot(
    boot_fb: &boot_proto::FramebufferInfo,
    phys_map_base: u64,
    video_modes: Option<&boot_proto::VideoModeList>,
) {
    // Store video modes if available
    if let Some(modes) = video_modes {
        *VIDEO_MODES.lock() = Some(*modes);
        CURRENT_MODE.store(modes.current_mode, Ordering::SeqCst);
    }
    // Convert boot_proto pixel format to fb pixel format
    let format = match boot_fb.format {
        boot_proto::PixelFormat::Rgb => PixelFormat::RGBA8888,
        boot_proto::PixelFormat::Bgr => PixelFormat::BGRA8888,
        boot_proto::PixelFormat::Rgba8888 => PixelFormat::RGBA8888,
        boot_proto::PixelFormat::Bgra8888 => PixelFormat::BGRA8888,
        boot_proto::PixelFormat::Rgb565 => PixelFormat::RGB565,
        // For indexed/grayscale, we don't have direct support - default to BGRA
        boot_proto::PixelFormat::Indexed8 => PixelFormat::Unknown,
        boot_proto::PixelFormat::Grayscale8 => PixelFormat::Unknown,
        boot_proto::PixelFormat::Unknown => PixelFormat::BGRA8888, // Default assumption for x86 UEFI
    };

    // Store physical address for /dev/fb0
    *FB_PHYS_BASE.lock() = boot_fb.base;

    // Convert physical address to virtual using the direct physical map
    let virtual_base = phys_map_base + boot_fb.base;

    let info = FramebufferInfo {
        base: virtual_base as usize,
        size: boot_fb.size as usize,
        width: boot_fb.width,
        height: boot_fb.height,
        stride: boot_fb.stride * (boot_fb.bpp / 8), // Convert stride from pixels to bytes
        format,
    };

    init(info);
}

/// Check if framebuffer is initialized
pub fn is_initialized() -> bool {
    INITIALIZED.load(Ordering::SeqCst)
}

/// Get framebuffer information for /dev/fb0
///
/// Returns physical and virtual addresses along with framebuffer parameters.
pub fn get_fb_info() -> Option<FbDeviceInfo> {
    let fb = FRAMEBUFFER.lock();
    let fb = fb.as_ref()?;

    // Get the base virtual address (stored in our FramebufferInfo)
    let base = fb.buffer() as usize;
    let phys_base = *FB_PHYS_BASE.lock();
    let size = fb.size();
    let width = fb.width();
    let height = fb.height();
    let stride = fb.stride();
    let format = fb.format();
    let bpp = format.bytes_per_pixel() * 8;
    let is_bgr = matches!(format, PixelFormat::BGRA8888 | PixelFormat::BGR888);

    Some(FbDeviceInfo {
        base,
        phys_base,
        size,
        width,
        height,
        stride,
        bpp,
        is_bgr,
    })
}

/// Framebuffer device info for /dev/fb0
#[derive(Debug, Clone, Copy)]
pub struct FbDeviceInfo {
    pub base: usize,      // Virtual address
    pub phys_base: u64,   // Physical address
    pub size: usize,      // Total size in bytes
    pub width: u32,       // Width in pixels
    pub height: u32,      // Height in pixels
    pub stride: u32,      // Bytes per scanline
    pub bpp: u32,         // Bits per pixel
    pub is_bgr: bool,     // BGR vs RGB format
}

/// Get the framebuffer
pub fn framebuffer() -> Option<Arc<dyn Framebuffer>> {
    FRAMEBUFFER.lock().clone()
}

/// Get the console
pub fn console() -> &'static Mutex<Option<FbConsole>> {
    &CONSOLE
}

/// Write a character to the console
pub fn putchar(ch: char) {
    if let Some(ref mut console) = *CONSOLE.lock() {
        console.putchar(ch);
    }
}

/// Write a string to the console
pub fn puts(s: &str) {
    if let Some(ref mut console) = *CONSOLE.lock() {
        for ch in s.chars() {
            console.putchar(ch);
        }
    }
}

/// Clear the console
pub fn clear() {
    if let Some(ref mut console) = *CONSOLE.lock() {
        console.clear();
    }
}

/// Set console colors
pub fn set_colors(fg: Color, bg: Color) {
    if let Some(ref mut console) = *CONSOLE.lock() {
        console.set_fg_color(fg);
        console.set_bg_color(bg);
    }
}

// ============================================================================
// Video Mode Enumeration
// ============================================================================

/// Get the number of available video modes
pub fn get_mode_count() -> u32 {
    VIDEO_MODES.lock().as_ref().map(|m| m.count).unwrap_or(1)
}

/// Get the current video mode index
pub fn get_current_mode() -> u32 {
    CURRENT_MODE.load(Ordering::SeqCst)
}

/// Video mode info for userspace
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct VideoModeInfo {
    /// Mode number (used for set_mode)
    pub mode_number: u32,
    /// Width in pixels
    pub width: u32,
    /// Height in pixels
    pub height: u32,
    /// Bits per pixel
    pub bpp: u32,
    /// Stride in bytes per scanline
    pub stride: u32,
    /// Framebuffer size for this mode
    pub framebuffer_size: u64,
    /// Is this BGR format (vs RGB)
    pub is_bgr: bool,
    /// Padding for alignment
    pub _pad: [u8; 7],
}

/// Get video mode info by index
pub fn get_mode_info(index: u32) -> Option<VideoModeInfo> {
    let modes_guard = VIDEO_MODES.lock();
    let modes = modes_guard.as_ref()?;

    if index >= modes.count {
        return None;
    }

    let mode = &modes.modes[index as usize];
    let is_bgr = matches!(mode.format, boot_proto::PixelFormat::Bgr | boot_proto::PixelFormat::Bgra8888);

    Some(VideoModeInfo {
        mode_number: mode.mode_number,
        width: mode.width,
        height: mode.height,
        bpp: mode.bpp,
        stride: mode.stride * (mode.bpp / 8), // Convert to bytes
        framebuffer_size: mode.framebuffer_size,
        is_bgr,
        _pad: [0; 7],
    })
}
