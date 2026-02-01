//! Terminal cell and attribute types
//!
//! Defines the Cell structure for screen buffer storage.

use crate::color::TermColor;

bitflags::bitflags! {
    /// Cell attribute flags
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub struct CellFlags: u16 {
        /// Bold/bright text
        const BOLD = 0x0001;
        /// Dim/faint text
        const DIM = 0x0002;
        /// Italic text
        const ITALIC = 0x0004;
        /// Underlined text
        const UNDERLINE = 0x0008;
        /// Blinking text
        const BLINK = 0x0010;
        /// Reversed foreground/background
        const REVERSE = 0x0020;
        /// Hidden/invisible text
        const HIDDEN = 0x0040;
        /// Strikethrough text
        const STRIKETHROUGH = 0x0080;
        /// Wide character (occupies 2 cells)
        const WIDE = 0x0100;
        /// Wide character continuation (second cell of wide char)
        const WIDE_CONTINUATION = 0x0200;
    }
}

/// Cell attributes (colors and flags)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CellAttrs {
    /// Foreground color
    pub fg: TermColor,
    /// Background color
    pub bg: TermColor,
    /// Attribute flags
    pub flags: CellFlags,
}

impl Default for CellAttrs {
    fn default() -> Self {
        CellAttrs {
            fg: TermColor::DefaultFg,
            bg: TermColor::DefaultBg,
            flags: CellFlags::empty(),
        }
    }
}

impl CellAttrs {
    /// Create new attributes with default colors
    pub const fn new() -> Self {
        CellAttrs {
            fg: TermColor::DefaultFg,
            bg: TermColor::DefaultBg,
            flags: CellFlags::empty(),
        }
    }

    /// Check if bold is set
    pub fn is_bold(&self) -> bool {
        self.flags.contains(CellFlags::BOLD)
    }

    /// Check if reverse is set
    pub fn is_reverse(&self) -> bool {
        self.flags.contains(CellFlags::REVERSE)
    }

    /// Get effective foreground color (accounting for reverse)
    pub fn effective_fg(&self) -> TermColor {
        if self.is_reverse() { self.bg } else { self.fg }
    }

    /// Get effective background color (accounting for reverse)
    pub fn effective_bg(&self) -> TermColor {
        if self.is_reverse() { self.fg } else { self.bg }
    }
}

/// A single cell in the terminal buffer
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cell {
    /// The character stored in this cell
    pub ch: char,
    /// Cell attributes (colors, flags)
    pub attrs: CellAttrs,
}

impl Default for Cell {
    fn default() -> Self {
        Cell {
            ch: ' ',
            attrs: CellAttrs::default(),
        }
    }
}

impl Cell {
    /// Create an empty cell with default attributes
    pub const fn empty() -> Self {
        Cell {
            ch: ' ',
            attrs: CellAttrs::new(),
        }
    }

    /// Create a cell with a character and attributes
    pub const fn new(ch: char, attrs: CellAttrs) -> Self {
        Cell { ch, attrs }
    }
}

/// Cursor shape
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorShape {
    /// Block cursor (full cell)
    Block,
    /// Underline cursor
    Underline,
    /// Vertical bar cursor
    Bar,
}

impl Default for CursorShape {
    fn default() -> Self {
        CursorShape::Block
    }
}

/// Cursor state
#[derive(Debug, Clone, Copy)]
pub struct Cursor {
    /// Row position (0-indexed)
    pub row: u32,
    /// Column position (0-indexed)
    pub col: u32,
    /// Cursor visibility
    pub visible: bool,
    /// Cursor shape
    pub shape: CursorShape,
    /// Blink state (for rendering)
    pub blink_on: bool,
}

impl Default for Cursor {
    fn default() -> Self {
        Cursor {
            row: 0,
            col: 0,
            visible: true,
            shape: CursorShape::Block,
            blink_on: true,
        }
    }
}

impl Cursor {
    /// Create a new cursor at (0, 0)
    pub const fn new() -> Self {
        Cursor {
            row: 0,
            col: 0,
            visible: true,
            shape: CursorShape::Block,
            blink_on: true,
        }
    }

    /// Move cursor to position
    pub fn goto(&mut self, row: u32, col: u32) {
        self.row = row;
        self.col = col;
    }
}
