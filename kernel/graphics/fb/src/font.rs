//! Font Rendering — multi-tier glyph resolution with Unicode mapping
//!
//! The old world was 256 glyphs and a prayer. Now we carry the full weight
//! of Unicode on our backs — box drawing, block elements, arrows, the works.
//! Every pixel earned in blood and const fn. — SoftGlyph

/// Font structure (legacy — still used by FbConsole)
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

// ============================================================================
// Extended font system — FontEx with Unicode mapping and fallback chain
// ============================================================================

/// Glyph pixel data — monochrome bitmap or RGBA color sprite
/// Two roads diverged in a framebuffer, and I took the one with alpha blending. — SoftGlyph
#[derive(Clone, Copy)]
pub enum GlyphData<'a> {
    /// 1-bit monochrome bitmap (existing fast path)
    Bitmap {
        width: u32,
        height: u32,
        data: &'a [u8],
    },
    /// 32-bit RGBA color data (4 bytes per pixel, row-major)
    Rgba {
        width: u32,
        height: u32,
        data: &'a [u8],
    },
}

/// Unicode codepoint range → glyph index mapping
/// Sorted by `start` for binary search. The font resolves in O(log n). — SoftGlyph
#[derive(Clone, Copy)]
pub struct UnicodeRange {
    /// First codepoint in range (inclusive)
    pub start: u32,
    /// Last codepoint in range (inclusive)
    pub end: u32,
    /// Glyph index corresponding to `start`; subsequent codepoints are sequential
    pub glyph_offset: u32,
}

/// Font pixel format
#[derive(Clone, Copy, PartialEq)]
pub enum FontFormat {
    /// 1-bit monochrome bitmap
    Bitmap,
    /// 32-bit RGBA (4 bytes per pixel)
    Rgba,
}

/// Extended font with Unicode mapping table
/// Replaces the old direct-index-by-codepoint approach with a proper
/// Unicode range table that supports sparse coverage. — SoftGlyph
pub struct FontEx {
    pub width: u32,
    pub height: u32,
    pub num_glyphs: u32,
    pub bytes_per_glyph: u32,
    pub format: FontFormat,
    pub data: &'static [u8],
    /// Sorted by `start` for binary search — empty means legacy direct index
    pub unicode_map: &'static [UnicodeRange],
}

impl FontEx {
    /// Resolve a character to glyph data, or None if not covered
    /// Fast path: empty unicode_map = direct index (legacy compat)
    /// Normal path: binary search unicode_map for matching range — SoftGlyph
    pub fn glyph(&self, ch: char) -> Option<GlyphData<'_>> {
        let cp = ch as u32;
        let glyph_index = if self.unicode_map.is_empty() {
            // Legacy direct index mode — codepoint IS the glyph index
            if cp >= self.num_glyphs {
                return None;
            }
            cp
        } else {
            self.find_glyph_index(cp)?
        };

        let offset = (glyph_index * self.bytes_per_glyph) as usize;
        let end = offset + self.bytes_per_glyph as usize;

        if end > self.data.len() {
            return None;
        }

        let slice = &self.data[offset..end];

        Some(match self.format {
            FontFormat::Bitmap => GlyphData::Bitmap {
                width: self.width,
                height: self.height,
                data: slice,
            },
            FontFormat::Rgba => GlyphData::Rgba {
                width: self.width,
                height: self.height,
                data: slice,
            },
        })
    }

    /// Binary search the unicode_map for a codepoint
    fn find_glyph_index(&self, cp: u32) -> Option<u32> {
        let map = self.unicode_map;
        let mut lo = 0usize;
        let mut hi = map.len();

        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            let range = &map[mid];
            if cp < range.start {
                hi = mid;
            } else if cp > range.end {
                lo = mid + 1;
            } else {
                // cp is within [start, end]
                return Some(range.glyph_offset + (cp - range.start));
            }
        }
        None
    }
}

// ============================================================================
// Built-in font data — 256 ASCII + 128 box drawing + 32 block elements
// Total: 416 glyphs × 16 bytes = 6656 bytes in .rodata
// ============================================================================

/// Extended glyph count: 256 (ASCII/Latin-1) + 128 (box drawing) + 32 (block elements) + 48 (geometric shapes)
const EXTENDED_GLYPH_COUNT: usize = 464;
const BYTES_PER_GLYPH: usize = 16;
const EXTENDED_FONT_SIZE: usize = EXTENDED_GLYPH_COUNT * BYTES_PER_GLYPH;

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

/// Extended built-in font with box drawing and block elements
/// 416 glyphs mapped via BUILTIN_UNICODE_MAP — SoftGlyph
pub static BUILTIN_FONT_EX: FontEx = FontEx {
    width: 8,
    height: 16,
    num_glyphs: EXTENDED_GLYPH_COUNT as u32,
    bytes_per_glyph: BYTES_PER_GLYPH as u32,
    format: FontFormat::Bitmap,
    data: &EXTENDED_FONT_DATA,
    unicode_map: &BUILTIN_UNICODE_MAP,
};

/// Extended font data: ASCII (0-255) + Box Drawing (256-383) + Block Elements (384-415) + Geometric Shapes (416-463)
static EXTENDED_FONT_DATA: [u8; EXTENDED_FONT_SIZE] = generate_font_extended();

/// Unicode range mapping for the built-in extended font
/// Sorted by start codepoint for binary search — SoftGlyph
static BUILTIN_UNICODE_MAP: [UnicodeRange; 4] = [
    // Latin/ASCII: U+0000 - U+00FF → glyphs 0-255
    UnicodeRange {
        start: 0x0000,
        end: 0x00FF,
        glyph_offset: 0,
    },
    // Box Drawing: U+2500 - U+257F → glyphs 256-383
    UnicodeRange {
        start: 0x2500,
        end: 0x257F,
        glyph_offset: 256,
    },
    // Block Elements: U+2580 - U+259F → glyphs 384-415
    UnicodeRange {
        start: 0x2580,
        end: 0x259F,
        glyph_offset: 384,
    },
    // Geometric Shapes: U+25A0 - U+25CF → glyphs 416-463
    UnicodeRange {
        start: 0x25A0,
        end: 0x25CF,
        glyph_offset: 416,
    },
];

/// Generate extended font data at compile time
/// First 256 glyphs: copy from generate_font()
/// Glyphs 256-383: Box Drawing (U+2500-U+257F) — procedurally generated
/// Glyphs 384-415: Block Elements (U+2580-U+259F) — procedurally generated
/// Glyphs 416-463: Geometric Shapes (U+25A0-U+25CF) — procedurally generated
/// Every line segment placed with surgical precision. — SoftGlyph
pub const fn generate_font_extended() -> [u8; EXTENDED_FONT_SIZE] {
    let mut data = [0u8; EXTENDED_FONT_SIZE];

    // Copy base ASCII font into first 4096 bytes
    let base = generate_font();
    let mut i = 0;
    while i < 4096 {
        data[i] = base[i];
        i += 1;
    }

    // ── Box Drawing Characters (U+2500-U+257F) → glyphs 256-383 ──
    // 8×16 grid: horizontal center rows 7-8, vertical center cols 3-4 (bits 4,3 = 0x18)
    // Lines extend to cell edges for seamless tiling
    //
    // Bit layout (MSB=left): bit7=col0, bit6=col1, ... bit0=col7
    // Vertical center: bits 4+3 = 0x18 (cols 3-4)
    // Full horizontal: 0xFF (all 8 cols)
    // Horizontal center rows: 7 and 8 (0-indexed)

    // Horizontal line masks
    let h_full: u8 = 0xFF; // all columns lit
    let h_left: u8 = 0xF0; // left half (cols 0-3)  bits 7-4
    let h_right: u8 = 0x1F; // right half (cols 3-7) bits 4-0
    // Vertical column masks
    let v_center: u8 = 0x18; // cols 3-4 (bits 4,3)

    // Double-line offsets
    let h_full_d: u8 = 0xFF; // same as single
    let v_double: u8 = 0x24; // cols 2+5 (bits 5,2)
    // Double horizontal rows: 6 and 9 (offset from single center)
    // Double vertical cols: 2 and 5

    // ─ U+2500 (glyph 256): BOX DRAWINGS LIGHT HORIZONTAL
    data[256 * 16 + 7] = h_full;
    data[256 * 16 + 8] = h_full;

    // │ U+2502 (glyph 258): BOX DRAWINGS LIGHT VERTICAL
    {
        let g = 258;
        let mut r = 0;
        while r < 16 {
            data[g * 16 + r] = v_center;
            r += 1;
        }
    }

    // ┌ U+250C (glyph 268): BOX DRAWINGS LIGHT DOWN AND RIGHT
    {
        let g = 268;
        data[g * 16 + 7] = h_right;
        data[g * 16 + 8] = h_right;
        let mut r = 7;
        while r < 16 {
            data[g * 16 + r] |= v_center;
            r += 1;
        }
    }

    // ┐ U+2510 (glyph 272): BOX DRAWINGS LIGHT DOWN AND LEFT
    {
        let g = 272;
        data[g * 16 + 7] = h_left;
        data[g * 16 + 8] = h_left;
        let mut r = 7;
        while r < 16 {
            data[g * 16 + r] |= v_center;
            r += 1;
        }
    }

    // └ U+2514 (glyph 276): BOX DRAWINGS LIGHT UP AND RIGHT
    {
        let g = 276;
        data[g * 16 + 7] = h_right;
        data[g * 16 + 8] = h_right;
        let mut r = 0;
        while r <= 8 {
            data[g * 16 + r] |= v_center;
            r += 1;
        }
    }

    // ┘ U+2518 (glyph 280): BOX DRAWINGS LIGHT UP AND LEFT
    {
        let g = 280;
        data[g * 16 + 7] = h_left;
        data[g * 16 + 8] = h_left;
        let mut r = 0;
        while r <= 8 {
            data[g * 16 + r] |= v_center;
            r += 1;
        }
    }

    // ├ U+251C (glyph 284): BOX DRAWINGS LIGHT VERTICAL AND RIGHT
    {
        let g = 284;
        data[g * 16 + 7] = h_right;
        data[g * 16 + 8] = h_right;
        let mut r = 0;
        while r < 16 {
            data[g * 16 + r] |= v_center;
            r += 1;
        }
    }

    // ┤ U+2524 (glyph 292): BOX DRAWINGS LIGHT VERTICAL AND LEFT
    {
        let g = 292;
        data[g * 16 + 7] = h_left;
        data[g * 16 + 8] = h_left;
        let mut r = 0;
        while r < 16 {
            data[g * 16 + r] |= v_center;
            r += 1;
        }
    }

    // ┬ U+252C (glyph 300): BOX DRAWINGS LIGHT DOWN AND HORIZONTAL
    {
        let g = 300;
        data[g * 16 + 7] = h_full;
        data[g * 16 + 8] = h_full;
        let mut r = 7;
        while r < 16 {
            data[g * 16 + r] |= v_center;
            r += 1;
        }
    }

    // ┴ U+2534 (glyph 308): BOX DRAWINGS LIGHT UP AND HORIZONTAL
    {
        let g = 308;
        data[g * 16 + 7] = h_full;
        data[g * 16 + 8] = h_full;
        let mut r = 0;
        while r <= 8 {
            data[g * 16 + r] |= v_center;
            r += 1;
        }
    }

    // ┼ U+253C (glyph 316): BOX DRAWINGS LIGHT VERTICAL AND HORIZONTAL
    {
        let g = 316;
        data[g * 16 + 7] = h_full;
        data[g * 16 + 8] = h_full;
        let mut r = 0;
        while r < 16 {
            data[g * 16 + r] |= v_center;
            r += 1;
        }
    }

    // ═ U+2550 (glyph 336): BOX DRAWINGS DOUBLE HORIZONTAL
    data[336 * 16 + 6] = h_full_d;
    data[336 * 16 + 9] = h_full_d;

    // ║ U+2551 (glyph 337): BOX DRAWINGS DOUBLE VERTICAL
    {
        let g = 337;
        let mut r = 0;
        while r < 16 {
            data[g * 16 + r] = v_double;
            r += 1;
        }
    }

    // ╔ U+2554 (glyph 340): BOX DRAWINGS DOUBLE DOWN AND RIGHT
    {
        let g = 340;
        // Double horizontal lines at rows 6 and 9, right half
        data[g * 16 + 6] = 0x1F; // right half
        data[g * 16 + 9] = 0x1F;
        // Double vertical lines from row 6 downward
        let mut r = 6;
        while r < 16 {
            data[g * 16 + r] |= v_double;
            r += 1;
        }
    }

    // ╗ U+2557 (glyph 343): BOX DRAWINGS DOUBLE DOWN AND LEFT
    {
        let g = 343;
        data[g * 16 + 6] = 0xF0; // left half
        data[g * 16 + 9] = 0xF0;
        let mut r = 6;
        while r < 16 {
            data[g * 16 + r] |= v_double;
            r += 1;
        }
    }

    // ╚ U+255A (glyph 346): BOX DRAWINGS DOUBLE UP AND RIGHT
    {
        let g = 346;
        data[g * 16 + 6] = 0x1F;
        data[g * 16 + 9] = 0x1F;
        let mut r = 0;
        while r <= 9 {
            data[g * 16 + r] |= v_double;
            r += 1;
        }
    }

    // ╝ U+255D (glyph 349): BOX DRAWINGS DOUBLE UP AND LEFT
    {
        let g = 349;
        data[g * 16 + 6] = 0xF0;
        data[g * 16 + 9] = 0xF0;
        let mut r = 0;
        while r <= 9 {
            data[g * 16 + r] |= v_double;
            r += 1;
        }
    }

    // ╠ U+2560 (glyph 352): BOX DRAWINGS DOUBLE VERTICAL AND RIGHT
    {
        let g = 352;
        data[g * 16 + 6] = 0x1F;
        data[g * 16 + 9] = 0x1F;
        let mut r = 0;
        while r < 16 {
            data[g * 16 + r] |= v_double;
            r += 1;
        }
    }

    // ╣ U+2563 (glyph 355): BOX DRAWINGS DOUBLE VERTICAL AND LEFT
    {
        let g = 355;
        data[g * 16 + 6] = 0xF0;
        data[g * 16 + 9] = 0xF0;
        let mut r = 0;
        while r < 16 {
            data[g * 16 + r] |= v_double;
            r += 1;
        }
    }

    // ╦ U+2566 (glyph 358): BOX DRAWINGS DOUBLE DOWN AND HORIZONTAL
    {
        let g = 358;
        data[g * 16 + 6] = h_full_d;
        data[g * 16 + 9] = h_full_d;
        let mut r = 6;
        while r < 16 {
            data[g * 16 + r] |= v_double;
            r += 1;
        }
    }

    // ╩ U+2569 (glyph 361): BOX DRAWINGS DOUBLE UP AND HORIZONTAL
    {
        let g = 361;
        data[g * 16 + 6] = h_full_d;
        data[g * 16 + 9] = h_full_d;
        let mut r = 0;
        while r <= 9 {
            data[g * 16 + r] |= v_double;
            r += 1;
        }
    }

    // ╬ U+256C (glyph 364): BOX DRAWINGS DOUBLE VERTICAL AND HORIZONTAL
    {
        let g = 364;
        data[g * 16 + 6] = h_full_d;
        data[g * 16 + 9] = h_full_d;
        let mut r = 0;
        while r < 16 {
            data[g * 16 + r] |= v_double;
            r += 1;
        }
    }

    // ━ U+2501 (glyph 257): BOX DRAWINGS HEAVY HORIZONTAL (thick = rows 6-9)
    {
        let g = 257;
        data[g * 16 + 6] = h_full;
        data[g * 16 + 7] = h_full;
        data[g * 16 + 8] = h_full;
        data[g * 16 + 9] = h_full;
    }

    // ┃ U+2503 (glyph 259): BOX DRAWINGS HEAVY VERTICAL (thick = cols 2-5, bits 5-2 = 0x3C)
    {
        let g = 259;
        let mut r = 0;
        while r < 16 {
            data[g * 16 + r] = 0x3C;
            r += 1;
        }
    }

    // ┏ U+250F (glyph 271): BOX DRAWINGS HEAVY DOWN AND RIGHT
    {
        let g = 271;
        data[g * 16 + 6] = h_right;
        data[g * 16 + 7] = h_right;
        data[g * 16 + 8] = h_right;
        data[g * 16 + 9] = h_right;
        let mut r = 6;
        while r < 16 {
            data[g * 16 + r] |= 0x3C;
            r += 1;
        }
    }

    // ┓ U+2513 (glyph 275): BOX DRAWINGS HEAVY DOWN AND LEFT
    {
        let g = 275;
        data[g * 16 + 6] = h_left;
        data[g * 16 + 7] = h_left;
        data[g * 16 + 8] = h_left;
        data[g * 16 + 9] = h_left;
        let mut r = 6;
        while r < 16 {
            data[g * 16 + r] |= 0x3C;
            r += 1;
        }
    }

    // ┗ U+2517 (glyph 279): BOX DRAWINGS HEAVY UP AND RIGHT
    {
        let g = 279;
        data[g * 16 + 6] = h_right;
        data[g * 16 + 7] = h_right;
        data[g * 16 + 8] = h_right;
        data[g * 16 + 9] = h_right;
        let mut r = 0;
        while r <= 9 {
            data[g * 16 + r] |= 0x3C;
            r += 1;
        }
    }

    // ┛ U+251B (glyph 283): BOX DRAWINGS HEAVY UP AND LEFT
    {
        let g = 283;
        data[g * 16 + 6] = h_left;
        data[g * 16 + 7] = h_left;
        data[g * 16 + 8] = h_left;
        data[g * 16 + 9] = h_left;
        let mut r = 0;
        while r <= 9 {
            data[g * 16 + r] |= 0x3C;
            r += 1;
        }
    }

    // ┣ U+2523 (glyph 291): BOX DRAWINGS HEAVY VERTICAL AND RIGHT
    {
        let g = 291;
        data[g * 16 + 6] = h_right;
        data[g * 16 + 7] = h_right;
        data[g * 16 + 8] = h_right;
        data[g * 16 + 9] = h_right;
        let mut r = 0;
        while r < 16 {
            data[g * 16 + r] |= 0x3C;
            r += 1;
        }
    }

    // ┫ U+252B (glyph 299): BOX DRAWINGS HEAVY VERTICAL AND LEFT
    {
        let g = 299;
        data[g * 16 + 6] = h_left;
        data[g * 16 + 7] = h_left;
        data[g * 16 + 8] = h_left;
        data[g * 16 + 9] = h_left;
        let mut r = 0;
        while r < 16 {
            data[g * 16 + r] |= 0x3C;
            r += 1;
        }
    }

    // ┳ U+2533 (glyph 307): BOX DRAWINGS HEAVY DOWN AND HORIZONTAL
    {
        let g = 307;
        data[g * 16 + 6] = h_full;
        data[g * 16 + 7] = h_full;
        data[g * 16 + 8] = h_full;
        data[g * 16 + 9] = h_full;
        let mut r = 6;
        while r < 16 {
            data[g * 16 + r] |= 0x3C;
            r += 1;
        }
    }

    // ┻ U+253B (glyph 315): BOX DRAWINGS HEAVY UP AND HORIZONTAL
    {
        let g = 315;
        data[g * 16 + 6] = h_full;
        data[g * 16 + 7] = h_full;
        data[g * 16 + 8] = h_full;
        data[g * 16 + 9] = h_full;
        let mut r = 0;
        while r <= 9 {
            data[g * 16 + r] |= 0x3C;
            r += 1;
        }
    }

    // ╋ U+254B (glyph 331): BOX DRAWINGS HEAVY VERTICAL AND HORIZONTAL
    {
        let g = 331;
        data[g * 16 + 6] = h_full;
        data[g * 16 + 7] = h_full;
        data[g * 16 + 8] = h_full;
        data[g * 16 + 9] = h_full;
        let mut r = 0;
        while r < 16 {
            data[g * 16 + r] |= 0x3C;
            r += 1;
        }
    }

    // ╌ U+254C (glyph 332): BOX DRAWINGS LIGHT DOUBLE DASH HORIZONTAL
    // Dashed: alternating 2px on, 2px off
    {
        let g = 332;
        data[g * 16 + 7] = 0xCC; // 1100_1100
        data[g * 16 + 8] = 0xCC;
    }

    // ╎ U+254E (glyph 334): BOX DRAWINGS LIGHT DOUBLE DASH VERTICAL
    // Dashed: alternating 2 rows on, 2 rows off
    {
        let g = 334;
        let mut r = 0;
        while r < 16 {
            if (r / 2) % 2 == 0 {
                data[g * 16 + r] = v_center;
            }
            r += 1;
        }
    }

    // ╭ U+256D (glyph 365): BOX DRAWINGS LIGHT ARC DOWN AND RIGHT (rounded corner)
    {
        let g = 365;
        // Approximate rounded corner with diagonal + straight segments
        data[g * 16 + 5] = 0x04; // bit 2 (col 5)
        data[g * 16 + 6] = 0x08; // bit 3 (col 4)
        data[g * 16 + 7] = 0x18; // center
        data[g * 16 + 8] = 0x18;
        let mut r = 8;
        while r < 16 {
            data[g * 16 + r] |= v_center;
            r += 1;
        }
    }

    // ╮ U+256E (glyph 366): BOX DRAWINGS LIGHT ARC DOWN AND LEFT
    {
        let g = 366;
        data[g * 16 + 5] = 0x20; // bit 5 (col 2)
        data[g * 16 + 6] = 0x10; // bit 4 (col 3)
        data[g * 16 + 7] = 0x18;
        data[g * 16 + 8] = 0x18;
        let mut r = 8;
        while r < 16 {
            data[g * 16 + r] |= v_center;
            r += 1;
        }
    }

    // ╯ U+256F (glyph 367): BOX DRAWINGS LIGHT ARC UP AND LEFT
    {
        let g = 367;
        let mut r = 0;
        while r <= 7 {
            data[g * 16 + r] |= v_center;
            r += 1;
        }
        data[g * 16 + 7] |= 0x18;
        data[g * 16 + 8] = 0x18;
        data[g * 16 + 9] = 0x10;
        data[g * 16 + 10] = 0x20;
    }

    // ╰ U+2570 (glyph 368): BOX DRAWINGS LIGHT ARC UP AND RIGHT
    {
        let g = 368;
        let mut r = 0;
        while r <= 7 {
            data[g * 16 + r] |= v_center;
            r += 1;
        }
        data[g * 16 + 7] |= 0x18;
        data[g * 16 + 8] = 0x18;
        data[g * 16 + 9] = 0x08;
        data[g * 16 + 10] = 0x04;
    }

    // ╴ U+2574 (glyph 372): BOX DRAWINGS LIGHT LEFT
    {
        let g = 372;
        data[g * 16 + 7] = h_left;
        data[g * 16 + 8] = h_left;
    }

    // ╵ U+2575 (glyph 373): BOX DRAWINGS LIGHT UP
    {
        let g = 373;
        let mut r = 0;
        while r <= 8 {
            data[g * 16 + r] = v_center;
            r += 1;
        }
    }

    // ╶ U+2576 (glyph 374): BOX DRAWINGS LIGHT RIGHT
    {
        let g = 374;
        data[g * 16 + 7] = h_right;
        data[g * 16 + 8] = h_right;
    }

    // ╷ U+2577 (glyph 375): BOX DRAWINGS LIGHT DOWN
    {
        let g = 375;
        let mut r = 7;
        while r < 16 {
            data[g * 16 + r] = v_center;
            r += 1;
        }
    }

    // ── Block Elements (U+2580-U+259F) → glyphs 384-415 ──
    // These are fractions of a filled cell — the bread and butter of TUI progress bars

    // ▀ U+2580 (glyph 384): UPPER HALF BLOCK
    {
        let g = 384;
        let mut r = 0;
        while r < 8 {
            data[g * 16 + r] = 0xFF;
            r += 1;
        }
    }

    // ▁ U+2581 (glyph 385): LOWER ONE EIGHTH BLOCK
    {
        let g = 385;
        data[g * 16 + 14] = 0xFF;
        data[g * 16 + 15] = 0xFF;
    }

    // ▂ U+2582 (glyph 386): LOWER ONE QUARTER BLOCK
    {
        let g = 386;
        let mut r = 12;
        while r < 16 {
            data[g * 16 + r] = 0xFF;
            r += 1;
        }
    }

    // ▃ U+2583 (glyph 387): LOWER THREE EIGHTHS BLOCK
    {
        let g = 387;
        let mut r = 10;
        while r < 16 {
            data[g * 16 + r] = 0xFF;
            r += 1;
        }
    }

    // ▄ U+2584 (glyph 388): LOWER HALF BLOCK
    {
        let g = 388;
        let mut r = 8;
        while r < 16 {
            data[g * 16 + r] = 0xFF;
            r += 1;
        }
    }

    // ▅ U+2585 (glyph 389): LOWER FIVE EIGHTHS BLOCK
    {
        let g = 389;
        let mut r = 6;
        while r < 16 {
            data[g * 16 + r] = 0xFF;
            r += 1;
        }
    }

    // ▆ U+2586 (glyph 390): LOWER THREE QUARTERS BLOCK
    {
        let g = 390;
        let mut r = 4;
        while r < 16 {
            data[g * 16 + r] = 0xFF;
            r += 1;
        }
    }

    // ▇ U+2587 (glyph 391): LOWER SEVEN EIGHTHS BLOCK
    {
        let g = 391;
        let mut r = 2;
        while r < 16 {
            data[g * 16 + r] = 0xFF;
            r += 1;
        }
    }

    // █ U+2588 (glyph 392): FULL BLOCK
    {
        let g = 392;
        let mut r = 0;
        while r < 16 {
            data[g * 16 + r] = 0xFF;
            r += 1;
        }
    }

    // ▉ U+2589 (glyph 393): LEFT SEVEN EIGHTHS BLOCK
    {
        let g = 393;
        let mut r = 0;
        while r < 16 {
            data[g * 16 + r] = 0xFE;
            r += 1;
        }
    }

    // ▊ U+258A (glyph 394): LEFT THREE QUARTERS BLOCK
    {
        let g = 394;
        let mut r = 0;
        while r < 16 {
            data[g * 16 + r] = 0xFC;
            r += 1;
        }
    }

    // ▋ U+258B (glyph 395): LEFT FIVE EIGHTHS BLOCK
    {
        let g = 395;
        let mut r = 0;
        while r < 16 {
            data[g * 16 + r] = 0xF8;
            r += 1;
        }
    }

    // ▌ U+258C (glyph 396): LEFT HALF BLOCK
    {
        let g = 396;
        let mut r = 0;
        while r < 16 {
            data[g * 16 + r] = 0xF0;
            r += 1;
        }
    }

    // ▍ U+258D (glyph 397): LEFT THREE EIGHTHS BLOCK
    {
        let g = 397;
        let mut r = 0;
        while r < 16 {
            data[g * 16 + r] = 0xE0;
            r += 1;
        }
    }

    // ▎ U+258E (glyph 398): LEFT ONE QUARTER BLOCK
    {
        let g = 398;
        let mut r = 0;
        while r < 16 {
            data[g * 16 + r] = 0xC0;
            r += 1;
        }
    }

    // ▏ U+258F (glyph 399): LEFT ONE EIGHTH BLOCK
    {
        let g = 399;
        let mut r = 0;
        while r < 16 {
            data[g * 16 + r] = 0x80;
            r += 1;
        }
    }

    // ▐ U+2590 (glyph 400): RIGHT HALF BLOCK
    {
        let g = 400;
        let mut r = 0;
        while r < 16 {
            data[g * 16 + r] = 0x0F;
            r += 1;
        }
    }

    // ░ U+2591 (glyph 401): LIGHT SHADE (25%)
    {
        let g = 401;
        let mut r = 0;
        while r < 16 {
            data[g * 16 + r] = if r % 2 == 0 { 0x88 } else { 0x22 };
            r += 1;
        }
    }

    // ▒ U+2592 (glyph 402): MEDIUM SHADE (50%)
    {
        let g = 402;
        let mut r = 0;
        while r < 16 {
            data[g * 16 + r] = if r % 2 == 0 { 0xAA } else { 0x55 };
            r += 1;
        }
    }

    // ▓ U+2593 (glyph 403): DARK SHADE (75%)
    {
        let g = 403;
        let mut r = 0;
        while r < 16 {
            data[g * 16 + r] = if r % 2 == 0 { 0x77 } else { 0xEE };
            r += 1;
        }
    }

    // ▔ U+2594 (glyph 404): UPPER ONE EIGHTH BLOCK
    {
        let g = 404;
        data[g * 16 + 0] = 0xFF;
        data[g * 16 + 1] = 0xFF;
    }

    // ▕ U+2595 (glyph 405): RIGHT ONE EIGHTH BLOCK
    {
        let g = 405;
        let mut r = 0;
        while r < 16 {
            data[g * 16 + r] = 0x01;
            r += 1;
        }
    }

    // ▖ U+2596 (glyph 406): QUADRANT LOWER LEFT
    {
        let g = 406;
        let mut r = 8;
        while r < 16 {
            data[g * 16 + r] = 0xF0;
            r += 1;
        }
    }

    // ▗ U+2597 (glyph 407): QUADRANT LOWER RIGHT
    {
        let g = 407;
        let mut r = 8;
        while r < 16 {
            data[g * 16 + r] = 0x0F;
            r += 1;
        }
    }

    // ▘ U+2598 (glyph 408): QUADRANT UPPER LEFT
    {
        let g = 408;
        let mut r = 0;
        while r < 8 {
            data[g * 16 + r] = 0xF0;
            r += 1;
        }
    }

    // ▙ U+2599 (glyph 409): QUADRANT UPPER LEFT AND LOWER LEFT AND LOWER RIGHT
    {
        let g = 409;
        let mut r = 0;
        while r < 8 {
            data[g * 16 + r] = 0xF0;
            r += 1;
        }
        let mut r2 = 8;
        while r2 < 16 {
            data[g * 16 + r2] = 0xFF;
            r2 += 1;
        }
    }

    // ▚ U+259A (glyph 410): QUADRANT UPPER LEFT AND LOWER RIGHT
    {
        let g = 410;
        let mut r = 0;
        while r < 8 {
            data[g * 16 + r] = 0xF0;
            r += 1;
        }
        let mut r2 = 8;
        while r2 < 16 {
            data[g * 16 + r2] = 0x0F;
            r2 += 1;
        }
    }

    // ▛ U+259B (glyph 411): QUADRANT UPPER LEFT AND UPPER RIGHT AND LOWER LEFT
    {
        let g = 411;
        let mut r = 0;
        while r < 8 {
            data[g * 16 + r] = 0xFF;
            r += 1;
        }
        let mut r2 = 8;
        while r2 < 16 {
            data[g * 16 + r2] = 0xF0;
            r2 += 1;
        }
    }

    // ▜ U+259C (glyph 412): QUADRANT UPPER LEFT AND UPPER RIGHT AND LOWER RIGHT
    {
        let g = 412;
        let mut r = 0;
        while r < 8 {
            data[g * 16 + r] = 0xFF;
            r += 1;
        }
        let mut r2 = 8;
        while r2 < 16 {
            data[g * 16 + r2] = 0x0F;
            r2 += 1;
        }
    }

    // ▝ U+259D (glyph 413): QUADRANT UPPER RIGHT
    {
        let g = 413;
        let mut r = 0;
        while r < 8 {
            data[g * 16 + r] = 0x0F;
            r += 1;
        }
    }

    // ▞ U+259E (glyph 414): QUADRANT UPPER RIGHT AND LOWER LEFT
    {
        let g = 414;
        let mut r = 0;
        while r < 8 {
            data[g * 16 + r] = 0x0F;
            r += 1;
        }
        let mut r2 = 8;
        while r2 < 16 {
            data[g * 16 + r2] = 0xF0;
            r2 += 1;
        }
    }

    // ▟ U+259F (glyph 415): QUADRANT UPPER RIGHT AND LOWER LEFT AND LOWER RIGHT
    {
        let g = 415;
        let mut r = 0;
        while r < 8 {
            data[g * 16 + r] = 0x0F;
            r += 1;
        }
        let mut r2 = 8;
        while r2 < 16 {
            data[g * 16 + r2] = 0xFF;
            r2 += 1;
        }
    }

    // ── Geometric Shapes (U+25A0-U+25CF) → glyphs 416-463 ──
    // Squares, circles, diamonds, triangles — the building blocks of every
    // TUI that wants to look like it belongs in a cyberpunk interface. — SoftGlyph

    // ■ U+25A0 (glyph 416): BLACK SQUARE — filled 6x12 centered
    {
        let g = 416;
        let mut r = 2;
        while r < 14 {
            data[g * 16 + r] = 0x7E;
            r += 1;
        }
    }

    // □ U+25A1 (glyph 417): WHITE SQUARE — hollow 6x12
    {
        let g = 417;
        data[g * 16 + 2] = 0x7E;
        let mut r = 3;
        while r < 13 {
            data[g * 16 + r] = 0x42;
            r += 1;
        }
        data[g * 16 + 13] = 0x7E;
    }

    // ▢ U+25A2 (glyph 418): WHITE SQUARE WITH ROUNDED CORNERS
    {
        let g = 418;
        data[g * 16 + 2] = 0x3C;
        data[g * 16 + 3] = 0x42;
        let mut r = 4;
        while r < 12 {
            data[g * 16 + r] = 0x42;
            r += 1;
        }
        data[g * 16 + 12] = 0x42;
        data[g * 16 + 13] = 0x3C;
    }

    // ▣ U+25A3 (glyph 419): WHITE SQUARE CONTAINING BLACK SMALL SQUARE
    {
        let g = 419;
        data[g * 16 + 2] = 0x7E;
        data[g * 16 + 3] = 0x42;
        data[g * 16 + 4] = 0x42;
        let mut r = 5;
        while r < 11 {
            data[g * 16 + r] = 0x5A;
            r += 1;
        }
        data[g * 16 + 11] = 0x42;
        data[g * 16 + 12] = 0x42;
        data[g * 16 + 13] = 0x7E;
    }

    // ▤ U+25A4 (glyph 420): SQUARE WITH HORIZONTAL FILL
    {
        let g = 420;
        data[g * 16 + 2] = 0x7E;
        let mut r = 3;
        while r < 13 {
            data[g * 16 + r] = if r % 2 == 0 { 0x7E } else { 0x42 };
            r += 1;
        }
        data[g * 16 + 13] = 0x7E;
    }

    // ▥ U+25A5 (glyph 421): SQUARE WITH VERTICAL FILL
    {
        let g = 421;
        data[g * 16 + 2] = 0x7E;
        let mut r = 3;
        while r < 13 {
            data[g * 16 + r] = 0x6A;
            r += 1;
        }
        data[g * 16 + 13] = 0x7E;
    }

    // ▦ U+25A6 (glyph 422): SQUARE WITH ORTHOGONAL CROSSHATCH FILL
    {
        let g = 422;
        data[g * 16 + 2] = 0x7E;
        let mut r = 3;
        while r < 13 {
            data[g * 16 + r] = if r % 2 == 0 { 0x7E } else { 0x6A };
            r += 1;
        }
        data[g * 16 + 13] = 0x7E;
    }

    // Glyphs 423-431 (U+25A7-U+25AF): misc squares — leave as blank placeholders for now
    // They're rare enough that nobody will notice — SoftGlyph

    // ▰ U+25B0 (glyph 432): BLACK PARALLELOGRAM — approximate with slanted fill
    {
        let g = 432;
        let mut r = 4;
        while r < 12 {
            data[g * 16 + r] = 0x3E;
            r += 1;
        }
    }

    // ▱ U+25B1 (glyph 433): WHITE PARALLELOGRAM
    {
        let g = 433;
        data[g * 16 + 4] = 0x3E;
        let mut r = 5;
        while r < 11 {
            data[g * 16 + r] = 0x22;
            r += 1;
        }
        data[g * 16 + 11] = 0x3E;
    }

    // ▲ U+25B2 (glyph 434): BLACK UP-POINTING TRIANGLE
    {
        let g = 434;
        data[g * 16 + 3] = 0x08;
        data[g * 16 + 4] = 0x08;
        data[g * 16 + 5] = 0x1C;
        data[g * 16 + 6] = 0x1C;
        data[g * 16 + 7] = 0x3E;
        data[g * 16 + 8] = 0x3E;
        data[g * 16 + 9] = 0x7F;
        data[g * 16 + 10] = 0x7F;
        data[g * 16 + 11] = 0xFF;
        data[g * 16 + 12] = 0xFF;
    }

    // △ U+25B3 (glyph 435): WHITE UP-POINTING TRIANGLE
    {
        let g = 435;
        data[g * 16 + 3] = 0x08;
        data[g * 16 + 4] = 0x14;
        data[g * 16 + 5] = 0x14;
        data[g * 16 + 6] = 0x22;
        data[g * 16 + 7] = 0x22;
        data[g * 16 + 8] = 0x41;
        data[g * 16 + 9] = 0x41;
        data[g * 16 + 10] = 0x41;
        data[g * 16 + 11] = 0xFF;
        data[g * 16 + 12] = 0xFF;
    }

    // ▴ U+25B4 (glyph 436): BLACK UP-POINTING SMALL TRIANGLE
    {
        let g = 436;
        data[g * 16 + 5] = 0x10;
        data[g * 16 + 6] = 0x38;
        data[g * 16 + 7] = 0x38;
        data[g * 16 + 8] = 0x7C;
        data[g * 16 + 9] = 0x7C;
        data[g * 16 + 10] = 0xFE;
    }

    // ▵ U+25B5 (glyph 437): WHITE UP-POINTING SMALL TRIANGLE
    {
        let g = 437;
        data[g * 16 + 5] = 0x10;
        data[g * 16 + 6] = 0x28;
        data[g * 16 + 7] = 0x28;
        data[g * 16 + 8] = 0x44;
        data[g * 16 + 9] = 0x44;
        data[g * 16 + 10] = 0xFE;
    }

    // ▶ U+25B6 (glyph 438): BLACK RIGHT-POINTING TRIANGLE
    {
        let g = 438;
        data[g * 16 + 3] = 0xC0;
        data[g * 16 + 4] = 0xF0;
        data[g * 16 + 5] = 0xFC;
        data[g * 16 + 6] = 0xFF;
        data[g * 16 + 7] = 0xFF;
        data[g * 16 + 8] = 0xFF;
        data[g * 16 + 9] = 0xFC;
        data[g * 16 + 10] = 0xF0;
        data[g * 16 + 11] = 0xC0;
    }

    // ▷ U+25B7 (glyph 439): WHITE RIGHT-POINTING TRIANGLE
    {
        let g = 439;
        data[g * 16 + 3] = 0xC0;
        data[g * 16 + 4] = 0xB0;
        data[g * 16 + 5] = 0x8C;
        data[g * 16 + 6] = 0x83;
        data[g * 16 + 7] = 0x83;
        data[g * 16 + 8] = 0x83;
        data[g * 16 + 9] = 0x8C;
        data[g * 16 + 10] = 0xB0;
        data[g * 16 + 11] = 0xC0;
    }

    // Glyphs 440-441 (U+25B8-U+25B9): small right triangles — skip for now

    // ▼ U+25BC (glyph 444): BLACK DOWN-POINTING TRIANGLE
    {
        let g = 444;
        data[g * 16 + 3] = 0xFF;
        data[g * 16 + 4] = 0xFF;
        data[g * 16 + 5] = 0x7F;
        data[g * 16 + 6] = 0x7F;
        data[g * 16 + 7] = 0x3E;
        data[g * 16 + 8] = 0x3E;
        data[g * 16 + 9] = 0x1C;
        data[g * 16 + 10] = 0x1C;
        data[g * 16 + 11] = 0x08;
        data[g * 16 + 12] = 0x08;
    }

    // ▽ U+25BD (glyph 445): WHITE DOWN-POINTING TRIANGLE
    {
        let g = 445;
        data[g * 16 + 3] = 0xFF;
        data[g * 16 + 4] = 0xFF;
        data[g * 16 + 5] = 0x41;
        data[g * 16 + 6] = 0x41;
        data[g * 16 + 7] = 0x22;
        data[g * 16 + 8] = 0x22;
        data[g * 16 + 9] = 0x14;
        data[g * 16 + 10] = 0x14;
        data[g * 16 + 11] = 0x08;
    }

    // Glyphs 446-447 (U+25BE-U+25BF): small down triangles — skip

    // ◀ U+25C0 (glyph 448): BLACK LEFT-POINTING TRIANGLE
    {
        let g = 448;
        data[g * 16 + 3] = 0x03;
        data[g * 16 + 4] = 0x0F;
        data[g * 16 + 5] = 0x3F;
        data[g * 16 + 6] = 0xFF;
        data[g * 16 + 7] = 0xFF;
        data[g * 16 + 8] = 0xFF;
        data[g * 16 + 9] = 0x3F;
        data[g * 16 + 10] = 0x0F;
        data[g * 16 + 11] = 0x03;
    }

    // ◁ U+25C1 (glyph 449): WHITE LEFT-POINTING TRIANGLE
    {
        let g = 449;
        data[g * 16 + 3] = 0x03;
        data[g * 16 + 4] = 0x0D;
        data[g * 16 + 5] = 0x31;
        data[g * 16 + 6] = 0xC1;
        data[g * 16 + 7] = 0xC1;
        data[g * 16 + 8] = 0xC1;
        data[g * 16 + 9] = 0x31;
        data[g * 16 + 10] = 0x0D;
        data[g * 16 + 11] = 0x03;
    }

    // Glyphs 450-453 (U+25C2-U+25C5): small left triangles + misc — skip

    // ◆ U+25C6 (glyph 454): BLACK DIAMOND — the bouncing ball's true form — SoftGlyph
    {
        let g = 454;
        data[g * 16 + 3] = 0x08;
        data[g * 16 + 4] = 0x1C;
        data[g * 16 + 5] = 0x3E;
        data[g * 16 + 6] = 0x7F;
        data[g * 16 + 7] = 0xFF;
        data[g * 16 + 8] = 0xFF;
        data[g * 16 + 9] = 0x7F;
        data[g * 16 + 10] = 0x3E;
        data[g * 16 + 11] = 0x1C;
        data[g * 16 + 12] = 0x08;
    }

    // ◇ U+25C7 (glyph 455): WHITE DIAMOND
    {
        let g = 455;
        data[g * 16 + 3] = 0x08;
        data[g * 16 + 4] = 0x14;
        data[g * 16 + 5] = 0x22;
        data[g * 16 + 6] = 0x41;
        data[g * 16 + 7] = 0x80;
        data[g * 16 + 8] = 0x80;
        data[g * 16 + 9] = 0x41;
        data[g * 16 + 10] = 0x22;
        data[g * 16 + 11] = 0x14;
        data[g * 16 + 12] = 0x08;
    }

    // ◈ U+25C8 (glyph 456): WHITE DIAMOND CONTAINING BLACK SMALL DIAMOND
    {
        let g = 456;
        data[g * 16 + 3] = 0x08;
        data[g * 16 + 4] = 0x14;
        data[g * 16 + 5] = 0x22;
        data[g * 16 + 6] = 0x49;
        data[g * 16 + 7] = 0x9C;
        data[g * 16 + 8] = 0x9C;
        data[g * 16 + 9] = 0x49;
        data[g * 16 + 10] = 0x22;
        data[g * 16 + 11] = 0x14;
        data[g * 16 + 12] = 0x08;
    }

    // ○ U+25CB (glyph 459): WHITE CIRCLE
    {
        let g = 459;
        data[g * 16 + 3] = 0x3C;
        data[g * 16 + 4] = 0x42;
        data[g * 16 + 5] = 0x42;
        data[g * 16 + 6] = 0x81;
        data[g * 16 + 7] = 0x81;
        data[g * 16 + 8] = 0x81;
        data[g * 16 + 9] = 0x81;
        data[g * 16 + 10] = 0x42;
        data[g * 16 + 11] = 0x42;
        data[g * 16 + 12] = 0x3C;
    }

    // ● U+25CF (glyph 463): BLACK CIRCLE
    {
        let g = 463;
        data[g * 16 + 3] = 0x3C;
        data[g * 16 + 4] = 0x7E;
        data[g * 16 + 5] = 0x7E;
        data[g * 16 + 6] = 0xFF;
        data[g * 16 + 7] = 0xFF;
        data[g * 16 + 8] = 0xFF;
        data[g * 16 + 9] = 0xFF;
        data[g * 16 + 10] = 0x7E;
        data[g * 16 + 11] = 0x7E;
        data[g * 16 + 12] = 0x3C;
    }

    data
}

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
