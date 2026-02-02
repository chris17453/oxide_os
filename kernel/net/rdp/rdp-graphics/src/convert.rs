//! Pixel Format Conversion
//!
//! Converts between framebuffer pixel formats and RDP wire format (BGRA8888).

use rdp_traits::PixelFormat;

/// Pixel format converter
pub struct PixelConverter {
    source: PixelFormat,
    target: PixelFormat,
}

impl PixelConverter {
    /// Create a new pixel converter
    pub fn new(source: PixelFormat, target: PixelFormat) -> Self {
        Self { source, target }
    }

    /// Convert a single pixel to the target format
    ///
    /// Returns a 4-byte array in BGRA order (RDP wire format).
    pub fn convert_pixel(&self, pixel: &[u8]) -> [u8; 4] {
        // First decode to RGBA
        let (r, g, b, a) = self.decode_pixel(pixel);

        // Then encode to target format
        self.encode_pixel(r, g, b, a)
    }

    /// Decode a pixel from source format to RGBA components
    fn decode_pixel(&self, pixel: &[u8]) -> (u8, u8, u8, u8) {
        match self.source {
            PixelFormat::Bgra8888 => (pixel[2], pixel[1], pixel[0], pixel[3]),
            PixelFormat::Rgba8888 => (pixel[0], pixel[1], pixel[2], pixel[3]),
            PixelFormat::Bgr888 => (pixel[2], pixel[1], pixel[0], 255),
            PixelFormat::Rgb888 => (pixel[0], pixel[1], pixel[2], 255),
            PixelFormat::Rgb565 => {
                let val = u16::from_le_bytes([pixel[0], pixel[1]]);
                let r = ((val >> 11) & 0x1F) as u8;
                let g = ((val >> 5) & 0x3F) as u8;
                let b = (val & 0x1F) as u8;
                // Scale up to 8 bits
                let r = (r << 3) | (r >> 2);
                let g = (g << 2) | (g >> 4);
                let b = (b << 3) | (b >> 2);
                (r, g, b, 255)
            }
            PixelFormat::Rgb555 => {
                let val = u16::from_le_bytes([pixel[0], pixel[1]]);
                let r = ((val >> 10) & 0x1F) as u8;
                let g = ((val >> 5) & 0x1F) as u8;
                let b = (val & 0x1F) as u8;
                // Scale up to 8 bits
                let r = (r << 3) | (r >> 2);
                let g = (g << 3) | (g >> 2);
                let b = (b << 3) | (b >> 2);
                (r, g, b, 255)
            }
            PixelFormat::Indexed8 => {
                // Grayscale fallback for indexed
                let gray = pixel[0];
                (gray, gray, gray, 255)
            }
        }
    }

    /// Encode RGBA components to target format
    fn encode_pixel(&self, r: u8, g: u8, b: u8, a: u8) -> [u8; 4] {
        match self.target {
            PixelFormat::Bgra8888 => [b, g, r, a],
            PixelFormat::Rgba8888 => [r, g, b, a],
            PixelFormat::Bgr888 => [b, g, r, 0],
            PixelFormat::Rgb888 => [r, g, b, 0],
            PixelFormat::Rgb565 => {
                let val = ((r as u16 >> 3) << 11) | ((g as u16 >> 2) << 5) | (b as u16 >> 3);
                let bytes = val.to_le_bytes();
                [bytes[0], bytes[1], 0, 0]
            }
            PixelFormat::Rgb555 => {
                let val = ((r as u16 >> 3) << 10) | ((g as u16 >> 3) << 5) | (b as u16 >> 3);
                let bytes = val.to_le_bytes();
                [bytes[0], bytes[1], 0, 0]
            }
            PixelFormat::Indexed8 => {
                // Grayscale conversion
                let gray = ((r as u16 + g as u16 + b as u16) / 3) as u8;
                [gray, 0, 0, 0]
            }
        }
    }

    /// Convert a row of pixels in place
    pub fn convert_row(&self, src: &[u8], dst: &mut [u8]) {
        let src_bpp = self.source.bytes_per_pixel() as usize;
        let dst_bpp = self.target.bytes_per_pixel() as usize;

        let pixel_count = src.len() / src_bpp;
        for i in 0..pixel_count {
            let src_offset = i * src_bpp;
            let dst_offset = i * dst_bpp;
            let converted = self.convert_pixel(&src[src_offset..src_offset + src_bpp]);
            dst[dst_offset..dst_offset + dst_bpp].copy_from_slice(&converted[..dst_bpp]);
        }
    }

    /// Check if conversion is needed (formats are different)
    pub fn needs_conversion(&self) -> bool {
        self.source != self.target
    }
}

/// Convert framebuffer format to RDP BGRA
pub fn to_bgra(data: &[u8], format: PixelFormat, width: u32, height: u32) -> alloc::vec::Vec<u8> {
    let converter = PixelConverter::new(format, PixelFormat::Bgra8888);
    let src_bpp = format.bytes_per_pixel() as usize;
    let dst_bpp = 4usize;

    let mut output = alloc::vec![0u8; (width * height) as usize * dst_bpp];
    let row_pixels = width as usize;

    for y in 0..height as usize {
        let src_row_start = y * row_pixels * src_bpp;
        let dst_row_start = y * row_pixels * dst_bpp;

        for x in 0..row_pixels {
            let src_offset = src_row_start + x * src_bpp;
            let dst_offset = dst_row_start + x * dst_bpp;
            let converted = converter.convert_pixel(&data[src_offset..src_offset + src_bpp]);
            output[dst_offset..dst_offset + dst_bpp].copy_from_slice(&converted);
        }
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bgra_to_bgra() {
        let converter = PixelConverter::new(PixelFormat::Bgra8888, PixelFormat::Bgra8888);
        let pixel = [0x12, 0x34, 0x56, 0xFF]; // BGRA
        let result = converter.convert_pixel(&pixel);
        assert_eq!(result, pixel);
    }

    #[test]
    fn test_rgba_to_bgra() {
        let converter = PixelConverter::new(PixelFormat::Rgba8888, PixelFormat::Bgra8888);
        let pixel = [0xFF, 0x00, 0x00, 0xFF]; // Red in RGBA
        let result = converter.convert_pixel(&pixel);
        assert_eq!(result, [0x00, 0x00, 0xFF, 0xFF]); // Red in BGRA
    }

    #[test]
    fn test_rgb565_to_bgra() {
        let converter = PixelConverter::new(PixelFormat::Rgb565, PixelFormat::Bgra8888);
        // Pure red in RGB565: 11111 000000 00000 = 0xF800
        let pixel = [0x00, 0xF8];
        let result = converter.convert_pixel(&pixel);
        // Should be close to red
        assert!(result[2] > 240); // R channel
        assert!(result[1] < 16); // G channel
        assert!(result[0] < 16); // B channel
    }
}
