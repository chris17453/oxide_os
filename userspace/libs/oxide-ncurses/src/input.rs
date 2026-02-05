//! # Input Handling
//!
//! Character and string input from the terminal.
//! Uses VTE parser to decode escape sequences into KEY_* codes.
//!
//! -- TorqueJax: Input driver - raw bytes in, decoded keycodes out
//! -- InputShade: Escape sequence decoder - CSI to KEY_* translation

use crate::chtype;
use crate::keys;
use crate::output::waddch;
use crate::screen;
use crate::{Error, Result, WINDOW};
use alloc::vec::Vec;
use vte::{Action, Parser};

/// Input state for escape sequence decoding
/// -- InputShade: Stateful decoder - VTE parser persists across calls
struct InputState {
    parser: Parser,
    pushback: Vec<i32>,
    pending: Vec<i32>,
}

impl InputState {
    fn new() -> Self {
        Self {
            parser: Parser::new(),
            pushback: Vec::new(),
            pending: Vec::new(),
        }
    }
}

/// Global input state
static mut INPUT_STATE: Option<InputState> = None;

/// Get or initialize the global input state
fn input_state() -> &'static mut InputState {
    unsafe {
        let ptr = core::ptr::addr_of_mut!(INPUT_STATE);
        if (*ptr).is_none() {
            *ptr = Some(InputState::new());
        }
        (*ptr).as_mut().unwrap()
    }
}

/// Get a character from standard input
pub fn getch() -> i32 {
    wgetch(screen::stdscr())
}

/// Get a character from a window
/// -- InputShade: Core input loop - read byte, parse escape, map to keycode
pub fn wgetch(win: WINDOW) -> i32 {
    if win.is_null() {
        return -1;
    }

    let state = input_state();

    // Check pushback buffer first (ungetch LIFO)
    if let Some(ch) = state.pushback.pop() {
        return ch;
    }

    // Check pending decoded keys (FIFO)
    if !state.pending.is_empty() {
        return state.pending.remove(0);
    }

    // Read from stdin
    let mut byte = [0u8; 1];
    let nodelay = unsafe { (*win).nodelay };

    // — InputShade: nodelay mode must not block. Use poll(timeout=0) to
    // check for available data before attempting read(). Without this,
    // read(0,...) blocks until a byte arrives, stalling apps like top
    // that need timer-driven refresh between keystrokes.
    if nodelay {
        let mut pfd = libc::poll::PollFd::new(0, libc::poll::events::POLLIN);
        let ready = libc::poll::poll(core::slice::from_mut(&mut pfd), 0);
        if ready <= 0 || (pfd.revents & libc::poll::events::POLLIN) == 0 {
            return -1;
        }
    }

    let n = libc::unistd::read(0, &mut byte);
    if n <= 0 {
        return -1;
    }

    // Feed byte through VTE parser
    let action = state.parser.advance(byte[0]);
    let keypad = unsafe { (*win).keypad };

    let result = match action {
        Action::Print(ch) => ch as i32,

        Action::Execute(b) => {
            match b {
                0x0D => '\n' as i32, // CR -> newline
                0x0A => '\n' as i32, // LF -> newline
                0x08 => keys::KEY_BACKSPACE,
                0x09 => '\t' as i32, // Tab
                0x7F => keys::KEY_BACKSPACE,
                0x1B => {
                    // — InputShade: ESC received. Could be raw ESC or start of
                    // an escape sequence (CSI, SS3). Use poll with short timeout
                    // to check — if no follow-up byte arrives, it's a raw ESC.
                    // Without this timeout, read() blocks forever on bare ESC.
                    let mut next = [0u8; 1];
                    let mut pfd = libc::poll::PollFd::new(0, libc::poll::events::POLLIN);
                    let ready = libc::poll::poll(core::slice::from_mut(&mut pfd), 50);
                    if ready <= 0 {
                        27 // Raw ESC — no follow-up within 50ms
                    } else {
                    let n2 = libc::unistd::read(0, &mut next);
                    if n2 <= 0 {
                        27 // Read failed
                    } else {
                        // Feed through parser
                        let action2 = state.parser.advance(next[0]);
                        match action2 {
                            Action::None => {
                                // Parser needs more input - keep reading
                                read_escape_sequence(state, keypad)
                            }
                            Action::CsiDispatch {
                                params,
                                intermediates: _,
                                final_char,
                            } => map_csi_to_key(&params, final_char, keypad),
                            Action::EscDispatch {
                                intermediates: _,
                                final_char,
                            } => map_esc_to_key(final_char),
                            Action::Print(ch) => {
                                // Alt+char
                                // Push char to pending, return ESC
                                state.pending.push(ch as i32);
                                27
                            }
                            _ => 27,
                        }
                    }
                    }
                }
                _ => b as i32,
            }
        }

        Action::CsiDispatch {
            params,
            intermediates: _,
            final_char,
        } => map_csi_to_key(&params, final_char, keypad),

        Action::EscDispatch {
            intermediates: _,
            final_char,
        } => map_esc_to_key(final_char),

        Action::None => {
            // Parser needs more input (mid-sequence)
            // Read more bytes until sequence completes
            read_escape_sequence(state, keypad)
        }

        _ => -1,
    };

    // Echo if enabled
    if result >= 0 {
        let echo_on = screen::current_screen().map(|s| s.echo).unwrap_or(false);
        if echo_on && result >= 0x20 && result < 0x7F {
            if let Some(ch) = char::from_u32(result as u32) {
                let ch_val = chtype::new(ch, 0);
                let _ = waddch(win, ch_val);
                let _ = screen::wrefresh(win);
            }
        }
    }

    result
}

/// Read remaining bytes of an escape sequence from stdin
/// — InputShade: Continue reading until VTE parser emits an action.
/// Uses poll() with short timeout so partial/broken sequences don't
/// block forever. If no byte arrives within 50ms, abandon the sequence.
fn read_escape_sequence(state: &mut InputState, keypad: bool) -> i32 {
    let mut attempts = 0;
    loop {
        // Check if more data is available before blocking
        let mut pfd = libc::poll::PollFd::new(0, libc::poll::events::POLLIN);
        let ready = libc::poll::poll(core::slice::from_mut(&mut pfd), 50);
        if ready <= 0 {
            return -1; // Timeout — incomplete escape sequence
        }

        let mut byte = [0u8; 1];
        let n = libc::unistd::read(0, &mut byte);
        if n <= 0 || attempts > 16 {
            return -1;
        }
        attempts += 1;

        let action = state.parser.advance(byte[0]);
        match action {
            Action::CsiDispatch {
                params,
                intermediates: _,
                final_char,
            } => {
                return map_csi_to_key(&params, final_char, keypad);
            }
            Action::EscDispatch {
                intermediates: _,
                final_char,
            } => {
                return map_esc_to_key(final_char);
            }
            Action::Print(ch) => return ch as i32,
            Action::Execute(b) => return b as i32,
            Action::None => continue,
            _ => return -1,
        }
    }
}

/// Map CSI final character + params to KEY_* constants
/// -- InputShade: CSI decoder - arrows, home/end, page up/down, F-keys
fn map_csi_to_key(params: &[i32], final_char: u8, keypad: bool) -> i32 {
    if !keypad {
        // Without keypad mode, return raw character
        return final_char as i32;
    }

    match final_char {
        b'A' => keys::KEY_UP,
        b'B' => keys::KEY_DOWN,
        b'C' => keys::KEY_RIGHT,
        b'D' => keys::KEY_LEFT,
        b'H' => keys::KEY_HOME,
        b'F' => keys::KEY_END,
        b'~' => {
            // CSI N ~ sequences
            let n = params.first().copied().unwrap_or(0);
            match n {
                2 => keys::KEY_IC,    // Insert
                3 => keys::KEY_DC,    // Delete
                5 => keys::KEY_PPAGE, // Page Up
                6 => keys::KEY_NPAGE, // Page Down
                15 => keys::KEY_F5,
                17 => keys::KEY_F6,
                18 => keys::KEY_F7,
                19 => keys::KEY_F8,
                20 => keys::KEY_F9,
                21 => keys::KEY_F10,
                23 => keys::KEY_F11,
                24 => keys::KEY_F12,
                _ => -1,
            }
        }
        _ => final_char as i32,
    }
}

/// Map ESC + final char to key code (SS3 sequences)
/// -- InputShade: SS3 decoder - xterm function keys and alt-screen arrows
fn map_esc_to_key(final_char: u8) -> i32 {
    match final_char {
        b'P' => keys::KEY_F1, // SS3 P
        b'Q' => keys::KEY_F2, // SS3 Q
        b'R' => keys::KEY_F3, // SS3 R
        b'S' => keys::KEY_F4, // SS3 S
        b'A' => keys::KEY_UP, // SS3 A (application mode)
        b'B' => keys::KEY_DOWN,
        b'C' => keys::KEY_RIGHT,
        b'D' => keys::KEY_LEFT,
        b'H' => keys::KEY_HOME,
        b'F' => keys::KEY_END,
        _ => final_char as i32,
    }
}

/// Get a string from standard input
pub fn getstr(s: &mut [u8]) -> Result<()> {
    wgetstr(screen::stdscr(), s)
}

/// Get a string from a window
/// -- InputShade: Line editor - reads until newline with backspace handling
pub fn wgetstr(win: WINDOW, s: &mut [u8]) -> Result<()> {
    if win.is_null() || s.is_empty() {
        return Err(Error::Err);
    }

    let max = s.len() - 1; // Leave room for null terminator
    let mut pos = 0;

    loop {
        let ch = wgetch(win);
        if ch < 0 {
            continue;
        }

        match ch {
            0x0A | 0x0D => {
                // Enter - finish
                s[pos] = 0;
                return Ok(());
            }
            0x08 | 0x7F => {
                // Backspace
                if pos > 0 {
                    pos -= 1;
                }
            }
            _ if ch >= 0x20 && ch < 0x7F => {
                if pos < max {
                    s[pos] = ch as u8;
                    pos += 1;
                }
            }
            _ => {}
        }
    }
}

/// Get a string with length limit
pub fn getnstr(s: &mut [u8], n: i32) -> Result<()> {
    wgetnstr(screen::stdscr(), s, n)
}

/// Get a string with length limit from a window
pub fn wgetnstr(win: WINDOW, s: &mut [u8], n: i32) -> Result<()> {
    if n <= 0 {
        return Err(Error::Err);
    }
    let limit = (n as usize).min(s.len());
    wgetstr(win, &mut s[..limit])
}

/// Ungetch - push character back onto input queue (LIFO)
/// -- InputShade: Pushback stack - unread a keycode
pub fn ungetch(ch: i32) -> Result<()> {
    let state = input_state();
    state.pushback.push(ch);
    Ok(())
}

/// Has key - check if a key has been pressed
pub fn has_key(_ch: i32) -> bool {
    false
}
