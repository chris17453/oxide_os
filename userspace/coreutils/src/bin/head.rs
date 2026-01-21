//! head - output first part of files
//!
//! Full-featured implementation with:
//! - Line mode (-n, default 10 lines)
//! - Byte mode (-c)
//! - Quiet mode (-q, never print headers)
//! - Verbose mode (-v, always print headers)
//! - Zero-terminated lines (-z)
//! - Multiple file support
//! - Stdin support
//! - Proper argument parsing

#![no_std]
#![no_main]

use libc::*;

struct HeadConfig {
    count: i64,
    bytes: bool,
    quiet: bool,
    verbose: bool,
    zero_terminated: bool,
}

impl HeadConfig {
    fn new() -> Self {
        HeadConfig {
            count: 10,
            bytes: false,
            quiet: false,
            verbose: false,
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

fn parse_int(s: &[u8]) -> Option<i64> {
    let mut result: i64 = 0;
    for &c in s {
        if c < b'0' || c > b'9' {
            return None;
        }
        result = result * 10 + (c - b'0') as i64;
    }
    if s.is_empty() { None } else { Some(result) }
}

fn show_help() {
    eprintlns("Usage: head [OPTIONS] [FILE...]");
    eprintlns("");
    eprintlns("Print first 10 lines (or N with -n) of each FILE to stdout.");
    eprintlns("");
    eprintlns("Options:");
    eprintlns("  -c NUM       Print first NUM bytes");
    eprintlns("  -n NUM       Print first NUM lines (default: 10)");
    eprintlns("  -NUM         Same as -n NUM");
    eprintlns("  -q           Never print headers");
    eprintlns("  -v           Always print headers");
    eprintlns("  -z           Line delimiter is NUL, not newline");
    eprintlns("  -h           Show this help");
}

#[unsafe(no_mangle)]
fn main(argc: i32, argv: *const *const u8) -> i32 {
    if argc > 1 {
        let arg = cstr_to_str(unsafe { *argv.add(1) });
        if arg == "-h" || arg == "--help" {
            show_help();
            return 0;
        }
    }

    let mut config = HeadConfig::new();
    let mut arg_idx = 1;

    // Parse options
    while arg_idx < argc {
        let arg = unsafe { cstr_to_str(*argv.add(arg_idx as usize)) };

        if str_starts_with(arg, "-") && arg.len() > 1 && arg != "--" {
            if arg == "-n" || arg == "-c" {
                // -n NUM or -c NUM form
                arg_idx += 1;
                if arg_idx >= argc {
                    eprintlns("head: option requires an argument");
                    return 1;
                }
                let num_str = unsafe { cstr_to_str(*argv.add(arg_idx as usize)) };
                match parse_int(num_str.as_bytes()) {
                    Some(n) => {
                        config.count = n;
                        if arg == "-c" {
                            config.bytes = true;
                        }
                    }
                    None => {
                        eprints("head: invalid number: ");
                        prints(num_str);
                        eprintlns("");
                        return 1;
                    }
                }
                arg_idx += 1;
            } else if str_starts_with(arg, "-n") && arg.len() > 2 {
                // -n10 form
                match parse_int(&arg.as_bytes()[2..]) {
                    Some(n) => config.count = n,
                    None => {
                        eprints("head: invalid number: ");
                        prints(&arg[2..]);
                        eprintlns("");
                        return 1;
                    }
                }
                arg_idx += 1;
            } else if str_starts_with(arg, "-c") && arg.len() > 2 {
                // -c100 form
                match parse_int(&arg.as_bytes()[2..]) {
                    Some(n) => {
                        config.count = n;
                        config.bytes = true;
                    }
                    None => {
                        eprints("head: invalid number: ");
                        prints(&arg[2..]);
                        eprintlns("");
                        return 1;
                    }
                }
                arg_idx += 1;
            } else if arg.as_bytes()[1] >= b'0' && arg.as_bytes()[1] <= b'9' {
                // -10 form
                match parse_int(&arg.as_bytes()[1..]) {
                    Some(n) => config.count = n,
                    None => {
                        eprints("head: invalid number: ");
                        prints(&arg[1..]);
                        eprintlns("");
                        return 1;
                    }
                }
                arg_idx += 1;
            } else {
                // Single-char flags
                for c in arg.bytes().skip(1) {
                    match c {
                        b'q' => config.quiet = true,
                        b'v' => config.verbose = true,
                        b'z' => config.zero_terminated = true,
                        b'h' => {
                            show_help();
                            return 0;
                        }
                        _ => {
                            eprints("head: invalid option: -");
                            putchar(c);
                            eprintlns("");
                            return 1;
                        }
                    }
                }
                arg_idx += 1;
            }
        } else {
            break;
        }
    }

    // If no files specified, read from stdin
    if arg_idx >= argc {
        head_fd(&config, STDIN_FILENO);
        return 0;
    }

    // Process files
    let mut status = 0;
    let file_count = argc - arg_idx;
    let show_headers = if config.quiet {
        false
    } else if config.verbose {
        true
    } else {
        file_count > 1
    };

    for i in arg_idx..argc {
        let path = unsafe { cstr_to_str(*argv.add(i as usize)) };

        if show_headers {
            if i > arg_idx {
                printlns("");
            }
            prints("==> ");
            prints(path);
            printlns(" <==");
        }

        let fd = open2(path, O_RDONLY);
        if fd < 0 {
            eprints("head: cannot open '");
            prints(path);
            eprintlns("'");
            status = 1;
            continue;
        }

        head_fd(&config, fd);
        close(fd);
    }

    status
}

fn head_fd(config: &HeadConfig, fd: i32) {
    if config.bytes {
        head_bytes(config, fd);
    } else {
        head_lines(config, fd);
    }
}

fn head_bytes(config: &HeadConfig, fd: i32) {
    let mut remaining = config.count;
    let mut buf = [0u8; 4096];

    while remaining > 0 {
        let to_read = if remaining < buf.len() as i64 {
            remaining as usize
        } else {
            buf.len()
        };

        let n = read(fd, &mut buf[..to_read]);
        if n <= 0 {
            break;
        }

        for i in 0..n as usize {
            putchar(buf[i]);
        }

        remaining -= n as i64;
    }
}

fn head_lines(config: &HeadConfig, fd: i32) {
    let mut count = 0i64;
    let mut buf = [0u8; 4096];
    let delimiter = if config.zero_terminated { b'\0' } else { b'\n' };

    'outer: loop {
        let n = read(fd, &mut buf);
        if n <= 0 {
            break;
        }

        for i in 0..n as usize {
            putchar(buf[i]);
            if buf[i] == delimiter {
                count += 1;
                if count >= config.count {
                    break 'outer;
                }
            }
        }
    }
}
