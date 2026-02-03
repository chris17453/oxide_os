//! # Terminal Database
//!
//! Built-in terminal definitions for common terminal types.
//! Provides fallback when terminfo database is not available.
//!
//! -- SableWire: Terminal hardware definitions, know every quirk

use crate::TerminalEntry;
use crate::capabilities::{bools, numbers, strings};
use alloc::string::ToString;
use core::option::Option;

/// Get a terminal definition by name
pub fn get_terminal(name: &str) -> Option<TerminalEntry> {
    match name {
        "xterm" | "xterm-256color" | "xterm-color" => Some(xterm()),
        "linux" | "console" => Some(linux_console()),
        "vt100" => Some(vt100()),
        "vt220" => Some(vt220()),
        "ansi" => Some(ansi()),
        "dumb" => Some(dumb()),
        _ => {
            // Try to find a partial match
            if name.starts_with("xterm") {
                Some(xterm())
            } else if name.starts_with("vt") {
                Some(vt100())
            } else {
                None
            }
        }
    }
}

/// XTerm terminal definition (most common modern terminal)
fn xterm() -> TerminalEntry {
    let mut entry = TerminalEntry::new("xterm-256color");

    // Dimensions
    entry.set_number(numbers::COLUMNS, 80);
    entry.set_number(numbers::LINES, 24);
    entry.set_number(numbers::COLORS, 256);
    entry.set_number(numbers::COLOR_PAIRS, 32767);

    // Boolean capabilities
    entry.set_flag(bools::AUTO_RIGHT_MARGIN, true);
    entry.set_flag(bools::EAT_NEWLINE_GLITCH, true);
    entry.set_flag(bools::HAS_META_KEY, true);
    entry.set_flag(bools::MOVE_INSERT_MODE, true);
    entry.set_flag(bools::MOVE_STANDOUT_MODE, true);

    // Cursor movement
    entry.set_string(strings::CURSOR_ADDRESS, "\x1b[%i%p1%d;%p2%dH");
    entry.set_string(strings::CURSOR_HOME, "\x1b[H");
    entry.set_string(strings::CURSOR_UP, "\x1b[A");
    entry.set_string(strings::CURSOR_DOWN, "\x1b[B");
    entry.set_string(strings::CURSOR_LEFT, "\x08");
    entry.set_string(strings::CURSOR_RIGHT, "\x1b[C");
    entry.set_string(strings::CURSOR_INVISIBLE, "\x1b[?25l");
    entry.set_string(strings::CURSOR_VISIBLE, "\x1b[?25h");
    entry.set_string(strings::CURSOR_VERY_VISIBLE, "\x1b[?12;25h");

    // Screen manipulation
    entry.set_string(strings::CLEAR, "\x1b[H\x1b[2J");
    entry.set_string(strings::CLRTOBOT, "\x1b[J");
    entry.set_string(strings::CLRTOEOL, "\x1b[K");
    entry.set_string(strings::CLRTOBOL, "\x1b[1K");

    // Scrolling
    entry.set_string(strings::SCROLL_FORWARD, "\n");
    entry.set_string(strings::SCROLL_REVERSE, "\x1bM");
    entry.set_string(strings::CHANGE_SCROLL_REGION, "\x1b[%i%p1%d;%p2%dr");

    // Insert/delete
    entry.set_string(strings::INSERT_LINE, "\x1b[L");
    entry.set_string(strings::DELETE_LINE, "\x1b[M");
    entry.set_string(strings::INSERT_CHARACTER, "\x1b[@");
    entry.set_string(strings::DELETE_CHARACTER, "\x1b[P");

    // Attributes
    entry.set_string(strings::ENTER_BOLD, "\x1b[1m");
    entry.set_string(strings::ENTER_DIM, "\x1b[2m");
    entry.set_string(strings::ENTER_BLINK, "\x1b[5m");
    entry.set_string(strings::ENTER_REVERSE, "\x1b[7m");
    entry.set_string(strings::ENTER_STANDOUT, "\x1b[7m");
    entry.set_string(strings::EXIT_STANDOUT, "\x1b[27m");
    entry.set_string(strings::ENTER_UNDERLINE, "\x1b[4m");
    entry.set_string(strings::EXIT_UNDERLINE, "\x1b[24m");
    entry.set_string(strings::EXIT_ATTRIBUTES, "\x1b[0m");
    entry.set_string(
        strings::SET_ATTRIBUTES,
        "\x1b[0%?%p6%t;1%;%?%p2%t;4%;%?%p1%p3%|%t;7%;%?%p4%t;5%;m",
    );

    // Colors (256-color)
    entry.set_string(strings::SET_FOREGROUND, "\x1b[38;5;%p1%dm");
    entry.set_string(strings::SET_BACKGROUND, "\x1b[48;5;%p1%dm");
    entry.set_string(strings::ORIG_PAIR, "\x1b[39;49m");

    // Alternate screen
    entry.set_string(strings::ENTER_CA_MODE, "\x1b[?1049h");
    entry.set_string(strings::EXIT_CA_MODE, "\x1b[?1049l");

    // Keypad
    entry.set_string(strings::KEYPAD_XMIT, "\x1b[?1h\x1b=");
    entry.set_string(strings::KEYPAD_LOCAL, "\x1b[?1l\x1b>");

    // Function keys
    entry.set_string(strings::KEY_F1, "\x1bOP");
    entry.set_string(strings::KEY_F2, "\x1bOQ");
    entry.set_string(strings::KEY_F3, "\x1bOR");
    entry.set_string(strings::KEY_F4, "\x1bOS");
    entry.set_string(strings::KEY_F5, "\x1b[15~");
    entry.set_string(strings::KEY_F6, "\x1b[17~");
    entry.set_string(strings::KEY_F7, "\x1b[18~");
    entry.set_string(strings::KEY_F8, "\x1b[19~");
    entry.set_string(strings::KEY_F9, "\x1b[20~");
    entry.set_string(strings::KEY_F10, "\x1b[21~");
    entry.set_string(strings::KEY_F11, "\x1b[23~");
    entry.set_string(strings::KEY_F12, "\x1b[24~");

    // Arrow keys
    entry.set_string(strings::KEY_UP, "\x1b[A");
    entry.set_string(strings::KEY_DOWN, "\x1b[B");
    entry.set_string(strings::KEY_LEFT, "\x1b[D");
    entry.set_string(strings::KEY_RIGHT, "\x1b[C");
    entry.set_string(strings::KEY_HOME, "\x1b[H");
    entry.set_string(strings::KEY_END, "\x1b[F");

    // Editing keys
    entry.set_string(strings::KEY_BACKSPACE, "\x7f");
    entry.set_string(strings::KEY_DC, "\x1b[3~");
    entry.set_string(strings::KEY_IC, "\x1b[2~");
    entry.set_string(strings::KEY_PPAGE, "\x1b[5~");
    entry.set_string(strings::KEY_NPAGE, "\x1b[6~");

    // Alternate character set
    entry.set_string(strings::ENTER_ALT_CHARSET_MODE, "\x1b(0");
    entry.set_string(strings::EXIT_ALT_CHARSET_MODE, "\x1b(B");
    entry.set_string(
        strings::ACS_CHARS,
        "``aaffggiijjkkllmmnnooppqqrrssttuuvvwwxxyyzz{{||}}~~",
    );

    entry
}

/// Linux console terminal definition
fn linux_console() -> TerminalEntry {
    let mut entry = TerminalEntry::new("linux");

    // Dimensions
    entry.set_number(numbers::COLUMNS, 80);
    entry.set_number(numbers::LINES, 25);
    entry.set_number(numbers::COLORS, 8);
    entry.set_number(numbers::COLOR_PAIRS, 64);

    // Boolean capabilities
    entry.set_flag(bools::AUTO_RIGHT_MARGIN, true);
    entry.set_flag(bools::EAT_NEWLINE_GLITCH, true);
    entry.set_flag(bools::MOVE_STANDOUT_MODE, true);

    // Cursor movement (simpler than xterm)
    entry.set_string(strings::CURSOR_ADDRESS, "\x1b[%i%p1%d;%p2%dH");
    entry.set_string(strings::CURSOR_HOME, "\x1b[H");
    entry.set_string(strings::CURSOR_UP, "\x1b[A");
    entry.set_string(strings::CURSOR_DOWN, "\x1b[B");
    entry.set_string(strings::CURSOR_LEFT, "\x08");
    entry.set_string(strings::CURSOR_RIGHT, "\x1b[C");
    entry.set_string(strings::CURSOR_INVISIBLE, "\x1b[?25l");
    entry.set_string(strings::CURSOR_VISIBLE, "\x1b[?25h");

    // Screen manipulation
    entry.set_string(strings::CLEAR, "\x1b[H\x1b[J");
    entry.set_string(strings::CLRTOBOT, "\x1b[J");
    entry.set_string(strings::CLRTOEOL, "\x1b[K");

    // Scrolling
    entry.set_string(strings::SCROLL_FORWARD, "\n");
    entry.set_string(strings::SCROLL_REVERSE, "\x1bM");

    // Insert/delete
    entry.set_string(strings::INSERT_LINE, "\x1b[L");
    entry.set_string(strings::DELETE_LINE, "\x1b[M");

    // Attributes (basic 8-color)
    entry.set_string(strings::ENTER_BOLD, "\x1b[1m");
    entry.set_string(strings::ENTER_BLINK, "\x1b[5m");
    entry.set_string(strings::ENTER_REVERSE, "\x1b[7m");
    entry.set_string(strings::ENTER_UNDERLINE, "\x1b[4m");
    entry.set_string(strings::EXIT_ATTRIBUTES, "\x1b[0m");

    // Basic colors
    entry.set_string(strings::SET_FOREGROUND, "\x1b[3%p1%dm");
    entry.set_string(strings::SET_BACKGROUND, "\x1b[4%p1%dm");
    entry.set_string(strings::ORIG_PAIR, "\x1b[39;49m");

    // Keys
    entry.set_string(strings::KEY_BACKSPACE, "\x7f");
    entry.set_string(strings::KEY_UP, "\x1b[A");
    entry.set_string(strings::KEY_DOWN, "\x1b[B");
    entry.set_string(strings::KEY_LEFT, "\x1b[D");
    entry.set_string(strings::KEY_RIGHT, "\x1b[C");

    entry
}

/// VT100 terminal definition (classic)
fn vt100() -> TerminalEntry {
    let mut entry = TerminalEntry::new("vt100");

    // Dimensions
    entry.set_number(numbers::COLUMNS, 80);
    entry.set_number(numbers::LINES, 24);

    // Boolean capabilities
    entry.set_flag(bools::AUTO_RIGHT_MARGIN, true);
    entry.set_flag(bools::EAT_NEWLINE_GLITCH, true);

    // Cursor movement
    entry.set_string(strings::CURSOR_ADDRESS, "\x1b[%i%p1%d;%p2%dH");
    entry.set_string(strings::CURSOR_HOME, "\x1b[H");
    entry.set_string(strings::CURSOR_UP, "\x1b[A");
    entry.set_string(strings::CURSOR_DOWN, "\x1b[B");
    entry.set_string(strings::CURSOR_LEFT, "\x08");
    entry.set_string(strings::CURSOR_RIGHT, "\x1b[C");

    // Screen manipulation
    entry.set_string(strings::CLEAR, "\x1b[H\x1b[J");
    entry.set_string(strings::CLRTOBOT, "\x1b[J");
    entry.set_string(strings::CLRTOEOL, "\x1b[K");

    // Attributes (limited)
    entry.set_string(strings::ENTER_REVERSE, "\x1b[7m");
    entry.set_string(strings::EXIT_ATTRIBUTES, "\x1b[m");

    // Keys
    entry.set_string(strings::KEY_BACKSPACE, "\x08");
    entry.set_string(strings::KEY_UP, "\x1bOA");
    entry.set_string(strings::KEY_DOWN, "\x1bOB");
    entry.set_string(strings::KEY_LEFT, "\x1bOD");
    entry.set_string(strings::KEY_RIGHT, "\x1bOC");

    // Alternate character set
    entry.set_string(strings::ENTER_ALT_CHARSET_MODE, "\x1b(0");
    entry.set_string(strings::EXIT_ALT_CHARSET_MODE, "\x1b(B");

    entry
}

/// VT220 terminal definition
fn vt220() -> TerminalEntry {
    let mut entry = vt100();
    entry.name = "vt220".to_string();

    // VT220 adds more function keys
    entry.set_string(strings::KEY_F1, "\x1b[11~");
    entry.set_string(strings::KEY_F2, "\x1b[12~");
    entry.set_string(strings::KEY_F3, "\x1b[13~");
    entry.set_string(strings::KEY_F4, "\x1b[14~");
    entry.set_string(strings::KEY_F5, "\x1b[15~");

    // Additional keys
    entry.set_string(strings::KEY_IC, "\x1b[2~");
    entry.set_string(strings::KEY_DC, "\x1b[3~");
    entry.set_string(strings::KEY_HOME, "\x1b[1~");
    entry.set_string(strings::KEY_END, "\x1b[4~");
    entry.set_string(strings::KEY_PPAGE, "\x1b[5~");
    entry.set_string(strings::KEY_NPAGE, "\x1b[6~");

    entry
}

/// ANSI terminal definition (minimal)
fn ansi() -> TerminalEntry {
    let mut entry = TerminalEntry::new("ansi");

    // Dimensions
    entry.set_number(numbers::COLUMNS, 80);
    entry.set_number(numbers::LINES, 24);
    entry.set_number(numbers::COLORS, 8);

    // Boolean capabilities
    entry.set_flag(bools::AUTO_RIGHT_MARGIN, true);

    // Cursor movement
    entry.set_string(strings::CURSOR_ADDRESS, "\x1b[%i%p1%d;%p2%dH");
    entry.set_string(strings::CURSOR_HOME, "\x1b[H");
    entry.set_string(strings::CURSOR_UP, "\x1b[A");
    entry.set_string(strings::CURSOR_DOWN, "\x1b[B");
    entry.set_string(strings::CURSOR_LEFT, "\x1b[D");
    entry.set_string(strings::CURSOR_RIGHT, "\x1b[C");

    // Screen manipulation
    entry.set_string(strings::CLEAR, "\x1b[H\x1b[2J");
    entry.set_string(strings::CLRTOBOT, "\x1b[J");
    entry.set_string(strings::CLRTOEOL, "\x1b[K");

    // Attributes
    entry.set_string(strings::ENTER_BOLD, "\x1b[1m");
    entry.set_string(strings::ENTER_REVERSE, "\x1b[7m");
    entry.set_string(strings::EXIT_ATTRIBUTES, "\x1b[0m");

    // Colors
    entry.set_string(strings::SET_FOREGROUND, "\x1b[3%p1%dm");
    entry.set_string(strings::SET_BACKGROUND, "\x1b[4%p1%dm");

    entry
}

/// Dumb terminal (no special capabilities)
fn dumb() -> TerminalEntry {
    let mut entry = TerminalEntry::new("dumb");

    // Dimensions
    entry.set_number(numbers::COLUMNS, 80);
    entry.set_number(numbers::LINES, 24);

    // Boolean capabilities
    entry.set_flag(bools::AUTO_RIGHT_MARGIN, true);

    // Only basic output
    entry.set_string(strings::CURSOR_LEFT, "\x08");
    entry.set_string(strings::SCROLL_FORWARD, "\n");
    entry.set_string(strings::KEY_BACKSPACE, "\x08");

    entry
}
