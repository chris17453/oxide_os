//! tail - output last part of files

#![no_std]
#![no_main]

use libc::*;

const MAX_LINES: usize = 1000;
const LINE_SIZE: usize = 1024;

#[unsafe(no_mangle)]
fn main(argc: i32, argv: *const *const u8) -> i32 {
    let mut num_lines = 10usize;
    let mut file_start = 1;

    // Parse arguments
    if argc > 1 {
        let arg1 = unsafe { *argv.add(1) };
        if !arg1.is_null() && unsafe { *arg1 } == b'-' {
            let arg1_str = unsafe { cstr_to_str(arg1) };
            if arg1_str.starts_with("-n") {
                if arg1_str.len() > 2 {
                    if let Some(n) = parse_int(&arg1_str.as_bytes()[2..]) {
                        num_lines = n as usize;
                    }
                } else if argc > 2 {
                    let arg2 = unsafe { cstr_to_str(*argv.add(2)) };
                    if let Some(n) = parse_int(arg2.as_bytes()) {
                        num_lines = n as usize;
                        file_start = 3;
                    }
                }
                file_start = file_start.max(2);
            } else if let Some(n) = parse_int(&arg1_str.as_bytes()[1..]) {
                num_lines = n as usize;
                file_start = 2;
            }
        }
    }

    num_lines = num_lines.min(MAX_LINES);

    // If no files specified, read from stdin
    if file_start >= argc {
        tail_fd(STDIN_FILENO, num_lines);
        return 0;
    }

    // Process files
    let mut status = 0;
    let multiple = (argc - file_start) > 1;

    for i in file_start..argc {
        let path = unsafe { cstr_to_str(*argv.add(i as usize)) };

        if multiple {
            print("==> ");
            print(path);
            println(" <==");
        }

        let fd = open2(path, O_RDONLY);
        if fd < 0 {
            eprint("tail: cannot open '");
            print(path);
            eprintln("'");
            status = 1;
            continue;
        }

        tail_fd(fd, num_lines);
        close(fd);

        if multiple && i < argc - 1 {
            println("");
        }
    }

    status
}

fn tail_fd(fd: i32, num_lines: usize) {
    // Buffer to store last N lines
    let mut lines: [[u8; LINE_SIZE]; MAX_LINES] = [[0u8; LINE_SIZE]; MAX_LINES];
    let mut line_lens = [0usize; MAX_LINES];
    let mut line_idx = 0usize;
    let mut total_lines = 0usize;

    let mut buf = [0u8; 4096];
    let mut current_line = [0u8; LINE_SIZE];
    let mut current_len = 0usize;

    loop {
        let n = read(fd, &mut buf);
        if n <= 0 {
            break;
        }

        for i in 0..n as usize {
            if buf[i] == b'\n' {
                // Store complete line
                lines[line_idx][..current_len].copy_from_slice(&current_line[..current_len]);
                line_lens[line_idx] = current_len;
                line_idx = (line_idx + 1) % num_lines.min(MAX_LINES);
                total_lines += 1;
                current_len = 0;
            } else if current_len < LINE_SIZE - 1 {
                current_line[current_len] = buf[i];
                current_len += 1;
            }
        }
    }

    // Handle last line without newline
    if current_len > 0 {
        lines[line_idx][..current_len].copy_from_slice(&current_line[..current_len]);
        line_lens[line_idx] = current_len;
        total_lines += 1;
    }

    // Print collected lines
    let output_lines = total_lines.min(num_lines);
    let start = if total_lines >= num_lines {
        line_idx
    } else {
        0
    };

    for i in 0..output_lines {
        let idx = (start + i) % num_lines.min(MAX_LINES);
        for j in 0..line_lens[idx] {
            putchar(lines[idx][j]);
        }
        putchar(b'\n');
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
