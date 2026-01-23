//! Framebuffer Abstraction

use crate::color::{Color, PixelFormat};
use alloc::vec::Vec;
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

    /// Fill a rectangle with a color (OPTIMIZED)
    ///
    /// For memory-mapped framebuffers, uses volatile writes.
    /// For buffered framebuffers (like VirtIO GPU), uses regular writes + flush.
    fn fill_rect(&self, x: u32, y: u32, w: u32, h: u32, color: Color) {
        let x_end = (x + w).min(self.width());
        let y_end = (y + h).min(self.height());

        if x >= x_end || y >= y_end {
            return;
        }

        let bpp = self.format().bytes_per_pixel() as usize;
        let stride = self.stride() as usize;
        let buffer = self.buffer();
        let mut color_bytes = [0u8; 4];
        color.write_to(&mut color_bytes, self.format());

        unsafe {
            // For each row, use bulk memory operations
            for row in y..y_end {
                let line_start = (row as usize * stride) + (x as usize * bpp);
                let pixels_to_fill = (x_end - x) as usize;

                // Use optimized filling based on pixel format
                match bpp {
                    4 => {
                        // 32-bit pixels - fill as u32s for maximum speed
                        let pixel_value = u32::from_le_bytes([
                            color_bytes[0],
                            color_bytes[1],
                            color_bytes[2],
                            color_bytes[3],
                        ]);
                        let line_ptr = buffer.add(line_start) as *mut u32;
                        // Use regular write for buffered framebuffers (VirtIO GPU)
                        // Driver will flush to hardware. For direct mapped buffers,
                        // LinearFramebuffer overrides this to use volatile writes.
                        for i in 0..pixels_to_fill {
                            ptr::write(line_ptr.add(i), pixel_value);
                        }
                    }
                    3 => {
                        // 24-bit pixels - use memset-style pattern filling
                        let line_ptr = buffer.add(line_start);

                        // Create a 12-byte pattern for 4 pixels at once
                        let pattern = [
                            color_bytes[0],
                            color_bytes[1],
                            color_bytes[2], // pixel 1
                            color_bytes[0],
                            color_bytes[1],
                            color_bytes[2], // pixel 2
                            color_bytes[0],
                            color_bytes[1],
                            color_bytes[2], // pixel 3
                            color_bytes[0],
                            color_bytes[1],
                            color_bytes[2], // pixel 4
                        ];

                        // Fill in chunks of 4 pixels (12 bytes)
                        let full_chunks = pixels_to_fill / 4;
                        for i in 0..full_chunks {
                            ptr::copy_nonoverlapping(pattern.as_ptr(), line_ptr.add(i * 12), 12);
                        }

                        // Handle remaining pixels
                        let remaining = pixels_to_fill % 4;
                        for i in 0..remaining {
                            let pixel_offset = (full_chunks * 12) + (i * 3);
                            ptr::copy_nonoverlapping(
                                color_bytes.as_ptr(),
                                line_ptr.add(pixel_offset),
                                3,
                            );
                        }
                    }
                    2 => {
                        // 16-bit pixels - fill as u16s
                        let pixel_value = u16::from_le_bytes([color_bytes[0], color_bytes[1]]);
                        let line_ptr = buffer.add(line_start) as *mut u16;
                        for i in 0..pixels_to_fill {
                            ptr::write(line_ptr.add(i), pixel_value);
                        }
                    }
                    _ => {
                        // Fallback for unknown formats
                        let line_ptr = buffer.add(line_start);
                        for i in 0..pixels_to_fill {
                            let pixel_offset = i * bpp;
                            ptr::copy_nonoverlapping(
                                color_bytes.as_ptr(),
                                line_ptr.add(pixel_offset),
                                bpp,
                            );
                        }
                    }
                }
            }
        }
    }

    /// Fill the entire screen with a color (ULTRA-FAST)
    fn clear(&self, color: Color) {
        let bpp = self.format().bytes_per_pixel() as usize;
        let total_pixels = (self.width() * self.height()) as usize;
        let buffer = self.buffer();
        let mut color_bytes = [0u8; 4];
        color.write_to(&mut color_bytes, self.format());

        unsafe {
            match bpp {
                4 => {
                    // 32-bit: Use fast u32 writes (non-volatile for bulk)
                    let pixel_value = u32::from_le_bytes([
                        color_bytes[0],
                        color_bytes[1],
                        color_bytes[2],
                        color_bytes[3],
                    ]);
                    let buffer_u32 = buffer as *mut u32;
                    for i in 0..total_pixels {
                        ptr::write(buffer_u32.add(i), pixel_value);
                    }
                }
                3 => {
                    // 24-bit: Use chunked copying for better performance
                    let pattern = [
                        color_bytes[0],
                        color_bytes[1],
                        color_bytes[2], // pixel 1
                        color_bytes[0],
                        color_bytes[1],
                        color_bytes[2], // pixel 2
                        color_bytes[0],
                        color_bytes[1],
                        color_bytes[2], // pixel 3
                        color_bytes[0],
                        color_bytes[1],
                        color_bytes[2], // pixel 4
                    ];

                    let chunks_of_4 = total_pixels / 4;
                    let remaining = total_pixels % 4;

                    for i in 0..chunks_of_4 {
                        ptr::copy_nonoverlapping(pattern.as_ptr(), buffer.add(i * 12), 12);
                    }

                    // Handle remaining pixels
                    for i in 0..remaining {
                        let offset = (chunks_of_4 * 12) + (i * 3);
                        ptr::copy_nonoverlapping(color_bytes.as_ptr(), buffer.add(offset), 3);
                    }
                }
                2 => {
                    // 16-bit: Use fast u16 writes
                    let pixel_value = u16::from_le_bytes([color_bytes[0], color_bytes[1]]);
                    let buffer_u16 = buffer as *mut u16;
                    for i in 0..total_pixels {
                        ptr::write(buffer_u16.add(i), pixel_value);
                    }
                }
                _ => {
                    // Fallback: use fill_rect for safety
                    self.fill_rect(0, 0, self.width(), self.height(), color);
                }
            }
        }
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

/// Display information for the active mode
#[derive(Debug, Clone, Copy)]
pub struct DisplayInfo {
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    pub format: PixelFormat,
}

/// Rectangle for partial flushes
#[derive(Debug, Clone, Copy)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// Display mode description
#[derive(Debug, Clone, Copy)]
pub struct Mode {
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    pub bpp: u32,
    pub format: PixelFormat,
}

/// Errors returned by display backends
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayError {
    Unsupported,
    DeviceLost,
    InvalidParameter,
}

/// Display trait extends framebuffer with mode control and flushing
pub trait Display: Framebuffer {
    fn get_info(&self) -> DisplayInfo;
    fn get_modes(&self) -> Vec<Mode>;
    fn set_mode(&self, mode: Mode) -> Result<(), DisplayError>;
    fn framebuffer_mut(&self) -> &mut [u8];
    fn flush(&self, rect: Option<Rect>) -> Result<(), DisplayError>;
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

        // For direct-mapped framebuffers, use volatile writes
        unsafe {
            for py in y..y_end {
                let row_start = (py * stride + x * bpp) as usize;
                for px in 0..(x_end - x) {
                    let offset = row_start + (px * bpp) as usize;
                    // Use volatile writes for direct hardware access
                    match bpp {
                        4 => {
                            let pixel_value = u32::from_le_bytes([
                                pixel_data[0],
                                pixel_data[1],
                                pixel_data[2],
                                pixel_data[3],
                            ]);
                            ptr::write_volatile((buffer as *mut u32).add(offset / 4), pixel_value);
                        }
                        2 => {
                            let pixel_value = u16::from_le_bytes([pixel_data[0], pixel_data[1]]);
                            ptr::write_volatile((buffer as *mut u16).add(offset / 2), pixel_value);
                        }
                        _ => {
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
    }
}

impl Display for LinearFramebuffer {
    fn get_info(&self) -> DisplayInfo {
        DisplayInfo {
            width: self.info.width,
            height: self.info.height,
            stride: self.info.stride,
            format: self.info.format,
        }
    }

    fn get_modes(&self) -> Vec<Mode> {
        Vec::from([Mode {
            width: self.info.width,
            height: self.info.height,
            stride: self.info.stride,
            bpp: self.info.format.bytes_per_pixel() * 8,
            format: self.info.format,
        }])
    }

    fn set_mode(&self, _mode: Mode) -> Result<(), DisplayError> {
        Err(DisplayError::Unsupported)
    }

    fn framebuffer_mut(&self) -> &mut [u8] {
        unsafe { core::slice::from_raw_parts_mut(self.buffer(), self.size()) }
    }

    fn flush(&self, _rect: Option<Rect>) -> Result<(), DisplayError> {
        Ok(())
    }
}

unsafe impl Send for LinearFramebuffer {}
unsafe impl Sync for LinearFramebuffer {}
