//! Color and Pixel Format

/// Pixel format for framebuffer
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PixelFormat {
    /// 24-bit RGB (R at lowest address)
    RGB888,
    /// 32-bit RGBA (R at lowest address)
    RGBA8888,
    /// 24-bit BGR (B at lowest address)
    BGR888,
    /// 32-bit BGRA (B at lowest address) - common on x86 UEFI
    BGRA8888,
    /// 16-bit RGB565
    RGB565,
    /// Unknown format
    Unknown,
}

impl PixelFormat {
    /// Bytes per pixel for this format
    pub fn bytes_per_pixel(&self) -> u32 {
        match self {
            PixelFormat::RGB888 | PixelFormat::BGR888 => 3,
            PixelFormat::RGBA8888 | PixelFormat::BGRA8888 => 4,
            PixelFormat::RGB565 => 2,
            PixelFormat::Unknown => 4,
        }
    }
}

/// RGBA color
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    /// Create a new color
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Color { r, g, b, a: 255 }
    }

    /// Create a new color with alpha
    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Color { r, g, b, a }
    }

    /// Create from 32-bit value (0xAARRGGBB)
    pub const fn from_u32(value: u32) -> Self {
        Color {
            a: ((value >> 24) & 0xFF) as u8,
            r: ((value >> 16) & 0xFF) as u8,
            g: ((value >> 8) & 0xFF) as u8,
            b: (value & 0xFF) as u8,
        }
    }

    /// Convert to 32-bit value (0xAARRGGBB)
    pub const fn to_u32(&self) -> u32 {
        ((self.a as u32) << 24) | ((self.r as u32) << 16) | ((self.g as u32) << 8) | (self.b as u32)
    }

    /// Convert to RGB565
    pub const fn to_rgb565(&self) -> u16 {
        let r = (self.r as u16 >> 3) & 0x1F;
        let g = (self.g as u16 >> 2) & 0x3F;
        let b = (self.b as u16 >> 3) & 0x1F;
        (r << 11) | (g << 5) | b
    }

    /// Write to framebuffer in given format
    pub fn write_to(&self, buffer: &mut [u8], format: PixelFormat) {
        match format {
            PixelFormat::RGB888 => {
                buffer[0] = self.r;
                buffer[1] = self.g;
                buffer[2] = self.b;
            }
            PixelFormat::BGR888 => {
                buffer[0] = self.b;
                buffer[1] = self.g;
                buffer[2] = self.r;
            }
            PixelFormat::RGBA8888 => {
                buffer[0] = self.r;
                buffer[1] = self.g;
                buffer[2] = self.b;
                buffer[3] = self.a;
            }
            PixelFormat::BGRA8888 => {
                buffer[0] = self.b;
                buffer[1] = self.g;
                buffer[2] = self.r;
                buffer[3] = self.a;
            }
            PixelFormat::RGB565 => {
                let value = self.to_rgb565();
                buffer[0] = (value & 0xFF) as u8;
                buffer[1] = ((value >> 8) & 0xFF) as u8;
            }
            PixelFormat::Unknown => {
                // Default to BGRA
                buffer[0] = self.b;
                buffer[1] = self.g;
                buffer[2] = self.r;
                buffer[3] = self.a;
            }
        }
    }
}

// Standard colors
impl Color {
    pub const BLACK: Color = Color::new(0, 0, 0);
    pub const WHITE: Color = Color::new(255, 255, 255);
    pub const RED: Color = Color::new(255, 0, 0);
    pub const GREEN: Color = Color::new(0, 255, 0);
    pub const BLUE: Color = Color::new(0, 0, 255);
    pub const YELLOW: Color = Color::new(255, 255, 0);
    pub const CYAN: Color = Color::new(0, 255, 255);
    pub const MAGENTA: Color = Color::new(255, 0, 255);
    pub const GRAY: Color = Color::new(128, 128, 128);
    pub const DARK_GRAY: Color = Color::new(64, 64, 64);
    pub const LIGHT_GRAY: Color = Color::new(192, 192, 192);

    // VGA colors
    pub const VGA_BLACK: Color = Color::new(0, 0, 0);
    pub const VGA_BLUE: Color = Color::new(0, 0, 170);
    pub const VGA_GREEN: Color = Color::new(0, 170, 0);
    pub const VGA_CYAN: Color = Color::new(0, 170, 170);
    pub const VGA_RED: Color = Color::new(170, 0, 0);
    pub const VGA_MAGENTA: Color = Color::new(170, 0, 170);
    pub const VGA_BROWN: Color = Color::new(170, 85, 0);
    pub const VGA_LIGHT_GRAY: Color = Color::new(170, 170, 170);
    pub const VGA_DARK_GRAY: Color = Color::new(85, 85, 85);
    pub const VGA_LIGHT_BLUE: Color = Color::new(85, 85, 255);
    pub const VGA_LIGHT_GREEN: Color = Color::new(85, 255, 85);
    pub const VGA_LIGHT_CYAN: Color = Color::new(85, 255, 255);
    pub const VGA_LIGHT_RED: Color = Color::new(255, 85, 85);
    pub const VGA_LIGHT_MAGENTA: Color = Color::new(255, 85, 255);
    pub const VGA_YELLOW: Color = Color::new(255, 255, 85);
    pub const VGA_WHITE: Color = Color::new(255, 255, 255);
}

/// VGA color palette (16 colors)
pub static VGA_PALETTE: [Color; 16] = [
    Color::VGA_BLACK,
    Color::VGA_BLUE,
    Color::VGA_GREEN,
    Color::VGA_CYAN,
    Color::VGA_RED,
    Color::VGA_MAGENTA,
    Color::VGA_BROWN,
    Color::VGA_LIGHT_GRAY,
    Color::VGA_DARK_GRAY,
    Color::VGA_LIGHT_BLUE,
    Color::VGA_LIGHT_GREEN,
    Color::VGA_LIGHT_CYAN,
    Color::VGA_LIGHT_RED,
    Color::VGA_LIGHT_MAGENTA,
    Color::VGA_YELLOW,
    Color::VGA_WHITE,
];
