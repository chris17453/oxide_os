//! Framebuffer Abstraction

use crate::color::{Color, PixelFormat};
use core::ptr;

/// Framebuffer information from bootloader
#[derive(Debug, Clone, Copy)]
pub struct FramebufferInfo {
    /// Physical base address
    pub base: usize,
    /// Size in bytes
    pub size: usize,
    /// Width in pixels
    pub width: u32,
    /// Height in pixels
    pub height: u32,
    /// Stride in bytes (bytes per row)
    pub stride: u32,
    /// Pixel format
    pub format: PixelFormat,
}

/// Framebuffer trait
pub trait Framebuffer: Send + Sync {
    /// Get framebuffer width
    fn width(&self) -> u32;

    /// Get framebuffer height
    fn height(&self) -> u32;

    /// Get pixel format
    fn format(&self) -> PixelFormat;

    /// Get stride (bytes per row)
    fn stride(&self) -> u32;

    /// Get raw framebuffer pointer
    fn buffer(&self) -> *mut u8;

    /// Get framebuffer size in bytes
    fn size(&self) -> usize;

    /// Set a pixel
    fn set_pixel(&self, x: u32, y: u32, color: Color) {
        if x >= self.width() || y >= self.height() {
            return;
        }

        let bpp = self.format().bytes_per_pixel();
        let offset = (y * self.stride() + x * bpp) as usize;
        let buffer = self.buffer();

        unsafe {
            let pixel = buffer.add(offset);
            let mut bytes = [0u8; 4];
            color.write_to(&mut bytes, self.format());
            ptr::copy_nonoverlapping(bytes.as_ptr(), pixel, bpp as usize);
        }
    }

    /// Get a pixel color
    fn get_pixel(&self, x: u32, y: u32) -> Color {
        if x >= self.width() || y >= self.height() {
            return Color::BLACK;
        }

        let bpp = self.format().bytes_per_pixel();
        let offset = (y * self.stride() + x * bpp) as usize;
        let buffer = self.buffer();

        unsafe {
            let pixel = buffer.add(offset);
            match self.format() {
                PixelFormat::BGRA8888 => Color {
                    b: ptr::read_volatile(pixel),
                    g: ptr::read_volatile(pixel.add(1)),
                    r: ptr::read_volatile(pixel.add(2)),
                    a: ptr::read_volatile(pixel.add(3)),
                },
                PixelFormat::RGBA8888 => Color {
                    r: ptr::read_volatile(pixel),
                    g: ptr::read_volatile(pixel.add(1)),
                    b: ptr::read_volatile(pixel.add(2)),
                    a: ptr::read_volatile(pixel.add(3)),
                },
                PixelFormat::BGR888 => Color {
                    b: ptr::read_volatile(pixel),
                    g: ptr::read_volatile(pixel.add(1)),
                    r: ptr::read_volatile(pixel.add(2)),
                    a: 255,
                },
                PixelFormat::RGB888 => Color {
                    r: ptr::read_volatile(pixel),
                    g: ptr::read_volatile(pixel.add(1)),
                    b: ptr::read_volatile(pixel.add(2)),
                    a: 255,
                },
                _ => Color::BLACK,
            }
        }
    }

    /// Fill a rectangle with a color
    fn fill_rect(&self, x: u32, y: u32, w: u32, h: u32, color: Color) {
        let x_end = (x + w).min(self.width());
        let y_end = (y + h).min(self.height());

        for py in y..y_end {
            for px in x..x_end {
                self.set_pixel(px, py, color);
            }
        }
    }

    /// Fill the entire screen with a color
    fn clear(&self, color: Color) {
        self.fill_rect(0, 0, self.width(), self.height(), color);
    }

    /// Copy rectangle from one location to another
    fn copy_rect(&self, src_x: u32, src_y: u32, dst_x: u32, dst_y: u32, w: u32, h: u32) {
        let bpp = self.format().bytes_per_pixel() as usize;
        let stride = self.stride() as usize;
        let buffer = self.buffer();

        // Determine copy direction to handle overlapping regions
        let copy_forward = dst_y < src_y || (dst_y == src_y && dst_x <= src_x);

        if copy_forward {
            for row in 0..h {
                let src_offset = ((src_y + row) as usize * stride) + (src_x as usize * bpp);
                let dst_offset = ((dst_y + row) as usize * stride) + (dst_x as usize * bpp);
                let bytes = w as usize * bpp;

                unsafe {
                    ptr::copy(buffer.add(src_offset), buffer.add(dst_offset), bytes);
                }
            }
        } else {
            for row in (0..h).rev() {
                let src_offset = ((src_y + row) as usize * stride) + (src_x as usize * bpp);
                let dst_offset = ((dst_y + row) as usize * stride) + (dst_x as usize * bpp);
                let bytes = w as usize * bpp;

                unsafe {
                    ptr::copy(buffer.add(src_offset), buffer.add(dst_offset), bytes);
                }
            }
        }
    }

    /// Draw a horizontal line
    fn hline(&self, x: u32, y: u32, w: u32, color: Color) {
        if y >= self.height() {
            return;
        }

        let x_end = (x + w).min(self.width());
        for px in x..x_end {
            self.set_pixel(px, y, color);
        }
    }

    /// Draw a vertical line
    fn vline(&self, x: u32, y: u32, h: u32, color: Color) {
        if x >= self.width() {
            return;
        }

        let y_end = (y + h).min(self.height());
        for py in y..y_end {
            self.set_pixel(x, py, color);
        }
    }

    /// Draw a rectangle outline
    fn draw_rect(&self, x: u32, y: u32, w: u32, h: u32, color: Color) {
        self.hline(x, y, w, color);
        self.hline(x, y + h - 1, w, color);
        self.vline(x, y, h, color);
        self.vline(x + w - 1, y, h, color);
    }

    /// Flush framebuffer (for double buffering)
    fn flush(&self) {
        // Default: no-op for direct framebuffers
    }
}

/// Linear framebuffer implementation
pub struct LinearFramebuffer {
    info: FramebufferInfo,
}

impl LinearFramebuffer {
    /// Create a new linear framebuffer
    pub fn new(info: FramebufferInfo) -> Self {
        LinearFramebuffer { info }
    }

    /// Get framebuffer info
    pub fn info(&self) -> &FramebufferInfo {
        &self.info
    }
}

impl Framebuffer for LinearFramebuffer {
    fn width(&self) -> u32 {
        self.info.width
    }

    fn height(&self) -> u32 {
        self.info.height
    }

    fn format(&self) -> PixelFormat {
        self.info.format
    }

    fn stride(&self) -> u32 {
        self.info.stride
    }

    fn buffer(&self) -> *mut u8 {
        self.info.base as *mut u8
    }

    fn size(&self) -> usize {
        self.info.size
    }

    fn fill_rect(&self, x: u32, y: u32, w: u32, h: u32, color: Color) {
        let bpp = self.format().bytes_per_pixel();
        let stride = self.stride();
        let buffer = self.buffer();

        let x_end = (x + w).min(self.width());
        let y_end = (y + h).min(self.height());

        // Prepare pixel data
        let mut pixel_data = [0u8; 4];
        color.write_to(&mut pixel_data, self.format());

        for py in y..y_end {
            let row_start = (py * stride + x * bpp) as usize;
            for px in 0..(x_end - x) {
                let offset = row_start + (px * bpp) as usize;
                unsafe {
                    ptr::copy_nonoverlapping(
                        pixel_data.as_ptr(),
                        buffer.add(offset),
                        bpp as usize,
                    );
                }
            }
        }
    }
}

unsafe impl Send for LinearFramebuffer {}
unsafe impl Sync for LinearFramebuffer {}
