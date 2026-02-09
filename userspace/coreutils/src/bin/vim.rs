//! vim - vi improved text editor
//!
//! A minimal modal text editor with support for:
//! - Normal mode: hjkl navigation, basic commands (x, dd, yy, p)
//! - Insert mode: i, a, o, O to enter; ESC to exit
//! - Command mode: :w (write), :q (quit), :wq, :q!
//! - Search: / and ? for forward/backward search, n/N for repeat
//! - Visual feedback: status line, mode indicator
//!
//! ── BlackLatch: Paranoid memory bounds checking throughout ──
//! We're building an editor in a bare-metal OS. One off-by-one
//! and we're toast. Every buffer access is guarded.
//!
//! ── WireSaint: Heap-allocated buffers, termcap-driven rendering ──
//! No more 20MB stack arrays. Vec<Vec<u8>> keeps it sane.
//! Terminal control goes through termcap with VT100 fallback.

#![no_std]
#![no_main]

extern crate alloc;

use alloc::vec;
use alloc::vec::Vec;
use libc::termios::{
    Termios, Winsize,
    action::TCSAFLUSH,
    cc::{VMIN, VTIME},
    iflag::{ICRNL, IXON},
    lflag::{ECHO, ICANON, ISIG},
    tcgetattr, tcgetwinsize, tcsetattr,
};
use libc::*;
use termcap::capabilities::strings as tcap;
use termcap::expand::tparm;

const MAX_FILENAME: usize = 256;
const MAX_COMMAND: usize = 256;

/// ── WireSaint: Write a byte slice to stdout in one syscall ──
/// Much faster than putchar() in a loop - avoids N syscalls for N chars
#[inline]
fn write_bytes(bytes: &[u8]) {
    if !bytes.is_empty() {
        unistd::write(1, bytes);
    }
}

/// ── GraveShift: Editor modes for modal editing ──
#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Normal,
    Insert,
    Command,
    Search,
}

/// ── NeonVale: Terminal abstraction over termcap ──
/// Looks up capability strings from the termcap database and outputs
/// them. Falls back to hardcoded VT100 escapes if lookup fails.
struct Term {
    entry: termcap::TerminalEntry,
    rows: usize,
    cols: usize,
}

impl Term {
    /// ── NeonVale: Initialize terminal from termcap database ──
    /// — GraveShift: Gets ACTUAL terminal size via tcgetwinsize, not some bullshit hardcoded value.
    fn new_with_fd(tty_fd: i32) -> Self {
        let entry = termcap::load_terminal("xterm").unwrap_or_else(|_| {
            termcap::load_terminal("vt100").unwrap_or_else(|_| termcap::TerminalEntry::new("dumb"))
        });

        // — NeonVale: Get real terminal size via tcgetwinsize (wraps TIOCGWINSZ)
        let mut ws = Winsize::default();
        let ret = tcgetwinsize(tty_fd, &mut ws);

        let (rows, cols) = if ret == 0 && ws.ws_row > 0 && ws.ws_col > 0 {
            (ws.ws_row as usize, ws.ws_col as usize)
        } else {
            // — GraveShift: Fallback only if ioctl fails
            let r = entry
                .get_number(termcap::capabilities::numbers::LINES)
                .unwrap_or(24) as usize;
            let c = entry
                .get_number(termcap::capabilities::numbers::COLUMNS)
                .unwrap_or(80) as usize;
            (r, c)
        };

        Term { entry, rows, cols }
    }

    /// ── NeonVale: Legacy constructor for when we don't have a fd yet ──
    fn new() -> Self {
        Self::new_with_fd(0) // — Try stdin as fallback
    }

    /// ── NeonVale: Clear entire screen and home cursor ──
    fn clear_screen(&self) {
        if let Some(cap) = self.entry.get_string(tcap::CLEAR) {
            prints(cap);
        } else {
            // ── NeonVale: VT100 fallback -- never go dark ──
            prints("\x1b[2J\x1b[H");
        }
    }

    /// ── NeonVale: Position cursor at (row, col), 0-indexed ──
    fn move_cursor(&self, row: usize, col: usize) {
        if let Some(cap) = self.entry.get_string(tcap::CURSOR_ADDRESS) {
            // ── NeonVale: tparm uses 0-based params; the %i in the template handles 1-based ──
            if let Ok(seq) = tparm(cap, &[row as i32, col as i32]) {
                prints(&seq);
                return;
            }
        }
        // ── NeonVale: VT100 fallback -- manual escape construction ──
        prints("\x1b[");
        print_num(row + 1);
        prints(";");
        print_num(col + 1);
        prints("H");
    }

    /// ── NeonVale: Enter reverse video mode (for status line) ──
    fn enter_reverse(&self) {
        if let Some(cap) = self.entry.get_string(tcap::ENTER_REVERSE) {
            prints(cap);
        } else {
            prints("\x1b[7m");
        }
    }

    /// ── NeonVale: Reset all terminal attributes ──
    fn exit_attributes(&self) {
        if let Some(cap) = self.entry.get_string(tcap::EXIT_ATTRIBUTES) {
            prints(cap);
        } else {
            prints("\x1b[0m");
        }
    }

    /// ── NeonVale: Clear from cursor to end of line ──
    fn clrtoeol(&self) {
        if let Some(cap) = self.entry.get_string(tcap::CLRTOEOL) {
            prints(cap);
        } else {
            prints("\x1b[K");
        }
    }
}

/// ── WireSaint: Heap-allocated line buffer ──
/// Each line is a Vec<u8>, no fixed upper bound per line.
/// Total line count bounded only by available heap.
struct Buffer {
    lines: Vec<Vec<u8>>,
    modified: bool,
}

impl Buffer {
    fn new() -> Self {
        Buffer {
            lines: vec![Vec::new()],
            modified: false,
        }
    }

    /// ── BlackLatch: Safe line insertion with bounds checking ──
    fn insert_line(&mut self, at: usize) -> bool {
        if at > self.lines.len() {
            return false;
        }
        self.lines.insert(at, Vec::new());
        self.modified = true;
        true
    }

    /// ── BlackLatch: Safe line deletion with bounds checking ──
    fn delete_line(&mut self, at: usize) -> bool {
        if at >= self.lines.len() {
            return false;
        }

        if self.lines.len() > 1 {
            self.lines.remove(at);
        } else {
            // ── BlackLatch: Keep at least one empty line ──
            self.lines[0].clear();
        }
        self.modified = true;
        true
    }

    #[inline]
    fn line_count(&self) -> usize {
        self.lines.len()
    }

    #[inline]
    fn line_len(&self, idx: usize) -> usize {
        self.lines[idx].len()
    }

    /// ── WireSaint: Load file contents into buffer ──
    fn load_file(&mut self, filename: &str) -> bool {
        let fd = open2(filename, O_RDONLY);
        if fd < 0 {
            return false;
        }

        self.lines.clear();
        let mut buf = [0u8; 4096];
        let mut current_line: Vec<u8> = Vec::new();

        loop {
            let n = read(fd, &mut buf);
            if n <= 0 {
                break;
            }

            for i in 0..n as usize {
                if buf[i] == b'\n' {
                    self.lines.push(current_line);
                    current_line = Vec::new();
                } else {
                    current_line.push(buf[i]);
                }
            }
        }

        // ── WireSaint: Handle last line without trailing newline ──
        if !current_line.is_empty() || self.lines.is_empty() {
            self.lines.push(current_line);
        }

        close(fd);
        self.modified = false;

        if self.lines.is_empty() {
            self.lines.push(Vec::new());
        }

        true
    }

    /// ── WireSaint: Save buffer contents to file ──
    fn save_file(&mut self, filename: &str) -> bool {
        let fd = open(filename, O_WRONLY | O_CREAT | O_TRUNC, 0o644);
        if fd < 0 {
            return false;
        }

        for line in &self.lines {
            if !line.is_empty() {
                write(fd, line);
            }
            write(fd, b"\n");
        }

        close(fd);
        self.modified = false;
        true
    }
}

/// ── NeonRoot: Cursor and view state management ──
struct Editor {
    buffer: Buffer,
    term: Term,
    tty_fd: i32, // — GraveShift: Terminal fd for raw input
    cursor_line: usize,
    cursor_col: usize,
    top_line: usize,
    mode: Mode,
    filename: [u8; MAX_FILENAME],
    filename_len: usize,
    quit_requested: bool,
    command_buf: [u8; MAX_COMMAND],
    command_len: usize,
    message: [u8; 256],
    message_len: usize,
    yank_buf: Vec<u8>,
    search_pattern: [u8; MAX_COMMAND],
    search_len: usize,
    search_forward: bool,
}

impl Editor {
    fn new() -> Self {
        Editor {
            buffer: Buffer::new(),
            term: Term::new(),
            tty_fd: -1, // — GraveShift: Set by main() after opening terminal
            cursor_line: 0,
            cursor_col: 0,
            top_line: 0,
            mode: Mode::Normal,
            filename: [0; MAX_FILENAME],
            filename_len: 0,
            quit_requested: false,
            command_buf: [0; MAX_COMMAND],
            command_len: 0,
            message: [0; 256],
            message_len: 0,
            yank_buf: Vec::new(),
            search_pattern: [0; MAX_COMMAND],
            search_len: 0,
            search_forward: true,
        }
    }

    /// ── BlackLatch: Ensure cursor stays within valid buffer bounds ──
    fn normalize_cursor(&mut self) {
        if self.cursor_line >= self.buffer.line_count() {
            self.cursor_line = if self.buffer.line_count() > 0 {
                self.buffer.line_count() - 1
            } else {
                0
            };
        }

        let line_len = self.buffer.line_len(self.cursor_line);
        if self.mode == Mode::Normal {
            // ── BlackLatch: Normal mode cursor can't go past last character ──
            if line_len > 0 && self.cursor_col >= line_len {
                self.cursor_col = line_len - 1;
            } else if line_len == 0 {
                self.cursor_col = 0;
            }
        } else {
            // ── BlackLatch: Insert mode cursor can be at end of line ──
            if self.cursor_col > line_len {
                self.cursor_col = line_len;
            }
        }
    }

    /// ── NeonVale: Render the editor screen via termcap ──
    /// — GraveShift: NO FULL SCREEN CLEAR. We overwrite lines in place and use
    /// clrtoeol to handle shorter lines. Full clear causes horrific flicker and
    /// tanks performance to sub-1-FPS levels.
    fn render(&self) {
        // — TorqueJax: Hide cursor during redraw to prevent cursor flicker
        prints("\x1b[?25l");

        let visible_lines = if self.term.rows > 2 {
            self.term.rows - 2
        } else {
            1
        };

        // ── NeonVale: Render visible lines by positioning and overwriting ──
        for i in 0..visible_lines {
            // — GraveShift: Jump to line start, don't rely on newlines
            self.term.move_cursor(i, 0);

            let line_idx = self.top_line + i;
            if line_idx < self.buffer.line_count() {
                let line = &self.buffer.lines[line_idx];
                if !line.is_empty() {
                    // — WireSaint: Batch output - write entire line slice at once
                    write_bytes(line);
                }
            } else {
                prints("~");
            }
            // — NeonVale: Clear to end of line instead of padding with spaces
            prints("\x1b[K");
        }

        // ── NeonVale: Status line at row (rows - 2) ──
        self.term.move_cursor(self.term.rows - 2, 0);
        self.term.enter_reverse();
        match self.mode {
            Mode::Normal => prints(" NORMAL "),
            Mode::Insert => prints(" INSERT "),
            Mode::Command => prints(" COMMAND "),
            Mode::Search => prints(" SEARCH "),
        }

        prints(" ");
        if self.filename_len > 0 {
            write_bytes(&self.filename[..self.filename_len]);
        } else {
            prints("[No Name]");
        }

        if self.buffer.modified {
            prints(" [+]");
        }

        // ── NeonVale: Line/column info ──
        prints(" ");
        print_num(self.cursor_line + 1);
        prints(",");
        print_num(self.cursor_col + 1);
        prints("/");
        print_num(self.buffer.line_count());

        // — NeonVale: Clear to end of line for clean status bar
        prints("\x1b[K");
        self.term.exit_attributes();

        // ── NeonVale: Message / command line at row (rows - 1) ──
        self.term.move_cursor(self.term.rows - 1, 0);
        if self.message_len > 0 {
            write_bytes(&self.message[..self.message_len]);
        } else if self.mode == Mode::Command {
            prints(":");
            write_bytes(&self.command_buf[..self.command_len]);
        } else if self.mode == Mode::Search {
            if self.search_forward {
                prints("/");
            } else {
                prints("?");
            }
            write_bytes(&self.command_buf[..self.command_len]);
        }
        prints("\x1b[K");

        // ── NeonVale: Position cursor at edit location ──
        let screen_line = if self.cursor_line >= self.top_line {
            self.cursor_line - self.top_line
        } else {
            0
        };
        self.term.move_cursor(screen_line, self.cursor_col);

        // — TorqueJax: Show cursor again
        prints("\x1b[?25h");
    }

    /// ── GraveShift: Process normal mode commands ──
    fn process_normal(&mut self, ch: u8) {
        self.message_len = 0;

        match ch {
            // Movement
            b'h' => {
                if self.cursor_col > 0 {
                    self.cursor_col -= 1;
                }
            }
            b'j' => {
                if self.cursor_line + 1 < self.buffer.line_count() {
                    self.cursor_line += 1;
                    self.normalize_cursor();
                }
            }
            b'k' => {
                if self.cursor_line > 0 {
                    self.cursor_line -= 1;
                    self.normalize_cursor();
                }
            }
            b'l' => {
                let line_len = self.buffer.line_len(self.cursor_line);
                if line_len > 0 && self.cursor_col + 1 < line_len {
                    self.cursor_col += 1;
                }
            }
            b'0' => self.cursor_col = 0,
            b'$' => {
                let line_len = self.buffer.line_len(self.cursor_line);
                if line_len > 0 {
                    self.cursor_col = line_len - 1;
                } else {
                    self.cursor_col = 0;
                }
            }
            b'w' => self.move_word_forward(),
            b'b' => self.move_word_backward(),
            b'g' => {
                let ch2 = self.read_key();
                if ch2 == b'g' {
                    self.cursor_line = 0;
                    self.cursor_col = 0;
                    self.top_line = 0;
                }
            }
            b'G' => {
                self.cursor_line = if self.buffer.line_count() > 0 {
                    self.buffer.line_count() - 1
                } else {
                    0
                };
                self.cursor_col = 0;
            }

            // Insert mode
            b'i' => self.mode = Mode::Insert,
            b'a' => {
                let line_len = self.buffer.line_len(self.cursor_line);
                if self.cursor_col < line_len {
                    self.cursor_col += 1;
                }
                self.mode = Mode::Insert;
            }
            b'o' => {
                self.buffer.insert_line(self.cursor_line + 1);
                self.cursor_line += 1;
                self.cursor_col = 0;
                self.mode = Mode::Insert;
            }
            b'O' => {
                self.buffer.insert_line(self.cursor_line);
                self.cursor_col = 0;
                self.mode = Mode::Insert;
            }
            b'A' => {
                self.cursor_col = self.buffer.line_len(self.cursor_line);
                self.mode = Mode::Insert;
            }

            // Delete
            b'x' => {
                let line_len = self.buffer.line_len(self.cursor_line);
                if line_len > 0 && self.cursor_col < line_len {
                    self.buffer.lines[self.cursor_line].remove(self.cursor_col);
                    self.buffer.modified = true;
                }
                self.normalize_cursor();
            }
            b'd' => {
                let ch2 = self.read_key();
                if ch2 == b'd' {
                    self.yank_line();
                    self.buffer.delete_line(self.cursor_line);
                    self.normalize_cursor();
                }
            }

            // Yank (copy)
            b'y' => {
                let ch2 = self.read_key();
                if ch2 == b'y' {
                    self.yank_line();
                }
            }

            // Paste
            b'p' => {
                if !self.yank_buf.is_empty() {
                    self.buffer.insert_line(self.cursor_line + 1);
                    self.cursor_line += 1;
                    self.buffer.lines[self.cursor_line] = self.yank_buf.clone();
                    self.buffer.modified = true;
                }
            }

            // Command mode
            b':' => {
                self.mode = Mode::Command;
                self.command_len = 0;
            }

            // Search
            b'/' => {
                self.mode = Mode::Search;
                self.search_forward = true;
                self.command_len = 0;
            }
            b'?' => {
                self.mode = Mode::Search;
                self.search_forward = false;
                self.command_len = 0;
            }
            b'n' => self.search_next(true),
            b'N' => self.search_next(false),

            _ => {}
        }

        self.adjust_viewport();
    }

    /// ── GraveShift: Process insert mode input ──
    fn process_insert(&mut self, ch: u8) {
        if ch == 27 {
            // ESC
            self.mode = Mode::Normal;
            if self.cursor_col > 0 {
                self.cursor_col -= 1;
            }
            self.normalize_cursor();
            return;
        }

        match ch {
            b'\n' | b'\r' => {
                // ── WireSaint: Split line at cursor using Vec operations ──
                let rest: Vec<u8> = self.buffer.lines[self.cursor_line].split_off(self.cursor_col);

                self.buffer.insert_line(self.cursor_line + 1);
                self.buffer.lines[self.cursor_line + 1] = rest;

                self.cursor_line += 1;
                self.cursor_col = 0;
            }
            127 | 8 => {
                // Backspace
                if self.cursor_col > 0 {
                    // ── WireSaint: Remove char before cursor ──
                    self.buffer.lines[self.cursor_line].remove(self.cursor_col - 1);
                    self.cursor_col -= 1;
                    self.buffer.modified = true;
                } else if self.cursor_line > 0 {
                    // ── WireSaint: Join with previous line ──
                    let current = self.buffer.lines[self.cursor_line].clone();
                    let prev_len = self.buffer.line_len(self.cursor_line - 1);
                    self.buffer.lines[self.cursor_line - 1].extend_from_slice(&current);
                    self.buffer.delete_line(self.cursor_line);
                    self.cursor_line -= 1;
                    self.cursor_col = prev_len;
                    self.buffer.modified = true;
                }
            }
            _ if ch >= 32 && ch < 127 => {
                // ── WireSaint: Insert character at cursor position ──
                let line_len = self.buffer.line_len(self.cursor_line);
                if self.cursor_col <= line_len {
                    self.buffer.lines[self.cursor_line].insert(self.cursor_col, ch);
                    self.cursor_col += 1;
                    self.buffer.modified = true;
                }
            }
            _ => {}
        }

        self.adjust_viewport();
    }

    /// ── GraveShift: Process command mode input ──
    fn process_command(&mut self, ch: u8) {
        match ch {
            27 => {
                self.mode = Mode::Normal;
                self.command_len = 0;
            }
            b'\n' | b'\r' => {
                self.execute_command();
                self.mode = Mode::Normal;
                self.command_len = 0;
            }
            127 | 8 => {
                if self.command_len > 0 {
                    self.command_len -= 1;
                }
            }
            _ if ch >= 32 && ch < 127 => {
                if self.command_len < MAX_COMMAND - 1 {
                    self.command_buf[self.command_len] = ch;
                    self.command_len += 1;
                }
            }
            _ => {}
        }
    }

    /// ── GraveShift: Process search mode input ──
    fn process_search(&mut self, ch: u8) {
        match ch {
            27 => {
                self.mode = Mode::Normal;
                self.command_len = 0;
            }
            b'\n' | b'\r' => {
                self.search_len = self.command_len;
                for i in 0..self.command_len {
                    self.search_pattern[i] = self.command_buf[i];
                }
                self.search_next(true);
                self.mode = Mode::Normal;
                self.command_len = 0;
            }
            127 | 8 => {
                if self.command_len > 0 {
                    self.command_len -= 1;
                }
            }
            _ if ch >= 32 && ch < 127 => {
                if self.command_len < MAX_COMMAND - 1 {
                    self.command_buf[self.command_len] = ch;
                    self.command_len += 1;
                }
            }
            _ => {}
        }
    }

    /// ── WireSaint: Execute command mode command ──
    fn execute_command(&mut self) {
        if self.command_len == 0 {
            return;
        }

        let mut cmd = [0u8; MAX_COMMAND];
        let cmd_len = self.command_len;
        for i in 0..cmd_len {
            cmd[i] = self.command_buf[i];
        }

        if cmd[0] == b'w' {
            if self.filename_len == 0 {
                self.set_message(b"No filename");
            } else {
                let fname = core::str::from_utf8(&self.filename[..self.filename_len]);
                if let Ok(fname) = fname {
                    if self.buffer.save_file(fname) {
                        self.set_message(b"Written");
                    } else {
                        self.set_message(b"Write failed");
                    }
                }
            }

            if cmd_len >= 2 && cmd[1] == b'q' {
                self.quit_requested = true;
            }
        } else if cmd[0] == b'q' {
            if self.buffer.modified && (cmd_len < 2 || cmd[1] != b'!') {
                self.set_message(b"No write since last change (use :q! to override)");
            } else {
                self.quit_requested = true;
            }
        } else {
            self.set_message(b"Unknown command");
        }
    }

    /// ── NeonRoot: Helper functions ──

    fn yank_line(&mut self) {
        self.yank_buf = self.buffer.lines[self.cursor_line].clone();
    }

    fn move_word_forward(&mut self) {
        let line_len = self.buffer.line_len(self.cursor_line);
        if self.cursor_col >= line_len {
            return;
        }

        let line = &self.buffer.lines[self.cursor_line];

        // ── GraveShift: Skip non-whitespace ──
        while self.cursor_col < line_len && line[self.cursor_col] != b' ' {
            self.cursor_col += 1;
        }

        // ── GraveShift: Skip whitespace ──
        while self.cursor_col < line_len && line[self.cursor_col] == b' ' {
            self.cursor_col += 1;
        }

        self.normalize_cursor();
    }

    fn move_word_backward(&mut self) {
        if self.cursor_col == 0 {
            return;
        }

        let line = &self.buffer.lines[self.cursor_line];

        self.cursor_col -= 1;

        while self.cursor_col > 0 && line[self.cursor_col] == b' ' {
            self.cursor_col -= 1;
        }

        while self.cursor_col > 0 && line[self.cursor_col - 1] != b' ' {
            self.cursor_col -= 1;
        }
    }

    fn search_next(&mut self, use_last_direction: bool) {
        if self.search_len == 0 {
            return;
        }

        let forward = if use_last_direction {
            self.search_forward
        } else {
            !self.search_forward
        };

        let start_line = self.cursor_line;
        let start_col = if forward {
            self.cursor_col + 1
        } else {
            self.cursor_col
        };

        let mut line = start_line;
        let mut first_pass = true;

        let mut pattern = [0u8; MAX_COMMAND];
        let pattern_len = self.search_len;
        for i in 0..pattern_len {
            pattern[i] = self.search_pattern[i];
        }

        loop {
            if !first_pass && line == start_line {
                break;
            }
            first_pass = false;

            let line_len = self.buffer.line_len(line);
            if line_len > 0 {
                let text = &self.buffer.lines[line][..line_len];
                let pat = &pattern[..pattern_len];

                if let Some(pos) = find_pattern(text, pat) {
                    if line != start_line
                        || (forward && pos >= start_col)
                        || (!forward && pos < start_col)
                    {
                        self.cursor_line = line;
                        self.cursor_col = pos;
                        self.adjust_viewport();
                        return;
                    }
                }
            }

            if forward {
                line = (line + 1) % self.buffer.line_count();
            } else if line > 0 {
                line -= 1;
            } else {
                line = self.buffer.line_count() - 1;
            }
        }

        self.set_message(b"Pattern not found");
    }

    fn adjust_viewport(&mut self) {
        let visible_lines = if self.term.rows > 2 {
            self.term.rows - 2
        } else {
            1
        };

        if self.cursor_line < self.top_line {
            self.top_line = self.cursor_line;
        } else if self.cursor_line >= self.top_line + visible_lines {
            self.top_line = self.cursor_line - visible_lines + 1;
        }
    }

    fn set_message(&mut self, msg: &[u8]) {
        self.message_len = msg.len().min(256);
        self.message[..self.message_len].copy_from_slice(&msg[..self.message_len]);
    }

    fn read_key(&self) -> u8 {
        // — GraveShift: Use the stored tty_fd, don't open a new one every time
        if self.tty_fd < 0 {
            return 0;
        }
        let mut buf = [0u8; 1];
        let n = read(self.tty_fd, &mut buf);
        if n <= 0 {
            return 0;
        }
        buf[0]
    }
}

/// ── NeonVale: Helper functions for rendering ──

fn print_num(n: usize) {
    if n == 0 {
        prints("0");
        return;
    }

    let mut buf = [0u8; 20];
    let mut idx = 0;
    let mut num = n;

    while num > 0 {
        buf[idx] = b'0' + (num % 10) as u8;
        num /= 10;
        idx += 1;
    }

    for i in (0..idx).rev() {
        putchar(buf[i]);
    }
}

fn find_pattern(text: &[u8], pattern: &[u8]) -> Option<usize> {
    if pattern.is_empty() || pattern.len() > text.len() {
        return None;
    }

    for i in 0..=text.len() - pattern.len() {
        let mut matched = true;
        for j in 0..pattern.len() {
            if text[i + j] != pattern[j] {
                matched = false;
                break;
            }
        }
        if matched {
            return Some(i);
        }
    }

    None
}

#[unsafe(no_mangle)]
fn main(argc: i32, argv: *const *const u8) -> i32 {
    let mut editor = Editor::new();

    // Parse arguments
    if argc > 1 {
        let arg = unsafe { *argv.add(1) };
        let mut len = 0;
        unsafe {
            while *arg.add(len) != 0 && len < MAX_FILENAME {
                editor.filename[len] = *arg.add(len);
                len += 1;
            }
        }
        editor.filename_len = len;

        // ── WireSaint: Try to load file ──
        let fname = core::str::from_utf8(&editor.filename[..editor.filename_len]);
        if let Ok(fname) = fname {
            editor.buffer.load_file(fname);
        }
    }

    // ── TorqueJax: Set up raw terminal mode ──
    // — GraveShift: Without raw mode we're dead in the water. Line buffering
    // means no keypress until Enter, echo means double characters. Can't edit shit.
    let tty_fd = open2("/dev/tty", O_RDWR);
    if tty_fd < 0 {
        eprintlns("vi: cannot open terminal");
        return 1;
    }

    // — NeonVale: Store the fd and reinit terminal with actual size
    editor.tty_fd = tty_fd;
    editor.term = Term::new_with_fd(tty_fd);

    // — BlackLatch: Save original termios and switch to raw mode
    let mut orig_termios: Termios = unsafe { core::mem::zeroed() };
    tcgetattr(tty_fd, &mut orig_termios);

    let mut raw = orig_termios.clone();
    // — TorqueJax: Disable canonical mode (line buffering), echo, and signal chars
    raw.c_lflag &= !(ICANON | ECHO | ISIG);
    // — TorqueJax: Disable input processing (CR->NL, etc)
    raw.c_iflag &= !(ICRNL | IXON);
    // — TorqueJax: Read returns after 1 char, no timeout
    raw.c_cc[VMIN] = 1;
    raw.c_cc[VTIME] = 0;
    tcsetattr(tty_fd, TCSAFLUSH, &raw);

    // — GraveShift: Initial screen clear ONCE, not every frame
    editor.term.clear_screen();

    // ── GraveShift: Main editor loop ──
    loop {
        editor.render();
        fflush_stdout(); // — NeonVale: Force output to screen NOW

        let mut buf = [0u8; 1];
        let n = read(tty_fd, &mut buf);
        if n <= 0 {
            break;
        }

        let ch = buf[0];

        match editor.mode {
            Mode::Normal => editor.process_normal(ch),
            Mode::Insert => editor.process_insert(ch),
            Mode::Command => editor.process_command(ch),
            Mode::Search => editor.process_search(ch),
        }

        if editor.quit_requested {
            break;
        }
    }

    // — BlackLatch: Restore original terminal settings before exit
    tcsetattr(tty_fd, TCSAFLUSH, &orig_termios);
    close(tty_fd);

    // ── NeonVale: Clear screen before exit via termcap ──
    editor.term.clear_screen();
    fflush_stdout();

    0
}
