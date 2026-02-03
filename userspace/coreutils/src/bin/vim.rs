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

#![no_std]
#![no_main]

use libc::*;

const MAX_LINES: usize = 10000;
const LINE_SIZE: usize = 2048;
const MAX_FILENAME: usize = 256;
const MAX_COMMAND: usize = 256;

/// ── GraveShift: Editor modes for modal editing ──
#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Normal,
    Insert,
    Command,
    Search,
}

/// ── WireSaint: Line-based buffer for file storage ──
struct Buffer {
    lines: [[u8; LINE_SIZE]; MAX_LINES],
    line_lens: [usize; MAX_LINES],
    line_count: usize,
    modified: bool,
}

impl Buffer {
    fn new() -> Self {
        Buffer {
            lines: [[0; LINE_SIZE]; MAX_LINES],
            line_lens: [0; MAX_LINES],
            line_count: 1, // Start with one empty line
            modified: false,
        }
    }

    /// ── BlackLatch: Safe line insertion with bounds checking ──
    fn insert_line(&mut self, at: usize) -> bool {
        if self.line_count >= MAX_LINES || at > self.line_count {
            return false;
        }

        // Shift lines down
        for i in (at..self.line_count).rev() {
            self.lines[i + 1] = self.lines[i];
            self.line_lens[i + 1] = self.line_lens[i];
        }

        self.lines[at] = [0; LINE_SIZE];
        self.line_lens[at] = 0;
        self.line_count += 1;
        self.modified = true;
        true
    }

    /// ── BlackLatch: Safe line deletion with bounds checking ──
    fn delete_line(&mut self, at: usize) -> bool {
        if at >= self.line_count {
            return false;
        }

        // Shift lines up
        for i in at..self.line_count - 1 {
            self.lines[i] = self.lines[i + 1];
            self.line_lens[i] = self.line_lens[i + 1];
        }

        if self.line_count > 1 {
            self.line_count -= 1;
        } else {
            // Keep at least one empty line
            self.lines[0] = [0; LINE_SIZE];
            self.line_lens[0] = 0;
        }
        self.modified = true;
        true
    }

    /// ── WireSaint: Load file contents into buffer ──
    fn load_file(&mut self, filename: &str) -> bool {
        let fd = open2(filename, O_RDONLY);
        if fd < 0 {
            return false;
        }

        self.line_count = 0;
        let mut buf = [0u8; 4096];
        let mut current_line = [0u8; LINE_SIZE];
        let mut current_len = 0;

        loop {
            let n = read(fd, &mut buf);
            if n <= 0 {
                break;
            }

            for i in 0..n as usize {
                if buf[i] == b'\n' {
                    if self.line_count < MAX_LINES {
                        self.lines[self.line_count][..current_len]
                            .copy_from_slice(&current_line[..current_len]);
                        self.line_lens[self.line_count] = current_len;
                        self.line_count += 1;
                    }
                    current_len = 0;
                } else if current_len < LINE_SIZE - 1 {
                    current_line[current_len] = buf[i];
                    current_len += 1;
                }
            }
        }

        // Handle last line without newline
        if current_len > 0 || self.line_count == 0 {
            if self.line_count < MAX_LINES {
                self.lines[self.line_count][..current_len]
                    .copy_from_slice(&current_line[..current_len]);
                self.line_lens[self.line_count] = current_len;
                self.line_count += 1;
            }
        }

        close(fd);
        self.modified = false;

        if self.line_count == 0 {
            self.line_count = 1;
        }

        true
    }

    /// ── WireSaint: Save buffer contents to file ──
    fn save_file(&mut self, filename: &str) -> bool {
        let fd = open(filename, O_WRONLY | O_CREAT | O_TRUNC, 0o644);
        if fd < 0 {
            return false;
        }

        for i in 0..self.line_count {
            if self.line_lens[i] > 0 {
                write(fd, &self.lines[i][..self.line_lens[i]]);
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
    cursor_line: usize,
    cursor_col: usize,
    top_line: usize,
    mode: Mode,
    filename: [u8; MAX_FILENAME],
    filename_len: usize,
    quit_requested: bool,
    screen_height: usize,
    command_buf: [u8; MAX_COMMAND],
    command_len: usize,
    message: [u8; 256],
    message_len: usize,
    yank_buf: [u8; LINE_SIZE],
    yank_len: usize,
    search_pattern: [u8; MAX_COMMAND],
    search_len: usize,
    search_forward: bool,
}

impl Editor {
    fn new() -> Self {
        Editor {
            buffer: Buffer::new(),
            cursor_line: 0,
            cursor_col: 0,
            top_line: 0,
            mode: Mode::Normal,
            filename: [0; MAX_FILENAME],
            filename_len: 0,
            quit_requested: false,
            screen_height: 24,
            command_buf: [0; MAX_COMMAND],
            command_len: 0,
            message: [0; 256],
            message_len: 0,
            yank_buf: [0; LINE_SIZE],
            yank_len: 0,
            search_pattern: [0; MAX_COMMAND],
            search_len: 0,
            search_forward: true,
        }
    }

    /// ── BlackLatch: Ensure cursor stays within valid buffer bounds ──
    fn normalize_cursor(&mut self) {
        if self.cursor_line >= self.buffer.line_count {
            self.cursor_line = if self.buffer.line_count > 0 {
                self.buffer.line_count - 1
            } else {
                0
            };
        }

        let line_len = self.buffer.line_lens[self.cursor_line];
        if self.mode == Mode::Normal {
            // In normal mode, cursor can't go past last character
            if line_len > 0 && self.cursor_col >= line_len {
                self.cursor_col = line_len - 1;
            } else if line_len == 0 {
                self.cursor_col = 0;
            }
        } else {
            // In insert mode, cursor can be at end of line
            if self.cursor_col > line_len {
                self.cursor_col = line_len;
            }
        }
    }

    /// ── NeonVale: Render the editor screen ──
    fn render(&self) {
        prints("\x1b[2J\x1b[H"); // Clear screen, move to home

        let visible_lines = if self.screen_height > 2 {
            self.screen_height - 2
        } else {
            1
        };

        // Render visible lines
        for i in 0..visible_lines {
            let line_idx = self.top_line + i;
            if line_idx < self.buffer.line_count {
                let line_len = self.buffer.line_lens[line_idx];
                if line_len > 0 {
                    for j in 0..line_len {
                        putchar(self.buffer.lines[line_idx][j]);
                    }
                }
            } else {
                prints("~");
            }
            prints("\r\n");
        }

        // Status line
        prints("\x1b[7m"); // Reverse video
        match self.mode {
            Mode::Normal => prints(" NORMAL "),
            Mode::Insert => prints(" INSERT "),
            Mode::Command => prints(" COMMAND "),
            Mode::Search => prints(" SEARCH "),
        }

        prints(" ");
        if self.filename_len > 0 {
            for i in 0..self.filename_len {
                putchar(self.filename[i]);
            }
        } else {
            prints("[No Name]");
        }

        if self.buffer.modified {
            prints(" [+]");
        }

        // Line/column info
        prints(" ");
        print_num(self.cursor_line + 1);
        prints(",");
        print_num(self.cursor_col + 1);
        prints("/");
        print_num(self.buffer.line_count);

        // Pad to end of line
        for _ in 0..40 {
            prints(" ");
        }
        prints("\x1b[0m\r\n"); // Reset attributes

        // Message line
        if self.message_len > 0 {
            for i in 0..self.message_len {
                putchar(self.message[i]);
            }
        } else if self.mode == Mode::Command {
            prints(":");
            for i in 0..self.command_len {
                putchar(self.command_buf[i]);
            }
        } else if self.mode == Mode::Search {
            if self.search_forward {
                prints("/");
            } else {
                prints("?");
            }
            for i in 0..self.command_len {
                putchar(self.command_buf[i]);
            }
        }

        // Position cursor
        let screen_line = if self.cursor_line >= self.top_line {
            self.cursor_line - self.top_line
        } else {
            0
        };
        prints("\x1b[");
        print_num(screen_line + 1);
        prints(";");
        print_num(self.cursor_col + 1);
        prints("H");
    }

    /// ── GraveShift: Process normal mode commands ──
    fn process_normal(&mut self, ch: u8) {
        self.message_len = 0; // Clear message

        match ch {
            // Movement
            b'h' => {
                if self.cursor_col > 0 {
                    self.cursor_col -= 1;
                }
            }
            b'j' => {
                if self.cursor_line + 1 < self.buffer.line_count {
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
                let line_len = self.buffer.line_lens[self.cursor_line];
                if line_len > 0 && self.cursor_col + 1 < line_len {
                    self.cursor_col += 1;
                }
            }
            b'0' => self.cursor_col = 0,
            b'$' => {
                let line_len = self.buffer.line_lens[self.cursor_line];
                if line_len > 0 {
                    self.cursor_col = line_len - 1;
                } else {
                    self.cursor_col = 0;
                }
            }
            b'w' => self.move_word_forward(),
            b'b' => self.move_word_backward(),
            b'g' => {
                // Wait for another 'g'
                let ch2 = self.read_key();
                if ch2 == b'g' {
                    self.cursor_line = 0;
                    self.cursor_col = 0;
                    self.top_line = 0;
                }
            }
            b'G' => {
                self.cursor_line = if self.buffer.line_count > 0 {
                    self.buffer.line_count - 1
                } else {
                    0
                };
                self.cursor_col = 0;
            }

            // Insert mode
            b'i' => self.mode = Mode::Insert,
            b'a' => {
                let line_len = self.buffer.line_lens[self.cursor_line];
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
                self.cursor_col = self.buffer.line_lens[self.cursor_line];
                self.mode = Mode::Insert;
            }

            // Delete
            b'x' => {
                let line_len = self.buffer.line_lens[self.cursor_line];
                if line_len > 0 && self.cursor_col < line_len {
                    let line = &mut self.buffer.lines[self.cursor_line];
                    for i in self.cursor_col..line_len - 1 {
                        line[i] = line[i + 1];
                    }
                    self.buffer.line_lens[self.cursor_line] -= 1;
                    self.buffer.modified = true;
                }
                self.normalize_cursor();
            }
            b'd' => {
                // Wait for another 'd' (dd = delete line)
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
                if self.yank_len > 0 {
                    self.buffer.insert_line(self.cursor_line + 1);
                    self.cursor_line += 1;
                    self.buffer.lines[self.cursor_line][..self.yank_len]
                        .copy_from_slice(&self.yank_buf[..self.yank_len]);
                    self.buffer.line_lens[self.cursor_line] = self.yank_len;
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
                // Split line at cursor
                let line_len = self.buffer.line_lens[self.cursor_line];
                let rest_len = if self.cursor_col < line_len {
                    line_len - self.cursor_col
                } else {
                    0
                };

                // Copy rest of line before modifying buffer
                let mut rest = [0u8; LINE_SIZE];
                if rest_len > 0 {
                    for i in 0..rest_len {
                        rest[i] = self.buffer.lines[self.cursor_line][self.cursor_col + i];
                    }
                }

                self.buffer.insert_line(self.cursor_line + 1);

                if rest_len > 0 {
                    // Copy rest of line to new line
                    for i in 0..rest_len {
                        self.buffer.lines[self.cursor_line + 1][i] = rest[i];
                    }
                    self.buffer.line_lens[self.cursor_line + 1] = rest_len;
                    self.buffer.line_lens[self.cursor_line] = self.cursor_col;
                }

                self.cursor_line += 1;
                self.cursor_col = 0;
            }
            127 | 8 => {
                // Backspace
                let line_len = self.buffer.line_lens[self.cursor_line];
                if self.cursor_col > 0 {
                    // Delete character before cursor
                    for i in self.cursor_col - 1..line_len - 1 {
                        self.buffer.lines[self.cursor_line][i] =
                            self.buffer.lines[self.cursor_line][i + 1];
                    }
                    self.buffer.line_lens[self.cursor_line] -= 1;
                    self.cursor_col -= 1;
                    self.buffer.modified = true;
                } else if self.cursor_line > 0 {
                    // Join with previous line
                    let prev_len = self.buffer.line_lens[self.cursor_line - 1];
                    let curr_len = line_len;

                    if prev_len + curr_len < LINE_SIZE {
                        // Copy current line to temp buffer
                        let mut temp = [0u8; LINE_SIZE];
                        for i in 0..curr_len {
                            temp[i] = self.buffer.lines[self.cursor_line][i];
                        }

                        // Append to previous line
                        for i in 0..curr_len {
                            self.buffer.lines[self.cursor_line - 1][prev_len + i] = temp[i];
                        }
                        self.buffer.line_lens[self.cursor_line - 1] = prev_len + curr_len;
                        self.buffer.delete_line(self.cursor_line);
                        self.cursor_line -= 1;
                        self.cursor_col = prev_len;
                        self.buffer.modified = true;
                    }
                }
            }
            _ if ch >= 32 && ch < 127 => {
                // Insert character
                let line_len = self.buffer.line_lens[self.cursor_line];
                if line_len < LINE_SIZE - 1 {
                    // Shift characters right
                    for i in (self.cursor_col..line_len).rev() {
                        self.buffer.lines[self.cursor_line][i + 1] =
                            self.buffer.lines[self.cursor_line][i];
                    }
                    self.buffer.lines[self.cursor_line][self.cursor_col] = ch;
                    self.buffer.line_lens[self.cursor_line] += 1;
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
                // ESC
                self.mode = Mode::Normal;
                self.command_len = 0;
            }
            b'\n' | b'\r' => {
                self.execute_command();
                self.mode = Mode::Normal;
                self.command_len = 0;
            }
            127 | 8 => {
                // Backspace
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
                // ESC
                self.mode = Mode::Normal;
                self.command_len = 0;
            }
            b'\n' | b'\r' => {
                // Save pattern and search
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

        // Copy command to local buffer to avoid borrow conflicts
        let mut cmd = [0u8; MAX_COMMAND];
        let cmd_len = self.command_len;
        for i in 0..cmd_len {
            cmd[i] = self.command_buf[i];
        }

        if cmd[0] == b'w' {
            // Write file
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

            // Check for 'wq'
            if cmd_len >= 2 && cmd[1] == b'q' {
                self.quit_requested = true;
            }
        } else if cmd[0] == b'q' {
            // Quit
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
        let len = self.buffer.line_lens[self.cursor_line];
        if len > 0 {
            self.yank_len = len;
            self.yank_buf[..len].copy_from_slice(&self.buffer.lines[self.cursor_line][..len]);
        }
    }

    fn move_word_forward(&mut self) {
        let line_len = self.buffer.line_lens[self.cursor_line];
        if self.cursor_col >= line_len {
            return;
        }

        let line = &self.buffer.lines[self.cursor_line];

        // Skip non-whitespace
        while self.cursor_col < line_len && line[self.cursor_col] != b' ' {
            self.cursor_col += 1;
        }

        // Skip whitespace
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

        // Move back one
        self.cursor_col -= 1;

        // Skip whitespace
        while self.cursor_col > 0 && line[self.cursor_col] == b' ' {
            self.cursor_col -= 1;
        }

        // Skip non-whitespace to find start of word
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

        // Copy pattern to local buffer to avoid borrow conflicts
        let mut pattern = [0u8; MAX_COMMAND];
        let pattern_len = self.search_len;
        for i in 0..pattern_len {
            pattern[i] = self.search_pattern[i];
        }

        loop {
            if !first_pass && line == start_line {
                break; // Wrapped around
            }
            first_pass = false;

            let line_len = self.buffer.line_lens[line];
            if line_len > 0 {
                let text = &self.buffer.lines[line][..line_len];
                let pat = &pattern[..pattern_len];

                // Simple substring search
                if let Some(pos) = find_pattern(text, pat) {
                    if line != start_line || (forward && pos >= start_col)
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
                line = (line + 1) % self.buffer.line_count;
            } else if line > 0 {
                line -= 1;
            } else {
                line = self.buffer.line_count - 1;
            }
        }

        self.set_message(b"Pattern not found");
    }

    fn adjust_viewport(&mut self) {
        let visible_lines = if self.screen_height > 2 {
            self.screen_height - 2
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
        let tty_fd = open2("/dev/console", O_RDONLY);
        if tty_fd < 0 {
            return 0;
        }

        let mut buf = [0u8; 1];
        read(tty_fd, &mut buf);
        close(tty_fd);

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

        // Try to load file
        let fname = core::str::from_utf8(&editor.filename[..editor.filename_len]);
        if let Ok(fname) = fname {
            editor.buffer.load_file(fname);
        }
    }

    // ── TorqueJax: Set up raw terminal mode ──
    let tty_fd = open2("/dev/console", O_RDONLY);
    if tty_fd < 0 {
        eprintlns("vim: cannot open terminal");
        return 1;
    }

    // Main editor loop
    loop {
        editor.render();

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

    close(tty_fd);

    // Clear screen before exit
    prints("\x1b[2J\x1b[H");

    0
}
