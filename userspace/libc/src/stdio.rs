//! Standard I/O
//!
//! Basic printf-like formatting and I/O functions.

use crate::fcntl::*;
use crate::syscall;
use core::fmt::{self, Write};

/// Writer for stdout
pub struct StdoutWriter;

impl Write for StdoutWriter {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        syscall::sys_write(STDOUT_FILENO, s.as_bytes());
        Ok(())
    }
}

/// Writer for stderr
pub struct StderrWriter;

impl Write for StderrWriter {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        syscall::sys_write(STDERR_FILENO, s.as_bytes());
        Ok(())
    }
}

/// Print formatted text to stdout
#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => {{
        use core::fmt::Write;
        let _ = write!($crate::stdio::StdoutWriter, $($arg)*);
    }};
}

/// Print formatted text to stdout with newline
#[macro_export]
macro_rules! println {
    () => {{
        $crate::print!("\n");
    }};
    ($($arg:tt)*) => {{
        use core::fmt::Write;
        let _ = write!($crate::stdio::StdoutWriter, $($arg)*);
        let _ = write!($crate::stdio::StdoutWriter, "\n");
    }};
}

/// Print formatted text to stderr
#[macro_export]
macro_rules! eprint {
    ($($arg:tt)*) => {{
        use core::fmt::Write;
        let _ = write!($crate::stdio::StderrWriter, $($arg)*);
    }};
}

/// Print formatted text to stderr with newline
#[macro_export]
macro_rules! eprintln {
    () => {{
        $crate::eprint!("\n");
    }};
    ($($arg:tt)*) => {{
        use core::fmt::Write;
        let _ = write!($crate::stdio::StderrWriter, $($arg)*);
        let _ = write!($crate::stdio::StderrWriter, "\n");
    }};
}

/// Print a string to stdout (use `prints` to avoid macro conflict)
pub fn print(s: &str) {
    syscall::sys_write(STDOUT_FILENO, s.as_bytes());
}

/// Alias for `print` - prints a string to stdout
pub fn prints(s: &str) {
    syscall::sys_write(STDOUT_FILENO, s.as_bytes());
}

/// Print a string to stdout with newline (use `printlns` to avoid macro conflict)
pub fn println(s: &str) {
    syscall::sys_write(STDOUT_FILENO, s.as_bytes());
    syscall::sys_write(STDOUT_FILENO, b"\n");
}

/// Alias for `println` - prints a string to stdout with newline
pub fn printlns(s: &str) {
    syscall::sys_write(STDOUT_FILENO, s.as_bytes());
    syscall::sys_write(STDOUT_FILENO, b"\n");
}

/// Print to stderr (use `eprints` to avoid macro conflict)
pub fn eprint(s: &str) {
    syscall::sys_write(STDERR_FILENO, s.as_bytes());
}

/// Alias for `eprint` - prints a string to stderr
pub fn eprints(s: &str) {
    syscall::sys_write(STDERR_FILENO, s.as_bytes());
}

/// Print to stderr with newline (use `eprintlns` to avoid macro conflict)
pub fn eprintln(s: &str) {
    syscall::sys_write(STDERR_FILENO, s.as_bytes());
    syscall::sys_write(STDERR_FILENO, b"\n");
}

/// Alias for `eprintln` - prints a string to stderr with newline
pub fn eprintlns(s: &str) {
    syscall::sys_write(STDERR_FILENO, s.as_bytes());
    syscall::sys_write(STDERR_FILENO, b"\n");
}

/// Print a character
pub fn putchar(c: u8) {
    syscall::sys_write(STDOUT_FILENO, &[c]);
}

/// Read a character from stdin
pub fn getchar() -> i32 {
    let mut buf = [0u8; 1];
    let ret = syscall::sys_read(STDIN_FILENO, &mut buf);
    if ret <= 0 { -1 } else { buf[0] as i32 }
}

/// Print a null-terminated string with newline (C puts function)
pub unsafe fn puts(s: *const u8) -> i32 {
    if s.is_null() {
        return -1;
    }

    // Find string length
    let mut len = 0;
    while *s.add(len) != 0 {
        len += 1;
    }

    // Write string
    let slice = core::slice::from_raw_parts(s, len);
    syscall::sys_write(STDOUT_FILENO, slice);

    // Write newline
    syscall::sys_write(STDOUT_FILENO, b"\n");

    0 // Success
}

/// Print an unsigned integer
pub fn print_u64(n: u64) {
    let mut buf = [0u8; 20];
    let mut i = 20;
    let mut n = n;

    if n == 0 {
        putchar(b'0');
        return;
    }

    while n > 0 {
        i -= 1;
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
    }

    syscall::sys_write(STDOUT_FILENO, &buf[i..20]);
}

/// Print a signed integer
pub fn print_i64(n: i64) {
    if n < 0 {
        putchar(b'-');
        print_u64((-n) as u64);
    } else {
        print_u64(n as u64);
    }
}

/// Print an unsigned integer as hex
pub fn print_hex(n: u64) {
    let hex_chars = b"0123456789abcdef";
    let mut buf = [0u8; 16];
    let mut i = 16;
    let mut n = n;

    if n == 0 {
        putchar(b'0');
        return;
    }

    while n > 0 {
        i -= 1;
        buf[i] = hex_chars[(n & 0xF) as usize];
        n >>= 4;
    }

    syscall::sys_write(STDOUT_FILENO, &buf[i..16]);
}

/// Read a line from stdin into buffer
/// Returns number of bytes read (including newline) or 0 on EOF
pub fn getline(buf: &mut [u8]) -> usize {
    let mut count = 0;
    while count < buf.len() - 1 {
        let c = getchar();
        if c < 0 {
            break; // EOF
        }
        buf[count] = c as u8;
        count += 1;
        if c == b'\n' as i32 {
            break;
        }
    }
    buf[count] = 0; // Null terminate
    count
}

/// Convert integer to string
pub fn itoa(n: i64, buf: &mut [u8]) -> usize {
    let mut i = 0;
    let negative = n < 0;
    let mut n = if negative { -n } else { n } as u64;

    if n == 0 {
        buf[0] = b'0';
        buf[1] = 0;
        return 1;
    }

    // Generate digits in reverse order
    let mut tmp = [0u8; 20];
    let mut j = 0;
    while n > 0 {
        tmp[j] = b'0' + (n % 10) as u8;
        n /= 10;
        j += 1;
    }

    // Add negative sign
    if negative {
        buf[i] = b'-';
        i += 1;
    }

    // Reverse digits into buffer
    while j > 0 {
        j -= 1;
        buf[i] = tmp[j];
        i += 1;
    }

    buf[i] = 0;
    i
}

/// Convert string to integer
pub fn atoi(s: &[u8]) -> i64 {
    let mut result: i64 = 0;
    let mut negative = false;
    let mut i = 0;

    // Skip whitespace
    while i < s.len() && (s[i] == b' ' || s[i] == b'\t' || s[i] == b'\n') {
        i += 1;
    }

    // Check for sign
    if i < s.len() {
        if s[i] == b'-' {
            negative = true;
            i += 1;
        } else if s[i] == b'+' {
            i += 1;
        }
    }

    // Convert digits
    while i < s.len() && s[i] >= b'0' && s[i] <= b'9' {
        result = result * 10 + (s[i] - b'0') as i64;
        i += 1;
    }

    if negative { -result } else { result }
}

/// Convert string slice to integer
pub fn parse_int(s: &str) -> Option<i64> {
    let bytes = s.as_bytes();
    if bytes.is_empty() {
        return None;
    }

    let mut i = 0;
    let negative = if bytes[0] == b'-' {
        i = 1;
        true
    } else if bytes[0] == b'+' {
        i = 1;
        false
    } else {
        false
    };

    if i >= bytes.len() {
        return None;
    }

    let mut result: i64 = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if c < b'0' || c > b'9' {
            return None;
        }
        result = result * 10 + (c - b'0') as i64;
        i += 1;
    }

    Some(if negative { -result } else { result })
}
