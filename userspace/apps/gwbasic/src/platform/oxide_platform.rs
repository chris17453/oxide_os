//! OXIDE OS platform implementation (no_std)
//!
//! This module provides platform abstractions for running on OXIDE OS.
//! It interfaces with the kernel through the oxide-std and libc libraries.

extern crate alloc;

use super::{Console, FileHandle, FileOpenMode, FileSystem, Graphics, System};
use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};

/// OXIDE Console implementation
pub struct OxideConsole {
    cursor_row: usize,
    cursor_col: usize,
    input_buffer: [u8; 256],
    input_len: usize,
}

impl OxideConsole {
    pub fn new() -> Self {
        OxideConsole {
            cursor_row: 0,
            cursor_col: 0,
            input_buffer: [0; 256],
            input_len: 0,
        }
    }
}

impl Default for OxideConsole {
    fn default() -> Self {
        Self::new()
    }
}

impl Console for OxideConsole {
    fn print(&mut self, s: &str) {
        let bytes = s.as_bytes();
        libc::write(libc::STDOUT_FILENO, bytes);
    }

    fn print_char(&mut self, ch: char) {
        let mut buf = [0u8; 4];
        let s = ch.encode_utf8(&mut buf);
        libc::write(libc::STDOUT_FILENO, s.as_bytes());
    }

    fn read_line(&mut self) -> String {
        self.input_len = 0;
        loop {
            // Read one character at a time
            let mut buf = [0u8; 1];
            let n = libc::read(libc::STDIN_FILENO, &mut buf);
            if n <= 0 {
                // No data available, yield a bit
                libc::sys_nanosleep(0, 10_000_000); // 10ms
                continue;
            }

            let key = buf[0];
            if key == b'\r' || key == b'\n' {
                break;
            }
            if key == 0x08 || key == 0x7f {
                // Backspace or DEL
                if self.input_len > 0 {
                    self.input_len -= 1;
                    self.print_char('\x08');
                    self.print_char(' ');
                    self.print_char('\x08');
                }
                continue;
            }
            if self.input_len < 255 {
                self.input_buffer[self.input_len] = key;
                self.input_len += 1;
                self.print_char(key as char);
            }
        }
        self.print_char('\n');
        // Convert buffer to string
        let slice = &self.input_buffer[..self.input_len];
        String::from_utf8_lossy(slice).into_owned()
    }

    fn read_char(&mut self) -> Option<char> {
        let mut buf = [0u8; 1];
        let n = libc::read(libc::STDIN_FILENO, &mut buf);
        if n > 0 {
            Some(buf[0] as char)
        } else {
            None
        }
    }

    fn clear(&mut self) {
        // Send ANSI clear screen sequence
        self.print("\x1b[2J\x1b[H");
        self.cursor_row = 0;
        self.cursor_col = 0;
    }

    fn set_cursor(&mut self, row: usize, col: usize) {
        // Use ANSI escape sequence for cursor positioning
        // ESC [ row ; col H
        use alloc::format;
        let seq = format!("\x1b[{};{}H", row + 1, col + 1);
        self.print(&seq);
        self.cursor_row = row;
        self.cursor_col = col;
    }

    fn get_cursor(&self) -> (usize, usize) {
        (self.cursor_row, self.cursor_col)
    }

    fn set_color(&mut self, fg: u8, bg: u8) {
        // Use ANSI escape sequences for colors
        // Map GW-BASIC colors (0-15) to ANSI colors
        use alloc::format;
        let ansi_fg = match fg {
            0 => 30,  // Black
            1 => 34,  // Blue
            2 => 32,  // Green
            3 => 36,  // Cyan
            4 => 31,  // Red
            5 => 35,  // Magenta
            6 => 33,  // Brown/Yellow
            7 => 37,  // White
            8 => 90,  // Gray (bright black)
            9 => 94,  // Bright blue
            10 => 92, // Bright green
            11 => 96, // Bright cyan
            12 => 91, // Bright red
            13 => 95, // Bright magenta
            14 => 93, // Bright yellow
            15 => 97, // Bright white
            _ => 37,  // Default white
        };
        let ansi_bg = match bg {
            0 => 40,
            1 => 44,
            2 => 42,
            3 => 46,
            4 => 41,
            5 => 45,
            6 => 43,
            7 => 47,
            _ => 40,
        };
        let seq = format!("\x1b[{};{}m", ansi_fg, ansi_bg);
        self.print(&seq);
    }
}

/// OXIDE File System implementation
pub struct OxideFileSystem {
    open_files: BTreeMap<i32, i32>, // our handle -> fd
    next_handle: i32,
}

impl OxideFileSystem {
    pub fn new() -> Self {
        OxideFileSystem {
            open_files: BTreeMap::new(),
            next_handle: 1,
        }
    }
}

impl Default for OxideFileSystem {
    fn default() -> Self {
        Self::new()
    }
}

impl FileSystem for OxideFileSystem {
    fn open(&mut self, path: &str, mode: FileOpenMode) -> Result<FileHandle, &'static str> {
        let flags = match mode {
            FileOpenMode::Input => libc::O_RDONLY,
            FileOpenMode::Output => libc::O_WRONLY | libc::O_CREAT | libc::O_TRUNC,
            FileOpenMode::Append => libc::O_WRONLY | libc::O_CREAT | libc::O_APPEND,
            FileOpenMode::Random => libc::O_RDWR | libc::O_CREAT,
        };

        let fd = libc::open(path, flags, 0o644);
        if fd < 0 {
            return Err("Cannot open file");
        }

        let handle = self.next_handle;
        self.next_handle += 1;
        self.open_files.insert(handle, fd);
        Ok(FileHandle(handle))
    }

    fn close(&mut self, handle: FileHandle) -> Result<(), &'static str> {
        if let Some(fd) = self.open_files.remove(&handle.0) {
            libc::close(fd);
            Ok(())
        } else {
            Err("File not open")
        }
    }

    fn read_line(&mut self, handle: FileHandle) -> Result<String, &'static str> {
        if let Some(&fd) = self.open_files.get(&handle.0) {
            let mut buffer = [0u8; 256];
            let mut pos = 0;

            // Read character by character until newline or buffer full
            while pos < buffer.len() - 1 {
                let mut ch = [0u8; 1];
                let n = libc::read(fd, &mut ch);
                if n <= 0 {
                    if pos == 0 {
                        return Err("EOF");
                    }
                    break;
                }
                if ch[0] == b'\n' {
                    break;
                }
                if ch[0] != b'\r' {
                    // Skip CR
                    buffer[pos] = ch[0];
                    pos += 1;
                }
            }

            Ok(String::from_utf8_lossy(&buffer[..pos]).to_string())
        } else {
            Err("File not open")
        }
    }

    fn write_line(&mut self, handle: FileHandle, data: &str) -> Result<(), &'static str> {
        if let Some(&fd) = self.open_files.get(&handle.0) {
            libc::write(fd, data.as_bytes());
            libc::write(fd, b"\n");
            Ok(())
        } else {
            Err("File not open")
        }
    }

    fn eof(&self, handle: FileHandle) -> bool {
        !self.open_files.contains_key(&handle.0)
    }
}

/// OXIDE Graphics implementation
///
/// Uses ANSI escape sequences for basic graphics in text mode.
/// Full graphics would require framebuffer access.
pub struct OxideGraphics {
    width: usize,
    height: usize,
}

impl OxideGraphics {
    pub fn new() -> Self {
        OxideGraphics {
            width: 80,
            height: 25,
        }
    }
}

impl Default for OxideGraphics {
    fn default() -> Self {
        Self::new()
    }
}

impl Graphics for OxideGraphics {
    fn pset(&mut self, _x: i32, _y: i32, _color: u8) {
        // Text mode doesn't support pixel drawing
        // Would need framebuffer access
    }

    fn line(&mut self, _x1: i32, _y1: i32, _x2: i32, _y2: i32, _color: u8) {
        // Text mode doesn't support line drawing
    }

    fn circle(&mut self, _x: i32, _y: i32, _radius: i32, _color: u8) {
        // Text mode doesn't support circle drawing
    }

    fn cls(&mut self) {
        // Clear screen with ANSI
        libc::write(libc::STDOUT_FILENO, b"\x1b[2J\x1b[H");
    }

    fn set_mode(&mut self, mode: u8) {
        // In text mode, we just track the nominal size
        let (w, h) = match mode {
            0 => (80, 25),   // Text mode
            1 => (320, 200), // CGA
            2 => (640, 200), // CGA hi-res
            _ => (80, 25),
        };
        self.width = w;
        self.height = h;
    }

    fn get_size(&self) -> (usize, usize) {
        (self.width, self.height)
    }

    fn display(&mut self) {
        // No-op in text mode
    }
}

/// OXIDE System implementation
pub struct OxideSystem {
    rng_state: u64,
}

impl OxideSystem {
    pub fn new() -> Self {
        // Initialize RNG with current time
        let time = libc::time::time(None);
        OxideSystem {
            rng_state: time as u64 ^ 12345,
        }
    }
}

impl Default for OxideSystem {
    fn default() -> Self {
        Self::new()
    }
}

impl System for OxideSystem {
    fn timer(&self) -> f32 {
        // Get current time and convert to seconds since midnight
        let time = libc::time::time(None);
        // Return time of day in seconds (modulo seconds per day)
        (time % 86400) as f32
    }

    fn sleep(&self, ms: u32) {
        let secs = (ms / 1000) as u64;
        let nanos = ((ms % 1000) * 1_000_000) as u64;
        libc::sys_nanosleep(secs, nanos);
    }

    fn random(&mut self, seed: Option<i32>) -> f32 {
        if let Some(sv) = seed {
            if sv < 0 {
                self.rng_state = (sv.abs() as u64) * 1000;
            } else if sv == 0 {
                return (self.rng_state % 1000) as f32 / 1000.0;
            }
        }
        // Simple LCG (Linear Congruential Generator)
        self.rng_state = (self.rng_state.wrapping_mul(1103515245).wrapping_add(12345)) & 0x7fffffff;
        (self.rng_state % 1000000) as f32 / 1000000.0
    }
}

/// Exit the program
pub fn exit(code: i32) -> ! {
    libc::_exit(code)
}

/// Get environment variable
pub fn get_env(name: &str) -> Option<String> {
    libc::getenv(name).map(|s| s.to_string())
}

/// Get command line arguments
pub fn get_args() -> alloc::vec::Vec<String> {
    // Would need args passed from kernel
    alloc::vec::Vec::new()
}
