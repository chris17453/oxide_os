//! # OXIDE TUI Library (Terminal User Interface)
//!
//! Previously named "ncurses" - renamed to avoid CVE false positives.
//!
//! Full-featured terminal UI library providing ncurses-compatible API.
//! Pure Rust implementation - memory safe and free from C ncurses vulnerabilities.
//!
//! ## Architecture
//! ```text
//! ┌──────────────────────────────────────────────┐
//! │  Application (vim, htop, etc.)               │
//! └─────────────────┬────────────────────────────┘
//!                   │
//!      ┌────────────┴────────────┐
//!      │  Ncurses High-Level API │
//!      │  (windows, colors, etc.)│
//!      └────────────┬────────────┘
//!                   │
//!      ┌────────────┴────────────┐
//!      │  Screen Management      │
//!      │  (refresh, doupdate)    │
//!      └────────────┬────────────┘
//!                   │
//!      ┌────────────┴────────────┐
//!      │  Termcap/Terminfo       │
//!      │  (capabilities)         │
//!      └────────────┬────────────┘
//!                   │
//!      ┌────────────┴────────────┐
//!      │  TTY Driver             │
//!      └─────────────────────────┘
//! ```
//!
//! -- GraveShift: Terminal UI framework - the canvas for all text interfaces

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt;

pub mod window;
pub mod screen;
pub mod input;
pub mod output;
pub mod color;
pub mod attributes;
pub mod pad;
pub mod c_api;

#[cfg(feature = "panel")]
pub mod panel;

#[cfg(feature = "menu")]
pub mod menu;

#[cfg(feature = "form")]
pub mod form;

/// Window handle type
pub type WINDOW = *mut window::WindowData;

/// Screen handle type
pub type SCREEN = *mut screen::ScreenData;

/// Character type with attributes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct chtype {
    pub ch: u32,      // Character code
    pub attr: u32,    // Attributes
}

impl chtype {
    pub const fn new(ch: char, attr: u32) -> Self {
        Self {
            ch: ch as u32,
            attr,
        }
    }
    
    pub fn character(&self) -> char {
        char::from_u32(self.ch & 0xFF).unwrap_or('\0')
    }
    
    pub fn attributes(&self) -> u32 {
        self.attr
    }
}

/// Extended character type (wide characters)
#[cfg(feature = "wide")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct cchar_t {
    pub attr: u32,
    pub chars: [u32; 5],  // Up to 5 combining characters
}

/// Error codes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    /// Operation succeeded
    Ok = 0,
    /// Operation failed
    Err = -1,
}

/// Result type for ncurses operations
pub type Result<T> = core::result::Result<T, Error>;

/// Special key codes (KEY_* constants)
pub mod keys {
    pub const KEY_BREAK: i32 = 0o401;
    pub const KEY_DOWN: i32 = 0o402;
    pub const KEY_UP: i32 = 0o403;
    pub const KEY_LEFT: i32 = 0o404;
    pub const KEY_RIGHT: i32 = 0o405;
    pub const KEY_HOME: i32 = 0o406;
    pub const KEY_BACKSPACE: i32 = 0o407;
    pub const KEY_F0: i32 = 0o410;
    pub const KEY_F1: i32 = 0o411;
    pub const KEY_F2: i32 = 0o412;
    pub const KEY_F3: i32 = 0o413;
    pub const KEY_F4: i32 = 0o414;
    pub const KEY_F5: i32 = 0o415;
    pub const KEY_F6: i32 = 0o416;
    pub const KEY_F7: i32 = 0o417;
    pub const KEY_F8: i32 = 0o420;
    pub const KEY_F9: i32 = 0o421;
    pub const KEY_F10: i32 = 0o422;
    pub const KEY_F11: i32 = 0o423;
    pub const KEY_F12: i32 = 0o424;
    pub const KEY_DL: i32 = 0o510;      // Delete line
    pub const KEY_IL: i32 = 0o511;      // Insert line
    pub const KEY_DC: i32 = 0o512;      // Delete character
    pub const KEY_IC: i32 = 0o513;      // Insert character
    pub const KEY_EIC: i32 = 0o514;     // Exit insert mode
    pub const KEY_CLEAR: i32 = 0o515;   // Clear screen
    pub const KEY_EOS: i32 = 0o516;     // Clear to end of screen
    pub const KEY_EOL: i32 = 0o517;     // Clear to end of line
    pub const KEY_SF: i32 = 0o520;      // Scroll forward
    pub const KEY_SR: i32 = 0o521;      // Scroll reverse
    pub const KEY_NPAGE: i32 = 0o522;   // Next page
    pub const KEY_PPAGE: i32 = 0o523;   // Previous page
    pub const KEY_STAB: i32 = 0o524;    // Set tab
    pub const KEY_CTAB: i32 = 0o525;    // Clear tab
    pub const KEY_CATAB: i32 = 0o526;   // Clear all tabs
    pub const KEY_ENTER: i32 = 0o527;   // Enter key
    pub const KEY_PRINT: i32 = 0o532;   // Print key
    pub const KEY_LL: i32 = 0o533;      // Lower left (home down)
    pub const KEY_A1: i32 = 0o534;      // Upper left keypad
    pub const KEY_A3: i32 = 0o535;      // Upper right keypad
    pub const KEY_B2: i32 = 0o536;      // Center keypad
    pub const KEY_C1: i32 = 0o537;      // Lower left keypad
    pub const KEY_C3: i32 = 0o540;      // Lower right keypad
    pub const KEY_BTAB: i32 = 0o541;    // Back tab
    pub const KEY_BEG: i32 = 0o542;     // Begin key
    pub const KEY_CANCEL: i32 = 0o543;  // Cancel key
    pub const KEY_CLOSE: i32 = 0o544;   // Close key
    pub const KEY_COMMAND: i32 = 0o545; // Command key
    pub const KEY_COPY: i32 = 0o546;    // Copy key
    pub const KEY_CREATE: i32 = 0o547;  // Create key
    pub const KEY_END: i32 = 0o550;     // End key
    pub const KEY_EXIT: i32 = 0o551;    // Exit key
    pub const KEY_FIND: i32 = 0o552;    // Find key
    pub const KEY_HELP: i32 = 0o553;    // Help key
    pub const KEY_MARK: i32 = 0o554;    // Mark key
    pub const KEY_MESSAGE: i32 = 0o555; // Message key
    pub const KEY_MOUSE: i32 = 0o631;   // Mouse event
    pub const KEY_RESIZE: i32 = 0o632;  // Terminal resize event
}

/// Standard color constants
pub mod colors {
    pub const COLOR_BLACK: i16 = 0;
    pub const COLOR_RED: i16 = 1;
    pub const COLOR_GREEN: i16 = 2;
    pub const COLOR_YELLOW: i16 = 3;
    pub const COLOR_BLUE: i16 = 4;
    pub const COLOR_MAGENTA: i16 = 5;
    pub const COLOR_CYAN: i16 = 6;
    pub const COLOR_WHITE: i16 = 7;
}

/// Attribute constants
pub mod attrs {
    pub const A_NORMAL: u32 = 0;
    pub const A_STANDOUT: u32 = 1 << 8;
    pub const A_UNDERLINE: u32 = 1 << 9;
    pub const A_REVERSE: u32 = 1 << 10;
    pub const A_BLINK: u32 = 1 << 11;
    pub const A_DIM: u32 = 1 << 12;
    pub const A_BOLD: u32 = 1 << 13;
    pub const A_ALTCHARSET: u32 = 1 << 14;
    pub const A_INVIS: u32 = 1 << 15;
    pub const A_PROTECT: u32 = 1 << 16;
    pub const A_CHARTEXT: u32 = 0xFF;
    pub const A_COLOR: u32 = 0xFF << 17;
}

/// Alternate character set characters
pub mod acs {
    pub const ACS_ULCORNER: char = '┌';  // Upper left corner
    pub const ACS_LLCORNER: char = '└';  // Lower left corner
    pub const ACS_URCORNER: char = '┐';  // Upper right corner
    pub const ACS_LRCORNER: char = '┘';  // Lower right corner
    pub const ACS_LTEE: char = '├';      // Left tee
    pub const ACS_RTEE: char = '┤';      // Right tee
    pub const ACS_BTEE: char = '┴';      // Bottom tee
    pub const ACS_TTEE: char = '┬';      // Top tee
    pub const ACS_HLINE: char = '─';     // Horizontal line
    pub const ACS_VLINE: char = '│';     // Vertical line
    pub const ACS_PLUS: char = '┼';      // Plus sign (cross)
    pub const ACS_S1: char = '⎺';        // Scan line 1
    pub const ACS_S9: char = '⎻';        // Scan line 9
    pub const ACS_DIAMOND: char = '◆';   // Diamond
    pub const ACS_CKBOARD: char = '▒';   // Checker board
    pub const ACS_DEGREE: char = '°';    // Degree symbol
    pub const ACS_PLMINUS: char = '±';   // Plus/minus
    pub const ACS_BULLET: char = '·';    // Bullet
    pub const ACS_LARROW: char = '←';    // Arrow left
    pub const ACS_RARROW: char = '→';    // Arrow right
    pub const ACS_DARROW: char = '↓';    // Arrow down
    pub const ACS_UARROW: char = '↑';    // Arrow up
    pub const ACS_BOARD: char = '▒';     // Board of squares
    pub const ACS_LANTERN: char = '◊';   // Lantern symbol
    pub const ACS_BLOCK: char = '█';     // Solid block
}

/// Boolean options
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoolOption {
    False = 0,
    True = 1,
}

impl From<bool> for BoolOption {
    fn from(b: bool) -> Self {
        if b { BoolOption::True } else { BoolOption::False }
    }
}

impl From<BoolOption> for bool {
    fn from(opt: BoolOption) -> Self {
        opt == BoolOption::True
    }
}

// Re-export commonly used items
pub use window::{WindowData, newwin, delwin, mvwin};
pub use screen::{ScreenData, initscr, endwin, newterm};
pub use input::{getch, wgetch, getstr, wgetstr};
pub use output::{addch, waddch, addstr, waddstr, printw, wprintw, mvprintw};
pub use color::{start_color, init_pair, init_color, color_pair, has_colors, can_change_color};
pub use attributes::{attron, attroff, attrset, attr_get, attr_set};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chtype_creation() {
        let ch = chtype::new('A', attrs::A_BOLD);
        assert_eq!(ch.character(), 'A');
        assert_eq!(ch.attributes(), attrs::A_BOLD);
    }

    #[test]
    fn test_bool_option() {
        assert_eq!(BoolOption::from(true), BoolOption::True);
        assert_eq!(BoolOption::from(false), BoolOption::False);
        assert_eq!(bool::from(BoolOption::True), true);
        assert_eq!(bool::from(BoolOption::False), false);
    }
}
