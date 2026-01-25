//! RLE Bitmap Compression
//!
//! Implements RDP bitmap compression (Interleaved RLE).
//! The format encodes runs of identical pixels and literal runs.

use alloc::vec::Vec;

/// RLE compressor for bitmap data
pub struct RleCompressor {
    /// Output buffer (reused)
    output: Vec<u8>,
}

impl RleCompressor {
    /// Create a new RLE compressor
    pub fn new() -> Self {
        Self {
            output: Vec::with_capacity(64 * 1024),
        }
    }

    /// Compress bitmap data using Interleaved RLE
    ///
    /// Input is assumed to be in bottom-up scanline order (RDP standard).
    /// For top-down input, caller should flip the data first.
    pub fn compress(&mut self, data: &[u8], width: u32, height: u32, bpp: u16) -> Vec<u8> {
        self.output.clear();

        let bytes_per_pixel = (bpp / 8) as usize;
        let row_bytes = width as usize * bytes_per_pixel;

        // Process each row
        for row in (0..height as usize).rev() {
            let row_start = row * row_bytes;
            let row_end = row_start + row_bytes;

            if row_end > data.len() {
                break;
            }

            let row_data = &data[row_start..row_end];
            self.compress_row(row_data, bytes_per_pixel);
        }

        self.output.clone()
    }

    /// Compress a single row
    fn compress_row(&mut self, row: &[u8], bpp: usize) {
        let mut pos = 0;
        let pixel_count = row.len() / bpp;

        while pos < pixel_count {
            // Look for runs
            let run_len = self.find_run(row, pos, bpp);

            if run_len >= 3 {
                // Encode as color run
                self.encode_color_run(row, pos, run_len, bpp);
                pos += run_len;
            } else {
                // Look for foreground run (against background color)
                let fg_run = self.find_fgbg_run(row, pos, bpp);

                if fg_run >= 8 {
                    // Encode as foreground/background run
                    self.encode_fgbg_run(row, pos, fg_run, bpp);
                    pos += fg_run;
                } else {
                    // Encode as literal run
                    let lit_len = self.find_literal_run(row, pos, bpp);
                    self.encode_literal_run(row, pos, lit_len, bpp);
                    pos += lit_len;
                }
            }
        }
    }

    /// Find length of a color run (identical pixels)
    fn find_run(&self, row: &[u8], start: usize, bpp: usize) -> usize {
        let pixel_count = row.len() / bpp;
        if start >= pixel_count {
            return 0;
        }

        let start_offset = start * bpp;
        let start_pixel = &row[start_offset..start_offset + bpp];

        let mut len = 1;
        while start + len < pixel_count && len < 127 {
            let next_offset = (start + len) * bpp;
            let next_pixel = &row[next_offset..next_offset + bpp];

            if start_pixel != next_pixel {
                break;
            }
            len += 1;
        }

        len
    }

    /// Find length of foreground/background run
    fn find_fgbg_run(&self, row: &[u8], start: usize, bpp: usize) -> usize {
        let pixel_count = row.len() / bpp;
        if start >= pixel_count {
            return 0;
        }

        // Background is typically black (0x00)
        let bg = [0u8; 4];
        let start_offset = start * bpp;
        let fg = &row[start_offset..start_offset + bpp];

        let mut len = 1;
        while start + len < pixel_count && len < 127 {
            let next_offset = (start + len) * bpp;
            let next_pixel = &row[next_offset..next_offset + bpp];

            // Must be either foreground or background
            if next_pixel != fg && next_pixel != &bg[..bpp] {
                break;
            }
            len += 1;
        }

        len
    }

    /// Find length of literal (non-compressible) run
    fn find_literal_run(&self, row: &[u8], start: usize, bpp: usize) -> usize {
        let pixel_count = row.len() / bpp;
        if start >= pixel_count {
            return 0;
        }

        let mut len = 1;
        while start + len < pixel_count && len < 127 {
            // Check if a run starts here
            if self.find_run(row, start + len, bpp) >= 3 {
                break;
            }
            len += 1;
        }

        len
    }

    /// Encode a color run
    fn encode_color_run(&mut self, row: &[u8], start: usize, len: usize, bpp: usize) {
        let start_offset = start * bpp;
        let pixel = &row[start_offset..start_offset + bpp];

        // Order code + run length
        let order = match bpp {
            1 => 0x00, // REGULAR_BG_RUN / REGULAR_COLOR_RUN
            2 => 0x80, // MEGA variants for 16-bit
            3 => 0xC0, // 24-bit
            4 => 0xC0, // 32-bit
            _ => 0x00,
        };

        if len <= 31 {
            // Short form: RRRRR LLL (5 bits run, 3 bits length)
            self.output.push(order | ((len - 1) as u8 & 0x1F));
        } else {
            // Long form: 0xF0 + length byte
            self.output.push(0xF0 | order);
            self.output.push((len - 1) as u8);
        }

        // Color value
        self.output.extend_from_slice(pixel);
    }

    /// Encode a foreground/background run
    fn encode_fgbg_run(&mut self, row: &[u8], start: usize, len: usize, bpp: usize) {
        let start_offset = start * bpp;
        let fg = &row[start_offset..start_offset + bpp];
        let bg = [0u8; 4];

        // FGBG run marker
        self.output.push(0x40 | ((len - 1) as u8 & 0x3F));

        // Foreground color
        self.output.extend_from_slice(fg);

        // Bitmask (1 = foreground, 0 = background)
        let mut mask = 0u8;
        let mut bit = 0;
        for i in 0..len {
            let offset = (start + i) * bpp;
            let pixel = &row[offset..offset + bpp];

            if pixel != &bg[..bpp] {
                mask |= 1 << bit;
            }

            bit += 1;
            if bit == 8 {
                self.output.push(mask);
                mask = 0;
                bit = 0;
            }
        }
        if bit > 0 {
            self.output.push(mask);
        }
    }

    /// Encode a literal run
    fn encode_literal_run(&mut self, row: &[u8], start: usize, len: usize, bpp: usize) {
        // Literal marker
        if len <= 15 {
            self.output.push(0x80 | ((len - 1) as u8 & 0x0F));
        } else {
            self.output.push(0x8F);
            self.output.push((len - 1) as u8);
        }

        // Raw pixel data
        let start_offset = start * bpp;
        let end_offset = start_offset + len * bpp;
        self.output.extend_from_slice(&row[start_offset..end_offset]);
    }
}

impl Default for RleCompressor {
    fn default() -> Self {
        Self::new()
    }
}

/// Standalone RLE compression function
pub fn rle_compress(data: &[u8], width: u32, height: u32, bpp: u16) -> Vec<u8> {
    let mut compressor = RleCompressor::new();
    compressor.compress(data, width, height, bpp)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_run() {
        let compressor = RleCompressor::new();

        // 4 identical pixels (32-bit)
        let data = [
            0xFF, 0x00, 0x00, 0xFF, // Red
            0xFF, 0x00, 0x00, 0xFF, // Red
            0xFF, 0x00, 0x00, 0xFF, // Red
            0xFF, 0x00, 0x00, 0xFF, // Red
            0x00, 0xFF, 0x00, 0xFF, // Green (different)
        ];

        assert_eq!(compressor.find_run(&data, 0, 4), 4);
    }

    #[test]
    fn test_compression_reduces_size() {
        let mut compressor = RleCompressor::new();

        // Solid color image (very compressible)
        let width = 100;
        let height = 100;
        let bpp = 4;
        let data = vec![0xFF; width * height * bpp];

        let compressed = compressor.compress(&data, width as u32, height as u32, (bpp * 8) as u16);

        // Should be much smaller than original
        assert!(compressed.len() < data.len() / 10);
    }
}
