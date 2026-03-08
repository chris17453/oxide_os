//! Memory utilities for the OXIDE kernel.

use crate::globals::HEAP_ALLOCATOR;
use mm_manager::mm;

/// Get memory statistics for /proc/meminfo
pub fn get_memory_stats() -> procfs::MemoryStats {
    // Get memory manager stats (buddy allocator)
    let total_bytes = mm().total_bytes();
    let free_bytes = mm().free_bytes();

    // Get heap stats
    let heap_used = HEAP_ALLOCATOR.used() as u64;
    let heap_free = HEAP_ALLOCATOR.free() as u64;

    procfs::MemoryStats {
        total_mem: total_bytes,
        free_mem: free_bytes,
        total_swap: 0, // No swap
        free_swap: 0,
        heap_used,
        heap_free,
    }
}

/// Get framebuffer device info for /dev/fb0.
/// — GlassSignal: resolves to the calling process's VT's virtual framebuffer.
/// Like /dev/tty resolving to the caller's controlling terminal, /dev/fb0
/// resolves to the caller's VT's VFB. Apps never see the hardware FB.
/// Falls back to hardware FB if VT lookup fails (kernel context, no ctty).
pub fn get_fb_device_info() -> Option<devfs::devices::FramebufferDeviceInfo> {
    // — GlassSignal: try to resolve caller's VT from controlling terminal
    if let Some(pid) = sched::current_pid() {
        if let Some(meta) = sched::try_get_task_meta(pid) {
            if let Some(guard) = meta.try_lock() {
                let tty_nr = guard.tty_nr;
                // — GlassSignal: major 4 = VT device, minor = VT number
                if tty_nr != 0 && (tty_nr >> 8) == 4 {
                    let vt_num = (tty_nr & 0xFF) as usize;
                    if let Some((base, size, w, h, stride, bpp, is_bgr)) =
                        compositor::get_vfb_info_raw(vt_num)
                    {
                        return Some(devfs::devices::FramebufferDeviceInfo {
                            base,
                            phys_base: 0, // — GlassSignal: VFB is virtual, no physical addr
                            size,
                            width: w,
                            height: h,
                            stride,
                            bpp,
                            is_bgr,
                        });
                    }
                }
            }
        }
    }

    // — GlassSignal: fallback to hardware FB (kernel context or no VT assigned)
    let info = fb::get_fb_info()?;

    Some(devfs::devices::FramebufferDeviceInfo {
        base: info.base,
        phys_base: info.phys_base,
        size: info.size,
        width: info.width,
        height: info.height,
        stride: info.stride,
        bpp: info.bpp,
        is_bgr: info.is_bgr,
    })
}

/// Get video mode count for /dev/fb0
pub fn get_fb_mode_count() -> u32 {
    fb::get_mode_count()
}

/// Get video mode info by index for /dev/fb0
pub fn get_fb_mode_info(index: u32) -> Option<devfs::devices::VideoModeDeviceInfo> {
    let mode = fb::get_mode_info(index)?;

    Some(devfs::devices::VideoModeDeviceInfo {
        mode_number: mode.mode_number,
        width: mode.width,
        height: mode.height,
        bpp: mode.bpp,
        stride: mode.stride,
        framebuffer_size: mode.framebuffer_size,
        is_bgr: mode.is_bgr,
        _pad: [0; 7],
    })
}

/// Set video mode via fb module and return updated info
pub fn set_fb_mode(index: u32) -> Option<devfs::devices::VideoModeDeviceInfo> {
    let mode = fb::mode::set_mode(index)?;

    Some(devfs::devices::VideoModeDeviceInfo {
        mode_number: mode.mode_number,
        width: mode.width,
        height: mode.height,
        bpp: mode.bpp,
        stride: mode.stride,
        framebuffer_size: mode.framebuffer_size,
        is_bgr: mode.is_bgr,
        _pad: [0; 7],
    })
}
