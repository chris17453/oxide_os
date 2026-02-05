//! # Screen Management
//!
//! Manages the physical terminal screen and virtual screen buffers.
//! Handles initialization, refresh, and terminal state.
//!
//! -- NeonRoot: Screen manager - coordinates all visual updates
//! -- GlassSignal: Refresh pipeline - diff cells, emit minimal SGR + CUP

use crate::color;
use crate::window::{WindowData, delwin, newwin};
use crate::{Error, Result, WINDOW, attrs, chtype};
use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use termcap;

/// Output buffer for batching terminal writes
/// -- GlassSignal: Accumulate escape sequences before flushing to stdout
struct OutputBuffer {
    buf: Vec<u8>,
    cur_attr: u32,
    cur_row: i32,
    cur_col: i32,
}

impl OutputBuffer {
    fn new() -> Self {
        Self {
            buf: Vec::with_capacity(4096),
            cur_attr: attrs::A_NORMAL,
            cur_row: -1,
            cur_col: -1,
        }
    }

    /// Emit CUP (cursor position) sequence: ESC [ row ; col H
    /// -- GlassSignal: 1-based terminal coords from 0-based internal
    fn push_cup(&mut self, row: i32, col: i32) {
        if self.cur_row == row && self.cur_col == col {
            return;
        }
        self.buf.extend_from_slice(b"\x1b[");
        self.push_decimal((row + 1) as u32);
        self.buf.push(b';');
        self.push_decimal((col + 1) as u32);
        self.buf.push(b'H');
        self.cur_row = row;
        self.cur_col = col;
    }

    /// Emit SGR (set graphic rendition) for attribute changes
    /// -- GlassSignal: Only emit when attrs actually change - bandwidth matters
    fn push_sgr(&mut self, attr: u32) {
        if self.cur_attr == attr {
            return;
        }

        // Reset first, then apply new attributes
        self.buf.extend_from_slice(b"\x1b[0");

        if attr & attrs::A_BOLD != 0 {
            self.buf.extend_from_slice(b";1");
        }
        if attr & attrs::A_DIM != 0 {
            self.buf.extend_from_slice(b";2");
        }
        if attr & attrs::A_UNDERLINE != 0 {
            self.buf.extend_from_slice(b";4");
        }
        if attr & attrs::A_BLINK != 0 {
            self.buf.extend_from_slice(b";5");
        }
        if attr & attrs::A_REVERSE != 0 {
            self.buf.extend_from_slice(b";7");
        }
        if attr & attrs::A_INVIS != 0 {
            self.buf.extend_from_slice(b";8");
        }

        // Extract color pair and emit fg/bg
        let pair_idx = ((attr & attrs::A_COLOR) >> 17) as i16;
        if pair_idx > 0 {
            if let Ok((fg, bg)) = color::pair_content(pair_idx) {
                // Foreground: SGR 30-37 for basic, 38;5;N for 256-color
                if fg >= 0 && fg < 8 {
                    self.buf.extend_from_slice(b";3");
                    self.buf.push(b'0' + fg as u8);
                } else if fg >= 0 {
                    self.buf.extend_from_slice(b";38;5;");
                    self.push_decimal(fg as u32);
                }
                // Background: SGR 40-47 for basic, 48;5;N for 256-color
                if bg >= 0 && bg < 8 {
                    self.buf.extend_from_slice(b";4");
                    self.buf.push(b'0' + bg as u8);
                } else if bg >= 0 {
                    self.buf.extend_from_slice(b";48;5;");
                    self.push_decimal(bg as u32);
                }
            }
        }

        self.buf.push(b'm');
        self.cur_attr = attr;
    }

    /// Append a character and advance tracked column
    fn push_char(&mut self, ch: char) {
        let mut utf8_buf = [0u8; 4];
        let encoded = ch.encode_utf8(&mut utf8_buf);
        self.buf.extend_from_slice(encoded.as_bytes());
        self.cur_col += 1;
    }

    /// Write decimal number to buffer
    fn push_decimal(&mut self, value: u32) {
        if value == 0 {
            self.buf.push(b'0');
            return;
        }
        let mut digits = [0u8; 10];
        let mut i = 0;
        let mut v = value;
        while v > 0 {
            digits[i] = b'0' + (v % 10) as u8;
            v /= 10;
            i += 1;
        }
        for d in digits[..i].iter().rev() {
            self.buf.push(*d);
        }
    }

    /// Flush buffer to stdout via libc::write
    /// -- GlassSignal: Single write syscall for the entire frame
    fn flush(&mut self) {
        if !self.buf.is_empty() {
            libc::unistd::write(1, &self.buf);
            self.buf.clear();
            // — GlassSignal: Buffered write ain't worth jack til you flush it. Without this,
            // ncurses output sits in libc's stdout buffer til newline/256 bytes — demo ran at
            // <20 FPS with frames queued up like stuck packets. fflush_stdout kicks it to the
            // kernel immediately. Now we hit ~60 FPS like it's 1999.
            libc::fflush_stdout();
        }
    }
}

/// Screen data structure
#[repr(C)]
pub struct ScreenData {
    /// Standard screen (full terminal)
    pub stdscr: WINDOW,
    /// Current screen (alternate screen support)
    pub curscr: WINDOW,
    /// New screen buffer (for optimization)
    pub newscr: WINDOW,
    /// Terminal type
    pub term_type: String,
    /// Terminal entry
    pub term: Option<termcap::TerminalEntry>,
    /// Screen lines
    pub lines: i32,
    /// Screen columns
    pub cols: i32,
    /// Colors available
    pub colors: i32,
    /// Color pairs available
    pub color_pairs: i32,
    /// Cursor visibility (0=invisible, 1=normal, 2=very visible)
    pub cursor_vis: i32,
    /// Echo input
    pub echo: bool,
    /// Cbreak mode
    pub cbreak: bool,
    /// Raw mode
    pub raw: bool,
    /// NL mode
    pub nl: bool,
    /// — NeonVale: Saved termios from before initscr(). endwin() restores this
    /// so the terminal isn't left in raw/cbreak mode when the app exits.
    pub saved_termios: Option<libc::termios::Termios>,
}

impl ScreenData {
    /// Create a new screen
    /// — NeonVale: Probes real terminal size via TIOCGWINSZ, falls back to
    /// termcap values, saves the pre-init termios for endwin() to restore.
    pub fn new(term_type: &str) -> Result<Box<Self>> {
        // Load terminal definition
        let term = termcap::load_terminal(term_type).ok();

        // — NeonVale: Query the actual terminal dimensions first. Termcap
        // entries are compile-time defaults (often 24×80); the running
        // terminal may be any size. Only fall back if the ioctl fails.
        let mut ws = libc::termios::Winsize::default();
        let (lines, cols) = if libc::termios::tcgetwinsize(0, &mut ws) == 0
            && ws.ws_row > 0
            && ws.ws_col > 0
        {
            (ws.ws_row as i32, ws.ws_col as i32)
        } else {
            let l = term.as_ref().and_then(|t| t.get_number("lines")).unwrap_or(24);
            let c = term.as_ref().and_then(|t| t.get_number("cols")).unwrap_or(80);
            (l, c)
        };

        let colors = term
            .as_ref()
            .and_then(|t| t.get_number("colors"))
            .unwrap_or(8);
        let color_pairs = term
            .as_ref()
            .and_then(|t| t.get_number("pairs"))
            .unwrap_or(64);

        // — NeonVale: Save the original termios BEFORE we touch anything.
        // endwin() will restore this so the calling shell isn't stuck in
        // raw/cbreak mode after the ncurses app exits.
        let mut orig = libc::termios::Termios::default();
        let saved = if libc::termios::tcgetattr(0, &mut orig) == 0 {
            Some(orig)
        } else {
            None
        };

        // Create windows
        let stdscr = newwin(lines, cols, 0, 0);
        let curscr = newwin(lines, cols, 0, 0);
        let newscr = newwin(lines, cols, 0, 0);

        if stdscr.is_null() || curscr.is_null() || newscr.is_null() {
            if !stdscr.is_null() {
                let _ = delwin(stdscr);
            }
            if !curscr.is_null() {
                let _ = delwin(curscr);
            }
            if !newscr.is_null() {
                let _ = delwin(newscr);
            }
            return Err(Error::Err);
        }

        Ok(Box::new(Self {
            stdscr,
            curscr,
            newscr,
            term_type: term_type.into(),
            term,
            lines,
            cols,
            colors,
            color_pairs,
            cursor_vis: 1,
            echo: true,
            cbreak: false,
            raw: false,
            nl: true,
            saved_termios: saved,
        }))
    }

    /// Send terminal capability string to stdout
    /// -- NeonRoot: Capability output - translate termcap names to escape bytes
    pub fn putp(&self, cap_name: &str) -> Result<()> {
        if let Some(ref term) = self.term {
            if let Some(cap_str) = term.get_string(cap_name) {
                let bytes = cap_str.as_bytes();
                libc::unistd::write(1, bytes);
                return Ok(());
            }
        }
        Err(Error::Err)
    }

    /// Send parameterized capability string to stdout
    /// -- NeonRoot: Parameterized capability - tparm expansion then write
    pub fn putp_params(&self, cap_name: &str, params: &[i32]) -> Result<()> {
        if let Some(ref term) = self.term {
            if let Some(cap_str) = term.get_string(cap_name) {
                if let Ok(expanded) = termcap::expand::tparm(cap_str, params) {
                    let bytes = expanded.as_bytes();
                    libc::unistd::write(1, bytes);
                    return Ok(());
                }
            }
        }
        Err(Error::Err)
    }

    /// Initialize colors
    pub fn init_colors(&mut self) -> Result<()> {
        self.putp("setaf")?;
        self.putp("setab")?;
        Ok(())
    }

    /// Set cursor visibility
    pub fn curs_set(&mut self, visibility: i32) -> Result<i32> {
        let old = self.cursor_vis;
        self.cursor_vis = visibility;

        let cap = match visibility {
            0 => "civis", // invisible
            1 => "cnorm", // normal
            2 => "cvvis", // very visible
            _ => return Err(Error::Err),
        };

        self.putp(cap)?;
        Ok(old)
    }
}

impl Drop for ScreenData {
    fn drop(&mut self) {
        if !self.stdscr.is_null() {
            let _ = delwin(self.stdscr);
        }
        if !self.curscr.is_null() {
            let _ = delwin(self.curscr);
        }
        if !self.newscr.is_null() {
            let _ = delwin(self.newscr);
        }
    }
}

/// Global screen state
static mut CURRENT_SCREEN: Option<Box<ScreenData>> = None;

/// Initialize the screen
/// — NeonVale: Enters alternate screen buffer (smcup), clears screen.
/// The saved termios captured in ScreenData::new() lets endwin() undo
/// everything — even if the app crashes through the normal exit path.
pub fn initscr() -> WINDOW {
    // Get terminal type from environment or use default
    let term_type = "xterm"; // Would read from $TERM

    match ScreenData::new(term_type) {
        Ok(screen) => {
            // — NeonRoot: Tell color module whether this terminal has colors.
            // Standard ncurses pattern: initscr() → has_colors() → start_color().
            // Without this, has_colors() always returns false.
            color::set_has_colors(screen.colors > 0);

            // — NeonVale: Switch to alternate screen buffer and clear it.
            // Without smcup, the app stomps the shell's scrollback.
            let _ = screen.putp("smcup");
            let _ = screen.putp("clear");

            let stdscr = screen.stdscr;
            unsafe {
                let ptr = core::ptr::addr_of_mut!(CURRENT_SCREEN);
                *ptr = Some(screen);
            }
            stdscr
        }
        Err(_) => core::ptr::null_mut(),
    }
}

/// End ncurses mode
/// — NeonVale: Restores the terminal to its pre-initscr() state. Sends
/// rmcup (exit alternate screen), cnorm (show cursor), sgr0 (reset attrs),
/// AND restores the original termios via tcsetattr. Without the termios
/// restore, the shell inherits raw/cbreak mode and hangs on input.
pub fn endwin() -> Result<()> {
    unsafe {
        let ptr = core::ptr::addr_of_mut!(CURRENT_SCREEN);
        if let Some(ref screen) = *ptr {
            // Send terminal reset sequences
            let _ = screen.putp("rmcup");
            let _ = screen.putp("cnorm");
            // Reset attributes
            let _ = screen.putp("sgr0");

            // — NeonVale: Restore the termios we saved in initscr().
            // This is the critical piece — without it cbreak/noecho/raw
            // modes leak to the parent shell and it hangs on read().
            if let Some(ref saved) = screen.saved_termios {
                libc::termios::tcsetattr(0, libc::termios::action::TCSAFLUSH, saved);
            }
        }
        *ptr = None;
    }
    Ok(())
}

/// Create a new terminal screen
pub fn newterm(term_type: &str, _outfd: i32, _infd: i32) -> *mut ScreenData {
    match ScreenData::new(term_type) {
        Ok(screen) => {
            let raw_ptr = Box::into_raw(screen);
            unsafe {
                let ptr = core::ptr::addr_of_mut!(CURRENT_SCREEN);
                *ptr = Some(Box::from_raw(raw_ptr));
            }
            raw_ptr
        }
        Err(_) => core::ptr::null_mut(),
    }
}

/// Delete a terminal screen
pub fn delscreen(screen: *mut ScreenData) -> Result<()> {
    if screen.is_null() {
        return Err(Error::Err);
    }

    unsafe {
        let _ = Box::from_raw(screen);
        let ptr = core::ptr::addr_of_mut!(CURRENT_SCREEN);
        *ptr = None;
    }

    Ok(())
}

/// Get current screen
pub fn current_screen() -> Option<&'static ScreenData> {
    unsafe {
        let ptr = core::ptr::addr_of!(CURRENT_SCREEN);
        (*ptr).as_ref().map(|b| &**b)
    }
}

/// Get mutable current screen
pub fn current_screen_mut() -> Option<&'static mut ScreenData> {
    unsafe {
        let ptr = core::ptr::addr_of_mut!(CURRENT_SCREEN);
        (*ptr).as_mut().map(|b| &mut **b)
    }
}

/// Get standard screen window
pub fn stdscr() -> WINDOW {
    unsafe {
        let ptr = core::ptr::addr_of!(CURRENT_SCREEN);
        (*ptr)
            .as_ref()
            .map(|s| s.stdscr)
            .unwrap_or(core::ptr::null_mut())
    }
}

/// Refresh standard screen
pub fn refresh() -> Result<()> {
    wrefresh(stdscr())
}

/// Refresh a window - diff against curscr and emit minimal terminal output
/// -- GlassSignal: The core refresh engine - compare cells, emit only changes
pub fn wrefresh(win: WINDOW) -> Result<()> {
    if win.is_null() {
        return Err(Error::Err);
    }

    let screen = unsafe { (*core::ptr::addr_of!(CURRENT_SCREEN)).as_ref() }.ok_or(Error::Err)?;
    let curscr = screen.curscr;
    if curscr.is_null() {
        return Err(Error::Err);
    }

    unsafe {
        let win_ref = &*win;
        let cur_ref = &mut *curscr;

        if !win_ref.touched {
            return Ok(());
        }

        let mut out = OutputBuffer::new();

        if win_ref.clear_flag {
            // Full clear: emit clear capability and redraw everything
            out.buf.extend_from_slice(b"\x1b[H\x1b[2J");
            out.cur_row = 0;
            out.cur_col = 0;

            // Redraw all cells
            for row in 0..win_ref.lines {
                for col in 0..win_ref.cols {
                    let idx = (row * win_ref.cols + col) as usize;
                    if let Some(cell) = win_ref.cells.get(idx) {
                        let scr_row = row + win_ref.beg_y;
                        let scr_col = col + win_ref.beg_x;

                        out.push_cup(scr_row, scr_col);
                        out.push_sgr(cell.attr);
                        // — GlassSignal: Full Unicode codepoint — the old & 0xFF mask
                        // was a leftover from packed-chtype ncurses and nuked every
                        // non-ASCII glyph (box-drawing, diamonds, blocks) into oblivion
                        let ch = char::from_u32(cell.ch).unwrap_or(' ');
                        out.push_char(ch);

                        // Mirror to curscr
                        let cur_idx = (scr_row * cur_ref.cols + scr_col) as usize;
                        if let Some(cur_cell) = cur_ref.cells.get_mut(cur_idx) {
                            *cur_cell = *cell;
                        }
                    }
                }
            }
        } else {
            // Incremental: diff win against curscr, emit only changed cells
            for row in 0..win_ref.lines {
                for col in 0..win_ref.cols {
                    let idx = (row * win_ref.cols + col) as usize;
                    let scr_row = row + win_ref.beg_y;
                    let scr_col = col + win_ref.beg_x;
                    let cur_idx = (scr_row * cur_ref.cols + scr_col) as usize;

                    if let (Some(win_cell), Some(cur_cell)) =
                        (win_ref.cells.get(idx), cur_ref.cells.get(cur_idx))
                    {
                        // Only emit if cell differs
                        if win_cell.ch != cur_cell.ch || win_cell.attr != cur_cell.attr {
                            out.push_cup(scr_row, scr_col);
                            out.push_sgr(win_cell.attr);
                            // — GlassSignal: No masking — chtype.ch holds the full codepoint
                            let ch = char::from_u32(win_cell.ch).unwrap_or(' ');
                            out.push_char(ch);
                        }
                    }
                }
            }

            // Copy changed cells to curscr
            for row in 0..win_ref.lines {
                for col in 0..win_ref.cols {
                    let idx = (row * win_ref.cols + col) as usize;
                    let scr_row = row + win_ref.beg_y;
                    let scr_col = col + win_ref.beg_x;
                    let cur_idx = (scr_row * cur_ref.cols + scr_col) as usize;

                    if let (Some(win_cell), Some(cur_cell)) =
                        (win_ref.cells.get(idx), cur_ref.cells.get_mut(cur_idx))
                    {
                        *cur_cell = *win_cell;
                    }
                }
            }
        }

        // Position cursor at window's logical cursor position
        if !win_ref.leaveok {
            let final_row = win_ref.cur_y + win_ref.beg_y;
            let final_col = win_ref.cur_x + win_ref.beg_x;
            out.push_cup(final_row, final_col);
        }

        // Reset attributes at end
        out.push_sgr(attrs::A_NORMAL);
        out.flush();

        // Mark window as clean
        (*win).touched = false;
        (*win).clear_flag = false;
    }

    Ok(())
}

/// Update virtual screen without refreshing physical screen
/// -- GlassSignal: Stage window cells into newscr for batched doupdate
pub fn wnoutrefresh(win: WINDOW) -> Result<()> {
    if win.is_null() {
        return Err(Error::Err);
    }

    let screen = unsafe { (*core::ptr::addr_of!(CURRENT_SCREEN)).as_ref() }.ok_or(Error::Err)?;
    let newscr = screen.newscr;
    if newscr.is_null() {
        return Err(Error::Err);
    }

    unsafe {
        let win_ref = &*win;
        let new_ref = &mut *newscr;

        // Copy win cells into newscr at (beg_y, beg_x) offset
        for row in 0..win_ref.lines {
            for col in 0..win_ref.cols {
                let src_idx = (row * win_ref.cols + col) as usize;
                let dst_row = row + win_ref.beg_y;
                let dst_col = col + win_ref.beg_x;
                let dst_idx = (dst_row * new_ref.cols + dst_col) as usize;

                if let (Some(src_cell), Some(dst_cell)) =
                    (win_ref.cells.get(src_idx), new_ref.cells.get_mut(dst_idx))
                {
                    *dst_cell = *src_cell;
                }
            }
        }

        new_ref.touched = true;
        (*win).touched = false;
    }

    Ok(())
}

/// Refresh physical screen from newscr
/// -- GlassSignal: Diff newscr vs curscr, emit changes, sync buffers
pub fn doupdate() -> Result<()> {
    let screen = unsafe { (*core::ptr::addr_of!(CURRENT_SCREEN)).as_ref() }.ok_or(Error::Err)?;
    let newscr = screen.newscr;
    let curscr = screen.curscr;
    if newscr.is_null() || curscr.is_null() {
        return Err(Error::Err);
    }

    unsafe {
        let new_ref = &*newscr;
        let cur_ref = &mut *curscr;

        if !new_ref.touched {
            return Ok(());
        }

        let mut out = OutputBuffer::new();
        let total = (new_ref.lines * new_ref.cols) as usize;

        for i in 0..total {
            if let (Some(new_cell), Some(cur_cell)) = (new_ref.cells.get(i), cur_ref.cells.get(i)) {
                if new_cell.ch != cur_cell.ch || new_cell.attr != cur_cell.attr {
                    let row = i as i32 / new_ref.cols;
                    let col = i as i32 % new_ref.cols;
                    out.push_cup(row, col);
                    out.push_sgr(new_cell.attr);
                    // — GlassSignal: Full codepoint — doupdate path same fix
                    let ch = char::from_u32(new_cell.ch).unwrap_or(' ');
                    out.push_char(ch);
                }
            }
        }

        // Copy newscr to curscr
        for i in 0..total {
            if let (Some(new_cell), Some(cur_cell)) =
                (new_ref.cells.get(i), cur_ref.cells.get_mut(i))
            {
                *cur_cell = *new_cell;
            }
        }

        out.push_sgr(attrs::A_NORMAL);
        out.flush();

        (*newscr).touched = false;
    }

    Ok(())
}

/// — NeonVale: Apply the current cbreak/raw/echo/nl flags to the real
/// terminal via tcsetattr. Called after any mode-change function so the
/// kernel TTY actually reflects what the ncurses app requested.
fn apply_termios() {
    use libc::termios::*;
    let screen = match current_screen_mut() {
        Some(s) => s,
        None => return,
    };

    // Start from the saved termios (pre-initscr baseline)
    let mut t = match screen.saved_termios.clone() {
        Some(saved) => saved,
        None => {
            let mut cur = Termios::default();
            if tcgetattr(0, &mut cur) != 0 {
                return;
            }
            cur
        }
    };

    if screen.cbreak {
        // — NeonVale: cbreak = ICANON off, ISIG on, VMIN=1, VTIME=0
        t.c_lflag &= !lflag::ICANON;
        t.c_cc[cc::VMIN] = 1;
        t.c_cc[cc::VTIME] = 0;
    } else if screen.raw {
        // — NeonVale: raw = ICANON off, ISIG off, VMIN=1, VTIME=0
        t.c_lflag &= !(lflag::ICANON | lflag::ISIG | lflag::IEXTEN);
        t.c_iflag &= !(iflag::ICRNL | iflag::IXON);
        t.c_cc[cc::VMIN] = 1;
        t.c_cc[cc::VTIME] = 0;
    }

    if !screen.echo {
        t.c_lflag &= !(lflag::ECHO | lflag::ECHOE | lflag::ECHOK | lflag::ECHOCTL);
    }

    tcsetattr(0, action::TCSANOW, &t);
}

/// Set echo mode
pub fn echo() -> Result<()> {
    match current_screen_mut() {
        Some(screen) => screen.echo = true,
        None => return Err(Error::Err),
    }
    apply_termios();
    Ok(())
}

/// Disable echo mode
pub fn noecho() -> Result<()> {
    match current_screen_mut() {
        Some(screen) => screen.echo = false,
        None => return Err(Error::Err),
    }
    apply_termios();
    Ok(())
}

/// Set cbreak mode (characters available immediately)
pub fn cbreak() -> Result<()> {
    match current_screen_mut() {
        Some(screen) => {
            screen.cbreak = true;
            screen.raw = false;
        }
        None => return Err(Error::Err),
    }
    apply_termios();
    Ok(())
}

/// Disable cbreak mode
pub fn nocbreak() -> Result<()> {
    match current_screen_mut() {
        Some(screen) => screen.cbreak = false,
        None => return Err(Error::Err),
    }
    apply_termios();
    Ok(())
}

/// Set raw mode (no signal processing)
pub fn raw() -> Result<()> {
    match current_screen_mut() {
        Some(screen) => {
            screen.raw = true;
            screen.cbreak = false;
        }
        None => return Err(Error::Err),
    }
    apply_termios();
    Ok(())
}

/// Disable raw mode
pub fn noraw() -> Result<()> {
    match current_screen_mut() {
        Some(screen) => screen.raw = false,
        None => return Err(Error::Err),
    }
    apply_termios();
    Ok(())
}

/// Enable newline translation
pub fn nl() -> Result<()> {
    if let Some(screen) = current_screen_mut() {
        screen.nl = true;
        Ok(())
    } else {
        Err(Error::Err)
    }
}

/// Disable newline translation
pub fn nonl() -> Result<()> {
    if let Some(screen) = current_screen_mut() {
        screen.nl = false;
        Ok(())
    } else {
        Err(Error::Err)
    }
}

/// Set cursor visibility
pub fn curs_set(visibility: i32) -> Result<i32> {
    if let Some(screen) = current_screen_mut() {
        screen.curs_set(visibility)
    } else {
        Err(Error::Err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_screen_creation() {
        let screen = ScreenData::new("xterm").unwrap();
        assert!(!screen.stdscr.is_null());
        assert_eq!(screen.lines, 24);
        assert_eq!(screen.cols, 80);
    }
}
