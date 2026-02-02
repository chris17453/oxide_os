//! Screen Capture Provider Implementation

use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use core::ptr;
use fb::{Framebuffer, PixelFormat as FbPixelFormat};
use rdp_traits::{DirtyRegion, PixelFormat, RdpError, RdpResult, ScreenCaptureProvider};
use spin::Mutex;

/// Framebuffer-based screen capture provider
pub struct FramebufferCaptureProvider {
    /// The system framebuffer
    fb: Arc<dyn Framebuffer>,
    /// Dirty regions since last read
    dirty_regions: Vec<DirtyRegion>,
    /// Track if entire screen is dirty
    full_dirty: bool,
}

impl FramebufferCaptureProvider {
    /// Create a new capture provider wrapping the system framebuffer
    pub fn new(fb: Arc<dyn Framebuffer>) -> Self {
        Self {
            fb,
            dirty_regions: Vec::new(),
            full_dirty: true, // Start with full screen dirty
        }
    }

    /// Convert framebuffer pixel format to RDP pixel format
    fn convert_format(fb_format: FbPixelFormat) -> PixelFormat {
        match fb_format {
            FbPixelFormat::BGRA8888 => PixelFormat::Bgra8888,
            FbPixelFormat::RGBA8888 => PixelFormat::Rgba8888,
            FbPixelFormat::BGR888 => PixelFormat::Bgr888,
            FbPixelFormat::RGB888 => PixelFormat::Rgb888,
            FbPixelFormat::RGB565 => PixelFormat::Rgb565,
            FbPixelFormat::Unknown => PixelFormat::Bgra8888,
        }
    }
}

impl ScreenCaptureProvider for FramebufferCaptureProvider {
    fn dimensions(&self) -> (u32, u32) {
        (self.fb.width(), self.fb.height())
    }

    fn pixel_format(&self) -> PixelFormat {
        Self::convert_format(self.fb.format())
    }

    fn stride(&self) -> u32 {
        self.fb.stride()
    }

    fn capture_full(&self, buffer: &mut [u8]) -> RdpResult<()> {
        let required_size = (self.fb.height() * self.fb.stride()) as usize;
        if buffer.len() < required_size {
            return Err(RdpError::InsufficientData);
        }

        let src = self.fb.buffer();
        let size = self.fb.size().min(buffer.len());

        // Use volatile reads for memory-mapped framebuffer
        unsafe {
            // Copy in chunks for better cache performance
            let mut offset = 0;
            while offset + 8 <= size {
                let val = ptr::read_volatile((src as *const u64).add(offset / 8));
                ptr::write(buffer.as_mut_ptr().add(offset) as *mut u64, val);
                offset += 8;
            }
            // Handle remaining bytes
            while offset < size {
                buffer[offset] = ptr::read_volatile(src.add(offset));
                offset += 1;
            }
        }

        Ok(())
    }

    fn capture_region(
        &self,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
        buffer: &mut [u8],
    ) -> RdpResult<()> {
        let bpp = self.pixel_format().bytes_per_pixel();
        let required_size = (width * height * bpp) as usize;

        if buffer.len() < required_size {
            return Err(RdpError::InsufficientData);
        }

        // Bounds check
        if x + width > self.fb.width() || y + height > self.fb.height() {
            return Err(RdpError::InvalidProtocol);
        }

        let src = self.fb.buffer();
        let stride = self.fb.stride();
        let row_bytes = (width * bpp) as usize;

        unsafe {
            for row in 0..height {
                let src_offset = ((y + row) * stride + x * bpp) as usize;
                let dst_offset = (row as usize) * row_bytes;

                // Copy row with volatile reads
                let src_ptr = src.add(src_offset);
                let dst_ptr = buffer.as_mut_ptr().add(dst_offset);

                // Fast 8-byte copies
                let mut col = 0;
                while col + 8 <= row_bytes {
                    let val = ptr::read_volatile((src_ptr.add(col)) as *const u64);
                    ptr::write(dst_ptr.add(col) as *mut u64, val);
                    col += 8;
                }
                // Remaining bytes
                while col < row_bytes {
                    *dst_ptr.add(col) = ptr::read_volatile(src_ptr.add(col));
                    col += 1;
                }
            }
        }

        Ok(())
    }

    fn get_dirty_regions(&mut self) -> Vec<DirtyRegion> {
        if self.full_dirty {
            self.full_dirty = false;
            self.dirty_regions.clear();
            vec![DirtyRegion::new(0, 0, self.fb.width(), self.fb.height())]
        } else {
            core::mem::take(&mut self.dirty_regions)
        }
    }

    fn mark_all_dirty(&mut self) {
        self.full_dirty = true;
        self.dirty_regions.clear();
    }
}

/// Thread-safe wrapper for capture provider
pub struct SharedCaptureProvider {
    inner: Arc<Mutex<FramebufferCaptureProvider>>,
}

impl SharedCaptureProvider {
    pub fn new(fb: Arc<dyn Framebuffer>) -> Self {
        Self {
            inner: Arc::new(Mutex::new(FramebufferCaptureProvider::new(fb))),
        }
    }

    pub fn clone_inner(&self) -> Arc<Mutex<FramebufferCaptureProvider>> {
        Arc::clone(&self.inner)
    }
}

impl Clone for SharedCaptureProvider {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}
