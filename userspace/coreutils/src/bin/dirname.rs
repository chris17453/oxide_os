//! dirname - strip last component from filename
//!
//! Full-featured implementation with:
//! - Single file mode (default)
//! - Multiple file mode
//! - Zero-terminated output (-z)
//! - Help message (-h)
//! - Proper trailing slash handling
//! - Proper error handling

#![no_std]
#![no_main]

use libc::*;

struct DirnameConfig {
    zero_terminated: bool,
}

impl DirnameConfig {
    fn new() -> Self {
        DirnameConfig {
            zero_terminated: false,
        }
    }
}

fn cstr_to_str(ptr: *const u8) -> &'static str {
    if ptr.is_null() {
        return "";
    }
    let mut len = 0;
    unsafe {
        while *ptr.add(len) != 0 {
            len += 1;
        }
        core::str::from_utf8_unchecked(core::slice::from_raw_parts(ptr, len))
    }
}

fn str_starts_with(s: &str, prefix: &str) -> bool {
    if s.len() < prefix.len() {
        return false;
    }
    let s_bytes = s.as_bytes();
    let p_bytes = prefix.as_bytes();
    for i in 0..prefix.len() {
        if s_bytes[i] != p_bytes[i] {
            return false;
        }
    }
    true
}

fn show_help() {
    eprintlns("Usage: dirname [OPTION] NAME...");
    eprintlns("");
    eprintlns("Output each NAME with its last non-slash component and trailing slashes");
    eprintlns("removed; if NAME contains no /'s, output '.' (meaning the current directory).");
    eprintlns("");
    eprintlns("Options:");
    eprintlns("  -z              End each output line with NUL, not newline");
    eprintlns("  -h              Show this help");
}

#[unsafe(no_mangle)]
fn main(argc: i32, argv: *const *const u8) -> i32 {
    let mut config = DirnameConfig::new();
    let mut arg_idx = 1;

    // Parse options
    while arg_idx < argc {
        let arg = unsafe { cstr_to_str(*argv.add(arg_idx as usize)) };

        if arg == "-h" || arg == "--help" {
            show_help();
            return 0;
        } else if arg == "-z" || arg == "--zero" {
            config.zero_terminated = true;
            arg_idx += 1;
        } else if str_starts_with(arg, "-") && arg.len() > 1 && arg != "--" {
            // Handle combined short options
            for c in arg.bytes().skip(1) {
                match c {
                    b'z' => config.zero_terminated = true,
                    b'h' => {
                        show_help();
                        return 0;
                    }
                    _ => {
                        eprints("dirname: invalid option: -");
                        putchar(c);
                        eprintlns("");
                        return 1;
                    }
                }
            }
            arg_idx += 1;
        } else {
            break;
        }
    }

    // Check for arguments
    if arg_idx >= argc {
        eprintlns("dirname: missing operand");
        eprintlns("Try 'dirname -h' for more information.");
        return 1;
    }

    let delimiter = if config.zero_terminated { b'\0' } else { b'\n' };

    // Process each path argument
    for i in arg_idx..argc {
        let path = unsafe { cstr_to_str(*argv.add(i as usize)) };
        print_dirname(path);
        write(STDOUT_FILENO, &[delimiter]);
    }

    0
}

fn print_dirname(path: &str) {
    let bytes = path.as_bytes();

    // Handle empty path
    if bytes.is_empty() {
        write(STDOUT_FILENO, b".");
        return;
    }

    // Remove trailing slashes
    let mut end = bytes.len();
    while end > 1 && bytes[end - 1] == b'/' {
        end -= 1;
    }

    // Special case: if path is all slashes, return "/"
    if end == 1 && bytes[0] == b'/' {
        write(STDOUT_FILENO, b"/");
        return;
    }

    // Find last / before the trailing slashes
    let mut last_slash = None;
    for i in 0..end {
        if bytes[i] == b'/' {
            last_slash = Some(i);
        }
    }

    match last_slash {
        None => {
            // No slash found, return "."
            write(STDOUT_FILENO, b".");
        }
        Some(0) => {
            // Slash at position 0, return "/"
            write(STDOUT_FILENO, b"/");
        }
        Some(pos) => {
            // Remove trailing slashes from the dirname part
            let mut dirname_end = pos;
            while dirname_end > 1 && bytes[dirname_end - 1] == b'/' {
                dirname_end -= 1;
            }

            // Special case: if we end up with just "/", return "/"
            if dirname_end == 1 && bytes[0] == b'/' {
                write(STDOUT_FILENO, b"/");
            } else {
                write(STDOUT_FILENO, &bytes[..dirname_end]);
            }
        }
    }
}
