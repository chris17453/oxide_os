//! Font Rendering (PSF2 format)

/// Font structure
pub struct Font {
    /// Glyph width in pixels
    pub width: u32,
    /// Glyph height in pixels
    pub height: u32,
    /// Number of glyphs
    pub num_glyphs: u32,
    /// Bytes per glyph
    pub bytes_per_glyph: u32,
    /// Glyph data
    pub data: &'static [u8],
}

impl Font {
    /// Get glyph for a character
    pub fn glyph(&self, ch: char) -> Option<Glyph> {
        let index = ch as u32;
        if index >= self.num_glyphs {
            return None;
        }

        let offset = (index * self.bytes_per_glyph) as usize;
        let end = offset + self.bytes_per_glyph as usize;

        if end > self.data.len() {
            return None;
        }

        Some(Glyph {
            width: self.width,
            height: self.height,
            data: &self.data[offset..end],
        })
    }

    /// Get glyph or replacement character
    pub fn glyph_or_replacement(&self, ch: char) -> Glyph {
        self.glyph(ch)
            .or_else(|| self.glyph('?'))
            .or_else(|| self.glyph(' '))
            .unwrap_or(Glyph {
                width: self.width,
                height: self.height,
                data: &[],
            })
    }
}

/// Single glyph
pub struct Glyph<'a> {
    /// Width in pixels
    pub width: u32,
    /// Height in pixels
    pub height: u32,
    /// Bitmap data (1 bit per pixel, MSB first)
    pub data: &'a [u8],
}

impl<'a> Glyph<'a> {
    /// Check if pixel is set
    pub fn pixel(&self, x: u32, y: u32) -> bool {
        if x >= self.width || y >= self.height {
            return false;
        }

        let bytes_per_row = (self.width + 7) / 8;
        let byte_index = (y * bytes_per_row + x / 8) as usize;
        let bit_index = 7 - (x % 8);

        if byte_index >= self.data.len() {
            return false;
        }

        (self.data[byte_index] >> bit_index) & 1 != 0
    }
}

/// Built-in 8x16 font (VGA compatible)
/// This is a simple 8x16 bitmap font for basic ASCII characters (32-126)
pub static PSF2_FONT: Font = Font {
    width: 8,
    height: 16,
    num_glyphs: 256,
    bytes_per_glyph: 16,
    data: &BUILTIN_FONT_DATA,
};

/// Built-in font data (8x16, 256 glyphs)
/// Each glyph is 16 bytes (16 rows, 1 byte per row)
static BUILTIN_FONT_DATA: [u8; 4096] = {
    let mut data = [0u8; 4096];

    // Space (32)
    // All zeros

    // ! (33) at offset 33*16 = 528
    // We'll embed a simple font inline

    data
};

/// Generate font data at compile time
/// For now, use a simple placeholder - real implementation would include
/// actual font bitmap data
pub const fn generate_font() -> [u8; 4096] {
    let mut data = [0u8; 4096];
    let mut i = 0;

    // Generate simple block characters for visible ASCII
    while i < 256 {
        let offset = i * 16;

        if i >= 33 && i <= 126 {
            // Visible characters - draw simple boxes for now
            // A real implementation would have proper glyph bitmaps

            // Top and bottom border
            data[offset] = 0x00;
            data[offset + 1] = 0x00;
            data[offset + 14] = 0x00;
            data[offset + 15] = 0x00;

            // Middle rows - will be overwritten for specific chars
            let mut row = 2;
            while row < 14 {
                data[offset + row] = 0x00;
                row += 1;
            }
        }

        i += 1;
    }

    // Define some basic characters

    // 'A' (65)
    let a_offset = 65 * 16;
    data[a_offset + 2] = 0x18;   // 00011000
    data[a_offset + 3] = 0x3C;   // 00111100
    data[a_offset + 4] = 0x66;   // 01100110
    data[a_offset + 5] = 0x66;   // 01100110
    data[a_offset + 6] = 0x7E;   // 01111110
    data[a_offset + 7] = 0x66;   // 01100110
    data[a_offset + 8] = 0x66;   // 01100110
    data[a_offset + 9] = 0x66;   // 01100110
    data[a_offset + 10] = 0x66;  // 01100110

    // 'B' (66)
    let b_offset = 66 * 16;
    data[b_offset + 2] = 0x7C;   // 01111100
    data[b_offset + 3] = 0x66;   // 01100110
    data[b_offset + 4] = 0x66;   // 01100110
    data[b_offset + 5] = 0x7C;   // 01111100
    data[b_offset + 6] = 0x66;   // 01100110
    data[b_offset + 7] = 0x66;   // 01100110
    data[b_offset + 8] = 0x66;   // 01100110
    data[b_offset + 9] = 0x7C;   // 01111100

    // Continue with more characters...
    // For brevity, we'll define the essential ones

    // 'E' (69)
    let e_offset = 69 * 16;
    data[e_offset + 2] = 0x7E;
    data[e_offset + 3] = 0x60;
    data[e_offset + 4] = 0x60;
    data[e_offset + 5] = 0x7C;
    data[e_offset + 6] = 0x60;
    data[e_offset + 7] = 0x60;
    data[e_offset + 8] = 0x60;
    data[e_offset + 9] = 0x7E;

    // 'F' (70)
    let f_offset = 70 * 16;
    data[f_offset + 2] = 0x7E;
    data[f_offset + 3] = 0x60;
    data[f_offset + 4] = 0x60;
    data[f_offset + 5] = 0x7C;
    data[f_offset + 6] = 0x60;
    data[f_offset + 7] = 0x60;
    data[f_offset + 8] = 0x60;
    data[f_offset + 9] = 0x60;

    // 'L' (76)
    let l_offset = 76 * 16;
    data[l_offset + 2] = 0x60;
    data[l_offset + 3] = 0x60;
    data[l_offset + 4] = 0x60;
    data[l_offset + 5] = 0x60;
    data[l_offset + 6] = 0x60;
    data[l_offset + 7] = 0x60;
    data[l_offset + 8] = 0x60;
    data[l_offset + 9] = 0x7E;

    // 'O' (79)
    let o_offset = 79 * 16;
    data[o_offset + 2] = 0x3C;
    data[o_offset + 3] = 0x66;
    data[o_offset + 4] = 0x66;
    data[o_offset + 5] = 0x66;
    data[o_offset + 6] = 0x66;
    data[o_offset + 7] = 0x66;
    data[o_offset + 8] = 0x66;
    data[o_offset + 9] = 0x3C;

    // 'U' (85)
    let u_offset = 85 * 16;
    data[u_offset + 2] = 0x66;
    data[u_offset + 3] = 0x66;
    data[u_offset + 4] = 0x66;
    data[u_offset + 5] = 0x66;
    data[u_offset + 6] = 0x66;
    data[u_offset + 7] = 0x66;
    data[u_offset + 8] = 0x66;
    data[u_offset + 9] = 0x3C;

    // 'X' (88)
    let x_offset = 88 * 16;
    data[x_offset + 2] = 0x66;
    data[x_offset + 3] = 0x66;
    data[x_offset + 4] = 0x3C;
    data[x_offset + 5] = 0x18;
    data[x_offset + 6] = 0x18;
    data[x_offset + 7] = 0x3C;
    data[x_offset + 8] = 0x66;
    data[x_offset + 9] = 0x66;

    // Lowercase letters (same patterns, shifted down a bit)
    // 'a' (97)
    let la_offset = 97 * 16;
    data[la_offset + 5] = 0x3C;
    data[la_offset + 6] = 0x06;
    data[la_offset + 7] = 0x3E;
    data[la_offset + 8] = 0x66;
    data[la_offset + 9] = 0x3E;

    // 'e' (101)
    let le_offset = 101 * 16;
    data[le_offset + 5] = 0x3C;
    data[le_offset + 6] = 0x66;
    data[le_offset + 7] = 0x7E;
    data[le_offset + 8] = 0x60;
    data[le_offset + 9] = 0x3C;

    // 'l' (108)
    let ll_offset = 108 * 16;
    data[ll_offset + 2] = 0x18;
    data[ll_offset + 3] = 0x18;
    data[ll_offset + 4] = 0x18;
    data[ll_offset + 5] = 0x18;
    data[ll_offset + 6] = 0x18;
    data[ll_offset + 7] = 0x18;
    data[ll_offset + 8] = 0x18;
    data[ll_offset + 9] = 0x0E;

    // 'o' (111)
    let lo_offset = 111 * 16;
    data[lo_offset + 5] = 0x3C;
    data[lo_offset + 6] = 0x66;
    data[lo_offset + 7] = 0x66;
    data[lo_offset + 8] = 0x66;
    data[lo_offset + 9] = 0x3C;

    // Numbers
    // '0' (48)
    let n0_offset = 48 * 16;
    data[n0_offset + 2] = 0x3C;
    data[n0_offset + 3] = 0x66;
    data[n0_offset + 4] = 0x6E;
    data[n0_offset + 5] = 0x76;
    data[n0_offset + 6] = 0x66;
    data[n0_offset + 7] = 0x66;
    data[n0_offset + 8] = 0x66;
    data[n0_offset + 9] = 0x3C;

    // '1' (49)
    let n1_offset = 49 * 16;
    data[n1_offset + 2] = 0x18;
    data[n1_offset + 3] = 0x38;
    data[n1_offset + 4] = 0x18;
    data[n1_offset + 5] = 0x18;
    data[n1_offset + 6] = 0x18;
    data[n1_offset + 7] = 0x18;
    data[n1_offset + 8] = 0x18;
    data[n1_offset + 9] = 0x7E;

    // Special characters
    // '!' (33)
    let excl_offset = 33 * 16;
    data[excl_offset + 2] = 0x18;
    data[excl_offset + 3] = 0x18;
    data[excl_offset + 4] = 0x18;
    data[excl_offset + 5] = 0x18;
    data[excl_offset + 6] = 0x18;
    data[excl_offset + 7] = 0x00;
    data[excl_offset + 8] = 0x18;
    data[excl_offset + 9] = 0x18;

    // ':' (58)
    let colon_offset = 58 * 16;
    data[colon_offset + 4] = 0x18;
    data[colon_offset + 5] = 0x18;
    data[colon_offset + 6] = 0x00;
    data[colon_offset + 7] = 0x00;
    data[colon_offset + 8] = 0x18;
    data[colon_offset + 9] = 0x18;

    // '>' (62)
    let gt_offset = 62 * 16;
    data[gt_offset + 3] = 0x60;
    data[gt_offset + 4] = 0x30;
    data[gt_offset + 5] = 0x18;
    data[gt_offset + 6] = 0x0C;
    data[gt_offset + 7] = 0x18;
    data[gt_offset + 8] = 0x30;
    data[gt_offset + 9] = 0x60;

    // '_' (95) underscore
    let under_offset = 95 * 16;
    data[under_offset + 12] = 0xFF;

    // Block character (219)
    let block_offset = 219 * 16;
    let mut row = 0;
    while row < 16 {
        data[block_offset + row] = 0xFF;
        row += 1;
    }

    data
}
