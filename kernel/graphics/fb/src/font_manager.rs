//! Font Manager — multi-font fallback chain with Unicode resolution
//!
//! One font to rule them all? Please. We run a fallback chain, binary search
//! every codepoint, and still hit O(1) for ASCII. The glyph industrial complex
//! stops for no one. — SoftGlyph

extern crate alloc;

use alloc::vec::Vec;
use crate::font::{FontEx, GlyphData, BUILTIN_FONT_EX};

/// Resolved glyph with cell width metadata
/// Because some glyphs think they deserve two cells. — SoftGlyph
pub struct ResolvedGlyph<'a> {
    /// The actual glyph pixel data (bitmap or RGBA)
    pub data: GlyphData<'a>,
    /// Cell width: 1 for normal, 2 for wide (CJK/emoji)
    pub cell_width: u32,
}

/// Font manager with ordered fallback chain
///
/// Resolution order:
/// 1. ASCII fast path (codepoint < 128): O(1) direct index in primary font
/// 2. Walk the fallback chain: each FontEx does binary search O(log n)
/// 3. Give up → replacement glyph ('?' from primary font)
///
/// No allocations during resolve. No locks. ISR-safe. — SoftGlyph
pub struct FontManager {
    /// Ordered font chain — first match wins
    fonts: Vec<&'static FontEx>,
    /// Cell width in pixels (from primary font)
    pub cell_width: u32,
    /// Cell height in pixels (from primary font)
    pub cell_height: u32,
}

impl FontManager {
    /// Create a new font manager with the given primary font
    /// The primary font defines cell dimensions for the entire terminal. — SoftGlyph
    pub fn new(primary: &'static FontEx) -> Self {
        let cell_width = primary.width;
        let cell_height = primary.height;
        let mut fonts = Vec::with_capacity(4);
        fonts.push(primary);
        FontManager {
            fonts,
            cell_width,
            cell_height,
        }
    }

    /// Create a font manager using the built-in extended font
    /// The default choice for anyone who respects box drawing characters. — SoftGlyph
    pub fn with_builtin() -> Self {
        Self::new(&BUILTIN_FONT_EX)
    }

    /// Add a fallback font to the end of the chain
    /// Later fonts are searched last — put your best glyphs first. — SoftGlyph
    pub fn add_fallback(&mut self, font: &'static FontEx) {
        self.fonts.push(font);
    }

    /// Resolve a character to renderable glyph data
    ///
    /// Fast path: ASCII < 128 goes straight to primary font index
    /// Slow path: walk the fallback chain, binary search each font
    /// Fallback: '?' replacement glyph from primary font
    ///
    /// Zero allocations. Zero locks. Pure lookup. — SoftGlyph
    pub fn resolve(&self, ch: char) -> ResolvedGlyph<'_> {
        let cp = ch as u32;

        // -- Fast path: ASCII hits the primary font directly
        if cp < 128 {
            if let Some(data) = self.fonts[0].glyph(ch) {
                return ResolvedGlyph {
                    data,
                    cell_width: 1,
                };
            }
        }

        // -- Walk the fallback chain for everything else
        for font in &self.fonts {
            if let Some(data) = font.glyph(ch) {
                return ResolvedGlyph {
                    data,
                    cell_width: char_width(ch),
                };
            }
        }

        // -- Nothing found: replacement glyph from primary font
        // The void stares back with a question mark. — SoftGlyph
        let replacement = self.fonts[0]
            .glyph('?')
            .unwrap_or(GlyphData::Bitmap {
                width: self.cell_width,
                height: self.cell_height,
                data: &[],
            });
        ResolvedGlyph {
            data: replacement,
            cell_width: 1,
        }
    }

    /// Check if a character has a glyph in any font in the chain
    pub fn has_glyph(&self, ch: char) -> bool {
        for font in &self.fonts {
            if font.glyph(ch).is_some() {
                return true;
            }
        }
        false
    }

    /// Get the number of fonts in the fallback chain
    pub fn font_count(&self) -> usize {
        self.fonts.len()
    }
}

/// Determine the cell width of a character
/// CJK Unified Ideographs, CJK Compatibility, fullwidth forms, and emoji
/// occupy 2 cells. Everything else is 1. — SoftGlyph
fn char_width(ch: char) -> u32 {
    let cp = ch as u32;
    match cp {
        // CJK Unified Ideographs
        0x4E00..=0x9FFF => 2,
        // CJK Unified Ideographs Extension A
        0x3400..=0x4DBF => 2,
        // CJK Unified Ideographs Extension B-F
        0x20000..=0x2FA1F => 2,
        // CJK Compatibility Ideographs
        0xF900..=0xFAFF => 2,
        // Fullwidth Forms
        0xFF01..=0xFF60 => 2,
        0xFFE0..=0xFFE6 => 2,
        // Hangul Syllables
        0xAC00..=0xD7AF => 2,
        // CJK Radicals Supplement
        0x2E80..=0x2EFF => 2,
        // Kangxi Radicals
        0x2F00..=0x2FDF => 2,
        // CJK Symbols and Punctuation
        0x3000..=0x303F => 2,
        // Hiragana
        0x3040..=0x309F => 2,
        // Katakana
        0x30A0..=0x30FF => 2,
        // Bopomofo
        0x3100..=0x312F => 2,
        // Enclosed CJK Letters
        0x3200..=0x32FF => 2,
        // CJK Compatibility
        0x3300..=0x33FF => 2,
        // Emoji modifiers and common emoji ranges
        0x1F300..=0x1F9FF => 2,
        0x1FA00..=0x1FA6F => 2,
        0x1FA70..=0x1FAFF => 2,
        // Everything else is width 1
        _ => 1,
    }
}
