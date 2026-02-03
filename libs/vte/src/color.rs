//! Terminal color support
//!
//! Provides 16-color, 256-color, and 24-bit RGB color support.
//! No kernel/fb dependencies - returns raw RGB tuples.
//!
//! -- NeonVale: Color engine - painting the terminal canvas in 16M hues

/// Terminal color representation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TermColor {
    /// Standard 16-color ANSI color (0-15)
    Ansi16(u8),
    /// 256-color palette (0-255)
    Ansi256(u8),
    /// 24-bit RGB color
    Rgb(u8, u8, u8),
    /// Default foreground color
    DefaultFg,
    /// Default background color
    DefaultBg,
}

impl Default for TermColor {
    fn default() -> Self {
        TermColor::DefaultFg
    }
}

/// Default foreground RGB (VGA light gray)
pub const DEFAULT_FG_RGB: (u8, u8, u8) = (170, 170, 170);
/// Default background RGB (VGA black)
pub const DEFAULT_BG_RGB: (u8, u8, u8) = (0, 0, 0);

impl TermColor {
    /// Convert to RGB tuple using default palette
    pub fn to_rgb(&self, _is_fg: bool) -> (u8, u8, u8) {
        match self {
            TermColor::Ansi16(n) => ansi16_to_rgb(*n),
            TermColor::Ansi256(n) => ansi256_to_rgb(*n),
            TermColor::Rgb(r, g, b) => (*r, *g, *b),
            TermColor::DefaultFg => DEFAULT_FG_RGB,
            TermColor::DefaultBg => DEFAULT_BG_RGB,
        }
    }

    /// Convert to RGB tuple using custom palette and custom defaults
    pub fn to_rgb_with_palette(
        &self,
        _is_fg: bool,
        palette: &[(u8, u8, u8); 256],
        default_fg: (u8, u8, u8),
        default_bg: (u8, u8, u8),
    ) -> (u8, u8, u8) {
        match self {
            TermColor::Ansi16(n) => palette[*n as usize],
            TermColor::Ansi256(n) => palette[*n as usize],
            TermColor::Rgb(r, g, b) => (*r, *g, *b),
            TermColor::DefaultFg => default_fg,
            TermColor::DefaultBg => default_bg,
        }
    }
}

/// Convert ANSI 16-color palette index to RGB tuple
///
/// VGA color constants matching standard terminal colors.
pub fn ansi16_to_rgb(n: u8) -> (u8, u8, u8) {
    match n {
        0  => (0, 0, 0),         // Black
        1  => (170, 0, 0),       // Red
        2  => (0, 170, 0),       // Green
        3  => (170, 85, 0),      // Brown
        4  => (0, 0, 170),       // Blue
        5  => (170, 0, 170),     // Magenta
        6  => (0, 170, 170),     // Cyan
        7  => (170, 170, 170),   // Light gray
        8  => (85, 85, 85),      // Dark gray
        9  => (255, 85, 85),     // Light red
        10 => (85, 255, 85),     // Light green
        11 => (255, 255, 85),    // Yellow
        12 => (85, 85, 255),     // Light blue
        13 => (255, 85, 255),    // Light magenta
        14 => (85, 255, 255),    // Light cyan
        15 => (255, 255, 255),   // White
        _  => (170, 170, 170),   // Default: light gray
    }
}

/// Convert ANSI 256-color palette index to RGB tuple
pub fn ansi256_to_rgb(n: u8) -> (u8, u8, u8) {
    if n < 16 {
        // Standard 16 colors
        ansi16_to_rgb(n)
    } else if n < 232 {
        // 6x6x6 color cube (indices 16-231)
        let n = n - 16;
        let r = (n / 36) % 6;
        let g = (n / 6) % 6;
        let b = n % 6;
        // Convert to 0-255 range (0, 95, 135, 175, 215, 255)
        let r = if r == 0 { 0 } else { 55 + r * 40 };
        let g = if g == 0 { 0 } else { 55 + g * 40 };
        let b = if b == 0 { 0 } else { 55 + b * 40 };
        (r, g, b)
    } else {
        // Grayscale (indices 232-255)
        let gray = 8 + (n - 232) * 10;
        (gray, gray, gray)
    }
}
