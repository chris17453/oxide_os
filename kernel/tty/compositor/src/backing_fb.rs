//! VT Backing Framebuffer — physical-frame-backed pixel buffer as a Framebuffer trait object.
//!
//! — NeonRoot: every VT gets its own pixel playground. Allocated from the buddy
//! allocator (physical frames), NOT the kernel heap. The heap is 32MB and six
//! 4MB buffers would eat it alive. Physical memory is 166MB+ — plenty of room.

use fb::{Framebuffer, PixelFormat};
use mm_manager::mm;
use mm_traits::FrameAllocator;
use os_core::PhysAddr;

/// Physical memory mapping base (identity map region)
const PHYS_MAP_BASE: u64 = 0xFFFF_8000_0000_0000;

/// A physical-frame-backed framebuffer that implements the Framebuffer trait.
/// Used as the per-VT backing buffer that the terminal renderer writes to.
/// The compositor then blits from here to the hardware framebuffer.
///
/// — NeonRoot: allocated from buddy allocator, not heap. The kernel heap is a
/// precious 32MB; six 4MB pixel buffers would starve every other subsystem.
/// Physical frames are cheap — 166MB+ available. Same pattern as VirtIO DMA buffers.
pub struct BackingFramebuffer {
    /// Virtual address of the buffer (via PHYS_MAP identity map)
    virt_ptr: *mut u8,
    /// Physical address (for deallocation)
    phys_base: u64,
    /// Number of pages allocated
    num_pages: usize,
    /// Width in pixels
    width: u32,
    /// Height in pixels
    height: u32,
    /// Bytes per scanline (may include padding)
    stride: u32,
    /// Pixel format (matches hardware fb)
    format: PixelFormat,
    /// Total buffer size in bytes
    buf_size: usize,
}

impl BackingFramebuffer {
    /// Create a new backing framebuffer matching the given dimensions and format.
    /// — NeonRoot: allocates from buddy allocator. ~4MB for 1280x800x4bpp.
    /// Returns the buffer or panics if the frame allocator is out of memory
    /// (shouldn't happen with 166MB+ available).
    pub fn new(width: u32, height: u32, stride: u32, format: PixelFormat) -> Self {
        let buf_size = (stride * height) as usize;
        let num_pages = (buf_size + 4095) / 4096;

        // — NeonRoot: buddy allocator gives physical frames with known addresses.
        // Same pattern as virtio-core DMA buffers — no heap, no lies.
        let phys_addr = mm().alloc_contiguous(num_pages)
            .expect("BackingFramebuffer: frame allocator OOM");
        let phys_base = phys_addr.as_u64();
        let virt_ptr = (phys_base + PHYS_MAP_BASE) as *mut u8;

        // Zero the buffer
        unsafe {
            core::ptr::write_bytes(virt_ptr, 0, num_pages * 4096);
        }

        BackingFramebuffer {
            virt_ptr,
            phys_base,
            num_pages,
            width,
            height,
            stride,
            format,
            buf_size,
        }
    }

    /// Get a raw pointer to the pixel data for direct compositor blitting
    pub fn raw_ptr(&self) -> *const u8 {
        self.virt_ptr as *const u8
    }

    /// Get a mutable raw pointer for direct writes
    pub fn raw_ptr_mut(&mut self) -> *mut u8 {
        self.virt_ptr
    }
}

impl Framebuffer for BackingFramebuffer {
    fn width(&self) -> u32 {
        self.width
    }

    fn height(&self) -> u32 {
        self.height
    }

    fn format(&self) -> PixelFormat {
        self.format
    }

    fn stride(&self) -> u32 {
        self.stride
    }

    fn buffer(&self) -> *mut u8 {
        self.virt_ptr
    }

    fn size(&self) -> usize {
        self.buf_size
    }

    // — NeonRoot: no flush needed for RAM buffers. The compositor handles
    // copying our pixels to the hardware framebuffer. We're just a canvas.
    fn flush(&self) {}
    fn flush_region(&self, _x: u32, _y: u32, _w: u32, _h: u32) {}
}

/// — NeonRoot: free the physical frames when the backing buffer is dropped.
/// Without this, every VT teardown leaks ~4MB of physical memory.
impl Drop for BackingFramebuffer {
    fn drop(&mut self) {
        if self.phys_base != 0 && self.num_pages > 0 {
            let _ = mm().free_contiguous(PhysAddr::new(self.phys_base), self.num_pages);
        }
    }
}

// SAFETY: The backing buffer is a physical frame allocation with no MMIO or hardware ties.
// It can be sent between threads and accessed from any context.
unsafe impl Send for BackingFramebuffer {}
unsafe impl Sync for BackingFramebuffer {}
