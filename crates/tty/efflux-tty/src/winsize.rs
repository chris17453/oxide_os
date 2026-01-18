//! Window size structure
//!
//! Terminal window dimensions.

/// Terminal window size
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct Winsize {
    /// Number of rows
    pub ws_row: u16,
    /// Number of columns
    pub ws_col: u16,
    /// Pixel width (unused)
    pub ws_xpixel: u16,
    /// Pixel height (unused)
    pub ws_ypixel: u16,
}

impl Winsize {
    /// Create a new window size with default dimensions (80x24)
    pub fn new() -> Self {
        Winsize {
            ws_row: 24,
            ws_col: 80,
            ws_xpixel: 0,
            ws_ypixel: 0,
        }
    }

    /// Create a window size with specific dimensions
    pub fn with_size(rows: u16, cols: u16) -> Self {
        Winsize {
            ws_row: rows,
            ws_col: cols,
            ws_xpixel: 0,
            ws_ypixel: 0,
        }
    }
}
