//! Screen Buffer — the RAM-side double buffer that makes boot graphics not suck.
//!
//! — NeonVale: every pixel goes to RAM first. One BltBufferToVideo call flushes
//! the whole screen. Turns 50,000 individual UEFI firmware calls into ONE.
//! The OVMF firmware is emulated, so each BLT call has ridiculous overhead.
//! Drawing a single 8x16 glyph used to be 128 firmware calls. Now it's zero
//! until flush. The difference is night and day.

use crate::efi::{EfiBltPixel, EfiBltOperation, EfiGraphicsOutputProtocol};

/// — NeonVale: the screen buffer. Allocated from UEFI pages at boot, freed never
/// (we're jumping to a kernel, not coming back). Width × Height pixels, BGRX.
static mut SCREEN_BUF: *mut EfiBltPixel = core::ptr::null_mut();
static mut SCREEN_WIDTH: usize = 0;
static mut SCREEN_HEIGHT: usize = 0;
static mut SCREEN_INITIALIZED: bool = false;

/// Initialize the screen buffer. Call once after GOP is available and screen
/// dimensions are known. Allocates width×height×4 bytes from UEFI page allocator.
/// — NeonVale: the moment we stop being slaves to firmware BLT latency
pub fn init(width: usize, height: usize) -> bool {
    let pixel_count = width * height;
    let byte_count = pixel_count * core::mem::size_of::<EfiBltPixel>();
    let page_count = (byte_count + 4095) / 4096;

    if let Some(phys) = crate::efi::allocate_pages(page_count) {
        let ptr = phys as *mut EfiBltPixel;
        // — NeonVale: zero the buffer (black screen)
        unsafe {
            core::ptr::write_bytes(ptr as *mut u8, 0, byte_count);
            SCREEN_BUF = ptr;
            SCREEN_WIDTH = width;
            SCREEN_HEIGHT = height;
            SCREEN_INITIALIZED = true;
        }
        true
    } else {
        // — NeonVale: allocation failed. Fall back to direct BLT (the slow path).
        // This shouldn't happen unless UEFI is critically low on memory.
        false
    }
}

/// Check if the screen buffer is active.
#[inline]
pub fn is_initialized() -> bool {
    unsafe { SCREEN_INITIALIZED }
}

/// Write a filled rectangle to the screen buffer. No firmware calls.
/// — NeonVale: the fast path — array indexing instead of UEFI BLT.
/// Each pixel write is a single memory store. Compare to the old path:
/// set up EfiBltPixel struct → call through firmware function pointer →
/// firmware validates parameters → firmware writes to framebuffer.
#[inline]
pub fn fill_rect(x: usize, y: usize, w: usize, h: usize, color: EfiBltPixel) {
    unsafe {
        if !SCREEN_INITIALIZED { return; }
        let buf = SCREEN_BUF;
        let sw = SCREEN_WIDTH;
        let sh = SCREEN_HEIGHT;

        // — NeonVale: clip to screen bounds. Callers don't always check.
        let x_end = (x + w).min(sw);
        let y_end = (y + h).min(sh);
        if x >= sw || y >= sh { return; }

        for row in y..y_end {
            let row_start = row * sw + x;
            for col in 0..(x_end - x) {
                *buf.add(row_start + col) = color;
            }
        }
    }
}

/// Write a single pixel to the screen buffer. No bounds checking — caller must ensure valid coords.
/// — NeonVale: the tightest possible write — one pointer add + one store.
#[inline(always)]
pub unsafe fn set_pixel_unchecked(x: usize, y: usize, color: EfiBltPixel) {
    unsafe {
        *SCREEN_BUF.add(y * SCREEN_WIDTH + x) = color;
    }
}

/// Get the raw buffer pointer and dimensions for bulk writes.
/// — NeonVale: for the background renderer — direct buffer access, zero overhead.
#[inline]
pub fn raw_buffer() -> Option<(*mut EfiBltPixel, usize, usize)> {
    unsafe {
        if SCREEN_INITIALIZED {
            Some((SCREEN_BUF, SCREEN_WIDTH, SCREEN_HEIGHT))
        } else {
            None
        }
    }
}

/// Flush the entire screen buffer to the GOP framebuffer in ONE call.
/// — NeonVale: this is the money shot. One BltBufferToVideo = one firmware call
/// for the entire screen. 786,432 pixels in a single DMA-like transfer instead
/// of pixel-by-pixel agony through the UEFI abstraction layer.
pub fn flush(gop: *mut EfiGraphicsOutputProtocol) {
    unsafe {
        if !SCREEN_INITIALIZED { return; }
        ((*gop).blt)(
            gop,
            SCREEN_BUF as *const EfiBltPixel,
            EfiBltOperation::BltBufferToVideo,
            0, 0,   // source X, Y (top-left of buffer)
            0, 0,   // destination X, Y (top-left of screen)
            SCREEN_WIDTH,
            SCREEN_HEIGHT,
            0,       // Delta = 0 means buffer stride = Width
        );
    }
}
