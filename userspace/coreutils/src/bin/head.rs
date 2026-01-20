//! head - output first part of files

#![no_std]
#![no_main]

use libc::*;

#[unsafe(no_mangle)]
fn main(argc: i32, argv: *const *const u8) -> i32 {
    let mut num_lines = 10i32;
    let mut file_start = 1;

    // Parse arguments
    if argc > 1 {
        let arg1 = unsafe { *argv.add(1) };
        if !arg1.is_null() && unsafe { *arg1 } == b'-' {
            // Check for -n option
            let arg1_str = unsafe { cstr_to_str(arg1) };
            if arg1_str.starts_with("-n") {
                if arg1_str.len() > 2 {
                    // -n10 form
                    if let Some(n) = parse_int(&arg1_str.as_bytes()[2..]) {
                        num_lines = n as i32;
                    }
                } else if argc > 2 {
                    // -n 10 form
                    let arg2 = unsafe { cstr_to_str(*argv.add(2)) };
                    if let Some(n) = parse_int(arg2.as_bytes()) {
                        num_lines = n as i32;
                        file_start = 3;
                    }
                }
                file_start = file_start.max(2);
            } else if let Some(n) = parse_int(&arg1_str.as_bytes()[1..]) {
                // -10 form
                num_lines = n as i32;
                file_start = 2;
            }
        }
    }

    // If no files specified, read from stdin
    if file_start >= argc {
        head_fd(STDIN_FILENO, num_lines);
        return 0;
    }

    // Process files
    let mut status = 0;
    let multiple = (argc - file_start) > 1;

    for i in file_start..argc {
        let path = unsafe { cstr_to_str(*argv.add(i as usize)) };

        if multiple {
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

        head_fd(fd, num_lines);
        close(fd);

        if multiple && i < argc - 1 {
            printlns("");
        }
    }

    status
}

fn head_fd(fd: i32, num_lines: i32) {
    let mut lines = 0;
    let mut buf = [0u8; 4096];

    'outer: loop {
        let n = read(fd, &mut buf);
        if n <= 0 {
            break;
        }

        for i in 0..n as usize {
            putchar(buf[i]);
            if buf[i] == b'\n' {
                lines += 1;
                if lines >= num_lines {
                    break 'outer;
                }
            }
        }
    }
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
