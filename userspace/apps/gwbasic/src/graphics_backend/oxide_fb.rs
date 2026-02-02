//! OXIDE Framebuffer Graphics Backend
//!
//! Implements pixel-buffer graphics for OXIDE OS using /dev/fb0.
//! Supports the native framebuffer provided by UEFI GOP via mmap.

#![cfg(not(feature = "std"))]

extern crate alloc;

use alloc::format;
use alloc::string::ToString;
use alloc::vec;
use alloc::vec::Vec;

use crate::error::{Error, Result};
use crate::graphics_backend::GraphicsBackend;

/// Standard EGA/VGA 16-color palette
pub const PALETTE_16: [u32; 16] = [
    0x000000, // 0: Black
    0x0000AA, // 1: Blue
    0x00AA00, // 2: Green
    0x00AAAA, // 3: Cyan
    0xAA0000, // 4: Red
    0xAA00AA, // 5: Magenta
    0xAA5500, // 6: Brown
    0xAAAAAA, // 7: Light Gray
    0x555555, // 8: Dark Gray
    0x5555FF, // 9: Light Blue
    0x55FF55, // 10: Light Green
    0x55FFFF, // 11: Light Cyan
    0xFF5555, // 12: Light Red
    0xFF55FF, // 13: Light Magenta
    0xFFFF55, // 14: Yellow
    0xFFFFFF, // 15: White
];

/// Framebuffer ioctl numbers (Linux-compatible)
mod fb_ioctl {
    pub const FBIOGET_VSCREENINFO: u64 = 0x4600;
    pub const FBIOGET_FSCREENINFO: u64 = 0x4602;
}

/// Variable screen info (from Linux fb.h)
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct FbVarScreenInfo {
    pub xres: u32,
    pub yres: u32,
    pub xres_virtual: u32,
    pub yres_virtual: u32,
    pub xoffset: u32,
    pub yoffset: u32,
    pub bits_per_pixel: u32,
    pub grayscale: u32,
    pub red_offset: u32,
    pub red_length: u32,
    pub green_offset: u32,
    pub green_length: u32,
    pub blue_offset: u32,
    pub blue_length: u32,
    pub transp_offset: u32,
    pub transp_length: u32,
    pub nonstd: u32,
    pub activate: u32,
    pub height: u32,
    pub width: u32,
    pub accel_flags: u32,
    // Timing fields (not used)
    pub pixclock: u32,
    pub left_margin: u32,
    pub right_margin: u32,
    pub upper_margin: u32,
    pub lower_margin: u32,
    pub hsync_len: u32,
    pub vsync_len: u32,
    pub sync: u32,
    pub vmode: u32,
    pub rotate: u32,
    pub colorspace: u32,
    pub reserved: [u32; 4],
}

/// Fixed screen info (from Linux fb.h)
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct FbFixScreenInfo {
    pub id: [u8; 16],
    pub smem_start: u64,
    pub smem_len: u32,
    pub fb_type: u32,
    pub type_aux: u32,
    pub visual: u32,
    pub xpanstep: u16,
    pub ypanstep: u16,
    pub ywrapstep: u16,
    pub _pad: u16,
    pub line_length: u32,
    pub mmio_start: u64,
    pub mmio_len: u32,
    pub accel: u32,
    pub capabilities: u16,
    pub reserved: [u16; 2],
}

/// OXIDE framebuffer graphics backend
pub struct OxideFramebufferBackend {
    fb_fd: i32,                    // File descriptor for /dev/fb0
    framebuffer: *mut u8,          // Mapped framebuffer memory
    local_buffer: Vec<u8>,         // Local double-buffer for drawing
    width: usize,
    height: usize,
    stride: usize,                 // Bytes per scanline
    bpp: u8,                       // Bits per pixel
    is_bgr: bool,                  // BGR vs RGB pixel order
    cursor_x: usize,
    cursor_y: usize,
    fg_color: u8,
    bg_color: u8,
    dirty: bool,
}

impl OxideFramebufferBackend {
    /// Create a new framebuffer backend by opening /dev/fb0
    pub fn new() -> Result<Self> {
        // Open /dev/fb0
        let fb_fd = libc::open("/dev/fb0", libc::O_RDWR, 0);
        if fb_fd < 0 {
            return Err(Error::RuntimeError(format!(
                "Failed to open /dev/fb0: errno={}",
                -fb_fd
            )));
        }

        // Get screen info via ioctl
        let mut var_info = FbVarScreenInfo::default();
        let ret = libc::sys_ioctl(fb_fd, fb_ioctl::FBIOGET_VSCREENINFO, &mut var_info as *mut _ as u64);
        if ret < 0 {
            libc::close(fb_fd);
            return Err(Error::RuntimeError(format!(
                "FBIOGET_VSCREENINFO failed: {}",
                ret
            )));
        }

        let mut fix_info = FbFixScreenInfo::default();
        let ret = libc::sys_ioctl(fb_fd, fb_ioctl::FBIOGET_FSCREENINFO, &mut fix_info as *mut _ as u64);
        if ret < 0 {
            libc::close(fb_fd);
            return Err(Error::RuntimeError(format!(
                "FBIOGET_FSCREENINFO failed: {}",
                ret
            )));
        }

        let width = var_info.xres as usize;
        let height = var_info.yres as usize;
        let bpp = var_info.bits_per_pixel as u8;
        let stride = fix_info.line_length as usize;
        let fb_size = fix_info.smem_len as usize;
        
        // Determine pixel order (BGR vs RGB)
        let is_bgr = var_info.blue_offset < var_info.red_offset;

        // Map the framebuffer into our address space
        let framebuffer = libc::sys_mmap(
            core::ptr::null_mut(),
            fb_size,
            libc::prot::PROT_READ | libc::prot::PROT_WRITE,
            libc::map_flags::MAP_SHARED,
            fb_fd,
            0,
        );

        if framebuffer == libc::MAP_FAILED {
            libc::close(fb_fd);
            return Err(Error::RuntimeError(
                "Failed to mmap framebuffer".to_string(),
            ));
        }

        // Create local double-buffer
        let local_buffer = vec![0u8; fb_size];

        Ok(OxideFramebufferBackend {
            fb_fd,
            framebuffer,
            local_buffer,
            width,
            height,
            stride,
            bpp,
            is_bgr,
            cursor_x: 0,
            cursor_y: 0,
            fg_color: 15, // White
            bg_color: 0,  // Black
            dirty: true,
        })
    }

    /// Convert 8-bit palette color to 32-bit RGBA/BGRA
    fn color_to_pixel(&self, color: u8) -> u32 {
        let rgb = if color < 16 {
            PALETTE_16[color as usize]
        } else {
            // Extended colors: grayscale
            let gray = color as u32;
            (gray << 16) | (gray << 8) | gray
        };

        if self.is_bgr {
            // BGRA format
            let r = (rgb >> 16) & 0xFF;
            let g = (rgb >> 8) & 0xFF;
            let b = rgb & 0xFF;
            (0xFF << 24) | (r << 16) | (g << 8) | b
        } else {
            // RGBA format
            (0xFF << 24) | rgb
        }
    }

    /// Set a pixel in the local buffer
    fn set_pixel_local(&mut self, x: usize, y: usize, color: u8) {
        if x >= self.width || y >= self.height {
            return;
        }

        let pixel = self.color_to_pixel(color);
        let bytes_per_pixel = (self.bpp / 8) as usize;
        let offset = y * self.stride + x * bytes_per_pixel;

        if offset + bytes_per_pixel <= self.local_buffer.len() {
            match bytes_per_pixel {
                4 => {
                    self.local_buffer[offset] = (pixel & 0xFF) as u8;
                    self.local_buffer[offset + 1] = ((pixel >> 8) & 0xFF) as u8;
                    self.local_buffer[offset + 2] = ((pixel >> 16) & 0xFF) as u8;
                    self.local_buffer[offset + 3] = ((pixel >> 24) & 0xFF) as u8;
                }
                3 => {
                    self.local_buffer[offset] = (pixel & 0xFF) as u8;
                    self.local_buffer[offset + 1] = ((pixel >> 8) & 0xFF) as u8;
                    self.local_buffer[offset + 2] = ((pixel >> 16) & 0xFF) as u8;
                }
                2 => {
                    // RGB565 - simplified conversion
                    let r = ((pixel >> 16) & 0xFF) >> 3;
                    let g = ((pixel >> 8) & 0xFF) >> 2;
                    let b = (pixel & 0xFF) >> 3;
                    let rgb565 = ((r << 11) | (g << 5) | b) as u16;
                    self.local_buffer[offset] = (rgb565 & 0xFF) as u8;
                    self.local_buffer[offset + 1] = (rgb565 >> 8) as u8;
                }
                _ => {}
            }
            self.dirty = true;
        }
    }

    /// Copy local buffer to framebuffer
    pub fn commit(&mut self) {
        if !self.dirty {
            return;
        }

        // Copy local buffer to mapped framebuffer
        unsafe {
            core::ptr::copy_nonoverlapping(
                self.local_buffer.as_ptr(),
                self.framebuffer,
                self.local_buffer.len(),
            );
        }

        self.dirty = false;
    }

    /// Get screen dimensions
    pub fn dimensions(&self) -> (usize, usize) {
        (self.width, self.height)
    }
}

impl GraphicsBackend for OxideFramebufferBackend {
    fn pset(&mut self, x: i32, y: i32, color: u8) -> Result<()> {
        if x >= 0 && y >= 0 {
            self.set_pixel_local(x as usize, y as usize, color);
        }
        Ok(())
    }

    fn line(&mut self, x1: i32, y1: i32, x2: i32, y2: i32, color: u8) -> Result<()> {
        // Bresenham's line algorithm
        let dx = (x2 - x1).abs();
        let dy = (y2 - y1).abs();
        let sx = if x1 < x2 { 1 } else { -1 };
        let sy = if y1 < y2 { 1 } else { -1 };
        let mut err = dx - dy;
        let mut x = x1;
        let mut y = y1;

        loop {
            self.pset(x, y, color)?;
            if x == x2 && y == y2 {
                break;
            }
            let e2 = 2 * err;
            if e2 > -dy {
                err -= dy;
                x += sx;
            }
            if e2 < dx {
                err += dx;
                y += sy;
            }
        }
        Ok(())
    }

    fn circle(&mut self, cx: i32, cy: i32, radius: i32, color: u8) -> Result<()> {
        // Midpoint circle algorithm
        let mut x = radius;
        let mut y = 0;
        let mut err = 0;

        while x >= y {
            self.pset(cx + x, cy + y, color)?;
            self.pset(cx + y, cy + x, color)?;
            self.pset(cx - y, cy + x, color)?;
            self.pset(cx - x, cy + y, color)?;
            self.pset(cx - x, cy - y, color)?;
            self.pset(cx - y, cy - x, color)?;
            self.pset(cx + y, cy - x, color)?;
            self.pset(cx + x, cy - y, color)?;

            if err <= 0 {
                y += 1;
                err += 2 * y + 1;
            }
            if err > 0 {
                x -= 1;
                err -= 2 * x + 1;
            }
        }
        Ok(())
    }

    fn cls(&mut self) {
        // Fill with background color
        let pixel = self.color_to_pixel(self.bg_color);
        let bytes_per_pixel = (self.bpp / 8) as usize;

        for y in 0..self.height {
            for x in 0..self.width {
                let offset = y * self.stride + x * bytes_per_pixel;
                if offset + bytes_per_pixel <= self.local_buffer.len() {
                    match bytes_per_pixel {
                        4 => {
                            self.local_buffer[offset] = (pixel & 0xFF) as u8;
                            self.local_buffer[offset + 1] = ((pixel >> 8) & 0xFF) as u8;
                            self.local_buffer[offset + 2] = ((pixel >> 16) & 0xFF) as u8;
                            self.local_buffer[offset + 3] = ((pixel >> 24) & 0xFF) as u8;
                        }
                        3 => {
                            self.local_buffer[offset] = (pixel & 0xFF) as u8;
                            self.local_buffer[offset + 1] = ((pixel >> 8) & 0xFF) as u8;
                            self.local_buffer[offset + 2] = ((pixel >> 16) & 0xFF) as u8;
                        }
                        _ => {}
                    }
                }
            }
        }

        self.cursor_x = 0;
        self.cursor_y = 0;
        self.dirty = true;
    }

    fn locate(&mut self, row: usize, col: usize) -> Result<()> {
        // For graphics mode, locate uses pixel coordinates
        // For text mode emulation, would convert row/col to pixels
        self.cursor_y = row;
        self.cursor_x = col;
        Ok(())
    }

    fn color(&mut self, fg: Option<u8>, bg: Option<u8>) {
        if let Some(foreground) = fg {
            self.fg_color = foreground;
        }
        if let Some(background) = bg {
            self.bg_color = background;
        }
    }

    fn display(&mut self) {
        self.commit();
    }

    fn get_size(&self) -> (usize, usize) {
        (self.height, self.width)
    }

    fn get_cursor(&self) -> (usize, usize) {
        (self.cursor_y, self.cursor_x)
    }

    fn should_close(&self) -> bool {
        false
    }

    fn update(&mut self) -> Result<()> {
        if self.dirty {
            self.commit();
        }
        Ok(())
    }
}

impl Drop for OxideFramebufferBackend {
    fn drop(&mut self) {
        // Unmap framebuffer
        if !self.framebuffer.is_null() {
            libc::sys_munmap(self.framebuffer, self.local_buffer.len());
        }
        // Close file descriptor
        if self.fb_fd >= 0 {
            libc::close(self.fb_fd);
        }
    }
}

/// Create an OXIDE framebuffer backend
pub fn create_oxide_backend() -> Result<OxideFramebufferBackend> {
    OxideFramebufferBackend::new()
}
