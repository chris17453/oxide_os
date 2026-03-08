//! Standard I/O
//!
//! Basic printf-like formatting and I/O functions.
//!
//! 🔥 PERFORMANCE: Buffered I/O for stdout 🔥
//! All writes to stdout are buffered to reduce syscalls.
//! Buffer is auto-flushed on newline or when full.

extern crate alloc;
use crate::fcntl::*;
use crate::syscall;
use alloc::vec::Vec;
use core::fmt::{self, Write};
use core::sync::atomic::{AtomicBool, Ordering};

/// Stdout buffer (8KB capacity)
/// 🔥 GraveShift: Buffered I/O cuts syscalls by 100x - essential for performance 🔥
static mut STDOUT_BUFFER: Option<Vec<u8>> = None;
static STDOUT_BUFFER_INITIALIZED: AtomicBool = AtomicBool::new(false);

/// Initialize stdout buffer (called automatically on first use)
fn init_stdout_buffer() {
    if !STDOUT_BUFFER_INITIALIZED.load(Ordering::Relaxed) {
        unsafe {
            STDOUT_BUFFER = Some(Vec::with_capacity(8192));
        }
        STDOUT_BUFFER_INITIALIZED.store(true, Ordering::Relaxed);
    }
}

/// Flush stdout buffer (write accumulated bytes to stdout)
/// 🔥 GraveShift: Batch syscalls - one write is 100x faster than 100 writes 🔥
pub fn fflush_stdout() {
    init_stdout_buffer();
    unsafe {
        if let Some(ref mut buf) = STDOUT_BUFFER {
            if !buf.is_empty() {
                syscall::sys_write(STDOUT_FILENO, buf.as_slice());
                buf.clear();
            }
        }
    }
}

/// Flush all stdio buffers (currently only stdout)
/// Matches standard C library fflush(NULL)
pub fn fflush_all() {
    fflush_stdout();
}

/// Writer for stdout
/// 🔥 GraveShift: Buffered stdout writer for Rust fmt macros 🔥
pub struct StdoutWriter;

impl Write for StdoutWriter {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        init_stdout_buffer();
        unsafe {
            if let Some(ref mut buf) = STDOUT_BUFFER {
                buf.extend_from_slice(s.as_bytes());

                // Flush on newline or small threshold for responsiveness
                if s.contains('\n') || buf.len() >= 256 {
                    fflush_stdout();
                }
            }
        }
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
/// 🔥 GraveShift: Smart buffering - flush on newline or threshold 🔥
pub fn print(s: &str) {
    init_stdout_buffer();
    unsafe {
        if let Some(ref mut buf) = STDOUT_BUFFER {
            buf.extend_from_slice(s.as_bytes());

            // Flush if newline or buffer getting full
            if s.contains('\n') || buf.len() >= 256 {
                fflush_stdout();
            }
        }
    }
}

/// Alias for `print` - prints a string to stdout
pub fn prints(s: &str) {
    print(s);
}

/// Print a string to stdout with newline (use `printlns` to avoid macro conflict)
/// 🔥 GraveShift: Flush on newline for interactive responsiveness 🔥
pub fn println(s: &str) {
    init_stdout_buffer();
    unsafe {
        if let Some(ref mut buf) = STDOUT_BUFFER {
            buf.extend_from_slice(s.as_bytes());
            buf.push(b'\n');
            fflush_stdout(); // Always flush on newline
        }
    }
}

/// Alias for `println` - prints a string to stdout with newline
pub fn printlns(s: &str) {
    println(s);
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
/// 🔥 GraveShift: Smart buffering - flush on newline, ESC, or small threshold 🔥
pub fn putchar(c: u8) {
    init_stdout_buffer();
    unsafe {
        if let Some(ref mut buf) = STDOUT_BUFFER {
            buf.push(c);

            // Flush on newline for interactive output
            // Flush on ESC (0x1b) for terminal escape sequences (vim needs this)
            // Flush every 256 bytes for responsiveness
            if c == b'\n' || c == 0x1b || buf.len() >= 256 {
                fflush_stdout();
            }
        }
    }
}

/// Read a character from stdin
/// — GraveShift: must retry on EINTR (-4). Signal delivery during blocking
/// read returns -EINTR from the kernel. Without retry, readline sees EOF
/// and the shell exits on every stray SIGCHLD. Classic signal-vs-read bug.
pub fn getchar() -> i32 {
    let mut buf = [0u8; 1];
    let ret = loop {
        let r = syscall::sys_read(STDIN_FILENO, &mut buf);
        if r != -4 { break r; } // -4 = EINTR, retry
    };
    if ret <= 0 {
        // — GraveShift: Log the actual error code so we can see what's killing readline.
        // Without this, every non-EINTR error silently becomes EOF and the shell dies.
        let _ = syscall::sys_write(2, b"[GETCHAR] err=");
        let code = if ret < 0 { (-ret) as u8 } else { 0 };
        let digits = [b'0' + (code / 100) % 10, b'0' + (code / 10) % 10, b'0' + code % 10];
        let _ = syscall::sys_write(2, &digits);
        let _ = syscall::sys_write(2, b"\n");
        -1
    } else {
        let byte = buf[0];
        #[cfg(feature = "debug-readline")]
        {
            // Debug output for non-printable characters
            if byte < 32 || byte > 126 {
                let _ = syscall::sys_write(2, b"[GETCHAR] 0x");
                // Simple hex output
                let nibbles = [(byte >> 4) & 0xF, byte & 0xF];
                for &nib in &nibbles {
                    let hex_char = if nib < 10 {
                        b'0' + nib
                    } else {
                        b'a' + (nib - 10)
                    };
                    let _ = syscall::sys_write(2, &[hex_char]);
                }
                let _ = syscall::sys_write(2, b"\n");
            }
        }
        byte as i32
    }
}

/// Print a null-terminated string with newline (C puts function)
/// 🔥 GraveShift: Buffered puts - C programs get free performance boost 🔥
pub unsafe fn puts(s: *const u8) -> i32 {
    if s.is_null() {
        return -1;
    }

    init_stdout_buffer();

    // Find string length
    let mut len = 0;
    while *s.add(len) != 0 {
        len += 1;
    }

    // Write string to buffer
    let slice = core::slice::from_raw_parts(s, len);
    if let Some(ref mut buf) = STDOUT_BUFFER {
        buf.extend_from_slice(slice);
        buf.push(b'\n');
        fflush_stdout(); // Always flush on newline
    }

    0 // Success
}

/// Print an unsigned integer
/// 🔥 GraveShift: Buffered integer printing - batch digits into single write 🔥
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

    print(core::str::from_utf8(&buf[i..20]).unwrap_or(""));
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
/// 🔥 GraveShift: Hex printing goes to buffer - no direct syscalls 🔥
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

    print(core::str::from_utf8(&buf[i..16]).unwrap_or(""));
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
