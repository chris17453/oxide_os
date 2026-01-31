//! Readline library for OXIDE OS
//!
//! Provides GNU readline-compatible line editing with:
//! - Cursor movement (arrows, Home, End, Ctrl+Left/Right)
//! - History navigation (Up/Down arrows)
//! - Tab completion with pluggable callback
//! - Raw terminal mode management
//!
//! Used by the shell (via Rust API) and CPython (via C exports).

use crate::stdio::{getchar, putchar};
use crate::termios::{self, Termios};
use crate::unistd;

/// Maximum line buffer size
const MAX_LINE: usize = 4096;

/// Maximum history entries
const MAX_HISTORY: usize = 500;

/// Maximum completions to display
const MAX_COMPLETIONS: usize = 128;

/// Escape byte
const ESC: u8 = 0x1B;

/// Tab
const TAB: u8 = 0x09;

/// Backspace (DEL key)
const BACKSPACE: u8 = 0x7F;

/// Ctrl-H (alternate backspace)
const CTRL_H: u8 = 0x08;

/// History entry (C-compatible)
#[repr(C)]
pub struct HistEntry {
    pub line: *mut u8,
    pub timestamp: *mut u8,
    pub data: *mut u8,
}

/// History state (C-compatible)
#[repr(C)]
pub struct HistoryState {
    pub entries: *mut *mut HistEntry,
    pub offset: i32,
    pub length: i32,
    pub size: i32,
    pub flags: i32,
}

/// Completion function type: (text, start, end) -> *mut *mut u8
pub type CompletionFunc = Option<unsafe extern "C" fn(*const u8, i32, i32) -> *mut *mut u8>;

/// Entry generator function type: (text, state) -> *mut u8
pub type CompEntryFunc = Option<unsafe extern "C" fn(*const u8, i32) -> *mut u8>;

/// Command function type: (count, key) -> i32
pub type CommandFunc = Option<unsafe extern "C" fn(i32, i32) -> i32>;

/// Display matches hook type
pub type CompDispFunc = Option<unsafe extern "C" fn(*mut *mut u8, i32, i32)>;

/// Hook function type
pub type HookFunc = Option<unsafe extern "C" fn() -> i32>;

/// Callback handler type for rl_callback_handler_install
pub type CallbackHandler = Option<unsafe extern "C" fn(*mut u8)>;

// ── Internal state ──────────────────────────────────────────────────

/// Line editing buffer
static mut LINE_BUF: [u8; MAX_LINE] = [0; MAX_LINE];
static mut LINE_LEN: usize = 0;
static mut CURSOR: usize = 0;

/// History ring buffer
static mut HISTORY: [[u8; MAX_LINE]; MAX_HISTORY] = [[0; MAX_LINE]; MAX_HISTORY];
static mut HISTORY_COUNT: usize = 0;
static mut HISTORY_POS: usize = 0;

/// Saved terminal state
static mut ORIG_TERMIOS: Termios = Termios {
    c_iflag: 0,
    c_oflag: 0,
    c_cflag: 0,
    c_lflag: 0,
    c_line: 0,
    c_cc: [0; termios::NCCS],
    c_ispeed: 0,
    c_ospeed: 0,
};
static mut TERMIOS_SAVED: bool = false;

/// Whether the library has been initialized
static mut INITIALIZED: bool = false;

/// Static HistEntry pool for history_get() return values
static mut HIST_ENTRIES: [HistEntry; MAX_HISTORY] = {
    const EMPTY: HistEntry = HistEntry {
        line: core::ptr::null_mut(),
        timestamp: core::ptr::null_mut(),
        data: core::ptr::null_mut(),
    };
    [EMPTY; MAX_HISTORY]
};

/// Static HistoryState for history_get_history_state()
static mut HIST_STATE: HistoryState = HistoryState {
    entries: core::ptr::null_mut(),
    offset: 0,
    length: 0,
    size: 0,
    flags: 0,
};

/// Callback mode state
static mut CALLBACK_HANDLER: CallbackHandler = None;
static mut CALLBACK_PROMPT: [u8; 256] = [0; 256];

// ── Public global variables (C-compatible) ──────────────────────────

/// Application name
#[unsafe(no_mangle)]
pub static mut rl_readline_name: *mut u8 = core::ptr::null_mut();

/// Current line buffer pointer (points to LINE_BUF)
#[unsafe(no_mangle)]
pub static mut rl_line_buffer: *mut u8 = core::ptr::null_mut();

/// Current cursor position
#[unsafe(no_mangle)]
pub static mut rl_point: i32 = 0;

/// End of input position
#[unsafe(no_mangle)]
pub static mut rl_end: i32 = 0;

/// Input stream (opaque FILE*)
#[unsafe(no_mangle)]
pub static mut rl_instream: *mut u8 = core::ptr::null_mut();

/// Output stream (opaque FILE*)
#[unsafe(no_mangle)]
pub static mut rl_outstream: *mut u8 = core::ptr::null_mut();

/// Library version string
static RL_VERSION_STR: [u8; 4] = *b"8.0\0";

#[unsafe(no_mangle)]
pub static mut rl_library_version: *const u8 = RL_VERSION_STR.as_ptr();

/// Readline version number (8.0 = 0x0800)
#[unsafe(no_mangle)]
pub static mut rl_readline_version: i32 = 0x0800;

/// Whether to catch signals
#[unsafe(no_mangle)]
pub static mut rl_catch_signals: i32 = 1;

/// Attempted completion function — called before default file completion
#[unsafe(no_mangle)]
pub static mut rl_attempted_completion_function: CompletionFunc = None;

/// Set to non-zero to suppress default filename completion
#[unsafe(no_mangle)]
pub static mut rl_attempted_completion_over: i32 = 0;

/// Characters that delimit words for completion
static DEFAULT_WORD_BREAK: [u8; 19] = *b" \t\n\"\\'`@$><=;|&{(\0";

#[unsafe(no_mangle)]
pub static mut rl_completer_word_break_characters: *const u8 = DEFAULT_WORD_BREAK.as_ptr();

#[unsafe(no_mangle)]
pub static mut rl_basic_word_break_characters: *const u8 = DEFAULT_WORD_BREAK.as_ptr();

/// Character to append after a completion (default: space)
#[unsafe(no_mangle)]
pub static mut rl_completion_append_character: i32 = b' ' as i32;

/// Set to suppress appending the completion character
#[unsafe(no_mangle)]
pub static mut rl_completion_suppress_append: i32 = 0;

/// Display matches hook
#[unsafe(no_mangle)]
pub static mut rl_completion_display_matches_hook: CompDispFunc = None;

/// Completion type (TAB, '?', etc.)
#[unsafe(no_mangle)]
pub static mut rl_completion_type: i32 = 0;

/// Startup hook — called before reading each line
#[unsafe(no_mangle)]
pub static mut rl_startup_hook: HookFunc = None;

/// Pre-input hook — called after prompt, before reading
#[unsafe(no_mangle)]
pub static mut rl_pre_input_hook: HookFunc = None;

/// Emacs meta keymap (opaque pointer, stub)
#[unsafe(no_mangle)]
pub static mut emacs_meta_keymap: *mut u8 = core::ptr::null_mut();

/// History length (exported for C)
#[unsafe(no_mangle)]
pub static mut history_length: i32 = 0;

// ── Terminal management ─────────────────────────────────────────────

/// Enter raw mode for line editing
fn enter_raw_mode() {
    use termios::*;

    let mut raw = Termios::default();
    if tcgetattr(0, &mut raw) < 0 {
        return;
    }

    unsafe {
        ORIG_TERMIOS = raw.clone();
        TERMIOS_SAVED = true;
    }

    // Disable canonical mode, echo, signal generation
    raw.c_lflag &= !(lflag::ICANON
        | lflag::ECHO
        | lflag::ECHOE
        | lflag::ECHOK
        | lflag::ECHOKE
        | lflag::ECHOCTL
        | lflag::ISIG
        | lflag::IEXTEN);

    // Disable ICRNL so CR comes through as-is
    raw.c_iflag &= !iflag::ICRNL;

    // Return after 1 byte, no timeout
    raw.c_cc[cc::VMIN] = 1;
    raw.c_cc[cc::VTIME] = 0;

    tcsetattr(0, action::TCSANOW, &raw);
}

/// Restore original terminal mode
fn leave_raw_mode() {
    use termios::*;
    unsafe {
        if TERMIOS_SAVED {
            tcsetattr(0, action::TCSANOW, &ORIG_TERMIOS);
        }
    }
}

// ── Output helpers ──────────────────────────────────────────────────

/// Print a string slice to stdout
fn write_str(s: &str) {
    for &b in s.as_bytes() {
        putchar(b);
    }
}

/// Print bytes until NUL
fn write_bytes(s: &[u8]) {
    for &b in s {
        if b == 0 {
            break;
        }
        putchar(b);
    }
}

/// Print a usize as decimal
fn write_num(mut n: usize) {
    if n == 0 {
        putchar(b'0');
        return;
    }
    let mut digits = [0u8; 20];
    let mut i = 0;
    while n > 0 {
        digits[i] = b'0' + (n % 10) as u8;
        n /= 10;
        i += 1;
    }
    while i > 0 {
        i -= 1;
        putchar(digits[i]);
    }
}

/// Move terminal cursor by `delta` positions (positive = right, negative = left)
fn move_cursor(delta: isize) {
    if delta > 0 {
        write_str("\x1b[");
        write_num(delta as usize);
        putchar(b'C');
    } else if delta < 0 {
        write_str("\x1b[");
        write_num((-delta) as usize);
        putchar(b'D');
    }
}

/// Length of a NUL-terminated byte slice
fn cstrlen(s: &[u8]) -> usize {
    for i in 0..s.len() {
        if s[i] == 0 {
            return i;
        }
    }
    s.len()
}

// ── Prompt rendering ────────────────────────────────────────────────

/// Print the prompt and update cursor tracking
fn print_prompt(prompt: &[u8]) {
    write_bytes(prompt);
}

// ── Core line editing ───────────────────────────────────────────────

/// Redraw the current line (clear line, print prompt, print buffer, position cursor)
unsafe fn redraw_line(prompt: &[u8]) {
    write_str("\r\x1b[2K");
    print_prompt(prompt);
    for i in 0..LINE_LEN {
        putchar(LINE_BUF[i]);
    }
    // Move cursor back to correct position
    if CURSOR < LINE_LEN {
        move_cursor(-((LINE_LEN - CURSOR) as isize));
    }
}

/// Insert a character at the current cursor position
unsafe fn insert_char(c: u8, prompt: &[u8]) {
    if LINE_LEN >= MAX_LINE - 1 {
        return;
    }

    if CURSOR == LINE_LEN {
        // Append at end
        LINE_BUF[LINE_LEN] = c;
        LINE_LEN += 1;
        CURSOR += 1;
        putchar(c);
    } else {
        // Insert in middle: shift right
        let mut i = LINE_LEN;
        while i > CURSOR {
            LINE_BUF[i] = LINE_BUF[i - 1];
            i -= 1;
        }
        LINE_BUF[CURSOR] = c;
        LINE_LEN += 1;
        CURSOR += 1;
        redraw_line(prompt);
    }
}

/// Delete the character before the cursor (backspace)
unsafe fn backspace_char(prompt: &[u8]) {
    if CURSOR == 0 {
        return;
    }

    if CURSOR == LINE_LEN {
        // At end: simple erase
        CURSOR -= 1;
        LINE_LEN -= 1;
        LINE_BUF[LINE_LEN] = 0;
        write_str("\x08 \x08");
    } else {
        // In middle: shift left
        CURSOR -= 1;
        let mut i = CURSOR;
        while i < LINE_LEN - 1 {
            LINE_BUF[i] = LINE_BUF[i + 1];
            i += 1;
        }
        LINE_LEN -= 1;
        LINE_BUF[LINE_LEN] = 0;
        redraw_line(prompt);
    }
}

/// Delete the character under the cursor
unsafe fn delete_char(prompt: &[u8]) {
    if CURSOR >= LINE_LEN {
        return;
    }

    let mut i = CURSOR;
    while i < LINE_LEN - 1 {
        LINE_BUF[i] = LINE_BUF[i + 1];
        i += 1;
    }
    LINE_LEN -= 1;
    LINE_BUF[LINE_LEN] = 0;
    redraw_line(prompt);
}

/// Replace the line buffer with a history entry and redraw
unsafe fn replace_line(content: &[u8], prompt: &[u8]) {
    LINE_LEN = cstrlen(content).min(MAX_LINE - 1);
    LINE_BUF[..LINE_LEN].copy_from_slice(&content[..LINE_LEN]);
    LINE_BUF[LINE_LEN] = 0;
    CURSOR = LINE_LEN;
    redraw_line(prompt);
}

/// Move cursor to the start of the previous word
unsafe fn move_prev_word() {
    if CURSOR == 0 {
        return;
    }
    let old = CURSOR;
    // Skip spaces left
    while CURSOR > 0 && LINE_BUF[CURSOR - 1] == b' ' {
        CURSOR -= 1;
    }
    // Skip word chars left
    while CURSOR > 0 && LINE_BUF[CURSOR - 1] != b' ' {
        CURSOR -= 1;
    }
    move_cursor(-((old - CURSOR) as isize));
}

/// Move cursor to the start of the next word
unsafe fn move_next_word() {
    if CURSOR >= LINE_LEN {
        return;
    }
    let old = CURSOR;
    // Skip current word
    while CURSOR < LINE_LEN && LINE_BUF[CURSOR] != b' ' {
        CURSOR += 1;
    }
    // Skip spaces
    while CURSOR < LINE_LEN && LINE_BUF[CURSOR] == b' ' {
        CURSOR += 1;
    }
    move_cursor((CURSOR - old) as isize);
}

/// Kill from cursor to end of line
unsafe fn kill_to_end(prompt: &[u8]) {
    if CURSOR >= LINE_LEN {
        return;
    }
    LINE_LEN = CURSOR;
    LINE_BUF[LINE_LEN] = 0;
    // Clear from cursor to end of line
    write_str("\x1b[K");
}

/// Kill from start to cursor
unsafe fn kill_to_start(prompt: &[u8]) {
    if CURSOR == 0 {
        return;
    }
    let shift = CURSOR;
    let mut i = 0;
    while i + shift < LINE_LEN {
        LINE_BUF[i] = LINE_BUF[i + shift];
        i += 1;
    }
    LINE_LEN -= shift;
    LINE_BUF[LINE_LEN] = 0;
    CURSOR = 0;
    redraw_line(prompt);
}

/// Transpose the two characters before the cursor
unsafe fn transpose_chars(prompt: &[u8]) {
    if CURSOR < 2 || LINE_LEN < 2 {
        return;
    }
    let tmp = LINE_BUF[CURSOR - 2];
    LINE_BUF[CURSOR - 2] = LINE_BUF[CURSOR - 1];
    LINE_BUF[CURSOR - 1] = tmp;
    redraw_line(prompt);
}

// ── Completion engine ───────────────────────────────────────────────

/// Find the start of the word being completed
unsafe fn find_word_start() -> usize {
    let break_chars = rl_completer_word_break_characters;
    let mut pos = CURSOR;
    while pos > 0 {
        let c = LINE_BUF[pos - 1];
        // Check if c is a word-break character
        if is_break_char(c, break_chars) {
            break;
        }
        pos -= 1;
    }
    pos
}

/// Check if a character is a word-break character
unsafe fn is_break_char(c: u8, break_chars: *const u8) -> bool {
    if break_chars.is_null() {
        return c == b' ' || c == b'\t';
    }
    let mut i = 0;
    loop {
        let bc = *break_chars.add(i);
        if bc == 0 {
            break;
        }
        if bc == c {
            return true;
        }
        i += 1;
    }
    false
}

/// Handle tab completion
unsafe fn handle_completion(prompt: &[u8]) {
    let word_start = find_word_start();
    let word_len = CURSOR - word_start;

    // Build NUL-terminated copy of the word
    let mut word = [0u8; MAX_LINE];
    if word_len > 0 {
        word[..word_len].copy_from_slice(&LINE_BUF[word_start..CURSOR]);
    }
    word[word_len] = 0;

    // Try the attempted completion function first
    let mut matches: *mut *mut u8 = core::ptr::null_mut();

    if let Some(func) = rl_attempted_completion_function {
        matches = func(word.as_ptr(), word_start as i32, CURSOR as i32);
    }

    if matches.is_null() && rl_attempted_completion_over == 0 {
        // No custom completion and not suppressed — no default completion
        // (filesystem completion would need to be registered by the caller)
        return;
    }

    if matches.is_null() {
        return;
    }

    // Count matches (NULL-terminated array, first element is common prefix)
    let mut count = 0;
    while !(*matches.add(count)).is_null() {
        count += 1;
    }

    if count == 0 {
        return;
    }

    if count == 1 {
        // Single match (or just common prefix) — insert it
        let m = *matches;
        let match_len = crate::string::strlen(m);
        // Replace from word_start
        if match_len > word_len {
            // Insert the extra characters
            let extra = &core::slice::from_raw_parts(m, match_len)[word_len..];
            for &c in extra {
                insert_char(c, prompt);
            }
            // Append the completion character if not suppressed
            if rl_completion_suppress_append == 0 && rl_completion_append_character != 0 {
                insert_char(rl_completion_append_character as u8, prompt);
            }
        }
    } else if count == 2 && !(*matches.add(0)).is_null() && !(*matches.add(1)).is_null() {
        // Two entries: matches[0] = common prefix, matches[1] = sole match
        // This is the standard readline format: first entry = LCD, rest = matches
        // With count == 2, there's exactly one match
        let common = *matches.add(0);
        let common_len = crate::string::strlen(common);
        if common_len > word_len {
            let extra = &core::slice::from_raw_parts(common, common_len)[word_len..];
            for &c in extra {
                insert_char(c, prompt);
            }
            if rl_completion_suppress_append == 0 && rl_completion_append_character != 0 {
                insert_char(rl_completion_append_character as u8, prompt);
            }
        }
    } else {
        // Multiple matches: matches[0] = common prefix (LCD), rest = individual matches
        // First, insert the common prefix
        let common = *matches.add(0);
        let common_len = crate::string::strlen(common);
        if common_len > word_len {
            let extra = &core::slice::from_raw_parts(common, common_len)[word_len..];
            for &c in extra {
                insert_char(c, prompt);
            }
        }

        // Display all matches (skip matches[0] which is the LCD)
        putchar(b'\n');
        for i in 1..count {
            let m = *matches.add(i);
            if !m.is_null() {
                let mlen = crate::string::strlen(m);
                let s = core::slice::from_raw_parts(m, mlen);
                write_bytes(s);
                write_str("  ");
            }
        }
        putchar(b'\n');
        redraw_line(prompt);
    }

    // Free the matches array (each entry and the array itself)
    // Note: with bump allocator, free is a no-op, but call it for correctness
    for i in 0..count {
        let m = *matches.add(i);
        if !m.is_null() {
            crate::c_exports::free(m);
        }
    }
    crate::c_exports::free(matches as *mut u8);
}

// ── Main readline function ──────────────────────────────────────────

/// Read a line with editing support.
///
/// Returns a malloc'd string (caller must free), or NULL on EOF.
/// The returned string does NOT include a trailing newline.
pub unsafe fn readline(prompt: *const u8) -> *mut u8 {
    if !INITIALIZED {
        rl_initialize_internal();
    }

    // Print prompt
    let prompt_len = if prompt.is_null() {
        0
    } else {
        crate::string::strlen(prompt)
    };
    let prompt_slice = if prompt.is_null() {
        &[]
    } else {
        core::slice::from_raw_parts(prompt, prompt_len)
    };
    print_prompt(prompt_slice);

    // Call startup hook if set
    if let Some(hook) = rl_startup_hook {
        hook();
    }

    // Reset line state
    LINE_LEN = 0;
    CURSOR = 0;
    LINE_BUF.fill(0);
    HISTORY_POS = HISTORY_COUNT;

    // Update global pointers
    rl_line_buffer = LINE_BUF.as_mut_ptr();
    rl_point = 0;
    rl_end = 0;

    // Call pre-input hook if set
    if let Some(hook) = rl_pre_input_hook {
        hook();
    }

    // Enter raw mode
    enter_raw_mode();

    let result = read_line_loop(prompt_slice);

    // Leave raw mode
    leave_raw_mode();

    result
}

/// The main character-reading loop
unsafe fn read_line_loop(prompt: &[u8]) -> *mut u8 {
    loop {
        // Sync globals
        rl_point = CURSOR as i32;
        rl_end = LINE_LEN as i32;

        let c = getchar();
        if c < 0 {
            // EOF
            return core::ptr::null_mut();
        }
        let c = c as u8;

        // Handle escape sequences
        if c == ESC {
            let next = getchar();
            if next == b'[' as i32 {
                let code = getchar() as u8;
                match code {
                    b'A' => {
                        // Up: previous history
                        if HISTORY_COUNT > 0 && HISTORY_POS > 0 {
                            HISTORY_POS -= 1;
                            replace_line(&HISTORY[HISTORY_POS], prompt);
                        }
                        continue;
                    }
                    b'B' => {
                        // Down: next history
                        if HISTORY_POS + 1 < HISTORY_COUNT {
                            HISTORY_POS += 1;
                            replace_line(&HISTORY[HISTORY_POS], prompt);
                        } else {
                            HISTORY_POS = HISTORY_COUNT;
                            LINE_LEN = 0;
                            CURSOR = 0;
                            LINE_BUF[0] = 0;
                            redraw_line(prompt);
                        }
                        continue;
                    }
                    b'C' => {
                        // Right
                        if CURSOR < LINE_LEN {
                            CURSOR += 1;
                            write_str("\x1b[C");
                        }
                        continue;
                    }
                    b'D' => {
                        // Left
                        if CURSOR > 0 {
                            CURSOR -= 1;
                            write_str("\x1b[D");
                        }
                        continue;
                    }
                    b'H' => {
                        // Home
                        if CURSOR > 0 {
                            move_cursor(-(CURSOR as isize));
                            CURSOR = 0;
                        }
                        continue;
                    }
                    b'F' => {
                        // End
                        if CURSOR < LINE_LEN {
                            move_cursor((LINE_LEN - CURSOR) as isize);
                            CURSOR = LINE_LEN;
                        }
                        continue;
                    }
                    b'3' => {
                        // Delete key: ESC [ 3 ~
                        let tilde = getchar();
                        if tilde == b'~' as i32 {
                            delete_char(prompt);
                        }
                        continue;
                    }
                    b'1' => {
                        // CSI 1;5D / 1;5C for Ctrl+Left/Right
                        let semi = getchar();
                        if semi == b';' as i32 {
                            let modch = getchar() as u8;
                            let last = getchar() as u8;
                            if modch == b'5' {
                                if last == b'D' {
                                    move_prev_word();
                                } else if last == b'C' {
                                    move_next_word();
                                }
                            }
                        }
                        continue;
                    }
                    _ => {
                        continue;
                    }
                }
            } else if next == b'b' as i32 {
                // Alt+B: backward word
                move_prev_word();
                continue;
            } else if next == b'f' as i32 {
                // Alt+F: forward word
                move_next_word();
                continue;
            } else if next == b'd' as i32 {
                // Alt+D: kill word forward (simplified: skip for now)
                continue;
            }
            continue;
        }

        match c {
            b'\n' | b'\r' => {
                // Accept line
                putchar(b'\n');
                LINE_BUF[LINE_LEN] = 0;

                // Allocate and copy result
                let result = crate::c_exports::malloc(LINE_LEN + 1);
                if result.is_null() {
                    return core::ptr::null_mut();
                }
                core::ptr::copy_nonoverlapping(LINE_BUF.as_ptr(), result, LINE_LEN);
                *result.add(LINE_LEN) = 0;
                return result;
            }
            TAB => {
                handle_completion(prompt);
                continue;
            }
            BACKSPACE | CTRL_H => {
                backspace_char(prompt);
                continue;
            }
            0x01 => {
                // Ctrl-A: beginning of line
                if CURSOR > 0 {
                    move_cursor(-(CURSOR as isize));
                    CURSOR = 0;
                }
            }
            0x02 => {
                // Ctrl-B: back one character
                if CURSOR > 0 {
                    CURSOR -= 1;
                    write_str("\x1b[D");
                }
            }
            0x03 => {
                // Ctrl-C: cancel line
                write_str("^C\n");
                // Return empty string (not NULL, to distinguish from EOF)
                let result = crate::c_exports::malloc(1);
                if !result.is_null() {
                    *result = 0;
                }
                return result;
            }
            0x04 => {
                // Ctrl-D: EOF if empty, delete char otherwise
                if LINE_LEN == 0 {
                    putchar(b'\n');
                    return core::ptr::null_mut();
                }
                delete_char(prompt);
            }
            0x05 => {
                // Ctrl-E: end of line
                if CURSOR < LINE_LEN {
                    move_cursor((LINE_LEN - CURSOR) as isize);
                    CURSOR = LINE_LEN;
                }
            }
            0x06 => {
                // Ctrl-F: forward one character
                if CURSOR < LINE_LEN {
                    CURSOR += 1;
                    write_str("\x1b[C");
                }
            }
            0x0B => {
                // Ctrl-K: kill to end of line
                kill_to_end(prompt);
            }
            0x0C => {
                // Ctrl-L: clear screen
                write_str("\x1b[H\x1b[2J");
                redraw_line(prompt);
            }
            0x14 => {
                // Ctrl-T: transpose characters
                transpose_chars(prompt);
            }
            0x15 => {
                // Ctrl-U: kill to beginning of line
                kill_to_start(prompt);
            }
            0x17 => {
                // Ctrl-W: kill previous word
                if CURSOR > 0 {
                    let old = CURSOR;
                    // Skip spaces
                    while CURSOR > 0 && LINE_BUF[CURSOR - 1] == b' ' {
                        CURSOR -= 1;
                    }
                    // Skip word
                    while CURSOR > 0 && LINE_BUF[CURSOR - 1] != b' ' {
                        CURSOR -= 1;
                    }
                    let shift = old - CURSOR;
                    let mut i = CURSOR;
                    while i + shift < LINE_LEN {
                        LINE_BUF[i] = LINE_BUF[i + shift];
                        i += 1;
                    }
                    LINE_LEN -= shift;
                    LINE_BUF[LINE_LEN] = 0;
                    redraw_line(prompt);
                }
            }
            _ => {
                // Printable characters
                if c >= 0x20 {
                    insert_char(c, prompt);
                }
            }
        }
    }
}

// ── History API ─────────────────────────────────────────────────────

/// Add a line to the history buffer
pub unsafe fn add_history(line: *const u8) {
    if line.is_null() {
        return;
    }

    let len = crate::string::strlen(line);
    if len == 0 {
        return;
    }

    // Deduplicate consecutive entries
    if HISTORY_COUNT > 0 {
        let last = &HISTORY[HISTORY_COUNT - 1];
        let last_len = cstrlen(last);
        if last_len == len {
            let s = core::slice::from_raw_parts(line, len);
            if &last[..len] == s {
                return;
            }
        }
    }

    let copy_len = len.min(MAX_LINE - 1);

    if HISTORY_COUNT < MAX_HISTORY {
        HISTORY[HISTORY_COUNT][..copy_len]
            .copy_from_slice(core::slice::from_raw_parts(line, copy_len));
        HISTORY[HISTORY_COUNT][copy_len] = 0;
        HISTORY_COUNT += 1;
    } else {
        // Shift up
        for i in 0..(MAX_HISTORY - 1) {
            HISTORY[i] = HISTORY[i + 1];
        }
        HISTORY[MAX_HISTORY - 1][..copy_len]
            .copy_from_slice(core::slice::from_raw_parts(line, copy_len));
        HISTORY[MAX_HISTORY - 1][copy_len] = 0;
    }

    history_length = HISTORY_COUNT as i32;
    HISTORY_POS = HISTORY_COUNT;
}

/// Initialize history (no-op, called by using_history)
pub unsafe fn using_history() {
    // Already initialized via statics
}

/// Clear all history entries
pub unsafe fn clear_history() {
    for i in 0..HISTORY_COUNT {
        HISTORY[i].fill(0);
    }
    HISTORY_COUNT = 0;
    HISTORY_POS = 0;
    history_length = 0;
}

/// Get a history entry by 1-based offset
pub unsafe fn history_get(offset: i32) -> *mut HistEntry {
    let idx = offset - 1; // Convert to 0-based
    if idx < 0 || idx as usize >= HISTORY_COUNT {
        return core::ptr::null_mut();
    }
    let i = idx as usize;

    // Point the HistEntry's line to the history buffer
    HIST_ENTRIES[i].line = HISTORY[i].as_mut_ptr();
    HIST_ENTRIES[i].timestamp = core::ptr::null_mut();
    HIST_ENTRIES[i].data = core::ptr::null_mut();

    &mut HIST_ENTRIES[i] as *mut HistEntry
}

/// Remove a history entry by 0-based index
pub unsafe fn remove_history(which: i32) -> *mut HistEntry {
    if which < 0 || which as usize >= HISTORY_COUNT {
        return core::ptr::null_mut();
    }
    let idx = which as usize;

    // Shift down
    for i in idx..(HISTORY_COUNT - 1) {
        HISTORY[i] = HISTORY[i + 1];
    }
    HISTORY[HISTORY_COUNT - 1].fill(0);
    HISTORY_COUNT -= 1;
    history_length = HISTORY_COUNT as i32;

    core::ptr::null_mut()
}

/// Replace a history entry
pub unsafe fn replace_history_entry(
    which: i32,
    line: *const u8,
    _data: *mut u8,
) -> *mut HistEntry {
    if which < 0 || which as usize >= HISTORY_COUNT || line.is_null() {
        return core::ptr::null_mut();
    }
    let idx = which as usize;
    let len = crate::string::strlen(line).min(MAX_LINE - 1);
    HISTORY[idx][..len].copy_from_slice(core::slice::from_raw_parts(line, len));
    HISTORY[idx][len] = 0;

    HIST_ENTRIES[idx].line = HISTORY[idx].as_mut_ptr();
    &mut HIST_ENTRIES[idx] as *mut HistEntry
}

/// Get the current history state
pub unsafe fn history_get_history_state() -> *mut HistoryState {
    HIST_STATE.length = HISTORY_COUNT as i32;
    HIST_STATE.size = MAX_HISTORY as i32;
    HIST_STATE.offset = 0;
    &mut HIST_STATE as *mut HistoryState
}

/// Free a history entry (no-op with bump allocator)
pub unsafe fn free_history_entry(_entry: *mut HistEntry) -> *mut u8 {
    core::ptr::null_mut()
}

/// Read history from a file (one entry per line)
pub unsafe fn read_history(filename: *const u8) -> i32 {
    if filename.is_null() {
        return -1;
    }
    let len = crate::string::strlen(filename);
    let path = core::str::from_utf8_unchecked(core::slice::from_raw_parts(filename, len));

    let fd = unistd::open2(path, crate::fcntl::O_RDONLY);
    if fd < 0 {
        return -1;
    }

    let mut buf = [0u8; MAX_LINE];
    let mut line_start = 0;

    loop {
        let n = unistd::read(fd, &mut buf[line_start..]);
        if n <= 0 {
            // Process remaining data
            if line_start > 0 {
                buf[line_start] = 0;
                add_history(buf.as_ptr());
            }
            break;
        }
        let total = line_start + n as usize;

        let mut i = 0;
        let mut last_line = 0;
        while i < total {
            if buf[i] == b'\n' {
                buf[i] = 0;
                if i > last_line {
                    add_history(buf[last_line..].as_ptr());
                }
                last_line = i + 1;
            }
            i += 1;
        }

        // Move leftover to start
        if last_line < total {
            let remain = total - last_line;
            for j in 0..remain {
                buf[j] = buf[last_line + j];
            }
            line_start = remain;
        } else {
            line_start = 0;
        }
    }

    unistd::close(fd);
    0
}

/// Write history to a file
pub unsafe fn write_history(filename: *const u8) -> i32 {
    if filename.is_null() {
        return -1;
    }
    let len = crate::string::strlen(filename);
    let path = core::str::from_utf8_unchecked(core::slice::from_raw_parts(filename, len));

    let fd = unistd::open(
        path,
        crate::fcntl::O_WRONLY | crate::fcntl::O_CREAT | crate::fcntl::O_TRUNC,
        0o644,
    );
    if fd < 0 {
        return -1;
    }

    for i in 0..HISTORY_COUNT {
        let entry_len = cstrlen(&HISTORY[i]);
        unistd::write(fd, &HISTORY[i][..entry_len]);
        unistd::write(fd, b"\n");
    }

    unistd::close(fd);
    0
}

/// Append the last N elements to a file
pub unsafe fn append_history(nelements: i32, filename: *const u8) -> i32 {
    if filename.is_null() {
        return -1;
    }
    let len = crate::string::strlen(filename);
    let path = core::str::from_utf8_unchecked(core::slice::from_raw_parts(filename, len));

    let fd = unistd::open(
        path,
        crate::fcntl::O_WRONLY | crate::fcntl::O_CREAT | crate::fcntl::O_APPEND,
        0o644,
    );
    if fd < 0 {
        return -1;
    }

    let n = (nelements as usize).min(HISTORY_COUNT);
    let start = HISTORY_COUNT - n;
    for i in start..HISTORY_COUNT {
        let entry_len = cstrlen(&HISTORY[i]);
        unistd::write(fd, &HISTORY[i][..entry_len]);
        unistd::write(fd, b"\n");
    }

    unistd::close(fd);
    0
}

/// Truncate history file to N lines
pub unsafe fn history_truncate_file(_filename: *const u8, _nlines: i32) -> i32 {
    // Write the last nlines entries to the file
    0
}

// ── Completion helpers ──────────────────────────────────────────────

/// Build a matches array from a generator function (GNU readline API)
///
/// Calls `func(text, state)` repeatedly (state=0 first, then state=N)
/// until it returns NULL, collecting results. Returns NULL-terminated array
/// where matches[0] is the longest common prefix (LCD) of all matches.
pub unsafe fn completion_matches(text: *const u8, func: CompEntryFunc) -> *mut *mut u8 {
    let generator = match func {
        Some(f) => f,
        None => return core::ptr::null_mut(),
    };

    // Collect matches
    let mut results: [*mut u8; MAX_COMPLETIONS] = [core::ptr::null_mut(); MAX_COMPLETIONS];
    let mut count = 0;

    // Call generator with state=0, then state=1, 2, ...
    let mut state = 0;
    loop {
        let m = generator(text, state);
        if m.is_null() || count >= MAX_COMPLETIONS - 1 {
            break;
        }
        results[count] = m;
        count += 1;
        state += 1;
    }

    if count == 0 {
        return core::ptr::null_mut();
    }

    // Allocate output array: [LCD, match1, match2, ..., NULL]
    let array = crate::c_exports::malloc((count + 2) * core::mem::size_of::<*mut u8>())
        as *mut *mut u8;
    if array.is_null() {
        return core::ptr::null_mut();
    }

    // Find LCD (longest common prefix)
    let text_len = crate::string::strlen(text);
    let first_len = crate::string::strlen(results[0]);

    let mut lcd_len = first_len;
    for i in 1..count {
        let other = results[i];
        let other_len = crate::string::strlen(other);
        let mut j = 0;
        while j < lcd_len && j < other_len && *results[0].add(j) == *other.add(j) {
            j += 1;
        }
        lcd_len = j;
    }

    // Allocate LCD string
    let lcd = crate::c_exports::malloc(lcd_len + 1);
    if !lcd.is_null() {
        core::ptr::copy_nonoverlapping(results[0], lcd, lcd_len);
        *lcd.add(lcd_len) = 0;
    }

    *array.add(0) = lcd;
    for i in 0..count {
        *array.add(i + 1) = results[i];
    }
    *array.add(count + 1) = core::ptr::null_mut();

    array
}

// ── Misc API functions ──────────────────────────────────────────────

/// Initialize the readline library
pub unsafe fn rl_initialize_internal() {
    INITIALIZED = true;
    rl_line_buffer = LINE_BUF.as_mut_ptr();
}

/// Insert text at the current cursor position
pub unsafe fn rl_insert_text(text: *const u8) -> i32 {
    if text.is_null() {
        return 0;
    }
    let len = crate::string::strlen(text);
    let mut inserted = 0;
    for i in 0..len {
        if LINE_LEN >= MAX_LINE - 1 {
            break;
        }
        // Insert at cursor
        if CURSOR < LINE_LEN {
            let mut j = LINE_LEN;
            while j > CURSOR {
                LINE_BUF[j] = LINE_BUF[j - 1];
                j -= 1;
            }
        }
        LINE_BUF[CURSOR] = *text.add(i);
        LINE_LEN += 1;
        CURSOR += 1;
        inserted += 1;
    }
    rl_point = CURSOR as i32;
    rl_end = LINE_LEN as i32;
    inserted
}

/// Redisplay the current line
pub unsafe fn rl_redisplay() {
    // Minimal: just sync and redraw
    rl_point = CURSOR as i32;
    rl_end = LINE_LEN as i32;
}

/// rl_complete stub
pub unsafe fn rl_complete(_count: i32, _key: i32) -> i32 {
    0
}

/// rl_insert stub
pub unsafe fn rl_insert(_count: i32, _key: i32) -> i32 {
    0
}

/// Bind a key to a function
pub unsafe fn rl_bind_key(_key: i32, _func: CommandFunc) -> i32 {
    0
}

/// Bind a key in a specific keymap
pub unsafe fn rl_bind_key_in_map(_key: i32, _func: CommandFunc, _map: *mut u8) -> i32 {
    0
}

/// Parse and bind a readline command
pub unsafe fn rl_parse_and_bind(_line: *mut u8) -> i32 {
    0
}

/// Read an init file (e.g., .inputrc)
pub unsafe fn rl_read_init_file(_filename: *const u8) -> i32 {
    0
}

/// Bind a variable
pub unsafe fn rl_variable_bind(_variable: *const u8, _value: *const u8) -> i32 {
    0
}

/// Prepare terminal for readline
pub unsafe fn rl_prep_terminal(_meta_flag: i32) {
    enter_raw_mode();
}

/// Handle terminal resize
pub unsafe fn rl_resize_terminal() {
    // No-op for now
}

/// Free internal line state
pub unsafe fn rl_free_line_state() {
    LINE_LEN = 0;
    CURSOR = 0;
    LINE_BUF.fill(0);
}

/// Cleanup after signal
pub unsafe fn rl_cleanup_after_signal() {
    leave_raw_mode();
}

// ── Callback mode (for event-loop integration) ─────────────────────

/// Install a callback handler for event-loop integration
pub unsafe fn rl_callback_handler_install(prompt: *const u8, handler: CallbackHandler) {
    CALLBACK_HANDLER = handler;
    if !prompt.is_null() {
        let len = crate::string::strlen(prompt).min(255);
        CALLBACK_PROMPT[..len].copy_from_slice(core::slice::from_raw_parts(prompt, len));
        CALLBACK_PROMPT[len] = 0;
    }
    // Print prompt
    if !prompt.is_null() {
        write_bytes(core::slice::from_raw_parts(prompt, crate::string::strlen(prompt)));
    }
    enter_raw_mode();
    LINE_LEN = 0;
    CURSOR = 0;
    LINE_BUF.fill(0);
}

/// Read a character in callback mode
pub unsafe fn rl_callback_read_char() {
    let c = getchar();
    if c < 0 {
        return;
    }

    let c = c as u8;
    match c {
        b'\n' | b'\r' => {
            putchar(b'\n');
            LINE_BUF[LINE_LEN] = 0;
            if let Some(handler) = CALLBACK_HANDLER {
                let result = crate::c_exports::malloc(LINE_LEN + 1);
                if !result.is_null() {
                    core::ptr::copy_nonoverlapping(LINE_BUF.as_ptr(), result, LINE_LEN);
                    *result.add(LINE_LEN) = 0;
                }
                handler(result);
            }
            LINE_LEN = 0;
            CURSOR = 0;
            LINE_BUF.fill(0);
        }
        BACKSPACE | CTRL_H => {
            if LINE_LEN > 0 {
                LINE_LEN -= 1;
                CURSOR = LINE_LEN;
                LINE_BUF[LINE_LEN] = 0;
                write_str("\x08 \x08");
            }
        }
        _ => {
            if c >= 0x20 && LINE_LEN < MAX_LINE - 1 {
                LINE_BUF[LINE_LEN] = c;
                LINE_LEN += 1;
                CURSOR = LINE_LEN;
                putchar(c);
            }
        }
    }
}

/// Remove the callback handler
pub unsafe fn rl_callback_handler_remove() {
    CALLBACK_HANDLER = None;
    leave_raw_mode();
}
