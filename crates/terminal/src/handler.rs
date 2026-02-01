//! Escape sequence handler
//!
//! Processes parsed escape sequences and updates terminal state.

extern crate alloc;

use crate::buffer::{ScreenBuffer, ScrollbackBuffer};
use crate::cell::{CellAttrs, CellFlags, Cursor, CursorShape};
use crate::color::TermColor;
use alloc::vec;
use alloc::vec::Vec;

/// Terminal mode flags
bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub struct TerminalModes: u32 {
        /// Auto-wrap mode (DECAWM)
        const AUTOWRAP = 0x0001;
        /// Cursor visible (DECTCEM)
        const CURSOR_VISIBLE = 0x0002;
        /// Application cursor keys (DECCKM)
        const APP_CURSOR = 0x0004;
        /// Application keypad mode (DECKPAM)
        const APP_KEYPAD = 0x0008;
        /// Origin mode (DECOM)
        const ORIGIN_MODE = 0x0010;
        /// Insert mode
        const INSERT_MODE = 0x0020;
        /// Alternate screen buffer
        const ALT_SCREEN = 0x0040;
        /// Bracketed paste mode
        const BRACKETED_PASTE = 0x0080;
        /// Mouse tracking (set when any mouse mode is active)
        const MOUSE_TRACKING = 0x0100;
        /// Focus events
        const FOCUS_EVENTS = 0x0200;
        /// Synchronized output mode (CSI ? 2026 h/l)
        const SYNCHRONIZED_OUTPUT = 0x0400;
    }
}

/// Mouse tracking mode
///
/// Applications request specific modes via CSI ?N h/l sequences.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseMode {
    /// No mouse tracking
    None,
    /// X10 compatibility mode (CSI ?9 h) — report button press only
    X10,
    /// Normal tracking mode (CSI ?1000 h) — report press and release
    Normal,
    /// Button-event tracking (CSI ?1002 h) — report motion while button held
    ButtonMotion,
    /// Any-event tracking (CSI ?1003 h) — report all motion
    AnyMotion,
}

impl Default for MouseMode {
    fn default() -> Self {
        MouseMode::None
    }
}

/// Mouse coordinate encoding format
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseEncoding {
    /// X10 encoding (default) — single byte, 223 column limit
    X10,
    /// UTF-8 encoding (CSI ?1005 h) — extended range via UTF-8
    Utf8,
    /// SGR encoding (CSI ?1006 h) — decimal params, no limit
    Sgr,
    /// Urxvt encoding (CSI ?1015 h) — decimal params
    Urxvt,
}

impl Default for MouseEncoding {
    fn default() -> Self {
        MouseEncoding::X10
    }
}

/// Character set for G0/G1 designators
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Charset {
    /// ASCII (ESC ( B or ESC ) B)
    Ascii,
    /// DEC Special Graphics - single-line box drawing (ESC ( 0 or ESC ) 0)
    DecSpecialGraphics,
    /// DEC Supplemental Graphics (ESC ( < or ESC ) <)
    DecSupplemental,
    /// DEC Technical (ESC ( > or ESC ) >)
    DecTechnical,
    /// UK/National (ESC ( A or ESC ) A)
    Uk,
}

impl Default for Charset {
    fn default() -> Self {
        Charset::Ascii
    }
}

impl Charset {
    /// Translate a character through this charset
    /// Returns the original character for ASCII, or the line drawing equivalent for DEC graphics
    pub fn translate(&self, ch: char) -> char {
        match self {
            Charset::Ascii => ch,

            Charset::DecSpecialGraphics => {
                // DEC Special Graphics character set (complete VT100 mapping)
                // Reference: VT100 User Guide, Table 3-9
                match ch {
                    '_' => '\u{0020}', // blank (space)
                    '`' => '\u{25C6}', // ◆ diamond
                    'a' => '\u{2592}', // ▒ checkerboard/stipple
                    'b' => '\u{2409}', // ␉ HT symbol
                    'c' => '\u{240C}', // ␌ FF symbol
                    'd' => '\u{240D}', // ␍ CR symbol
                    'e' => '\u{240A}', // ␊ LF symbol
                    'f' => '\u{00B0}', // ° degree symbol
                    'g' => '\u{00B1}', // ± plus/minus
                    'h' => '\u{2424}', // ␤ NL symbol
                    'i' => '\u{240B}', // ␋ VT symbol
                    'j' => '\u{2518}', // ┘ bottom-right corner (single)
                    'k' => '\u{2510}', // ┐ top-right corner (single)
                    'l' => '\u{250C}', // ┌ top-left corner (single)
                    'm' => '\u{2514}', // └ bottom-left corner (single)
                    'n' => '\u{253C}', // ┼ crossing lines (single)
                    'o' => '\u{23BA}', // ⎺ scan line 1 (top horizontal)
                    'p' => '\u{23BB}', // ⎻ scan line 3
                    'q' => '\u{2500}', // ─ horizontal line (single)
                    'r' => '\u{23BC}', // ⎼ scan line 7
                    's' => '\u{23BD}', // ⎽ scan line 9 (bottom horizontal)
                    't' => '\u{251C}', // ├ left tee (single)
                    'u' => '\u{2524}', // ┤ right tee (single)
                    'v' => '\u{2534}', // ┴ bottom tee (single)
                    'w' => '\u{252C}', // ┬ top tee (single)
                    'x' => '\u{2502}', // │ vertical line (single)
                    'y' => '\u{2264}', // ≤ less than or equal
                    'z' => '\u{2265}', // ≥ greater than or equal
                    '{' => '\u{03C0}', // π pi
                    '|' => '\u{2260}', // ≠ not equal
                    '}' => '\u{00A3}', // £ pound sterling
                    '~' => '\u{00B7}', // · middle dot/bullet
                    _ => ch,
                }
            }

            Charset::DecSupplemental => {
                // DEC Supplemental Graphics - includes double-line box drawing
                match ch {
                    // Double-line box drawing
                    'j' => '\u{255D}', // ╝ double bottom-right corner
                    'k' => '\u{2557}', // ╗ double top-right corner
                    'l' => '\u{2554}', // ╔ double top-left corner
                    'm' => '\u{255A}', // ╚ double bottom-left corner
                    'n' => '\u{256C}', // ╬ double cross
                    'q' => '\u{2550}', // ═ double horizontal line
                    't' => '\u{2560}', // ╠ double left tee
                    'u' => '\u{2563}', // ╣ double right tee
                    'v' => '\u{2569}', // ╩ double bottom tee
                    'w' => '\u{2566}', // ╦ double top tee
                    'x' => '\u{2551}', // ║ double vertical line
                    // Additional symbols
                    '`' => '\u{25C6}', // ◆ diamond
                    'a' => '\u{2592}', // ▒ checkerboard
                    'f' => '\u{00B0}', // ° degree
                    'g' => '\u{00B1}', // ± plus/minus
                    'y' => '\u{2264}', // ≤ less than or equal
                    'z' => '\u{2265}', // ≥ greater than or equal
                    '{' => '\u{03C0}', // π pi
                    '|' => '\u{2260}', // ≠ not equal
                    '}' => '\u{00A3}', // £ pound sterling
                    '~' => '\u{00B7}', // · middle dot
                    _ => ch,
                }
            }

            Charset::DecTechnical => {
                // DEC Technical character set - math and technical symbols
                match ch {
                    '!' => '\u{2191}', // ↑ up arrow
                    '"' => '\u{2193}', // ↓ down arrow
                    '#' => '\u{2192}', // → right arrow
                    '$' => '\u{2190}', // ← left arrow
                    '%' => '\u{2195}', // ↕ up-down arrow
                    '&' => '\u{2194}', // ↔ left-right arrow
                    '\'' => '\u{25B2}', // ▲ up triangle
                    '(' => '\u{25BC}', // ▼ down triangle
                    ')' => '\u{25B6}', // ▶ right triangle
                    '*' => '\u{25C0}', // ◀ left triangle
                    '+' => '\u{2211}', // ∑ summation
                    ',' => '\u{222B}', // ∫ integral
                    '-' => '\u{221A}', // √ square root
                    '.' => '\u{2248}', // ≈ approximately equal
                    '/' => '\u{2260}', // ≠ not equal
                    '0' => '\u{2261}', // ≡ identical to
                    '1' => '\u{2264}', // ≤ less than or equal
                    '2' => '\u{2265}', // ≥ greater than or equal
                    '3' => '\u{03C0}', // π pi
                    '4' => '\u{2202}', // ∂ partial differential
                    '5' => '\u{221E}', // ∞ infinity
                    '6' => '\u{2282}', // ⊂ subset of
                    '7' => '\u{2283}', // ⊃ superset of
                    '8' => '\u{2229}', // ∩ intersection
                    '9' => '\u{222A}', // ∪ union
                    ':' => '\u{2227}', // ∧ logical and
                    ';' => '\u{2228}', // ∨ logical or
                    '<' => '\u{00AC}', // ¬ not sign
                    '=' => '\u{21D4}', // ⇔ if and only if
                    '>' => '\u{21D2}', // ⇒ implies
                    '?' => '\u{2200}', // ∀ for all
                    '@' => '\u{2203}', // ∃ there exists
                    '[' => '\u{2208}', // ∈ element of
                    '\\' => '\u{2209}', // ∉ not an element of
                    ']' => '\u{2205}', // ∅ empty set
                    '^' => '\u{2207}', // ∇ nabla
                    '_' => '\u{00B0}', // ° degree
                    '`' => '\u{00B1}', // ± plus/minus
                    '{' => '\u{2220}', // ∠ angle
                    '|' => '\u{22A5}', // ⊥ perpendicular
                    '}' => '\u{2234}', // ∴ therefore
                    '~' => '\u{2235}', // ∵ because
                    _ => ch,
                }
            }

            Charset::Uk => {
                // UK character set - pound sign at # position
                match ch {
                    '#' => '\u{00A3}', // £ pound sterling
                    _ => ch,
                }
            }
        }
    }
}

/// Saved cursor state for DECSC/DECRC
#[derive(Debug, Clone)]
pub struct SavedCursor {
    pub cursor: Cursor,
    pub attrs: CellAttrs,
    pub origin_mode: bool,
}

impl Default for SavedCursor {
    fn default() -> Self {
        SavedCursor {
            cursor: Cursor::default(),
            attrs: CellAttrs::default(),
            origin_mode: false,
        }
    }
}

/// Terminal handler for escape sequences
pub struct Handler {
    /// Current cell attributes
    pub attrs: CellAttrs,
    /// Cursor state
    pub cursor: Cursor,
    /// Terminal modes
    pub modes: TerminalModes,
    /// Mouse tracking mode
    pub mouse_mode: MouseMode,
    /// Mouse coordinate encoding
    pub mouse_encoding: MouseEncoding,
    /// Scroll region top
    pub scroll_top: u32,
    /// Scroll region bottom
    pub scroll_bottom: u32,
    /// Saved cursor (primary screen)
    pub saved_cursor: SavedCursor,
    /// Saved cursor (alt screen)
    pub saved_cursor_alt: SavedCursor,
    /// Tab stops
    pub tabs: Vec<bool>,
    /// G0 character set
    pub g0_charset: Charset,
    /// G1 character set
    pub g1_charset: Charset,
    /// Active charset (false = G0, true = G1)
    pub active_g1: bool,
    /// Terminal width
    cols: u32,
    /// Terminal height
    rows: u32,
}

impl Handler {
    /// Create a new handler
    pub fn new(cols: u32, rows: u32) -> Self {
        let mut tabs = vec![false; cols as usize];
        // Set default tab stops every 8 columns
        for i in (0..cols as usize).step_by(8) {
            tabs[i] = true;
        }

        Handler {
            attrs: CellAttrs::default(),
            cursor: Cursor::default(),
            modes: TerminalModes::AUTOWRAP | TerminalModes::CURSOR_VISIBLE,
            mouse_mode: MouseMode::None,
            mouse_encoding: MouseEncoding::X10,
            scroll_top: 0,
            scroll_bottom: rows - 1,
            saved_cursor: SavedCursor::default(),
            saved_cursor_alt: SavedCursor::default(),
            tabs,
            g0_charset: Charset::Ascii,
            g1_charset: Charset::Ascii,
            active_g1: false,
            cols,
            rows,
        }
    }

    /// Get effective scroll bottom (clamped to screen)
    fn effective_scroll_bottom(&self) -> u32 {
        self.scroll_bottom.min(self.rows - 1)
    }

    /// Put a character at cursor position
    pub fn put_char(&mut self, ch: char, buffer: &mut ScreenBuffer) {
        // Handle autowrap
        if self.cursor.col >= self.cols {
            if self.modes.contains(TerminalModes::AUTOWRAP) {
                self.cursor.col = 0;
                self.linefeed(buffer, None);
            } else {
                self.cursor.col = self.cols - 1;
            }
        }

        // Insert mode: shift characters right
        if self.modes.contains(TerminalModes::INSERT_MODE) {
            buffer.insert_chars(self.cursor.row, self.cursor.col, 1);
        }

        // Translate character through active charset (fast path for ASCII)
        let translated_ch = if !self.active_g1 && self.g0_charset == Charset::Ascii {
            ch // Fast path: G0 ASCII, no translation needed
        } else {
            let active_charset = if self.active_g1 {
                &self.g1_charset
            } else {
                &self.g0_charset
            };
            active_charset.translate(ch)
        };

        // Set the character
        buffer.set_char(self.cursor.row, self.cursor.col, translated_ch, self.attrs);

        // Advance cursor
        self.cursor.col += 1;
    }

    /// Carriage return
    pub fn carriage_return(&mut self) {
        self.cursor.col = 0;
    }

    /// Line feed (moves down, may scroll)
    /// Returns true if scrolling occurred
    pub fn linefeed(
        &mut self,
        buffer: &mut ScreenBuffer,
        scrollback: Option<&mut ScrollbackBuffer>,
    ) -> bool {
        if self.cursor.row >= self.effective_scroll_bottom() {
            // At or past scroll region bottom - scroll up
            if let Some(sb) = scrollback {
                // Save top line to scrollback (only if at top of scroll region)
                if self.scroll_top == 0 {
                    if let Some(line) = buffer.get_row(self.scroll_top) {
                        sb.push(line);
                    }
                }
            }
            buffer.scroll_up(self.scroll_top, self.effective_scroll_bottom(), 1);
            true // Scrolled
        } else {
            self.cursor.row += 1;
            false // No scroll
        }
    }

    /// Reverse line feed (moves up, may scroll)
    pub fn reverse_linefeed(&mut self, buffer: &mut ScreenBuffer) {
        if self.cursor.row <= self.scroll_top {
            buffer.scroll_down(self.scroll_top, self.effective_scroll_bottom(), 1);
        } else {
            self.cursor.row -= 1;
        }
    }

    /// Tab to next tab stop
    pub fn tab(&mut self) {
        let mut col = self.cursor.col + 1;
        while col < self.cols {
            if self.tabs.get(col as usize).copied().unwrap_or(false) {
                break;
            }
            col += 1;
        }
        self.cursor.col = col.min(self.cols - 1);
    }

    /// Backspace
    pub fn backspace(&mut self) {
        if self.cursor.col > 0 {
            self.cursor.col -= 1;
        }
    }

    /// Handle CSI sequence
    pub fn handle_csi(
        &mut self,
        params: &[i32],
        intermediates: &[u8],
        final_char: u8,
        buffer: &mut ScreenBuffer,
        mut scrollback: Option<&mut ScrollbackBuffer>,
    ) {
        #[cfg(feature = "debug-tty-read")]
        {
            use core::fmt::Write;
            let mut w = arch_x86_64::serial::SerialWriter;
            let _ = write!(w, "[CSI] final={:?} intermediates={:?} params={:?}\n",
                final_char as char, intermediates, params);
        }

        // Check for private mode prefix
        let is_private = intermediates.first() == Some(&b'?');

        match final_char {
            b'@' => {
                // ICH - Insert Character
                let n = get_param(params, 0, 1) as u32;
                buffer.insert_chars(self.cursor.row, self.cursor.col, n);
            }
            b'A' => {
                // CUU - Cursor Up
                let n = get_param(params, 0, 1) as u32;
                self.cursor.row = self.cursor.row.saturating_sub(n);
                if self.cursor.row < self.scroll_top {
                    self.cursor.row = self.scroll_top;
                }
            }
            b'B' => {
                // CUD - Cursor Down
                let n = get_param(params, 0, 1) as u32;
                self.cursor.row = (self.cursor.row + n).min(self.effective_scroll_bottom());
            }
            b'C' => {
                // CUF - Cursor Forward
                let n = get_param(params, 0, 1) as u32;
                self.cursor.col = (self.cursor.col + n).min(self.cols - 1);
            }
            b'D' => {
                // CUB - Cursor Back
                let n = get_param(params, 0, 1) as u32;
                self.cursor.col = self.cursor.col.saturating_sub(n);
            }
            b'E' => {
                // CNL - Cursor Next Line
                let n = get_param(params, 0, 1) as u32;
                self.cursor.col = 0;
                self.cursor.row = (self.cursor.row + n).min(self.effective_scroll_bottom());
            }
            b'F' => {
                // CPL - Cursor Previous Line
                let n = get_param(params, 0, 1) as u32;
                self.cursor.col = 0;
                self.cursor.row = self.cursor.row.saturating_sub(n).max(self.scroll_top);
            }
            b'G' => {
                // CHA - Cursor Horizontal Absolute
                let col = get_param(params, 0, 1) as u32;
                self.cursor.col = (col.saturating_sub(1)).min(self.cols - 1);
            }
            b'H' | b'f' => {
                // CUP/HVP - Cursor Position
                let row = get_param(params, 0, 1) as u32;
                let col = get_param(params, 1, 1) as u32;
                self.cursor.row = (row.saturating_sub(1)).min(self.rows - 1);
                self.cursor.col = (col.saturating_sub(1)).min(self.cols - 1);
            }
            b'g' => {
                // TBC - Tab Clear
                let mode = get_param(params, 0, 0);
                match mode {
                    0 => {
                        // Clear tab stop at cursor position
                        if (self.cursor.col as usize) < self.tabs.len() {
                            self.tabs[self.cursor.col as usize] = false;
                        }
                    }
                    3 => {
                        // Clear all tab stops
                        for tab in self.tabs.iter_mut() {
                            *tab = false;
                        }
                    }
                    _ => {}
                }
            }
            b'J' => {
                // ED - Erase Display
                let mode = get_param(params, 0, 0);
                match mode {
                    0 => buffer.clear_to_eos(self.cursor.row, self.cursor.col),
                    1 => buffer.clear_to_bos(self.cursor.row, self.cursor.col),
                    2 | 3 => {
                        buffer.clear();
                        // Mode 3 also clears scrollback
                        if mode == 3 {
                            if let Some(sb) = scrollback {
                                sb.clear();
                            }
                        }
                    }
                    _ => {}
                }
            }
            b'K' => {
                // EL - Erase Line
                let mode = get_param(params, 0, 0);
                match mode {
                    0 => buffer.clear_to_eol(self.cursor.row, self.cursor.col),
                    1 => buffer.clear_to_bol(self.cursor.row, self.cursor.col),
                    2 => buffer.clear_row(self.cursor.row),
                    _ => {}
                }
            }
            b'L' => {
                // IL - Insert Lines
                let n = get_param(params, 0, 1) as u32;
                buffer.insert_lines(self.cursor.row, n, self.effective_scroll_bottom());
            }
            b'M' => {
                // DL - Delete Lines
                let n = get_param(params, 0, 1) as u32;
                buffer.delete_lines(self.cursor.row, n, self.effective_scroll_bottom());
            }
            b'P' => {
                // DCH - Delete Character
                let n = get_param(params, 0, 1) as u32;
                buffer.delete_chars(self.cursor.row, self.cursor.col, n);
            }
            b'S' => {
                // SU - Scroll Up
                let n = get_param(params, 0, 1) as u32;
                for _ in 0..n {
                    if let Some(ref mut sb) = scrollback {
                        if self.scroll_top == 0 {
                            if let Some(line) = buffer.get_row(self.scroll_top) {
                                sb.push(line);
                            }
                        }
                    }
                }
                buffer.scroll_up(self.scroll_top, self.effective_scroll_bottom(), n);
            }
            b'T' => {
                // SD - Scroll Down
                let n = get_param(params, 0, 1) as u32;
                buffer.scroll_down(self.scroll_top, self.effective_scroll_bottom(), n);
            }
            b'X' => {
                // ECH - Erase Character
                let n = get_param(params, 0, 1) as u32;
                buffer.erase_chars(self.cursor.row, self.cursor.col, n);
            }
            b'd' => {
                // VPA - Vertical Position Absolute
                let row = get_param(params, 0, 1) as u32;
                self.cursor.row = (row.saturating_sub(1)).min(self.rows - 1);
            }
            b'h' => {
                // SM - Set Mode
                if is_private {
                    self.set_private_mode(params, true);
                } else {
                    self.set_mode(params, true);
                }
            }
            b'l' => {
                // RM - Reset Mode
                if is_private {
                    self.set_private_mode(params, false);
                } else {
                    self.set_mode(params, false);
                }
            }
            b'm' => {
                // SGR - Select Graphic Rendition
                self.handle_sgr(params);
            }
            b'n' => {
                // DSR - Device Status Report
                let mode = get_param(params, 0, 0);
                #[cfg(feature = "debug-tty-read")]
                {
                    use core::fmt::Write;
                    let mut w = arch_x86_64::serial::SerialWriter;
                    let _ = write!(w, "[DSR] Device Status Report mode={}\n", mode);
                }

                match mode {
                    5 => {
                        // Status Report - always report OK
                        crate::send_response(b"\x1b[0n");
                    }
                    6 => {
                        // CPR - Cursor Position Report
                        // Report cursor position (1-indexed)
                        let row = self.cursor.row + 1;
                        let col = self.cursor.col + 1;
                        let response = alloc::format!("\x1b[{};{}R", row, col);
                        crate::send_response(response.as_bytes());
                    }
                    _ => {
                        // Unknown DSR - ignore
                    }
                }
            }
            b'r' => {
                // DECSTBM - Set Scroll Region
                let top = get_param(params, 0, 1) as u32;
                let bottom = get_param(params, 1, self.rows as i32) as u32;
                self.scroll_top = (top.saturating_sub(1)).min(self.rows - 1);
                self.scroll_bottom = (bottom.saturating_sub(1)).min(self.rows - 1);
                if self.scroll_top > self.scroll_bottom {
                    core::mem::swap(&mut self.scroll_top, &mut self.scroll_bottom);
                }
                // Home cursor after setting scroll region
                self.cursor.row = if self.modes.contains(TerminalModes::ORIGIN_MODE) {
                    self.scroll_top
                } else {
                    0
                };
                self.cursor.col = 0;
            }
            b's' => {
                // SCOSC - Save Cursor Position (or DECSLRM if alt)
                self.save_cursor();
            }
            b'u' => {
                // SCORC - Restore Cursor Position
                self.restore_cursor();
            }
            b'`' => {
                // HPA - Horizontal Position Absolute (same as CHA)
                let col = get_param(params, 0, 1) as u32;
                self.cursor.col = (col.saturating_sub(1)).min(self.cols - 1);
            }
            b'q' => {
                // DECSCUSR - Set Cursor Style
                let style = get_param(params, 0, 0);
                self.cursor.shape = match style {
                    0 | 1 => CursorShape::Block,
                    2 => CursorShape::Block,
                    3 | 4 => CursorShape::Underline,
                    5 | 6 => CursorShape::Bar,
                    _ => CursorShape::Block,
                };
            }
            b'c' => {
                // DA - Device Attributes
                #[cfg(feature = "debug-tty-read")]
                {
                    use core::fmt::Write;
                    let mut w = arch_x86_64::serial::SerialWriter;
                    let _ = write!(w, "[DA] Device Attributes query (secondary={})\n",
                        intermediates.first() == Some(&b'>'));
                }

                if intermediates.first() == Some(&b'>') {
                    // Secondary DA - report terminal version
                    // Format: CSI > Pp ; Pv ; Pc c
                    // Pp = terminal type (1 = VT220)
                    // Pv = firmware version (0)
                    // Pc = keyboard type (0)
                    crate::send_response(b"\x1b[>1;0;0c");
                } else {
                    // Primary DA - report terminal capabilities
                    // CSI ? 6 c = VT102
                    // CSI ? 6 2 ; c = VT220 (more features)
                    // We report VT220 for better compatibility
                    crate::send_response(b"\x1b[?62c");
                }
            }
            _ => {
                // Unhandled CSI sequence
            }
        }
    }

    /// Handle ESC sequence
    pub fn handle_esc(&mut self, intermediates: &[u8], final_char: u8, buffer: &mut ScreenBuffer) {
        match (intermediates.first(), final_char) {
            (None, b'7') => {
                // DECSC - Save Cursor
                self.save_cursor();
            }
            (None, b'8') => {
                // DECRC - Restore Cursor
                self.restore_cursor();
            }
            (None, b'D') => {
                // IND - Index (move down, scroll if needed)
                self.linefeed(buffer, None);
            }
            (None, b'E') => {
                // NEL - Next Line
                self.cursor.col = 0;
                self.linefeed(buffer, None);
            }
            (None, b'H') => {
                // HTS - Horizontal Tab Set
                if (self.cursor.col as usize) < self.tabs.len() {
                    self.tabs[self.cursor.col as usize] = true;
                }
            }
            (None, b'M') => {
                // RI - Reverse Index
                self.reverse_linefeed(buffer);
            }
            (None, b'c') => {
                // RIS - Reset to Initial State
                self.reset(buffer);
            }
            (Some(b'#'), b'8') => {
                // DECALN - Screen Alignment Pattern (fill with E)
                let attrs = CellAttrs::default();
                for row in 0..self.rows {
                    for col in 0..self.cols {
                        buffer.set_char(row, col, 'E', attrs);
                    }
                }
            }
            // G0 Character Set Selection (ESC ( X)
            (Some(b'('), b'B') => {
                self.g0_charset = Charset::Ascii;
            }
            (Some(b'('), b'0') => {
                self.g0_charset = Charset::DecSpecialGraphics;
            }
            (Some(b'('), b'<') => {
                self.g0_charset = Charset::DecSupplemental;
            }
            (Some(b'('), b'>') => {
                self.g0_charset = Charset::DecTechnical;
            }
            (Some(b'('), b'A') => {
                self.g0_charset = Charset::Uk;
            }
            (Some(b'('), b'U') | (Some(b'('), b'K') => {
                // User/null mapping - treat as ASCII
                self.g0_charset = Charset::Ascii;
            }

            // G1 Character Set Selection (ESC ) X)
            (Some(b')'), b'B') => {
                self.g1_charset = Charset::Ascii;
            }
            (Some(b')'), b'0') => {
                self.g1_charset = Charset::DecSpecialGraphics;
            }
            (Some(b')'), b'<') => {
                self.g1_charset = Charset::DecSupplemental;
            }
            (Some(b')'), b'>') => {
                self.g1_charset = Charset::DecTechnical;
            }
            (Some(b')'), b'A') => {
                self.g1_charset = Charset::Uk;
            }
            (Some(b')'), b'U') | (Some(b')'), b'K') => {
                // User/null mapping - treat as ASCII
                self.g1_charset = Charset::Ascii;
            }
            _ => {
                // Unhandled ESC sequence
            }
        }
    }

    /// Handle SGR (Select Graphic Rendition)
    fn handle_sgr(&mut self, params: &[i32]) {
        if params.is_empty() {
            self.attrs = CellAttrs::default();
            return;
        }

        let mut i = 0;
        while i < params.len() {
            let param = params[i];
            match param {
                0 | -1 => {
                    // Reset
                    self.attrs = CellAttrs::default();
                }
                1 => {
                    // Bold
                    self.attrs.flags |= CellFlags::BOLD;
                }
                2 => {
                    // Dim
                    self.attrs.flags |= CellFlags::DIM;
                }
                3 => {
                    // Italic
                    self.attrs.flags |= CellFlags::ITALIC;
                }
                4 => {
                    // Underline
                    self.attrs.flags |= CellFlags::UNDERLINE;
                }
                5 | 6 => {
                    // Blink
                    self.attrs.flags |= CellFlags::BLINK;
                }
                7 => {
                    // Reverse
                    self.attrs.flags |= CellFlags::REVERSE;
                }
                8 => {
                    // Hidden
                    self.attrs.flags |= CellFlags::HIDDEN;
                }
                9 => {
                    // Strikethrough
                    self.attrs.flags |= CellFlags::STRIKETHROUGH;
                }
                21 => {
                    // Double underline (treat as underline)
                    self.attrs.flags |= CellFlags::UNDERLINE;
                }
                22 => {
                    // Normal intensity (not bold, not dim)
                    self.attrs.flags &= !(CellFlags::BOLD | CellFlags::DIM);
                }
                23 => {
                    // Not italic
                    self.attrs.flags &= !CellFlags::ITALIC;
                }
                24 => {
                    // Not underlined
                    self.attrs.flags &= !CellFlags::UNDERLINE;
                }
                25 => {
                    // Not blinking
                    self.attrs.flags &= !CellFlags::BLINK;
                }
                27 => {
                    // Not reversed
                    self.attrs.flags &= !CellFlags::REVERSE;
                }
                28 => {
                    // Not hidden
                    self.attrs.flags &= !CellFlags::HIDDEN;
                }
                29 => {
                    // Not strikethrough
                    self.attrs.flags &= !CellFlags::STRIKETHROUGH;
                }
                30..=37 => {
                    // Foreground color (standard)
                    self.attrs.fg = TermColor::Ansi16((param - 30) as u8);
                }
                38 => {
                    // Extended foreground color
                    if let Some(color) = parse_extended_color(params, &mut i) {
                        self.attrs.fg = color;
                    }
                }
                39 => {
                    // Default foreground
                    self.attrs.fg = TermColor::DefaultFg;
                }
                40..=47 => {
                    // Background color (standard)
                    self.attrs.bg = TermColor::Ansi16((param - 40) as u8);
                }
                48 => {
                    // Extended background color
                    if let Some(color) = parse_extended_color(params, &mut i) {
                        self.attrs.bg = color;
                    }
                }
                49 => {
                    // Default background
                    self.attrs.bg = TermColor::DefaultBg;
                }
                90..=97 => {
                    // Foreground color (bright)
                    self.attrs.fg = TermColor::Ansi16((param - 90 + 8) as u8);
                }
                100..=107 => {
                    // Background color (bright)
                    self.attrs.bg = TermColor::Ansi16((param - 100 + 8) as u8);
                }
                _ => {}
            }
            i += 1;
        }
    }

    /// Set terminal mode
    fn set_mode(&mut self, params: &[i32], enable: bool) {
        for &param in params {
            match param {
                4 => {
                    // Insert mode
                    if enable {
                        self.modes |= TerminalModes::INSERT_MODE;
                    } else {
                        self.modes &= !TerminalModes::INSERT_MODE;
                    }
                }
                _ => {}
            }
        }
    }

    /// Set private (DEC) terminal mode
    fn set_private_mode(&mut self, params: &[i32], enable: bool) {
        for &param in params {
            match param {
                1 => {
                    // DECCKM - Application cursor keys
                    if enable {
                        self.modes |= TerminalModes::APP_CURSOR;
                    } else {
                        self.modes &= !TerminalModes::APP_CURSOR;
                    }
                }
                6 => {
                    // DECOM - Origin mode
                    if enable {
                        self.modes |= TerminalModes::ORIGIN_MODE;
                    } else {
                        self.modes &= !TerminalModes::ORIGIN_MODE;
                    }
                    // Home cursor
                    self.cursor.row = if enable { self.scroll_top } else { 0 };
                    self.cursor.col = 0;
                }
                7 => {
                    // DECAWM - Auto-wrap mode
                    if enable {
                        self.modes |= TerminalModes::AUTOWRAP;
                    } else {
                        self.modes &= !TerminalModes::AUTOWRAP;
                    }
                }
                25 => {
                    // DECTCEM - Cursor visible
                    self.cursor.visible = enable;
                    if enable {
                        self.modes |= TerminalModes::CURSOR_VISIBLE;
                    } else {
                        self.modes &= !TerminalModes::CURSOR_VISIBLE;
                    }
                }
                9 => {
                    // X10 mouse tracking
                    if enable {
                        self.mouse_mode = MouseMode::X10;
                        self.modes |= TerminalModes::MOUSE_TRACKING;
                    } else {
                        self.mouse_mode = MouseMode::None;
                        self.modes &= !TerminalModes::MOUSE_TRACKING;
                    }
                }
                1000 => {
                    // Normal mouse tracking (press + release)
                    if enable {
                        self.mouse_mode = MouseMode::Normal;
                        self.modes |= TerminalModes::MOUSE_TRACKING;
                    } else {
                        self.mouse_mode = MouseMode::None;
                        self.modes &= !TerminalModes::MOUSE_TRACKING;
                    }
                }
                1002 => {
                    // Button-event tracking (motion while button held)
                    if enable {
                        self.mouse_mode = MouseMode::ButtonMotion;
                        self.modes |= TerminalModes::MOUSE_TRACKING;
                    } else {
                        self.mouse_mode = MouseMode::None;
                        self.modes &= !TerminalModes::MOUSE_TRACKING;
                    }
                }
                1003 => {
                    // Any-event tracking (all motion)
                    if enable {
                        self.mouse_mode = MouseMode::AnyMotion;
                        self.modes |= TerminalModes::MOUSE_TRACKING;
                    } else {
                        self.mouse_mode = MouseMode::None;
                        self.modes &= !TerminalModes::MOUSE_TRACKING;
                    }
                }
                1005 => {
                    // UTF-8 mouse encoding
                    if enable {
                        self.mouse_encoding = MouseEncoding::Utf8;
                    } else {
                        self.mouse_encoding = MouseEncoding::X10;
                    }
                }
                1006 => {
                    // SGR mouse encoding
                    if enable {
                        self.mouse_encoding = MouseEncoding::Sgr;
                    } else {
                        self.mouse_encoding = MouseEncoding::X10;
                    }
                }
                1015 => {
                    // Urxvt mouse encoding
                    if enable {
                        self.mouse_encoding = MouseEncoding::Urxvt;
                    } else {
                        self.mouse_encoding = MouseEncoding::X10;
                    }
                }
                1004 => {
                    // Focus events
                    if enable {
                        self.modes |= TerminalModes::FOCUS_EVENTS;
                    } else {
                        self.modes &= !TerminalModes::FOCUS_EVENTS;
                    }
                }
                1049 => {
                    // Alternate screen buffer (with save/restore cursor)
                    if enable {
                        self.save_cursor();
                        self.modes |= TerminalModes::ALT_SCREEN;
                    } else {
                        self.modes &= !TerminalModes::ALT_SCREEN;
                        self.restore_cursor();
                    }
                }
                2004 => {
                    // Bracketed paste mode
                    if enable {
                        self.modes |= TerminalModes::BRACKETED_PASTE;
                    } else {
                        self.modes &= !TerminalModes::BRACKETED_PASTE;
                    }
                }
                2026 => {
                    // Synchronized output mode
                    if enable {
                        self.modes |= TerminalModes::SYNCHRONIZED_OUTPUT;
                    } else {
                        self.modes &= !TerminalModes::SYNCHRONIZED_OUTPUT;
                    }
                }
                _ => {}
            }
        }
    }

    /// Save cursor state
    pub fn save_cursor(&mut self) {
        let saved = SavedCursor {
            cursor: self.cursor,
            attrs: self.attrs,
            origin_mode: self.modes.contains(TerminalModes::ORIGIN_MODE),
        };
        if self.modes.contains(TerminalModes::ALT_SCREEN) {
            self.saved_cursor_alt = saved;
        } else {
            self.saved_cursor = saved;
        }
    }

    /// Restore cursor state
    pub fn restore_cursor(&mut self) {
        let saved = if self.modes.contains(TerminalModes::ALT_SCREEN) {
            &self.saved_cursor_alt
        } else {
            &self.saved_cursor
        };
        self.cursor = saved.cursor;
        self.attrs = saved.attrs;
        if saved.origin_mode {
            self.modes |= TerminalModes::ORIGIN_MODE;
        } else {
            self.modes &= !TerminalModes::ORIGIN_MODE;
        }
    }

    /// Reset terminal to initial state
    pub fn reset(&mut self, buffer: &mut ScreenBuffer) {
        self.attrs = CellAttrs::default();
        self.cursor = Cursor::default();
        self.modes = TerminalModes::AUTOWRAP | TerminalModes::CURSOR_VISIBLE;
        self.mouse_mode = MouseMode::None;
        self.mouse_encoding = MouseEncoding::X10;
        self.scroll_top = 0;
        self.scroll_bottom = self.rows - 1;
        self.saved_cursor = SavedCursor::default();
        self.saved_cursor_alt = SavedCursor::default();

        // Reset tab stops
        for tab in self.tabs.iter_mut() {
            *tab = false;
        }
        for i in (0..self.cols as usize).step_by(8) {
            self.tabs[i] = true;
        }

        buffer.clear();
    }
}

/// Get parameter with default value
fn get_param(params: &[i32], index: usize, default: i32) -> i32 {
    params.get(index).copied().unwrap_or(-1).max(0).max(default)
}

/// Parse extended color (256-color or RGB)
fn parse_extended_color(params: &[i32], i: &mut usize) -> Option<TermColor> {
    if *i + 1 >= params.len() {
        return None;
    }

    match params[*i + 1] {
        2 => {
            // RGB color: 38;2;r;g;b
            if *i + 4 >= params.len() {
                return None;
            }
            let r = params[*i + 2].clamp(0, 255) as u8;
            let g = params[*i + 3].clamp(0, 255) as u8;
            let b = params[*i + 4].clamp(0, 255) as u8;
            *i += 4;
            Some(TermColor::Rgb(r, g, b))
        }
        5 => {
            // 256-color: 38;5;n
            if *i + 2 >= params.len() {
                return None;
            }
            let n = params[*i + 2].clamp(0, 255) as u8;
            *i += 2;
            Some(TermColor::Ansi256(n))
        }
        _ => None,
    }
}
