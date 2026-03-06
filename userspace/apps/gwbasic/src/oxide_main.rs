//! OXIDE OS GW-BASIC Entry Point
//!
//! This is the main entry point for the GW-BASIC interpreter when running
//! as a native application on OXIDE OS.

#![no_std]
#![no_main]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use rust_gwbasic::platform::{Console, OxideConsole};
use rust_gwbasic::{Interpreter, Lexer, Parser};

/// Main function called by libc's _start
#[no_mangle]
pub extern "Rust" fn main(argc: i32, argv: *const *const u8) -> i32 {
    let mut console = OxideConsole::new();

    // Parse command line arguments
    let mut filename: Option<String> = None;
    let mut use_graphics = false;

    if argc > 1 && !argv.is_null() {
        for i in 1..argc {
            let arg_ptr = unsafe { *argv.offset(i as isize) };
            if arg_ptr.is_null() {
                continue;
            }

            // Convert C string to Rust string
            let mut len = 0;
            unsafe {
                while *arg_ptr.offset(len) != 0 {
                    len += 1;
                }
            }
            let arg_slice = unsafe { core::slice::from_raw_parts(arg_ptr, len as usize) };
            if let Ok(arg) = core::str::from_utf8(arg_slice) {
                if arg == "--gui" || arg == "-g" {
                    use_graphics = true;
                } else if arg == "--help" || arg == "-h" {
                    print_usage(&mut console);
                    return 0;
                } else if !arg.starts_with('-') && filename.is_none() {
                    filename = Some(String::from(arg));
                }
            }
        }
    }

    // If a filename is provided, run it
    if let Some(ref file) = filename {
        return run_file(&mut console, file, use_graphics);
    }

    // Otherwise, start REPL
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

fn print_usage(console: &mut OxideConsole) {
    console.print("GW-BASIC (Rust) v");
    console.print(rust_gwbasic::VERSION);
    console.print(" for OXIDE OS\n\n");
    console.print("USAGE:\n");
    console.print("  gwbasic [OPTIONS] [FILE]\n\n");
    console.print("OPTIONS:\n");
    console.print("  -g, --gui      Use graphics mode (framebuffer)\n");
    console.print("  -h, --help     Show this help message\n\n");
    console.print("EXAMPLES:\n");
    console.print("  gwbasic                           Start REPL\n");
    console.print("  gwbasic program.bas               Run program\n");
    console.print("  gwbasic --gui program.bas         Run with graphics\n");
}

fn run_file(console: &mut OxideConsole, filename: &str, use_graphics: bool) -> i32 {
    // Read the file
    let fd = libc::open(filename, libc::O_RDONLY, 0);
    if fd < 0 {
        console.print("Error: Cannot open file '");
        console.print(filename);
        console.print("'\n");
        return 1;
    }

    // Get file size via stat
    let mut stat_buf = libc::Stat::zeroed();
    if libc::fstat(fd, &mut stat_buf) < 0 {
        libc::close(fd);
        console.print("Error: Cannot stat file '");
        console.print(filename);
        console.print("'\n");
        return 1;
    }

    let file_size = stat_buf.size as usize;
    if file_size == 0 {
        libc::close(fd);
        console.print("Error: File is empty\n");
        return 1;
    }

    // Read file contents
    let mut content: Vec<u8> = alloc::vec![0u8; file_size];
    let bytes_read = libc::read(fd, &mut content);
    libc::close(fd);

    if bytes_read < 0 {
        console.print("Error: Cannot read file '");
        console.print(filename);
        console.print("'\n");
        return 1;
    }

    // Convert to string
    let content_str = match core::str::from_utf8(&content[..bytes_read as usize]) {
        Ok(s) => String::from(s),
        Err(_) => {
            console.print("Error: File contains invalid UTF-8\n");
            return 1;
        }
    };

    // — GlassSignal: if --gui was requested, fire up the framebuffer backend.
    // Otherwise fall back to text-only mode like a civilized terminal app.
    let mut interpreter = if use_graphics {
        match Interpreter::new_with_gui() {
            Ok(interp) => interp,
            Err(_) => {
                console.print("Warning: Graphics init failed, falling back to text mode\n");
                Interpreter::new()
            }
        }
    } else {
        Interpreter::new()
    };

    // Tokenize
    let mut lexer = Lexer::new(&content_str);
    let tokens = match lexer.tokenize() {
        Ok(t) => t,
        Err(e) => {
            console.print("Lexer error: ");
            console.print(&alloc::format!("{:?}\n", e));
            return 1;
        }
    };

    // Parse
    let mut parser = Parser::new(tokens);
    let ast = match parser.parse() {
        Ok(a) => a,
        Err(e) => {
            console.print("Parser error: ");
            console.print(&alloc::format!("{:?}\n", e));
            return 1;
        }
    };

    // Execute (this loads line-numbered programs)
    if let Err(e) = interpreter.execute(ast) {
        console.print("Runtime error: ");
        console.print(&alloc::format!("{:?}\n", e));
        return 1;
    }

    // If the program had line numbers, run it now
    if let Err(e) = interpreter.run_stored_program() {
        console.print("Runtime error: ");
        console.print(&alloc::format!("{:?}\n", e));
        return 1;
    }

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
