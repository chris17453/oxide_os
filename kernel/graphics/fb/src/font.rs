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
    pub fn glyph(&self, ch: char) -> Option<Glyph<'_>> {
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
    pub fn glyph_or_replacement(&self, ch: char) -> Glyph<'_> {
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
static BUILTIN_FONT_DATA: [u8; 4096] = generate_font();

/// Generate font data at compile time
/// Complete 8x16 VGA-compatible bitmap font for ASCII printable characters
pub const fn generate_font() -> [u8; 4096] {
    let mut data = [0u8; 4096];

    // Helper macro-like approach using const fn
    // Each character is 16 bytes (16 rows of 8 pixels each)

    // Space (32) - all zeros, already initialized

    // ! (33)
    data[33 * 16 + 2] = 0x18;
    data[33 * 16 + 3] = 0x18;
    data[33 * 16 + 4] = 0x18;
    data[33 * 16 + 5] = 0x18;
    data[33 * 16 + 6] = 0x18;
    data[33 * 16 + 7] = 0x00;
    data[33 * 16 + 8] = 0x18;
    data[33 * 16 + 9] = 0x00;

    // " (34)
    data[34 * 16 + 2] = 0x6C;
    data[34 * 16 + 3] = 0x6C;
    data[34 * 16 + 4] = 0x24;

    // # (35)
    data[35 * 16 + 2] = 0x6C;
    data[35 * 16 + 3] = 0x6C;
    data[35 * 16 + 4] = 0xFE;
    data[35 * 16 + 5] = 0x6C;
    data[35 * 16 + 6] = 0xFE;
    data[35 * 16 + 7] = 0x6C;
    data[35 * 16 + 8] = 0x6C;

    // $ (36)
    data[36 * 16 + 2] = 0x18;
    data[36 * 16 + 3] = 0x3E;
    data[36 * 16 + 4] = 0x60;
    data[36 * 16 + 5] = 0x3C;
    data[36 * 16 + 6] = 0x06;
    data[36 * 16 + 7] = 0x7C;
    data[36 * 16 + 8] = 0x18;

    // % (37)
    data[37 * 16 + 2] = 0x62;
    data[37 * 16 + 3] = 0x64;
    data[37 * 16 + 4] = 0x08;
    data[37 * 16 + 5] = 0x10;
    data[37 * 16 + 6] = 0x20;
    data[37 * 16 + 7] = 0x4C;
    data[37 * 16 + 8] = 0x8C;

    // & (38)
    data[38 * 16 + 2] = 0x30;
    data[38 * 16 + 3] = 0x48;
    data[38 * 16 + 4] = 0x30;
    data[38 * 16 + 5] = 0x56;
    data[38 * 16 + 6] = 0x88;
    data[38 * 16 + 7] = 0x88;
    data[38 * 16 + 8] = 0x76;

    // ' (39)
    data[39 * 16 + 2] = 0x18;
    data[39 * 16 + 3] = 0x18;
    data[39 * 16 + 4] = 0x30;

    // ( (40)
    data[40 * 16 + 2] = 0x0C;
    data[40 * 16 + 3] = 0x18;
    data[40 * 16 + 4] = 0x30;
    data[40 * 16 + 5] = 0x30;
    data[40 * 16 + 6] = 0x30;
    data[40 * 16 + 7] = 0x18;
    data[40 * 16 + 8] = 0x0C;

    // ) (41)
    data[41 * 16 + 2] = 0x30;
    data[41 * 16 + 3] = 0x18;
    data[41 * 16 + 4] = 0x0C;
    data[41 * 16 + 5] = 0x0C;
    data[41 * 16 + 6] = 0x0C;
    data[41 * 16 + 7] = 0x18;
    data[41 * 16 + 8] = 0x30;

    // * (42)
    data[42 * 16 + 3] = 0x66;
    data[42 * 16 + 4] = 0x3C;
    data[42 * 16 + 5] = 0xFF;
    data[42 * 16 + 6] = 0x3C;
    data[42 * 16 + 7] = 0x66;

    // + (43)
    data[43 * 16 + 4] = 0x18;
    data[43 * 16 + 5] = 0x18;
    data[43 * 16 + 6] = 0x7E;
    data[43 * 16 + 7] = 0x18;
    data[43 * 16 + 8] = 0x18;

    // , (44)
    data[44 * 16 + 8] = 0x18;
    data[44 * 16 + 9] = 0x18;
    data[44 * 16 + 10] = 0x30;

    // - (45)
    data[45 * 16 + 6] = 0x7E;

    // . (46)
    data[46 * 16 + 8] = 0x18;
    data[46 * 16 + 9] = 0x18;

    // / (47)
    data[47 * 16 + 2] = 0x06;
    data[47 * 16 + 3] = 0x0C;
    data[47 * 16 + 4] = 0x18;
    data[47 * 16 + 5] = 0x30;
    data[47 * 16 + 6] = 0x60;
    data[47 * 16 + 7] = 0xC0;

    // 0 (48)
    data[48 * 16 + 2] = 0x3C;
    data[48 * 16 + 3] = 0x66;
    data[48 * 16 + 4] = 0x6E;
    data[48 * 16 + 5] = 0x76;
    data[48 * 16 + 6] = 0x66;
    data[48 * 16 + 7] = 0x66;
    data[48 * 16 + 8] = 0x3C;

    // 1 (49)
    data[49 * 16 + 2] = 0x18;
    data[49 * 16 + 3] = 0x38;
    data[49 * 16 + 4] = 0x18;
    data[49 * 16 + 5] = 0x18;
    data[49 * 16 + 6] = 0x18;
    data[49 * 16 + 7] = 0x18;
    data[49 * 16 + 8] = 0x7E;

    // 2 (50)
    data[50 * 16 + 2] = 0x3C;
    data[50 * 16 + 3] = 0x66;
    data[50 * 16 + 4] = 0x06;
    data[50 * 16 + 5] = 0x0C;
    data[50 * 16 + 6] = 0x18;
    data[50 * 16 + 7] = 0x30;
    data[50 * 16 + 8] = 0x7E;

    // 3 (51)
    data[51 * 16 + 2] = 0x3C;
    data[51 * 16 + 3] = 0x66;
    data[51 * 16 + 4] = 0x06;
    data[51 * 16 + 5] = 0x1C;
    data[51 * 16 + 6] = 0x06;
    data[51 * 16 + 7] = 0x66;
    data[51 * 16 + 8] = 0x3C;

    // 4 (52)
    data[52 * 16 + 2] = 0x0C;
    data[52 * 16 + 3] = 0x1C;
    data[52 * 16 + 4] = 0x3C;
    data[52 * 16 + 5] = 0x6C;
    data[52 * 16 + 6] = 0x7E;
    data[52 * 16 + 7] = 0x0C;
    data[52 * 16 + 8] = 0x0C;

    // 5 (53)
    data[53 * 16 + 2] = 0x7E;
    data[53 * 16 + 3] = 0x60;
    data[53 * 16 + 4] = 0x7C;
    data[53 * 16 + 5] = 0x06;
    data[53 * 16 + 6] = 0x06;
    data[53 * 16 + 7] = 0x66;
    data[53 * 16 + 8] = 0x3C;

    // 6 (54)
    data[54 * 16 + 2] = 0x1C;
    data[54 * 16 + 3] = 0x30;
    data[54 * 16 + 4] = 0x60;
    data[54 * 16 + 5] = 0x7C;
    data[54 * 16 + 6] = 0x66;
    data[54 * 16 + 7] = 0x66;
    data[54 * 16 + 8] = 0x3C;

    // 7 (55)
    data[55 * 16 + 2] = 0x7E;
    data[55 * 16 + 3] = 0x06;
    data[55 * 16 + 4] = 0x0C;
    data[55 * 16 + 5] = 0x18;
    data[55 * 16 + 6] = 0x30;
    data[55 * 16 + 7] = 0x30;
    data[55 * 16 + 8] = 0x30;

    // 8 (56)
    data[56 * 16 + 2] = 0x3C;
    data[56 * 16 + 3] = 0x66;
    data[56 * 16 + 4] = 0x66;
    data[56 * 16 + 5] = 0x3C;
    data[56 * 16 + 6] = 0x66;
    data[56 * 16 + 7] = 0x66;
    data[56 * 16 + 8] = 0x3C;

    // 9 (57)
    data[57 * 16 + 2] = 0x3C;
    data[57 * 16 + 3] = 0x66;
    data[57 * 16 + 4] = 0x66;
    data[57 * 16 + 5] = 0x3E;
    data[57 * 16 + 6] = 0x06;
    data[57 * 16 + 7] = 0x0C;
    data[57 * 16 + 8] = 0x38;

    // : (58)
    data[58 * 16 + 4] = 0x18;
    data[58 * 16 + 5] = 0x18;
    data[58 * 16 + 7] = 0x18;
    data[58 * 16 + 8] = 0x18;

    // ; (59)
    data[59 * 16 + 4] = 0x18;
    data[59 * 16 + 5] = 0x18;
    data[59 * 16 + 7] = 0x18;
    data[59 * 16 + 8] = 0x18;
    data[59 * 16 + 9] = 0x30;

    // < (60)
    data[60 * 16 + 3] = 0x06;
    data[60 * 16 + 4] = 0x0C;
    data[60 * 16 + 5] = 0x18;
    data[60 * 16 + 6] = 0x30;
    data[60 * 16 + 7] = 0x18;
    data[60 * 16 + 8] = 0x0C;
    data[60 * 16 + 9] = 0x06;

    // = (61)
    data[61 * 16 + 4] = 0x7E;
    data[61 * 16 + 6] = 0x7E;

    // > (62)
    data[62 * 16 + 3] = 0x60;
    data[62 * 16 + 4] = 0x30;
    data[62 * 16 + 5] = 0x18;
    data[62 * 16 + 6] = 0x0C;
    data[62 * 16 + 7] = 0x18;
    data[62 * 16 + 8] = 0x30;
    data[62 * 16 + 9] = 0x60;

    // ? (63)
    data[63 * 16 + 2] = 0x3C;
    data[63 * 16 + 3] = 0x66;
    data[63 * 16 + 4] = 0x06;
    data[63 * 16 + 5] = 0x0C;
    data[63 * 16 + 6] = 0x18;
    data[63 * 16 + 7] = 0x00;
    data[63 * 16 + 8] = 0x18;

    // @ (64)
    data[64 * 16 + 2] = 0x3C;
    data[64 * 16 + 3] = 0x66;
    data[64 * 16 + 4] = 0x6E;
    data[64 * 16 + 5] = 0x6A;
    data[64 * 16 + 6] = 0x6E;
    data[64 * 16 + 7] = 0x60;
    data[64 * 16 + 8] = 0x3C;

    // A (65)
    data[65 * 16 + 2] = 0x18;
    data[65 * 16 + 3] = 0x3C;
    data[65 * 16 + 4] = 0x66;
    data[65 * 16 + 5] = 0x66;
    data[65 * 16 + 6] = 0x7E;
    data[65 * 16 + 7] = 0x66;
    data[65 * 16 + 8] = 0x66;

    // B (66)
    data[66 * 16 + 2] = 0x7C;
    data[66 * 16 + 3] = 0x66;
    data[66 * 16 + 4] = 0x66;
    data[66 * 16 + 5] = 0x7C;
    data[66 * 16 + 6] = 0x66;
    data[66 * 16 + 7] = 0x66;
    data[66 * 16 + 8] = 0x7C;

    // C (67)
    data[67 * 16 + 2] = 0x3C;
    data[67 * 16 + 3] = 0x66;
    data[67 * 16 + 4] = 0x60;
    data[67 * 16 + 5] = 0x60;
    data[67 * 16 + 6] = 0x60;
    data[67 * 16 + 7] = 0x66;
    data[67 * 16 + 8] = 0x3C;

    // D (68)
    data[68 * 16 + 2] = 0x78;
    data[68 * 16 + 3] = 0x6C;
    data[68 * 16 + 4] = 0x66;
    data[68 * 16 + 5] = 0x66;
    data[68 * 16 + 6] = 0x66;
    data[68 * 16 + 7] = 0x6C;
    data[68 * 16 + 8] = 0x78;

    // E (69)
    data[69 * 16 + 2] = 0x7E;
    data[69 * 16 + 3] = 0x60;
    data[69 * 16 + 4] = 0x60;
    data[69 * 16 + 5] = 0x7C;
    data[69 * 16 + 6] = 0x60;
    data[69 * 16 + 7] = 0x60;
    data[69 * 16 + 8] = 0x7E;

    // F (70)
    data[70 * 16 + 2] = 0x7E;
    data[70 * 16 + 3] = 0x60;
    data[70 * 16 + 4] = 0x60;
    data[70 * 16 + 5] = 0x7C;
    data[70 * 16 + 6] = 0x60;
    data[70 * 16 + 7] = 0x60;
    data[70 * 16 + 8] = 0x60;

    // G (71)
    data[71 * 16 + 2] = 0x3C;
    data[71 * 16 + 3] = 0x66;
    data[71 * 16 + 4] = 0x60;
    data[71 * 16 + 5] = 0x6E;
    data[71 * 16 + 6] = 0x66;
    data[71 * 16 + 7] = 0x66;
    data[71 * 16 + 8] = 0x3E;

    // H (72)
    data[72 * 16 + 2] = 0x66;
    data[72 * 16 + 3] = 0x66;
    data[72 * 16 + 4] = 0x66;
    data[72 * 16 + 5] = 0x7E;
    data[72 * 16 + 6] = 0x66;
    data[72 * 16 + 7] = 0x66;
    data[72 * 16 + 8] = 0x66;

    // I (73)
    data[73 * 16 + 2] = 0x3C;
    data[73 * 16 + 3] = 0x18;
    data[73 * 16 + 4] = 0x18;
    data[73 * 16 + 5] = 0x18;
    data[73 * 16 + 6] = 0x18;
    data[73 * 16 + 7] = 0x18;
    data[73 * 16 + 8] = 0x3C;

    // J (74)
    data[74 * 16 + 2] = 0x1E;
    data[74 * 16 + 3] = 0x06;
    data[74 * 16 + 4] = 0x06;
    data[74 * 16 + 5] = 0x06;
    data[74 * 16 + 6] = 0x66;
    data[74 * 16 + 7] = 0x66;
    data[74 * 16 + 8] = 0x3C;

    // K (75)
    data[75 * 16 + 2] = 0x66;
    data[75 * 16 + 3] = 0x6C;
    data[75 * 16 + 4] = 0x78;
    data[75 * 16 + 5] = 0x70;
    data[75 * 16 + 6] = 0x78;
    data[75 * 16 + 7] = 0x6C;
    data[75 * 16 + 8] = 0x66;

    // L (76)
    data[76 * 16 + 2] = 0x60;
    data[76 * 16 + 3] = 0x60;
    data[76 * 16 + 4] = 0x60;
    data[76 * 16 + 5] = 0x60;
    data[76 * 16 + 6] = 0x60;
    data[76 * 16 + 7] = 0x60;
    data[76 * 16 + 8] = 0x7E;

    // M (77)
    data[77 * 16 + 2] = 0xC6;
    data[77 * 16 + 3] = 0xEE;
    data[77 * 16 + 4] = 0xFE;
    data[77 * 16 + 5] = 0xD6;
    data[77 * 16 + 6] = 0xC6;
    data[77 * 16 + 7] = 0xC6;
    data[77 * 16 + 8] = 0xC6;

    // N (78)
    data[78 * 16 + 2] = 0x66;
    data[78 * 16 + 3] = 0x76;
    data[78 * 16 + 4] = 0x7E;
    data[78 * 16 + 5] = 0x7E;
    data[78 * 16 + 6] = 0x6E;
    data[78 * 16 + 7] = 0x66;
    data[78 * 16 + 8] = 0x66;

    // O (79)
    data[79 * 16 + 2] = 0x3C;
    data[79 * 16 + 3] = 0x66;
    data[79 * 16 + 4] = 0x66;
    data[79 * 16 + 5] = 0x66;
    data[79 * 16 + 6] = 0x66;
    data[79 * 16 + 7] = 0x66;
    data[79 * 16 + 8] = 0x3C;

    // P (80)
    data[80 * 16 + 2] = 0x7C;
    data[80 * 16 + 3] = 0x66;
    data[80 * 16 + 4] = 0x66;
    data[80 * 16 + 5] = 0x7C;
    data[80 * 16 + 6] = 0x60;
    data[80 * 16 + 7] = 0x60;
    data[80 * 16 + 8] = 0x60;

    // Q (81)
    data[81 * 16 + 2] = 0x3C;
    data[81 * 16 + 3] = 0x66;
    data[81 * 16 + 4] = 0x66;
    data[81 * 16 + 5] = 0x66;
    data[81 * 16 + 6] = 0x6A;
    data[81 * 16 + 7] = 0x6C;
    data[81 * 16 + 8] = 0x36;

    // R (82)
    data[82 * 16 + 2] = 0x7C;
    data[82 * 16 + 3] = 0x66;
    data[82 * 16 + 4] = 0x66;
    data[82 * 16 + 5] = 0x7C;
    data[82 * 16 + 6] = 0x6C;
    data[82 * 16 + 7] = 0x66;
    data[82 * 16 + 8] = 0x66;

    // S (83)
    data[83 * 16 + 2] = 0x3C;
    data[83 * 16 + 3] = 0x66;
    data[83 * 16 + 4] = 0x60;
    data[83 * 16 + 5] = 0x3C;
    data[83 * 16 + 6] = 0x06;
    data[83 * 16 + 7] = 0x66;
    data[83 * 16 + 8] = 0x3C;

    // T (84)
    data[84 * 16 + 2] = 0x7E;
    data[84 * 16 + 3] = 0x18;
    data[84 * 16 + 4] = 0x18;
    data[84 * 16 + 5] = 0x18;
    data[84 * 16 + 6] = 0x18;
    data[84 * 16 + 7] = 0x18;
    data[84 * 16 + 8] = 0x18;

    // U (85)
    data[85 * 16 + 2] = 0x66;
    data[85 * 16 + 3] = 0x66;
    data[85 * 16 + 4] = 0x66;
    data[85 * 16 + 5] = 0x66;
    data[85 * 16 + 6] = 0x66;
    data[85 * 16 + 7] = 0x66;
    data[85 * 16 + 8] = 0x3C;

    // V (86)
    data[86 * 16 + 2] = 0x66;
    data[86 * 16 + 3] = 0x66;
    data[86 * 16 + 4] = 0x66;
    data[86 * 16 + 5] = 0x66;
    data[86 * 16 + 6] = 0x3C;
    data[86 * 16 + 7] = 0x3C;
    data[86 * 16 + 8] = 0x18;

    // W (87)
    data[87 * 16 + 2] = 0xC6;
    data[87 * 16 + 3] = 0xC6;
    data[87 * 16 + 4] = 0xC6;
    data[87 * 16 + 5] = 0xD6;
    data[87 * 16 + 6] = 0xFE;
    data[87 * 16 + 7] = 0xEE;
    data[87 * 16 + 8] = 0xC6;

    // X (88)
    data[88 * 16 + 2] = 0x66;
    data[88 * 16 + 3] = 0x66;
    data[88 * 16 + 4] = 0x3C;
    data[88 * 16 + 5] = 0x18;
    data[88 * 16 + 6] = 0x3C;
    data[88 * 16 + 7] = 0x66;
    data[88 * 16 + 8] = 0x66;

    // Y (89)
    data[89 * 16 + 2] = 0x66;
    data[89 * 16 + 3] = 0x66;
    data[89 * 16 + 4] = 0x66;
    data[89 * 16 + 5] = 0x3C;
    data[89 * 16 + 6] = 0x18;
    data[89 * 16 + 7] = 0x18;
    data[89 * 16 + 8] = 0x18;

    // Z (90)
    data[90 * 16 + 2] = 0x7E;
    data[90 * 16 + 3] = 0x06;
    data[90 * 16 + 4] = 0x0C;
    data[90 * 16 + 5] = 0x18;
    data[90 * 16 + 6] = 0x30;
    data[90 * 16 + 7] = 0x60;
    data[90 * 16 + 8] = 0x7E;

    // [ (91)
    data[91 * 16 + 2] = 0x3C;
    data[91 * 16 + 3] = 0x30;
    data[91 * 16 + 4] = 0x30;
    data[91 * 16 + 5] = 0x30;
    data[91 * 16 + 6] = 0x30;
    data[91 * 16 + 7] = 0x30;
    data[91 * 16 + 8] = 0x3C;

    // \ (92)
    data[92 * 16 + 2] = 0xC0;
    data[92 * 16 + 3] = 0x60;
    data[92 * 16 + 4] = 0x30;
    data[92 * 16 + 5] = 0x18;
    data[92 * 16 + 6] = 0x0C;
    data[92 * 16 + 7] = 0x06;

    // ] (93)
    data[93 * 16 + 2] = 0x3C;
    data[93 * 16 + 3] = 0x0C;
    data[93 * 16 + 4] = 0x0C;
    data[93 * 16 + 5] = 0x0C;
    data[93 * 16 + 6] = 0x0C;
    data[93 * 16 + 7] = 0x0C;
    data[93 * 16 + 8] = 0x3C;

    // ^ (94)
    data[94 * 16 + 2] = 0x10;
    data[94 * 16 + 3] = 0x38;
    data[94 * 16 + 4] = 0x6C;

    // _ (95)
    data[95 * 16 + 10] = 0xFF;

    // ` (96)
    data[96 * 16 + 2] = 0x30;
    data[96 * 16 + 3] = 0x18;

    // a (97)
    data[97 * 16 + 4] = 0x3C;
    data[97 * 16 + 5] = 0x06;
    data[97 * 16 + 6] = 0x3E;
    data[97 * 16 + 7] = 0x66;
    data[97 * 16 + 8] = 0x3E;

    // b (98)
    data[98 * 16 + 2] = 0x60;
    data[98 * 16 + 3] = 0x60;
    data[98 * 16 + 4] = 0x7C;
    data[98 * 16 + 5] = 0x66;
    data[98 * 16 + 6] = 0x66;
    data[98 * 16 + 7] = 0x66;
    data[98 * 16 + 8] = 0x7C;

    // c (99)
    data[99 * 16 + 4] = 0x3C;
    data[99 * 16 + 5] = 0x66;
    data[99 * 16 + 6] = 0x60;
    data[99 * 16 + 7] = 0x66;
    data[99 * 16 + 8] = 0x3C;

    // d (100)
    data[100 * 16 + 2] = 0x06;
    data[100 * 16 + 3] = 0x06;
    data[100 * 16 + 4] = 0x3E;
    data[100 * 16 + 5] = 0x66;
    data[100 * 16 + 6] = 0x66;
    data[100 * 16 + 7] = 0x66;
    data[100 * 16 + 8] = 0x3E;

    // e (101)
    data[101 * 16 + 4] = 0x3C;
    data[101 * 16 + 5] = 0x66;
    data[101 * 16 + 6] = 0x7E;
    data[101 * 16 + 7] = 0x60;
    data[101 * 16 + 8] = 0x3C;

    // f (102)
    data[102 * 16 + 2] = 0x1C;
    data[102 * 16 + 3] = 0x30;
    data[102 * 16 + 4] = 0x7C;
    data[102 * 16 + 5] = 0x30;
    data[102 * 16 + 6] = 0x30;
    data[102 * 16 + 7] = 0x30;
    data[102 * 16 + 8] = 0x30;

    // g (103)
    data[103 * 16 + 4] = 0x3E;
    data[103 * 16 + 5] = 0x66;
    data[103 * 16 + 6] = 0x66;
    data[103 * 16 + 7] = 0x3E;
    data[103 * 16 + 8] = 0x06;
    data[103 * 16 + 9] = 0x3C;

    // h (104)
    data[104 * 16 + 2] = 0x60;
    data[104 * 16 + 3] = 0x60;
    data[104 * 16 + 4] = 0x7C;
    data[104 * 16 + 5] = 0x66;
    data[104 * 16 + 6] = 0x66;
    data[104 * 16 + 7] = 0x66;
    data[104 * 16 + 8] = 0x66;

    // i (105)
    data[105 * 16 + 2] = 0x18;
    data[105 * 16 + 3] = 0x00;
    data[105 * 16 + 4] = 0x38;
    data[105 * 16 + 5] = 0x18;
    data[105 * 16 + 6] = 0x18;
    data[105 * 16 + 7] = 0x18;
    data[105 * 16 + 8] = 0x3C;

    // j (106)
    data[106 * 16 + 2] = 0x0C;
    data[106 * 16 + 3] = 0x00;
    data[106 * 16 + 4] = 0x1C;
    data[106 * 16 + 5] = 0x0C;
    data[106 * 16 + 6] = 0x0C;
    data[106 * 16 + 7] = 0x0C;
    data[106 * 16 + 8] = 0x6C;
    data[106 * 16 + 9] = 0x38;

    // k (107)
    data[107 * 16 + 2] = 0x60;
    data[107 * 16 + 3] = 0x60;
    data[107 * 16 + 4] = 0x66;
    data[107 * 16 + 5] = 0x6C;
    data[107 * 16 + 6] = 0x78;
    data[107 * 16 + 7] = 0x6C;
    data[107 * 16 + 8] = 0x66;

    // l (108)
    data[108 * 16 + 2] = 0x38;
    data[108 * 16 + 3] = 0x18;
    data[108 * 16 + 4] = 0x18;
    data[108 * 16 + 5] = 0x18;
    data[108 * 16 + 6] = 0x18;
    data[108 * 16 + 7] = 0x18;
    data[108 * 16 + 8] = 0x3C;

    // m (109)
    data[109 * 16 + 4] = 0x6C;
    data[109 * 16 + 5] = 0xFE;
    data[109 * 16 + 6] = 0xD6;
    data[109 * 16 + 7] = 0xC6;
    data[109 * 16 + 8] = 0xC6;

    // n (110)
    data[110 * 16 + 4] = 0x7C;
    data[110 * 16 + 5] = 0x66;
    data[110 * 16 + 6] = 0x66;
    data[110 * 16 + 7] = 0x66;
    data[110 * 16 + 8] = 0x66;

    // o (111)
    data[111 * 16 + 4] = 0x3C;
    data[111 * 16 + 5] = 0x66;
    data[111 * 16 + 6] = 0x66;
    data[111 * 16 + 7] = 0x66;
    data[111 * 16 + 8] = 0x3C;

    // p (112)
    data[112 * 16 + 4] = 0x7C;
    data[112 * 16 + 5] = 0x66;
    data[112 * 16 + 6] = 0x66;
    data[112 * 16 + 7] = 0x7C;
    data[112 * 16 + 8] = 0x60;
    data[112 * 16 + 9] = 0x60;

    // q (113)
    data[113 * 16 + 4] = 0x3E;
    data[113 * 16 + 5] = 0x66;
    data[113 * 16 + 6] = 0x66;
    data[113 * 16 + 7] = 0x3E;
    data[113 * 16 + 8] = 0x06;
    data[113 * 16 + 9] = 0x06;

    // r (114)
    data[114 * 16 + 4] = 0x6C;
    data[114 * 16 + 5] = 0x76;
    data[114 * 16 + 6] = 0x60;
    data[114 * 16 + 7] = 0x60;
    data[114 * 16 + 8] = 0x60;

    // s (115)
    data[115 * 16 + 4] = 0x3E;
    data[115 * 16 + 5] = 0x60;
    data[115 * 16 + 6] = 0x3C;
    data[115 * 16 + 7] = 0x06;
    data[115 * 16 + 8] = 0x7C;

    // t (116)
    data[116 * 16 + 2] = 0x30;
    data[116 * 16 + 3] = 0x30;
    data[116 * 16 + 4] = 0x7C;
    data[116 * 16 + 5] = 0x30;
    data[116 * 16 + 6] = 0x30;
    data[116 * 16 + 7] = 0x30;
    data[116 * 16 + 8] = 0x1C;

    // u (117)
    data[117 * 16 + 4] = 0x66;
    data[117 * 16 + 5] = 0x66;
    data[117 * 16 + 6] = 0x66;
    data[117 * 16 + 7] = 0x66;
    data[117 * 16 + 8] = 0x3E;

    // v (118)
    data[118 * 16 + 4] = 0x66;
    data[118 * 16 + 5] = 0x66;
    data[118 * 16 + 6] = 0x66;
    data[118 * 16 + 7] = 0x3C;
    data[118 * 16 + 8] = 0x18;

    // w (119)
    data[119 * 16 + 4] = 0xC6;
    data[119 * 16 + 5] = 0xC6;
    data[119 * 16 + 6] = 0xD6;
    data[119 * 16 + 7] = 0xFE;
    data[119 * 16 + 8] = 0x6C;

    // x (120)
    data[120 * 16 + 4] = 0x66;
    data[120 * 16 + 5] = 0x3C;
    data[120 * 16 + 6] = 0x18;
    data[120 * 16 + 7] = 0x3C;
    data[120 * 16 + 8] = 0x66;

    // y (121)
    data[121 * 16 + 4] = 0x66;
    data[121 * 16 + 5] = 0x66;
    data[121 * 16 + 6] = 0x66;
    data[121 * 16 + 7] = 0x3E;
    data[121 * 16 + 8] = 0x06;
    data[121 * 16 + 9] = 0x3C;

    // z (122)
    data[122 * 16 + 4] = 0x7E;
    data[122 * 16 + 5] = 0x0C;
    data[122 * 16 + 6] = 0x18;
    data[122 * 16 + 7] = 0x30;
    data[122 * 16 + 8] = 0x7E;

    // { (123)
    data[123 * 16 + 2] = 0x0E;
    data[123 * 16 + 3] = 0x18;
    data[123 * 16 + 4] = 0x18;
    data[123 * 16 + 5] = 0x70;
    data[123 * 16 + 6] = 0x18;
    data[123 * 16 + 7] = 0x18;
    data[123 * 16 + 8] = 0x0E;

    // | (124)
    data[124 * 16 + 2] = 0x18;
    data[124 * 16 + 3] = 0x18;
    data[124 * 16 + 4] = 0x18;
    data[124 * 16 + 5] = 0x18;
    data[124 * 16 + 6] = 0x18;
    data[124 * 16 + 7] = 0x18;
    data[124 * 16 + 8] = 0x18;

    // } (125)
    data[125 * 16 + 2] = 0x70;
    data[125 * 16 + 3] = 0x18;
    data[125 * 16 + 4] = 0x18;
    data[125 * 16 + 5] = 0x0E;
    data[125 * 16 + 6] = 0x18;
    data[125 * 16 + 7] = 0x18;
    data[125 * 16 + 8] = 0x70;

    // ~ (126)
    data[126 * 16 + 4] = 0x32;
    data[126 * 16 + 5] = 0x4C;

    // Block character (219)
    let mut row = 0;
    while row < 16 {
        data[219 * 16 + row] = 0xFF;
        row += 1;
    }

    data
}
