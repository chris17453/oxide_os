//! tee - read from stdin and write to stdout and files
//!
//! Full-featured implementation with:
//! - Append mode (-a)
//! - Ignore interrupts (-i)
//! - Output error handling (-p)
//! - Multiple file support (up to 32 files)
//! - Proper error reporting
//! - Signal handling
//! - Help message

#![no_std]
#![no_main]

use libc::*;

const MAX_FILES: usize = 32;

struct TeeConfig {
    append: bool,
    ignore_interrupts: bool,
    output_error: bool,
}

impl TeeConfig {
    fn new() -> Self {
        TeeConfig {
            append: false,
            ignore_interrupts: false,
            output_error: false,
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
    eprintlns("Usage: tee [OPTIONS] [FILE...]");
    eprintlns("");
    eprintlns("Copy standard input to each FILE, and also to standard output.");
    eprintlns("");
    eprintlns("Options:");
    eprintlns("  -a              Append to the given FILEs, do not overwrite");
    eprintlns("  -i              Ignore interrupt signals");
    eprintlns("  -p              Diagnose errors writing to non-pipes");
    eprintlns("  --output-error  Same as -p");
    eprintlns("  -h              Show this help");
}

#[unsafe(no_mangle)]
fn main(argc: i32, argv: *const *const u8) -> i32 {
    let mut config = TeeConfig::new();
    let mut arg_idx = 1;

    // Parse options
    while arg_idx < argc {
        let arg = unsafe { cstr_to_str(*argv.add(arg_idx as usize)) };

        if arg == "-h" || arg == "--help" {
            show_help();
            return 0;
        } else if arg == "--output-error" {
            config.output_error = true;
            arg_idx += 1;
        } else if str_starts_with(arg, "-") && arg.len() > 1 && arg != "--" {
            for c in arg.bytes().skip(1) {
                match c {
                    b'a' => config.append = true,
                    b'i' => config.ignore_interrupts = true,
                    b'p' => config.output_error = true,
                    b'h' => {
                        show_help();
                        return 0;
                    }
                    _ => {
                        eprints("tee: invalid option: -");
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

    // Ignore interrupt signals if requested
    if config.ignore_interrupts {
        // Set SIGINT handler to ignore
        signal(SIGINT, SIG_IGN);
    }

    // Open output files
    let mut fds = [-1i32; MAX_FILES];
    let mut paths: [[u8; 256]; MAX_FILES] = [[0u8; 256]; MAX_FILES];
    let mut path_lens = [0usize; MAX_FILES];
    let mut num_fds = 0;
    let mut had_errors = false;

    for i in arg_idx..argc {
        if num_fds >= MAX_FILES {
            eprintlns("tee: too many files (maximum 32)");
            break;
        }

        let path = unsafe { cstr_to_str(*argv.add(i as usize)) };
        let flags = if config.append {
            O_WRONLY | O_CREAT | O_APPEND
        } else {
            O_WRONLY | O_CREAT | O_TRUNC
        };

        let fd = open(path, flags, 0o644);
        if fd < 0 {
            eprints("tee: ");
            prints(path);
            eprintlns(": cannot open for writing");
            had_errors = true;
        } else {
            // Store path for error reporting
            let copy_len = path.len().min(255);
            paths[num_fds][..copy_len].copy_from_slice(&path.as_bytes()[..copy_len]);
            path_lens[num_fds] = copy_len;

            fds[num_fds] = fd;
            num_fds += 1;
        }
    }

    // Track which FDs are still valid
    let mut fd_valid = [true; MAX_FILES];

    // Read from stdin, write to stdout and all files
    let mut buf = [0u8; 8192];
    loop {
        let n = read(STDIN_FILENO, &mut buf);
        if n <= 0 {
            break;
        }

        let bytes_read = n as usize;

        // Write to stdout
        let stdout_result = write(STDOUT_FILENO, &buf[..bytes_read]);
        if stdout_result < 0 || stdout_result != n {
            if config.output_error {
                eprintlns("tee: write error to stdout");
            }
            // Don't exit on stdout write error, continue writing to files
        }

        // Write to all open files
        for i in 0..num_fds {
            if !fd_valid[i] {
                continue; // Skip files that have failed
            }

            let write_result = write(fds[i], &buf[..bytes_read]);
            if write_result < 0 || write_result != n {
                // Write failed
                if config.output_error {
                    eprints("tee: write error to '");
                    let path_str =
                        unsafe { core::str::from_utf8_unchecked(&paths[i][..path_lens[i]]) };
                    prints(path_str);
                    eprintlns("'");
                }

                // Mark this FD as invalid and close it
                fd_valid[i] = false;
                close(fds[i]);
                fds[i] = -1;
                had_errors = true;
            }
        }
    }

    // Close all remaining open files
    for i in 0..num_fds {
        if fds[i] >= 0 {
            close(fds[i]);
        }
    }

    if had_errors { 1 } else { 0 }
}
