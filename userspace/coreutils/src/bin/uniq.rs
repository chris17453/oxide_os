//! uniq - report or filter out repeated lines

#![no_std]
#![no_main]

use libc::*;

const MAX_LINE: usize = 4096;

#[unsafe(no_mangle)]
fn main(argc: i32, argv: *const *const u8) -> i32 {
    let mut count = false;
    let mut repeated = false;
    let mut unique_only = false;
    let mut ignore_case = false;
    let mut arg_idx = 1;

    // Parse flags
    while arg_idx < argc {
        let arg = unsafe { cstr_to_str(*argv.add(arg_idx as usize)) };
        if arg.starts_with('-') && arg.len() > 1 {
            for c in arg.bytes().skip(1) {
                match c {
                    b'c' => count = true,
                    b'd' => repeated = true,
                    b'u' => unique_only = true,
                    b'i' => ignore_case = true,
                    _ => {}
                }
            }
            arg_idx += 1;
        } else {
            break;
        }
    }

    // Open input
    let fd = if arg_idx < argc {
        let path = unsafe { cstr_to_str(*argv.add(arg_idx as usize)) };
        let fd = open2(path, O_RDONLY);
        if fd < 0 {
            eprint("uniq: ");
            print(path);
            eprintln(": No such file");
            return 1;
        }
        fd
    } else {
        STDIN_FILENO
    };

    let mut buf = [0u8; 4096];
    let mut prev_line = [0u8; MAX_LINE];
    let mut prev_len = 0;
    let mut curr_line = [0u8; MAX_LINE];
    let mut curr_len = 0;
    let mut repeat_count = 0u64;
    let mut has_prev = false;

    loop {
        let n = read(fd, &mut buf);
        if n <= 0 {
            break;
        }

        for i in 0..n as usize {
            if buf[i] == b'\n' {
                if has_prev {
                    let same = lines_equal(&prev_line[..prev_len], &curr_line[..curr_len], ignore_case);
                    if same {
                        repeat_count += 1;
                    } else {
                        output_line(&prev_line[..prev_len], repeat_count, count, repeated, unique_only);
                        // Copy current to previous
                        prev_line[..curr_len].copy_from_slice(&curr_line[..curr_len]);
                        prev_len = curr_len;
                        repeat_count = 1;
                    }
                } else {
                    // First line
                    prev_line[..curr_len].copy_from_slice(&curr_line[..curr_len]);
                    prev_len = curr_len;
                    repeat_count = 1;
                    has_prev = true;
                }
                curr_len = 0;
            } else if curr_len < MAX_LINE - 1 {
                curr_line[curr_len] = buf[i];
                curr_len += 1;
            }
        }
    }

    // Handle last line without newline
    if curr_len > 0 {
        if has_prev {
            let same = lines_equal(&prev_line[..prev_len], &curr_line[..curr_len], ignore_case);
            if same {
                repeat_count += 1;
            } else {
                output_line(&prev_line[..prev_len], repeat_count, count, repeated, unique_only);
                prev_line[..curr_len].copy_from_slice(&curr_line[..curr_len]);
                prev_len = curr_len;
                repeat_count = 1;
            }
        } else {
            prev_line[..curr_len].copy_from_slice(&curr_line[..curr_len]);
            prev_len = curr_len;
            repeat_count = 1;
            has_prev = true;
        }
    }

    // Output last line
    if has_prev {
        output_line(&prev_line[..prev_len], repeat_count, count, repeated, unique_only);
    }

    if fd != STDIN_FILENO {
        close(fd);
    }

    0
}

fn output_line(line: &[u8], count_val: u64, show_count: bool, repeated: bool, unique_only: bool) {
    // -d: only show repeated lines
    if repeated && count_val < 2 {
        return;
    }
    // -u: only show unique lines
    if unique_only && count_val > 1 {
        return;
    }

    if show_count {
        print_u64_padded(count_val, 7);
        print(" ");
    }

    for &b in line {
        putchar(b);
    }
    putchar(b'\n');
}

fn print_u64_padded(n: u64, width: usize) {
    let mut buf = [b' '; 20];
    let mut val = n;
    let mut pos = 19;

    loop {
        buf[pos] = b'0' + (val % 10) as u8;
        val /= 10;
        if val == 0 {
            break;
        }
        pos -= 1;
    }

    let num_len = 20 - pos;
    let start = if num_len < width { width - num_len } else { 0 };

    for i in 0..start {
        putchar(b' ');
    }
    for i in pos..20 {
        putchar(buf[i]);
    }
}

fn lines_equal(a: &[u8], b: &[u8], ignore_case: bool) -> bool {
    if a.len() != b.len() {
        return false;
    }
    for i in 0..a.len() {
        let ca = if ignore_case { to_lower(a[i]) } else { a[i] };
        let cb = if ignore_case { to_lower(b[i]) } else { b[i] };
        if ca != cb {
            return false;
        }
    }
    true
}

fn to_lower(c: u8) -> u8 {
    if c >= b'A' && c <= b'Z' {
        c + 32
    } else {
        c
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
