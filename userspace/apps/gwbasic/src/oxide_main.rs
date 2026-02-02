//! OXIDE OS GW-BASIC Entry Point
//!
//! This is the main entry point for the GW-BASIC interpreter when running
//! as a native application on OXIDE OS.

#![no_std]
#![no_main]

extern crate alloc;

use rust_gwbasic::platform::{Console, OxideConsole};
use rust_gwbasic::{Interpreter, Lexer, Parser};

/// Main function called by libc's _start
#[no_mangle]
pub extern "Rust" fn main() -> i32 {
    let mut console = OxideConsole::new();

    console.print("GW-BASIC (Rust) v");
    console.print(rust_gwbasic::VERSION);
    console.print(" for OXIDE OS\n");
    console.print("Type BASIC statements or 'EXIT' to quit\n\n");

    let mut interpreter = Interpreter::new();

    loop {
        console.print("> ");

        let input = console.read_line();
        let input = input.trim();

        if input.is_empty() {
            continue;
        }

        if input.eq_ignore_ascii_case("EXIT") || input.eq_ignore_ascii_case("QUIT") {
            break;
        }

        // Try to tokenize, parse, and execute
        let mut lexer = Lexer::new(input);
        let tokens = match lexer.tokenize() {
            Ok(t) => t,
            Err(e) => {
                console.print("Lexer error: ");
                console.print(&alloc::format!("{:?}\n", e));
                continue;
            }
        };

        let mut parser = Parser::new(tokens);
        let ast = match parser.parse() {
            Ok(a) => a,
            Err(e) => {
                console.print("Parser error: ");
                console.print(&alloc::format!("{:?}\n", e));
                continue;
            }
        };

        if let Err(e) = interpreter.execute(ast) {
            console.print("Runtime error: ");
            console.print(&alloc::format!("{:?}\n", e));
        }
    }

    console.print("Goodbye!\n");
    0
}

// Note: Panic handler is provided by libc crate

// ============================================================================
// WATOS compatibility stubs - redirect to OXIDE libc calls
// These are required because the gwbasic library has extern "C" declarations
// for watos_* functions that must be provided by the executable.
// ============================================================================

/// Console write - called by gwbasic's print macros
#[no_mangle]
pub extern "C" fn watos_console_write(buf: *const u8, len: usize) {
    if buf.is_null() || len == 0 {
        return;
    }
    let slice = unsafe { core::slice::from_raw_parts(buf, len) };
    libc::write(libc::STDOUT_FILENO, slice);
}

/// Console read - called by INPUT$ and INKEY$
#[no_mangle]
pub extern "C" fn watos_console_read(buf: *mut u8, max_len: usize) -> usize {
    if buf.is_null() || max_len == 0 {
        return 0;
    }
    let slice = unsafe { core::slice::from_raw_parts_mut(buf, max_len) };
    let n = libc::read(libc::STDIN_FILENO, slice);
    if n < 0 {
        0
    } else {
        n as usize
    }
}

/// Timer syscall - returns ticks (used by TIMER function)
#[no_mangle]
pub extern "C" fn watos_timer_syscall() -> u64 {
    // Return time of day in "ticks" (1/18.2 sec for GW-BASIC compatibility)
    let t = libc::time::time(None) as u64;
    // Convert seconds to ~18.2 Hz ticks (rough approximation)
    (t % 86400) * 182 / 10
}

/// Get free memory - used by FRE function
#[no_mangle]
pub extern "C" fn watos_get_free_memory() -> usize {
    // Return a reasonable estimate (libc provides the allocator now)
    512 * 1024 // 512KB available
}

/// Get key without waiting - used by INKEY$
#[no_mangle]
pub extern "C" fn watos_get_key_no_wait() -> u8 {
    // Non-blocking read - return 0 if no key available
    // OXIDE doesn't have non-blocking stdin yet, return 0
    0
}

/// Get current date - used by DATE$ function
#[no_mangle]
#[allow(improper_ctypes_definitions)]
pub extern "C" fn watos_get_date() -> (u16, u8, u8) {
    // Get current time and convert to date
    let t = libc::time::time(None);
    let mut tm = libc::time::Tm::default();
    if libc::time::gmtime_r(&t, &mut tm).is_some() {
        let year = (tm.tm_year + 1900) as u16;
        let month = (tm.tm_mon + 1) as u8; // tm_mon is 0-11
        let day = tm.tm_mday as u8;
        (year, month, day)
    } else {
        // Fallback if gmtime fails
        (2025, 1, 1)
    }
}

/// Get current time - used by TIME$ function
#[no_mangle]
#[allow(improper_ctypes_definitions)]
pub extern "C" fn watos_get_time() -> (u8, u8, u8) {
    // Get time from libc
    let t = libc::time::time(None);
    let secs = (t % 86400) as u32;
    let hours = (secs / 3600) as u8;
    let mins = ((secs % 3600) / 60) as u8;
    let secs = (secs % 60) as u8;
    (hours, mins, secs)
}

/// Get cursor row - used by CSRLIN function
#[no_mangle]
pub extern "C" fn watos_get_cursor_row() -> u8 {
    // Placeholder - terminal doesn't expose cursor position
    0
}

/// Get cursor column - used by POS function
#[no_mangle]
pub extern "C" fn watos_get_cursor_col() -> u8 {
    // Placeholder - terminal doesn't expose cursor position
    0
}

/// Get pixel color - used by POINT function
#[no_mangle]
pub extern "C" fn watos_get_pixel(_x: i32, _y: i32) -> u8 {
    // Graphics not implemented in text mode
    0
}

/// File open - used by OPEN statement
#[no_mangle]
pub extern "C" fn watos_file_open(path: *const u8, path_len: usize, mode: u8) -> u64 {
    if path.is_null() || path_len == 0 {
        return u64::MAX; // Error indicator
    }

    // Convert to string
    let path_slice = unsafe { core::slice::from_raw_parts(path, path_len) };
    let path_str = match core::str::from_utf8(path_slice) {
        Ok(s) => s,
        Err(_) => return u64::MAX,
    };

    // Map BASIC mode to flags: 0=input, 1=output, 2=append, 3=random
    let flags = match mode {
        0 => libc::O_RDONLY,
        1 => libc::O_WRONLY | libc::O_CREAT | libc::O_TRUNC,
        2 => libc::O_WRONLY | libc::O_CREAT | libc::O_APPEND,
        3 => libc::O_RDWR | libc::O_CREAT,
        _ => libc::O_RDONLY,
    };

    let fd = libc::open(path_str, flags, 0o644);
    if fd < 0 {
        u64::MAX
    } else {
        fd as u64
    }
}

/// File close - used by CLOSE statement
#[no_mangle]
pub extern "C" fn watos_file_close(handle: u64) {
    if handle < u64::MAX {
        libc::close(handle as i32);
    }
}

/// File read - used by INPUT#, LINE INPUT#
#[no_mangle]
pub extern "C" fn watos_file_read(handle: u64, buf: *mut u8, len: usize) -> usize {
    if handle >= u64::MAX || buf.is_null() || len == 0 {
        return 0;
    }
    let slice = unsafe { core::slice::from_raw_parts_mut(buf, len) };
    let n = libc::read(handle as i32, slice);
    if n < 0 {
        0
    } else {
        n as usize
    }
}

/// File write - used by PRINT#, WRITE#
#[no_mangle]
pub extern "C" fn watos_file_write(handle: u64, buf: *const u8, len: usize) -> usize {
    if handle >= u64::MAX || buf.is_null() || len == 0 {
        return 0;
    }
    let slice = unsafe { core::slice::from_raw_parts(buf, len) };
    let n = libc::write(handle as i32, slice);
    if n < 0 {
        0
    } else {
        n as usize
    }
}

/// File tell - get current position
#[no_mangle]
pub extern "C" fn watos_file_tell(handle: u64) -> u64 {
    if handle >= u64::MAX {
        return 0;
    }
    let pos = libc::lseek(handle as i32, 0, libc::SEEK_CUR);
    if pos < 0 {
        0
    } else {
        pos as u64
    }
}

/// File size - get file size
#[no_mangle]
pub extern "C" fn watos_file_size(handle: u64) -> u64 {
    if handle >= u64::MAX {
        return 0;
    }
    // Save current position
    let cur = libc::lseek(handle as i32, 0, libc::SEEK_CUR);
    // Seek to end
    let size = libc::lseek(handle as i32, 0, libc::SEEK_END);
    // Restore position
    if cur >= 0 {
        libc::lseek(handle as i32, cur, libc::SEEK_SET);
    }
    if size < 0 {
        0
    } else {
        size as u64
    }
}

// Note: Global allocator is provided by libc crate
