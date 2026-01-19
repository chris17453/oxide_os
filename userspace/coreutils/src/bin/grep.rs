//! grep - search for patterns in files

#![no_std]
#![no_main]

use libc::*;

const MAX_LINE: usize = 4096;

#[unsafe(no_mangle)]
fn main(argc: i32, argv: *const *const u8) -> i32 {
    if argc < 2 {
        eprintln("usage: grep [-i] [-v] [-n] <pattern> [file...]");
        return 2;
    }

    let mut ignore_case = false;
    let mut invert = false;
    let mut line_numbers = false;
    let mut arg_idx = 1;

    // Parse flags
    while arg_idx < argc {
        let arg = unsafe { cstr_to_str(*argv.add(arg_idx as usize)) };
        if arg.starts_with('-') && arg.len() > 1 {
            for c in arg.bytes().skip(1) {
                match c {
                    b'i' => ignore_case = true,
                    b'v' => invert = true,
                    b'n' => line_numbers = true,
                    _ => {}
                }
            }
            arg_idx += 1;
        } else {
            break;
        }
    }

    if arg_idx >= argc {
        eprintln("grep: no pattern specified");
        return 2;
    }

    let pattern = unsafe { cstr_to_str(*argv.add(arg_idx as usize)) };
    arg_idx += 1;

    let mut found_any = false;

    // If no files, read from stdin
    if arg_idx >= argc {
        found_any = grep_fd(STDIN_FILENO, pattern, "", ignore_case, invert, line_numbers, false);
    } else {
        let multiple = (argc - arg_idx) > 1;

        for i in arg_idx..argc {
            let path = unsafe { cstr_to_str(*argv.add(i as usize)) };
            let fd = open2(path, O_RDONLY);
            if fd < 0 {
                eprint("grep: ");
                print(path);
                eprintln(": No such file");
                continue;
            }

            if grep_fd(fd, pattern, path, ignore_case, invert, line_numbers, multiple) {
                found_any = true;
            }
            close(fd);
        }
    }

    if found_any { 0 } else { 1 }
}

fn grep_fd(fd: i32, pattern: &str, filename: &str, ignore_case: bool, invert: bool, line_numbers: bool, show_filename: bool) -> bool {
    let mut buf = [0u8; 4096];
    let mut line = [0u8; MAX_LINE];
    let mut line_len = 0;
    let mut line_num = 0u64;
    let mut found = false;

    loop {
        let n = read(fd, &mut buf);
        if n <= 0 {
            break;
        }

        for i in 0..n as usize {
            if buf[i] == b'\n' {
                line_num += 1;
                let matches = contains(&line[..line_len], pattern, ignore_case);
                let should_print = if invert { !matches } else { matches };

                if should_print {
                    found = true;
                    if show_filename {
                        print(filename);
                        print(":");
                    }
                    if line_numbers {
                        print_u64(line_num);
                        print(":");
                    }
                    for j in 0..line_len {
                        putchar(line[j]);
                    }
                    putchar(b'\n');
                }
                line_len = 0;
            } else if line_len < MAX_LINE - 1 {
                line[line_len] = buf[i];
                line_len += 1;
            }
        }
    }

    // Handle last line without newline
    if line_len > 0 {
        line_num += 1;
        let matches = contains(&line[..line_len], pattern, ignore_case);
        let should_print = if invert { !matches } else { matches };

        if should_print {
            found = true;
            if show_filename {
                print(filename);
                print(":");
            }
            if line_numbers {
                print_u64(line_num);
                print(":");
            }
            for j in 0..line_len {
                putchar(line[j]);
            }
            putchar(b'\n');
        }
    }

    found
}

fn contains(haystack: &[u8], needle: &str, ignore_case: bool) -> bool {
    let needle_bytes = needle.as_bytes();
    if needle_bytes.is_empty() {
        return true;
    }
    if haystack.len() < needle_bytes.len() {
        return false;
    }

    for i in 0..=(haystack.len() - needle_bytes.len()) {
        let mut matches = true;
        for j in 0..needle_bytes.len() {
            let h = if ignore_case { to_lower(haystack[i + j]) } else { haystack[i + j] };
            let n = if ignore_case { to_lower(needle_bytes[j]) } else { needle_bytes[j] };
            if h != n {
                matches = false;
                break;
            }
        }
        if matches {
            return true;
        }
    }
    false
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
