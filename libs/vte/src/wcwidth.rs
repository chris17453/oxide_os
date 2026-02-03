//! Character width calculation (wcwidth implementation)
//!
//! Determines the display width of Unicode characters.
//! Returns 0 for combining marks, 1 for normal characters, 2 for wide (CJK, emoji).
//!
//! -- SableWire: Unicode width oracle - knows how many cells each glyph eats

/// Get the display width of a character
///
/// Returns:
/// - -1 for control characters
/// - 0 for combining characters and zero-width
/// - 1 for normal width characters
/// - 2 for wide characters (CJK, emoji, etc.)
pub fn wcwidth(ch: char) -> i32 {
    let c = ch as u32;

    // C0/C1 control characters
    if c < 0x20 || (c >= 0x7F && c < 0xA0) {
        return -1;
    }

    // NULL and DEL
    if c == 0 || c == 0x7F {
        return -1;
    }

    // Zero-width characters
    if is_zero_width(c) {
        return 0;
    }

    // Wide characters (CJK, emoji, etc.)
    if is_wide(c) {
        return 2;
    }

    // Default: normal width
    1
}

/// Check if character is zero-width (combining marks, etc.)
fn is_zero_width(c: u32) -> bool {
    // Combining Diacritical Marks (0x0300 - 0x036F)
    if c >= 0x0300 && c <= 0x036F {
        return true;
    }

    // Combining Diacritical Marks Extended (0x1AB0 - 0x1AFF)
    if c >= 0x1AB0 && c <= 0x1AFF {
        return true;
    }

    // Combining Diacritical Marks Supplement (0x1DC0 - 0x1DFF)
    if c >= 0x1DC0 && c <= 0x1DFF {
        return true;
    }

    // Combining Half Marks (0xFE20 - 0xFE2F)
    if c >= 0xFE20 && c <= 0xFE2F {
        return true;
    }

    // Variation Selectors (0xFE00 - 0xFE0F)
    if c >= 0xFE00 && c <= 0xFE0F {
        return true;
    }

    // Zero Width Joiner/Non-Joiner
    if c == 0x200B || c == 0x200C || c == 0x200D {
        return true;
    }

    false
}

/// Check if character is wide (2 cells)
fn is_wide(c: u32) -> bool {
    // CJK Unified Ideographs Extension A (0x3400 - 0x4DBF)
    if c >= 0x3400 && c <= 0x4DBF {
        return true;
    }

    // CJK Unified Ideographs (0x4E00 - 0x9FFF)
    if c >= 0x4E00 && c <= 0x9FFF {
        return true;
    }

    // Hangul Syllables (0xAC00 - 0xD7A3)
    if c >= 0xAC00 && c <= 0xD7A3 {
        return true;
    }

    // CJK Compatibility Ideographs (0xF900 - 0xFAFF)
    if c >= 0xF900 && c <= 0xFAFF {
        return true;
    }

    // Fullwidth Forms (0xFF00 - 0xFFEF)
    if c >= 0xFF00 && c <= 0xFFEF {
        return true;
    }

    // CJK Unified Ideographs Extension B and beyond (0x20000 - 0x2FFFD)
    if c >= 0x20000 && c <= 0x2FFFD {
        return true;
    }

    // CJK Compatibility Ideographs Supplement (0x2F800 - 0x2FA1F)
    if c >= 0x2F800 && c <= 0x2FA1F {
        return true;
    }

    // Emoji ranges (simplified - full emoji support needs more ranges)
    // Emoticons (0x1F600 - 0x1F64F)
    if c >= 0x1F600 && c <= 0x1F64F {
        return true;
    }

    // Miscellaneous Symbols and Pictographs (0x1F300 - 0x1F5FF)
    if c >= 0x1F300 && c <= 0x1F5FF {
        return true;
    }

    // Transport and Map Symbols (0x1F680 - 0x1F6FF)
    if c >= 0x1F680 && c <= 0x1F6FF {
        return true;
    }

    // Supplemental Symbols and Pictographs (0x1F900 - 0x1F9FF)
    if c >= 0x1F900 && c <= 0x1F9FF {
        return true;
    }

    false
}
