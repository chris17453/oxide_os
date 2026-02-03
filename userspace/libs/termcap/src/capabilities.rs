//! # Terminal Capability Definitions
//!
//! Complete list of standard termcap and terminfo capability names.
//! Maps short 2-character termcap codes to long terminfo names.
//!
//! -- BlackLatch: Capability definitions - the contract between app and terminal

/// String capabilities (escape sequences)
pub mod strings {
    /// Clear screen capability (termcap: cl, terminfo: clear)
    pub const CLEAR: &str = "clear";
    
    /// Cursor movement capabilities
    pub const CURSOR_ADDRESS: &str = "cup";        // Move cursor to row, col
    pub const CURSOR_HOME: &str = "home";          // Home cursor
    pub const CURSOR_UP: &str = "cuu1";            // Move cursor up one line
    pub const CURSOR_DOWN: &str = "cud1";          // Move cursor down one line
    pub const CURSOR_LEFT: &str = "cub1";          // Move cursor left one space
    pub const CURSOR_RIGHT: &str = "cuf1";         // Move cursor right one space
    pub const CURSOR_INVISIBLE: &str = "civis";    // Make cursor invisible
    pub const CURSOR_VISIBLE: &str = "cnorm";      // Make cursor visible
    pub const CURSOR_VERY_VISIBLE: &str = "cvvis"; // Make cursor very visible
    
    /// Scrolling capabilities
    pub const SCROLL_FORWARD: &str = "ind";        // Scroll forward one line
    pub const SCROLL_REVERSE: &str = "ri";         // Scroll reverse one line
    pub const CHANGE_SCROLL_REGION: &str = "csr";  // Change scrolling region
    
    /// Erasing capabilities
    pub const CLRTOBOT: &str = "ed";               // Clear to bottom of screen
    pub const CLRTOEOL: &str = "el";               // Clear to end of line
    pub const CLRTOBOL: &str = "el1";              // Clear to beginning of line
    
    /// Insert/delete capabilities
    pub const INSERT_LINE: &str = "il1";           // Insert line
    pub const DELETE_LINE: &str = "dl1";           // Delete line
    pub const INSERT_CHARACTER: &str = "ich1";     // Insert character
    pub const DELETE_CHARACTER: &str = "dch1";     // Delete character
    
    /// Character attributes
    pub const ENTER_BOLD: &str = "bold";           // Enter bold mode
    pub const ENTER_DIM: &str = "dim";             // Enter dim mode
    pub const ENTER_BLINK: &str = "blink";         // Enter blink mode
    pub const ENTER_REVERSE: &str = "rev";         // Enter reverse video
    pub const ENTER_STANDOUT: &str = "smso";       // Enter standout mode
    pub const EXIT_STANDOUT: &str = "rmso";        // Exit standout mode
    pub const ENTER_UNDERLINE: &str = "smul";      // Enter underline mode
    pub const EXIT_UNDERLINE: &str = "rmul";       // Exit underline mode
    pub const EXIT_ATTRIBUTES: &str = "sgr0";      // Turn off all attributes
    pub const SET_ATTRIBUTES: &str = "sgr";        // Set attributes
    
    /// Color capabilities
    pub const SET_FOREGROUND: &str = "setaf";      // Set ANSI foreground color
    pub const SET_BACKGROUND: &str = "setab";      // Set ANSI background color
    pub const SET_COLOR_PAIR: &str = "setcolor";   // Set color pair
    pub const ORIG_PAIR: &str = "op";              // Original color pair
    
    /// Keypad and special keys
    pub const KEYPAD_XMIT: &str = "smkx";          // Enter keypad transmit mode
    pub const KEYPAD_LOCAL: &str = "rmkx";         // Exit keypad transmit mode
    
    /// Key definitions
    pub const KEY_F1: &str = "kf1";
    pub const KEY_F2: &str = "kf2";
    pub const KEY_F3: &str = "kf3";
    pub const KEY_F4: &str = "kf4";
    pub const KEY_F5: &str = "kf5";
    pub const KEY_F6: &str = "kf6";
    pub const KEY_F7: &str = "kf7";
    pub const KEY_F8: &str = "kf8";
    pub const KEY_F9: &str = "kf9";
    pub const KEY_F10: &str = "kf10";
    pub const KEY_F11: &str = "kf11";
    pub const KEY_F12: &str = "kf12";
    pub const KEY_UP: &str = "kcuu1";
    pub const KEY_DOWN: &str = "kcud1";
    pub const KEY_LEFT: &str = "kcub1";
    pub const KEY_RIGHT: &str = "kcuf1";
    pub const KEY_HOME: &str = "khome";
    pub const KEY_END: &str = "kend";
    pub const KEY_BACKSPACE: &str = "kbs";
    pub const KEY_DC: &str = "kdch1";              // Delete character key
    pub const KEY_IC: &str = "kich1";              // Insert character key
    pub const KEY_PPAGE: &str = "kpp";             // Previous page (Page Up)
    pub const KEY_NPAGE: &str = "knp";             // Next page (Page Down)
    
    /// Terminal initialization
    pub const INIT_1STRING: &str = "is1";          // Init string 1
    pub const INIT_2STRING: &str = "is2";          // Init string 2
    pub const INIT_3STRING: &str = "is3";          // Init string 3
    pub const RESET_1STRING: &str = "rs1";         // Reset string 1
    pub const RESET_2STRING: &str = "rs2";         // Reset string 2
    pub const RESET_3STRING: &str = "rs3";         // Reset string 3
    
    /// Alternate character set
    pub const ENTER_ALT_CHARSET_MODE: &str = "smacs";  // Enter alternate charset
    pub const EXIT_ALT_CHARSET_MODE: &str = "rmacs";   // Exit alternate charset
    pub const ACS_CHARS: &str = "acsc";                // Alternate charset pairs
    
    /// Mouse support
    pub const MOUSE_INFO: &str = "minfo";          // Mouse status information
}

/// Numeric capabilities
pub mod numbers {
    /// Screen dimensions
    pub const COLUMNS: &str = "cols";              // Number of columns
    pub const LINES: &str = "lines";               // Number of lines
    
    /// Color support
    pub const COLORS: &str = "colors";             // Number of colors
    pub const COLOR_PAIRS: &str = "pairs";         // Number of color pairs
    
    /// Other numeric capabilities
    pub const MAX_COLORS: &str = "max_colors";
    pub const MAX_PAIRS: &str = "max_pairs";
}

/// Boolean capabilities (flags)
pub mod bools {
    /// Terminal characteristics
    pub const AUTO_LEFT_MARGIN: &str = "bw";       // Cursor wraps at left margin
    pub const AUTO_RIGHT_MARGIN: &str = "am";      // Cursor wraps at right margin
    pub const EAT_NEWLINE_GLITCH: &str = "xenl";   // Newline ignored after 80 cols
    pub const HAS_META_KEY: &str = "km";           // Has meta key
    pub const HAS_STATUS_LINE: &str = "hs";        // Has status line
    pub const INSERT_NULL_GLITCH: &str = "in";     // Insert mode distinguishes null
    pub const MEMORY_ABOVE: &str = "da";           // Display retained above screen
    pub const MEMORY_BELOW: &str = "db";           // Display retained below screen
    pub const MOVE_INSERT_MODE: &str = "mir";      // Safe to move in insert mode
    pub const MOVE_STANDOUT_MODE: &str = "msgr";   // Safe to move in standout
    pub const OVER_STRIKE: &str = "os";            // Terminal overstrikes
    pub const STATUS_LINE_ESC_OK: &str = "eslok";  // Escape in status line OK
    pub const TELERAY_GLITCH: &str = "xt";         // Teleray glitch
    pub const TILDE_GLITCH: &str = "hz";           // Can't print tilde
    pub const TRANSPARENT_UNDERLINE: &str = "ul";  // Underline overwrites
    pub const XON_XOFF: &str = "xon";              // Terminal uses XON/XOFF
    
    /// Color support
    pub const HAS_COLORS: &str = "colors";         // Terminal has colors
}

/// Map termcap 2-letter codes to terminfo names
pub fn termcap_to_terminfo(cap: &str) -> Option<&'static str> {
    match cap {
        // Cursor movement
        "cm" => Some(strings::CURSOR_ADDRESS),
        "ho" => Some(strings::CURSOR_HOME),
        "up" => Some(strings::CURSOR_UP),
        "do" => Some(strings::CURSOR_DOWN),
        "le" => Some(strings::CURSOR_LEFT),
        "nd" => Some(strings::CURSOR_RIGHT),
        "vi" => Some(strings::CURSOR_INVISIBLE),
        "ve" => Some(strings::CURSOR_VISIBLE),
        "vs" => Some(strings::CURSOR_VERY_VISIBLE),
        
        // Screen manipulation
        "cl" => Some(strings::CLEAR),
        "cd" => Some(strings::CLRTOBOT),
        "ce" => Some(strings::CLRTOEOL),
        "al" => Some(strings::INSERT_LINE),
        "dl" => Some(strings::DELETE_LINE),
        "ic" => Some(strings::INSERT_CHARACTER),
        "dc" => Some(strings::DELETE_CHARACTER),
        
        // Attributes
        "md" => Some(strings::ENTER_BOLD),
        "mh" => Some(strings::ENTER_DIM),
        "mb" => Some(strings::ENTER_BLINK),
        "mr" => Some(strings::ENTER_REVERSE),
        "so" => Some(strings::ENTER_STANDOUT),
        "se" => Some(strings::EXIT_STANDOUT),
        "us" => Some(strings::ENTER_UNDERLINE),
        "ue" => Some(strings::EXIT_UNDERLINE),
        "me" => Some(strings::EXIT_ATTRIBUTES),
        
        // Colors
        "AF" => Some(strings::SET_FOREGROUND),
        "AB" => Some(strings::SET_BACKGROUND),
        "op" => Some(strings::ORIG_PAIR),
        
        // Keypad
        "ks" => Some(strings::KEYPAD_XMIT),
        "ke" => Some(strings::KEYPAD_LOCAL),
        
        // Numeric
        "co" => Some(numbers::COLUMNS),
        "li" => Some(numbers::LINES),
        "Co" => Some(numbers::COLORS),
        "pa" => Some(numbers::COLOR_PAIRS),
        
        // Boolean
        "am" => Some(bools::AUTO_RIGHT_MARGIN),
        "bw" => Some(bools::AUTO_LEFT_MARGIN),
        "xn" => Some(bools::EAT_NEWLINE_GLITCH),
        "km" => Some(bools::HAS_META_KEY),
        
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_termcap_mapping() {
        assert_eq!(termcap_to_terminfo("cm"), Some(strings::CURSOR_ADDRESS));
        assert_eq!(termcap_to_terminfo("cl"), Some(strings::CLEAR));
        assert_eq!(termcap_to_terminfo("co"), Some(numbers::COLUMNS));
        assert_eq!(termcap_to_terminfo("am"), Some(bools::AUTO_RIGHT_MARGIN));
        assert_eq!(termcap_to_terminfo("XX"), None);
    }
}
