//! uniq - report or filter out repeated lines
//!
//! Full-featured implementation with:
//! - Count occurrences (-c)
//! - Show only repeated lines (-d)
//! - Show only unique lines (-u)
//! - Ignore case (-i)
//! - Skip fields (-f NUM)
//! - Skip characters (-s NUM)
//! - Compare only N characters (-w NUM)
//! - Zero-terminated lines (-z)
//! - Input and output file support
//! - Proper error handling

#![no_std]
#![no_main]

use libc::*;

const MAX_LINE: usize = 4096;

struct UniqConfig {
    count: bool,
    repeated: bool,
    unique_only: bool,
    ignore_case: bool,
    skip_fields: usize,
    skip_chars: usize,
    check_chars: usize, // 0 means unlimited
    zero_terminated: bool,
}

impl UniqConfig {
    fn new() -> Self {
        UniqConfig {
            count: false,
            repeated: false,
            unique_only: false,
            ignore_case: false,
            skip_fields: 0,
            skip_chars: 0,
            check_chars: 0,
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

fn parse_number(s: &str) -> Option<usize> {
    let mut result = 0usize;
    for b in s.bytes() {
        if b >= b'0' && b <= b'9' {
            result = result * 10 + (b - b'0') as usize;
        } else {
            return None;
        }
    }
    Some(result)
}

fn show_help() {
    eprintlns("Usage: uniq [OPTIONS] [INPUT [OUTPUT]]");
    eprintlns("");
    eprintlns("Filter adjacent matching lines from INPUT (or stdin) to OUTPUT (or stdout).");
    eprintlns("");
    eprintlns("Options:");
    eprintlns("  -c          Prefix lines with occurrence count");
    eprintlns("  -d          Only print duplicate lines");
    eprintlns("  -u          Only print unique lines");
    eprintlns("  -i          Ignore case when comparing");
    eprintlns("  -f NUM      Skip NUM fields before comparing");
    eprintlns("  -s NUM      Skip NUM characters before comparing");
    eprintlns("  -w NUM      Compare only NUM characters per line");
    eprintlns("  -z          Lines are terminated by NUL, not newline");
    eprintlns("  -h          Show this help");
}

#[unsafe(no_mangle)]
fn main(argc: i32, argv: *const *const u8) -> i32 {
    let mut config = UniqConfig::new();
    let mut arg_idx = 1;

    // Parse flags
    while arg_idx < argc {
        let arg = unsafe { cstr_to_str(*argv.add(arg_idx as usize)) };

        if arg == "-h" || arg == "--help" {
            show_help();
            return 0;
        } else if arg == "-f" || arg == "-s" || arg == "-w" {
            arg_idx += 1;
            if arg_idx >= argc {
                eprints("uniq: option ");
                prints(arg);
                eprintlns(" requires an argument");
                return 1;
            }
            let num_str = unsafe { cstr_to_str(*argv.add(arg_idx as usize)) };
            match parse_number(num_str) {
                Some(n) => match arg {
                    "-f" => config.skip_fields = n,
                    "-s" => config.skip_chars = n,
                    "-w" => config.check_chars = n,
                    _ => {}
                },
                None => {
                    eprints("uniq: invalid number: ");
                    prints(num_str);
                    eprintlns("");
                    return 1;
                }
            }
            arg_idx += 1;
        } else if str_starts_with(arg, "-") && arg.len() > 1 && arg != "--" {
            for c in arg.bytes().skip(1) {
                match c {
                    b'c' => config.count = true,
                    b'd' => config.repeated = true,
                    b'u' => config.unique_only = true,
                    b'i' => config.ignore_case = true,
                    b'z' => config.zero_terminated = true,
                    b'h' => {
                        show_help();
                        return 0;
                    }
                    _ => {
                        eprints("uniq: invalid option: -");
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

    // Open input
    let input_fd = if arg_idx < argc {
        let path = unsafe { cstr_to_str(*argv.add(arg_idx as usize)) };
        let fd = open2(path, O_RDONLY);
        if fd < 0 {
            eprints("uniq: ");
            prints(path);
            eprintlns(": No such file or directory");
            return 1;
        }
        arg_idx += 1;
        fd
    } else {
        STDIN_FILENO
    };

    // Open output
    let output_fd = if arg_idx < argc {
        let path = unsafe { cstr_to_str(*argv.add(arg_idx as usize)) };
        let fd = open(path, O_WRONLY | O_CREAT | O_TRUNC, 0o644);
        if fd < 0 {
            eprints("uniq: cannot create '");
            prints(path);
            eprintlns("'");
            if input_fd != STDIN_FILENO {
                close(input_fd);
            }
            return 1;
        }
        fd
    } else {
        STDOUT_FILENO
    };

    let result = process_file(&config, input_fd, output_fd);

    if input_fd != STDIN_FILENO {
        close(input_fd);
    }
    if output_fd != STDOUT_FILENO {
        close(output_fd);
    }

    result
}

fn process_file(config: &UniqConfig, input_fd: i32, output_fd: i32) -> i32 {
    let mut buf = [0u8; 4096];
    let mut prev_line = [0u8; MAX_LINE];
    let mut prev_len = 0;
    let mut curr_line = [0u8; MAX_LINE];
    let mut curr_len = 0;
    let mut repeat_count = 0u64;
    let mut has_prev = false;
    let delimiter = if config.zero_terminated { b'\0' } else { b'\n' };

    loop {
        let n = read(input_fd, &mut buf);
        if n <= 0 {
            break;
        }

        for i in 0..n as usize {
            if buf[i] == delimiter {
                if has_prev {
                    let same = lines_equal_with_options(
                        config,
                        &prev_line[..prev_len],
                        &curr_line[..curr_len],
                    );
                    if same {
                        repeat_count += 1;
                    } else {
                        output_line(
                            config,
                            output_fd,
                            &prev_line[..prev_len],
                            repeat_count,
                            delimiter,
                        );
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

    // Handle last line without delimiter
    if curr_len > 0 {
        if has_prev {
            let same =
                lines_equal_with_options(config, &prev_line[..prev_len], &curr_line[..curr_len]);
            if same {
                repeat_count += 1;
            } else {
                output_line(
                    config,
                    output_fd,
                    &prev_line[..prev_len],
                    repeat_count,
                    delimiter,
                );
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
        output_line(
            config,
            output_fd,
            &prev_line[..prev_len],
            repeat_count,
            delimiter,
        );
    }

    0
}

fn output_line(config: &UniqConfig, fd: i32, line: &[u8], count_val: u64, delimiter: u8) {
    // -d: only show repeated lines
    if config.repeated && count_val < 2 {
        return;
    }
    // -u: only show unique lines
    if config.unique_only && count_val > 1 {
        return;
    }

    if config.count {
        let count_str = format_count(count_val);
        write(fd, &count_str);
        write(fd, b" ");
    }

    write(fd, line);
    write(fd, &[delimiter]);
}

fn format_count(n: u64) -> [u8; 8] {
    let mut buf = [b' '; 8];
    let mut val = n;
    let mut pos = 7;

    loop {
        buf[pos] = b'0' + (val % 10) as u8;
        val /= 10;
        if val == 0 {
            break;
        }
        if pos == 0 {
            break;
        }
        pos -= 1;
    }

    buf
}

fn lines_equal_with_options(config: &UniqConfig, a: &[u8], b: &[u8]) -> bool {
    // Skip fields
    let a_start = skip_fields_and_chars(a, config.skip_fields, config.skip_chars);
    let b_start = skip_fields_and_chars(b, config.skip_fields, config.skip_chars);

    let a_slice = &a[a_start..];
    let b_slice = &b[b_start..];

    // Determine comparison length
    let cmp_len = if config.check_chars > 0 {
        config.check_chars.min(a_slice.len()).min(b_slice.len())
    } else {
        if a_slice.len() != b_slice.len() {
            return false;
        }
        a_slice.len()
    };

    // Compare with case sensitivity option
    for i in 0..cmp_len {
        let ca = if config.ignore_case {
            to_lower(a_slice[i])
        } else {
            a_slice[i]
        };
        let cb = if config.ignore_case {
            to_lower(b_slice[i])
        } else {
            b_slice[i]
        };
        if ca != cb {
            return false;
        }
    }

    // If check_chars is set, only compare that many characters
    if config.check_chars > 0 {
        true
    } else {
        a_slice.len() == b_slice.len()
    }
}

fn skip_fields_and_chars(line: &[u8], skip_fields: usize, skip_chars: usize) -> usize {
    let mut pos = 0;
    let mut fields_skipped = 0;

    // Skip fields (whitespace-separated)
    while pos < line.len() && fields_skipped < skip_fields {
        // Skip whitespace
        while pos < line.len() && (line[pos] == b' ' || line[pos] == b'\t') {
            pos += 1;
        }
        // Skip non-whitespace (the field)
        while pos < line.len() && line[pos] != b' ' && line[pos] != b'\t' {
            pos += 1;
        }
        fields_skipped += 1;
    }

    // Skip initial whitespace after fields
    while pos < line.len() && (line[pos] == b' ' || line[pos] == b'\t') {
        pos += 1;
    }

    // Skip characters
    for _ in 0..skip_chars {
        if pos < line.len() {
            pos += 1;
        }
    }

    pos.min(line.len())
}

fn to_lower(c: u8) -> u8 {
    if c >= b'A' && c <= b'Z' { c + 32 } else { c }
}
