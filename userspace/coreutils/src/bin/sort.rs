//! sort - sort lines of text files

#![no_std]
#![no_main]

use libc::*;

const MAX_LINES: usize = 1024;
const MAX_LINE_LEN: usize = 1024;

#[unsafe(no_mangle)]
fn main(argc: i32, argv: *const *const u8) -> i32 {
    let mut reverse = false;
    let mut numeric = false;
    let mut unique = false;
    let mut arg_idx = 1;

    // Parse flags
    while arg_idx < argc {
        let arg = unsafe { cstr_to_str(*argv.add(arg_idx as usize)) };
        if arg.starts_with('-') && arg.len() > 1 {
            for c in arg.bytes().skip(1) {
                match c {
                    b'r' => reverse = true,
                    b'n' => numeric = true,
                    b'u' => unique = true,
                    _ => {}
                }
            }
            arg_idx += 1;
        } else {
            break;
        }
    }

    // Storage for lines
    let mut lines: [[u8; MAX_LINE_LEN]; MAX_LINES] = [[0; MAX_LINE_LEN]; MAX_LINES];
    let mut line_lens: [usize; MAX_LINES] = [0; MAX_LINES];
    let mut line_count = 0;

    // Read from files or stdin
    if arg_idx >= argc {
        line_count = read_lines(STDIN_FILENO, &mut lines, &mut line_lens, line_count);
    } else {
        for i in arg_idx..argc {
            let path = unsafe { cstr_to_str(*argv.add(i as usize)) };
            let fd = open2(path, O_RDONLY);
            if fd < 0 {
                eprints("sort: ");
                prints(path);
                eprintlns(": No such file");
                continue;
            }
            line_count = read_lines(fd, &mut lines, &mut line_lens, line_count);
            close(fd);
        }
    }

    // Sort the lines using simple bubble sort (good enough for small inputs)
    let mut indices: [usize; MAX_LINES] = [0; MAX_LINES];
    for i in 0..line_count {
        indices[i] = i;
    }

    // Bubble sort with custom comparator
    for i in 0..line_count {
        for j in 0..line_count - i - 1 {
            let cmp = if numeric {
                compare_numeric(&lines[indices[j]][..line_lens[indices[j]]],
                               &lines[indices[j + 1]][..line_lens[indices[j + 1]]])
            } else {
                compare_str(&lines[indices[j]][..line_lens[indices[j]]],
                           &lines[indices[j + 1]][..line_lens[indices[j + 1]]])
            };

            let should_swap = if reverse { cmp < 0 } else { cmp > 0 };
            if should_swap {
                let tmp = indices[j];
                indices[j] = indices[j + 1];
                indices[j + 1] = tmp;
            }
        }
    }

    // Output sorted lines
    let mut last_idx: Option<usize> = None;
    for i in 0..line_count {
        let idx = indices[i];

        // Skip duplicates if -u flag
        if unique {
            if let Some(prev) = last_idx {
                if lines_equal(&lines[prev][..line_lens[prev]],
                              &lines[idx][..line_lens[idx]]) {
                    continue;
                }
            }
        }

        write(STDOUT_FILENO, &lines[idx][..line_lens[idx]]);
        putchar(b'\n');
        last_idx = Some(idx);
    }

    0
}

fn read_lines(fd: i32, lines: &mut [[u8; MAX_LINE_LEN]; MAX_LINES],
              lens: &mut [usize; MAX_LINES], mut count: usize) -> usize {
    let mut buf = [0u8; 4096];
    let mut current_line = [0u8; MAX_LINE_LEN];
    let mut current_len = 0;

    loop {
        let n = read(fd, &mut buf);
        if n <= 0 {
            break;
        }

        for i in 0..n as usize {
            if buf[i] == b'\n' {
                if count < MAX_LINES {
                    lines[count][..current_len].copy_from_slice(&current_line[..current_len]);
                    lens[count] = current_len;
                    count += 1;
                }
                current_len = 0;
            } else if current_len < MAX_LINE_LEN - 1 {
                current_line[current_len] = buf[i];
                current_len += 1;
            }
        }
    }

    // Handle last line without newline
    if current_len > 0 && count < MAX_LINES {
        lines[count][..current_len].copy_from_slice(&current_line[..current_len]);
        lens[count] = current_len;
        count += 1;
    }

    count
}

fn compare_str(a: &[u8], b: &[u8]) -> i32 {
    let min_len = if a.len() < b.len() { a.len() } else { b.len() };
    for i in 0..min_len {
        if a[i] < b[i] {
            return -1;
        }
        if a[i] > b[i] {
            return 1;
        }
    }
    if a.len() < b.len() {
        -1
    } else if a.len() > b.len() {
        1
    } else {
        0
    }
}

fn compare_numeric(a: &[u8], b: &[u8]) -> i32 {
    let na = parse_num(a);
    let nb = parse_num(b);
    if na < nb {
        -1
    } else if na > nb {
        1
    } else {
        0
    }
}

fn parse_num(s: &[u8]) -> i64 {
    let mut result: i64 = 0;
    let mut negative = false;
    let mut started = false;

    for &c in s {
        if c == b'-' && !started {
            negative = true;
            started = true;
        } else if c >= b'0' && c <= b'9' {
            result = result * 10 + (c - b'0') as i64;
            started = true;
        } else if started {
            break;
        }
    }

    if negative { -result } else { result }
}

fn lines_equal(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    for i in 0..a.len() {
        if a[i] != b[i] {
            return false;
        }
    }
    true
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
