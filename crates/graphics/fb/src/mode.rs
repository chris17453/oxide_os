//! Video mode switching for OXIDE framebuffer
//! Currently supports GOP-like linear framebuffers and rebuilds backing buffers on mode change.

#![allow(dead_code)]

extern crate alloc;

use alloc::sync::Arc;
use spin::Mutex;

use boot_proto::{PixelFormat, VideoMode};

use crate::{Framebuffer, FramebufferInfo, LinearFramebuffer, VIDEO_MODES, VideoModeInfo};

/// Hook to let platform-specific code actually perform the mode switch.
/// Takes boot-time VideoMode and returns new framebuffer info for that mode.
pub type ModeSetter = fn(mode: &VideoMode) -> Option<FramebufferInfo>;

static MODE_SETTER: Mutex<Option<ModeSetter>> = Mutex::new(None);

pub fn set_mode_setter(f: ModeSetter) {
    *MODE_SETTER.lock() = Some(f);
}

/// Attempt to change video mode and rebuild framebuffer/console state.
pub fn set_mode(index: u32) -> Option<VideoModeInfo> {
    let modes_guard = VIDEO_MODES.lock();
    let modes = modes_guard.as_ref()?;
    if index >= modes.count {
        return None;
    }
    let mode = modes.modes[index as usize];

    let setter = MODE_SETTER.lock();
    let setter = setter.as_ref()?;

    // Perform platform mode set and get new framebuffer mapping
    let fb_info = setter(&mode)?;

    // Rebuild framebuffer with new dimensions
    let fb = Arc::new(LinearFramebuffer::new(fb_info));
    crate::FRAMEBUFFER.lock().replace(fb.clone());
    crate::CONSOLE.lock().replace(crate::FbConsole::new(fb));

    // Persist current mode index
    crate::CURRENT_MODE.store(index, core::sync::atomic::Ordering::SeqCst);

    // Return user-facing mode info from the active framebuffer (authoritative)
    let bpp = match fb_info.format {
        PixelFormat::RGB565 => 16,
        PixelFormat::BGRA8888 | PixelFormat::RGBA8888 => 32,
        PixelFormat::Unknown => mode.bpp,
    };
    let is_bgr = matches!(
        fb_info.format,
        PixelFormat::Bgr | PixelFormat::Bgra8888 | PixelFormat::BGRA8888
    );
    Some(VideoModeInfo {
        mode_number: mode.mode_number,
        width: fb_info.width,
        height: fb_info.height,
        bpp,
        stride: fb_info.stride,
        framebuffer_size: fb_info.size as u64,
        is_bgr,
        _pad: [0; 7],
    })
}
