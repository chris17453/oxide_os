//! Memory utilities for the OXIDE kernel.

use crate::globals::{FRAME_ALLOCATOR, HEAP_ALLOCATOR};
use os_core::PhysAddr;

/// Wrapper to use the global frame allocator with trait bounds
pub struct FrameAllocatorWrapper;

impl mm_traits::FrameAllocator for FrameAllocatorWrapper {
    fn alloc_frame(&self) -> Option<PhysAddr> {
        FRAME_ALLOCATOR.alloc_frame()
    }

    fn free_frame(&self, addr: PhysAddr) {
        FRAME_ALLOCATOR.free_frame(addr);
    }

    fn alloc_frames(&self, count: usize) -> Option<PhysAddr> {
        FRAME_ALLOCATOR.alloc_frames(count)
    }

    fn free_frames(&self, addr: PhysAddr, count: usize) {
        FRAME_ALLOCATOR.free_frames(addr, count);
    }
}

/// Get memory statistics for /proc/meminfo
pub fn get_memory_stats() -> procfs::MemoryStats {
    // Get frame allocator stats
    let total_frames = FRAME_ALLOCATOR.total_frames();
    let free_frames = FRAME_ALLOCATOR.free_frame_count();
    let page_size = 4096u64;

    // Get heap stats
    let heap_used = HEAP_ALLOCATOR.used() as u64;
    let heap_free = HEAP_ALLOCATOR.free() as u64;

    procfs::MemoryStats {
        total_mem: (total_frames as u64) * page_size,
        free_mem: (free_frames as u64) * page_size,
        total_swap: 0, // No swap
        free_swap: 0,
        heap_used,
        heap_free,
    }
}

/// Get framebuffer device info for /dev/fb0
pub fn get_fb_device_info() -> Option<devfs::devices::FramebufferDeviceInfo> {
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
