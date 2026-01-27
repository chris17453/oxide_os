//! Line discipline implementation
//!
//! Handles input processing (echo, line editing, signals) for TTYs.

extern crate alloc;

use alloc::collections::VecDeque;
use alloc::vec::Vec;

use crate::termios::{InputFlags, LocalFlags, OutputFlags, Termios};
use crate::termios::{
    VEOF, VERASE, VINTR, VKILL, VLNEXT, VMIN, VQUIT, VREPRINT, VSUSP, VTIME, VWERASE,
};

/// Maximum input buffer size
const INPUT_BUF_SIZE: usize = 4096;

/// Maximum output buffer size
const OUTPUT_BUF_SIZE: usize = 4096;

/// Maximum line length in canonical mode
const MAX_CANON: usize = 255;

/// Signal to generate
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Signal {
    /// SIGINT (^C)
    Int,
    /// SIGQUIT (^\)
    Quit,
    /// SIGTSTP (^Z)
    Tstp,
}

impl Signal {
    /// Convert to signal number
    pub fn to_signo(self) -> i32 {
        match self {
            Signal::Int => signal::SIGINT,
            Signal::Quit => signal::SIGQUIT,
            Signal::Tstp => signal::SIGTSTP,
        }
    }
}

/// Line discipline state
pub struct LineDiscipline {
    /// Terminal settings
    termios: Termios,
    /// Input queue (cooked characters ready for reading)
    input_queue: VecDeque<u8>,
    /// Edit buffer (current line being edited in canonical mode)
    edit_buf: Vec<u8>,
    /// Output queue (data waiting to be written to device)
    output_queue: VecDeque<u8>,
    /// Column position (for echo)
    column: usize,
    /// Pending signal to deliver
    pending_signal: Option<Signal>,
    /// Next character should be literal (^V prefix)
    literal_next: bool,
}

impl LineDiscipline {
    /// Create a new line discipline with default settings
    pub fn new() -> Self {
        LineDiscipline {
            termios: Termios::default(),
            input_queue: VecDeque::with_capacity(INPUT_BUF_SIZE),
            edit_buf: Vec::with_capacity(MAX_CANON),
            output_queue: VecDeque::with_capacity(OUTPUT_BUF_SIZE),
            column: 0,
            pending_signal: None,
            literal_next: false,
        }
    }

    /// Get terminal settings
    pub fn termios(&self) -> &Termios {
        &self.termios
    }

    /// Get terminal settings (mutable)
    pub fn termios_mut(&mut self) -> &mut Termios {
        &mut self.termios
    }

    /// Set terminal settings
    pub fn set_termios(&mut self, termios: Termios) {
        self.termios = termios;
    }

    /// Process an input character from the hardware
    ///
    /// Returns data to echo back to the terminal (if echo enabled).
    pub fn input_char(&mut self, c: u8) -> Vec<u8> {
        let mut echo_buf = Vec::new();

        // Handle literal next (^V)
        if self.literal_next {
            self.literal_next = false;
            return self.add_input_char(c, &mut echo_buf);
        }

        // Input processing (c_iflag)
        let c = self.process_input_char(c);

        // Signal generation (ISIG)
        if self.termios.c_lflag.contains(LocalFlags::ISIG) {
            if c == self.termios.c_cc[VINTR] {
                self.pending_signal = Some(Signal::Int);
                if !self.termios.c_lflag.contains(LocalFlags::NOFLSH) {
                    self.flush_input();
                }
                self.echo_control_char(c, &mut echo_buf);
                return echo_buf;
            }
            if c == self.termios.c_cc[VQUIT] {
                self.pending_signal = Some(Signal::Quit);
                if !self.termios.c_lflag.contains(LocalFlags::NOFLSH) {
                    self.flush_input();
                }
                self.echo_control_char(c, &mut echo_buf);
                return echo_buf;
            }
            if c == self.termios.c_cc[VSUSP] {
                self.pending_signal = Some(Signal::Tstp);
                if !self.termios.c_lflag.contains(LocalFlags::NOFLSH) {
                    self.flush_input();
                }
                self.echo_control_char(c, &mut echo_buf);
                return echo_buf;
            }
        }

        // Canonical mode processing
        if self.termios.c_lflag.contains(LocalFlags::ICANON) {
            self.process_canonical(c, &mut echo_buf)
        } else {
            self.process_raw(c, &mut echo_buf)
        }
    }

    /// Process input character (c_iflag transformations)
    fn process_input_char(&self, c: u8) -> u8 {
        let mut c = c;

        // Strip 8th bit
        if self.termios.c_iflag.contains(InputFlags::ISTRIP) {
            c &= 0x7F;
        }

        // Map CR to NL
        if c == b'\r' && self.termios.c_iflag.contains(InputFlags::ICRNL) {
            c = b'\n';
        }

        // Map NL to CR
        if c == b'\n' && self.termios.c_iflag.contains(InputFlags::INLCR) {
            c = b'\r';
        }

        // Ignore CR
        if c == b'\r' && self.termios.c_iflag.contains(InputFlags::IGNCR) {
            return 0; // Will be filtered
        }

        c
    }

    /// Process character in canonical mode
    fn process_canonical(&mut self, c: u8, echo_buf: &mut Vec<u8>) -> Vec<u8> {
        // Handle VLNEXT (^V) - make next char literal
        if self.termios.c_lflag.contains(LocalFlags::IEXTEN) && c == self.termios.c_cc[VLNEXT] {
            self.literal_next = true;
            if self.termios.c_lflag.contains(LocalFlags::ECHO) {
                echo_buf.push(b'^');
                echo_buf.push(8); // backspace
            }
            return echo_buf.clone();
        }

        // Erase character
        if c == self.termios.c_cc[VERASE] && self.termios.c_cc[VERASE] != 0 {
            self.erase_char(echo_buf);
            return echo_buf.clone();
        }

        // Kill line
        if c == self.termios.c_cc[VKILL] && self.termios.c_cc[VKILL] != 0 {
            self.kill_line(echo_buf);
            return echo_buf.clone();
        }

        // Word erase
        if self.termios.c_lflag.contains(LocalFlags::IEXTEN)
            && c == self.termios.c_cc[VWERASE]
            && self.termios.c_cc[VWERASE] != 0
        {
            self.erase_word(echo_buf);
            return echo_buf.clone();
        }

        // Reprint line
        if self.termios.c_lflag.contains(LocalFlags::IEXTEN)
            && c == self.termios.c_cc[VREPRINT]
            && self.termios.c_cc[VREPRINT] != 0
        {
            self.reprint_line(echo_buf);
            return echo_buf.clone();
        }

        // EOF
        if c == self.termios.c_cc[VEOF] && self.termios.c_cc[VEOF] != 0 {
            // Move edit buffer to input queue
            self.commit_line();
            return echo_buf.clone();
        }

        // Newline or EOL - complete the line
        if c == b'\n' {
            if self.termios.c_lflag.contains(LocalFlags::ECHO)
                || self.termios.c_lflag.contains(LocalFlags::ECHONL)
            {
                echo_buf.push(b'\r');
                echo_buf.push(b'\n');
            }
            self.edit_buf.push(b'\n');

            // Debug: show state before commit
            let edit_len_before = self.edit_buf.len();
            let queue_len_before = self.input_queue.len();

            self.commit_line();

            let queue_len_after = self.input_queue.len();

            self.column = 0;
            return echo_buf.clone();
        }

        // Regular character - add to edit buffer
        self.add_input_char(c, echo_buf)
    }

    /// Process character in raw/non-canonical mode
    fn process_raw(&mut self, c: u8, echo_buf: &mut Vec<u8>) -> Vec<u8> {
        // In raw mode, characters go directly to input queue
        if self.input_queue.len() < INPUT_BUF_SIZE {
            self.input_queue.push_back(c);
        }

        // Echo if enabled
        if self.termios.c_lflag.contains(LocalFlags::ECHO) {
            self.echo_char(c, echo_buf);
        }

        echo_buf.clone()
    }

    /// Add a character to the edit buffer
    fn add_input_char(&mut self, c: u8, echo_buf: &mut Vec<u8>) -> Vec<u8> {
        if self.edit_buf.len() < MAX_CANON {
            self.edit_buf.push(c);

            if self.termios.c_lflag.contains(LocalFlags::ECHO) {
                self.echo_char(c, echo_buf);
            }
        }

        echo_buf.clone()
    }

    /// Echo a character
    fn echo_char(&mut self, c: u8, echo_buf: &mut Vec<u8>) {
        if c < 0x20 && c != b'\t' && c != b'\n' && c != b'\r' {
            // Control character
            if self.termios.c_lflag.contains(LocalFlags::ECHOCTL) {
                echo_buf.push(b'^');
                echo_buf.push(c + 0x40);
                self.column += 2;
            }
        } else if c == 0x7F {
            // DEL
            if self.termios.c_lflag.contains(LocalFlags::ECHOCTL) {
                echo_buf.push(b'^');
                echo_buf.push(b'?');
                self.column += 2;
            }
        } else if c == b'\t' {
            // Tab - expand to spaces
            let spaces = 8 - (self.column % 8);
            for _ in 0..spaces {
                echo_buf.push(b' ');
            }
            self.column += spaces;
        } else if c == b'\n' {
            if self.termios.c_oflag.contains(OutputFlags::ONLCR) {
                echo_buf.push(b'\r');
            }
            echo_buf.push(b'\n');
            self.column = 0;
        } else if c == b'\r' {
            echo_buf.push(b'\r');
            self.column = 0;
        } else {
            echo_buf.push(c);
            self.column += 1;
        }
    }

    /// Echo a control character (for signals)
    fn echo_control_char(&mut self, c: u8, echo_buf: &mut Vec<u8>) {
        if self.termios.c_lflag.contains(LocalFlags::ECHO) {
            if self.termios.c_lflag.contains(LocalFlags::ECHOCTL) {
                echo_buf.push(b'^');
                echo_buf.push(if c < 0x20 { c + 0x40 } else { b'?' });
            }
            echo_buf.push(b'\r');
            echo_buf.push(b'\n');
        }
    }

    /// Erase one character
    fn erase_char(&mut self, echo_buf: &mut Vec<u8>) {
        if let Some(c) = self.edit_buf.pop() {
            if self.termios.c_lflag.contains(LocalFlags::ECHO)
                && self.termios.c_lflag.contains(LocalFlags::ECHOE)
            {
                // Backspace-space-backspace to erase
                let width = self.char_width(c);
                for _ in 0..width {
                    echo_buf.push(8); // backspace
                    echo_buf.push(b' ');
                    echo_buf.push(8); // backspace
                }
                self.column = self.column.saturating_sub(width);
            }
        }
    }

    /// Kill the entire line
    fn kill_line(&mut self, echo_buf: &mut Vec<u8>) {
        if self.termios.c_lflag.contains(LocalFlags::ECHO) {
            if self.termios.c_lflag.contains(LocalFlags::ECHOKE) {
                // Erase the line visually
                while !self.edit_buf.is_empty() {
                    self.erase_char(echo_buf);
                }
            } else if self.termios.c_lflag.contains(LocalFlags::ECHOK) {
                // Just echo newline
                echo_buf.push(b'\r');
                echo_buf.push(b'\n');
                self.column = 0;
            }
        }
        self.edit_buf.clear();
    }

    /// Erase one word
    fn erase_word(&mut self, echo_buf: &mut Vec<u8>) {
        // Skip trailing whitespace
        while !self.edit_buf.is_empty()
            && self
                .edit_buf
                .last()
                .map(|&c| c == b' ' || c == b'\t')
                .unwrap_or(false)
        {
            self.erase_char(echo_buf);
        }

        // Erase word
        while !self.edit_buf.is_empty()
            && self
                .edit_buf
                .last()
                .map(|&c| c != b' ' && c != b'\t')
                .unwrap_or(false)
        {
            self.erase_char(echo_buf);
        }
    }

    /// Reprint the current line
    fn reprint_line(&mut self, echo_buf: &mut Vec<u8>) {
        if self.termios.c_lflag.contains(LocalFlags::ECHO) {
            echo_buf.push(b'^');
            echo_buf.push(b'R');
            echo_buf.push(b'\r');
            echo_buf.push(b'\n');
            // Clone edit_buf to avoid borrow conflict
            let edit_copy: Vec<u8> = self.edit_buf.clone();
            for c in edit_copy {
                self.echo_char(c, echo_buf);
            }
        }
    }

    /// Commit the edit buffer to the input queue
    fn commit_line(&mut self) {
        // Debug: show what we're committing
        let edit_len = self.edit_buf.len();

        for c in self.edit_buf.drain(..) {
            if self.input_queue.len() < INPUT_BUF_SIZE {
                self.input_queue.push_back(c);
            }
        }

        // Debug output (unfortunately can't write to driver from here)
        // Will rely on TTY read debug to show input_queue size
    }

    /// Get character display width
    fn char_width(&self, c: u8) -> usize {
        if c < 0x20 || c == 0x7F {
            if self.termios.c_lflag.contains(LocalFlags::ECHOCTL) {
                2 // ^X
            } else {
                0
            }
        } else if c == b'\t' {
            // Tab width varies
            8 - (self.column % 8)
        } else {
            1
        }
    }

    /// Read from the input queue
    ///
    /// Returns the number of bytes read.
    pub fn read(&mut self, buf: &mut [u8]) -> usize {
        if self.termios.c_lflag.contains(LocalFlags::ICANON) {
            // Canonical mode: return up to one line
            self.read_canonical(buf)
        } else {
            // Raw mode: return based on VMIN/VTIME
            self.read_raw(buf)
        }
    }

    /// Read in canonical mode (line by line)
    fn read_canonical(&mut self, buf: &mut [u8]) -> usize {
        let mut count = 0;

        // Check if we have a complete line (or EOF)
        let has_line = self.input_queue.iter().any(|&c| c == b'\n');
        if !has_line && self.input_queue.is_empty() {
            return 0;
        }

        // Read up to newline
        while count < buf.len() {
            if let Some(c) = self.input_queue.pop_front() {
                buf[count] = c;
                count += 1;
                if c == b'\n' {
                    break;
                }
            } else {
                break;
            }
        }

        count
    }

    /// Read in raw mode
    fn read_raw(&mut self, buf: &mut [u8]) -> usize {
        let vmin = self.termios.c_cc[VMIN] as usize;
        let _vtime = self.termios.c_cc[VTIME];

        // Simple implementation: return available data up to VMIN or buf.len()
        if self.input_queue.len() < vmin && vmin > 0 {
            return 0; // Not enough data yet
        }

        let count = buf.len().min(self.input_queue.len());
        for byte in buf.iter_mut().take(count) {
            if let Some(c) = self.input_queue.pop_front() {
                *byte = c;
            }
        }

        count
    }

    /// Check if there's data available to read
    pub fn can_read(&self) -> bool {
        if self.termios.c_lflag.contains(LocalFlags::ICANON) {
            // Canonical: need complete line (newline in queue)
            self.input_queue.iter().any(|&c| c == b'\n')
        } else {
            // Raw: check VMIN
            let vmin = self.termios.c_cc[VMIN] as usize;
            self.input_queue.len() >= vmin.max(1)
        }
    }

    /// Get number of bytes in input queue
    pub fn input_available(&self) -> usize {
        self.input_queue.len()
    }

    /// Flush input queue
    pub fn flush_input(&mut self) {
        self.input_queue.clear();
        self.edit_buf.clear();
    }

    /// Flush output queue
    pub fn flush_output(&mut self) {
        self.output_queue.clear();
    }

    /// Take pending signal
    pub fn take_signal(&mut self) -> Option<Signal> {
        self.pending_signal.take()
    }

    /// Process output (c_oflag transformations)
    pub fn process_output(&self, c: u8) -> Vec<u8> {
        let mut output = Vec::new();

        if !self.termios.c_oflag.contains(OutputFlags::OPOST) {
            output.push(c);
            return output;
        }

        match c {
            b'\n' => {
                if self.termios.c_oflag.contains(OutputFlags::ONLCR) {
                    output.push(b'\r');
                }
                output.push(b'\n');
            }
            b'\r' => {
                if self.termios.c_oflag.contains(OutputFlags::OCRNL) {
                    output.push(b'\n');
                } else if !self.termios.c_oflag.contains(OutputFlags::ONLRET) {
                    output.push(b'\r');
                }
            }
            _ => {
                output.push(c);
            }
        }

        output
    }
}

impl Default for LineDiscipline {
    fn default() -> Self {
        Self::new()
    }
}
