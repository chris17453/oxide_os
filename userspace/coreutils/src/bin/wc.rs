//! wc - word, line, and byte count

#![no_std]
#![no_main]

use libc::*;

#[unsafe(no_mangle)]
fn main(argc: i32, argv: *const *const u8) -> i32 {
    // Parse flags
    let mut show_lines = false;
    let mut show_words = false;
    let mut show_bytes = false;
    let mut file_start = 1;

    for i in 1..argc {
        let arg = unsafe { cstr_to_str(*argv.add(i as usize)) };
        if arg.starts_with('-') && arg.len() > 1 {
            for c in arg.bytes().skip(1) {
                match c {
                    b'l' => show_lines = true,
                    b'w' => show_words = true,
                    b'c' | b'm' => show_bytes = true,
                    _ => {}
                }
            }
            file_start = i + 1;
        } else {
            break;
        }
    }

    // Default: show all
    if !show_lines && !show_words && !show_bytes {
        show_lines = true;
        show_words = true;
        show_bytes = true;
    }

    let mut total_lines = 0u64;
    let mut total_words = 0u64;
    let mut total_bytes = 0u64;

    // If no files, read from stdin
    if file_start >= argc {
        let (lines, words, bytes) = count_fd(STDIN_FILENO);
        print_counts(lines, words, bytes, "", show_lines, show_words, show_bytes);
        return 0;
    }

    let multiple = (argc - file_start) > 1;

    for i in file_start..argc {
        let path = unsafe { cstr_to_str(*argv.add(i as usize)) };

        let fd = open2(path, O_RDONLY);
        if fd < 0 {
            eprints("wc: ");
            print(path);
            eprintlns(": No such file");
            continue;
        }

        let (lines, words, bytes) = count_fd(fd);
        close(fd);

        print_counts(lines, words, bytes, path, show_lines, show_words, show_bytes);

        total_lines += lines;
        total_words += words;
        total_bytes += bytes;
    }

    if multiple {
        print_counts(total_lines, total_words, total_bytes, "total", show_lines, show_words, show_bytes);
    }

    0
}

fn count_fd(fd: i32) -> (u64, u64, u64) {
    let mut lines = 0u64;
    let mut words = 0u64;
    let mut bytes = 0u64;
    let mut in_word = false;

    let mut buf = [0u8; 4096];

    loop {
        let n = read(fd, &mut buf);
        if n <= 0 {
            break;
        }

        bytes += n as u64;

        for i in 0..n as usize {
            let c = buf[i];

            if c == b'\n' {
                lines += 1;
            }

            let is_space = c == b' ' || c == b'\t' || c == b'\n' || c == b'\r';
            if is_space {
                if in_word {
                    words += 1;
                    in_word = false;
                }
            } else {
                in_word = true;
            }
        }
    }

    if in_word {
        words += 1;
    }

    (lines, words, bytes)
}

fn print_counts(lines: u64, words: u64, bytes: u64, name: &str, show_l: bool, show_w: bool, show_b: bool) {
    if show_l {
        print_u64_padded(lines, 8);
    }
    if show_w {
        print_u64_padded(words, 8);
    }
    if show_b {
        print_u64_padded(bytes, 8);
    }
    if !name.is_empty() {
        prints(" ");
        print(name);
    }
    printlns("");
}

fn print_u64_padded(n: u64, width: usize) {
    let mut buf = [b' '; 20];
    let mut val = n;
    let mut i = buf.len();

    if val == 0 {
        i -= 1;
        buf[i] = b'0';
    } else {
        while val > 0 {
            i -= 1;
            buf[i] = b'0' + (val % 10) as u8;
            val /= 10;
        }
    }

    let start = if buf.len() - i < width { buf.len() - width } else { i };
    for j in start..buf.len() {
        putchar(buf[j]);
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
