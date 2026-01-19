//! Terminal color support
//!
//! Provides 16-color, 256-color, and 24-bit RGB color support.

use fb::Color;

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

impl TermColor {
    /// Convert to framebuffer Color
    pub fn to_fb_color(&self, is_fg: bool) -> Color {
        match self {
            TermColor::Ansi16(n) => ansi16_to_color(*n),
            TermColor::Ansi256(n) => ansi256_to_color(*n),
            TermColor::Rgb(r, g, b) => Color::new(*r, *g, *b),
            TermColor::DefaultFg => Color::VGA_LIGHT_GRAY,
            TermColor::DefaultBg => Color::VGA_BLACK,
        }
    }
}

/// Convert ANSI 16-color palette index to Color
fn ansi16_to_color(n: u8) -> Color {
    match n {
        0 => Color::VGA_BLACK,
        1 => Color::VGA_RED,
        2 => Color::VGA_GREEN,
        3 => Color::VGA_BROWN,
        4 => Color::VGA_BLUE,
        5 => Color::VGA_MAGENTA,
        6 => Color::VGA_CYAN,
        7 => Color::VGA_LIGHT_GRAY,
        8 => Color::VGA_DARK_GRAY,
        9 => Color::VGA_LIGHT_RED,
        10 => Color::VGA_LIGHT_GREEN,
        11 => Color::VGA_YELLOW,
        12 => Color::VGA_LIGHT_BLUE,
        13 => Color::VGA_LIGHT_MAGENTA,
        14 => Color::VGA_LIGHT_CYAN,
        15 => Color::VGA_WHITE,
        _ => Color::VGA_LIGHT_GRAY,
    }
}

/// Convert ANSI 256-color palette index to Color
pub fn ansi256_to_color(n: u8) -> Color {
    if n < 16 {
        // Standard 16 colors
        ansi16_to_color(n)
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
        Color::new(r, g, b)
    } else {
        // Grayscale (indices 232-255)
        let gray = 8 + (n - 232) * 10;
        Color::new(gray, gray, gray)
    }
}
