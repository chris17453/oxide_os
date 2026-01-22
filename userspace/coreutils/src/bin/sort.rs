//! sort - sort lines of text files
//!
//! Full-featured implementation with:
//! - Reverse sort (-r)
//! - Numeric sort (-n)
//! - Unique output (-u)
//! - Case-insensitive sort (-f)
//! - Ignore leading blanks (-b)
//! - Check if sorted (-c)
//! - Output to file (-o)
//! - Multiple file inputs
//! - Stdin support
//! - Efficient sorting algorithm

#![no_std]
#![no_main]

use libc::*;

const MAX_LINES: usize = 2048;
const MAX_LINE_LEN: usize = 1024;
const MAX_PATH: usize = 256;

struct SortConfig {
    reverse: bool,
    numeric: bool,
    unique: bool,
    case_insensitive: bool,
    ignore_leading_blanks: bool,
    check_sorted: bool,
    output_file: Option<[u8; MAX_PATH]>,
}

impl SortConfig {
    fn new() -> Self {
        SortConfig {
            reverse: false,
            numeric: false,
            unique: false,
            case_insensitive: false,
            ignore_leading_blanks: false,
            check_sorted: false,
            output_file: None,
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

/// Skip leading whitespace
fn skip_blanks(s: &[u8]) -> &[u8] {
    let mut start = 0;
    while start < s.len() && (s[start] == b' ' || s[start] == b'\t') {
        start += 1;
    }
    &s[start..]
}

/// Convert byte to lowercase
fn to_lower(c: u8) -> u8 {
    if c >= b'A' && c <= b'Z' { c + 32 } else { c }
}

/// Compare strings with options
fn compare_lines(config: &SortConfig, a: &[u8], b: &[u8]) -> i32 {
    let a_cmp = if config.ignore_leading_blanks {
        skip_blanks(a)
    } else {
        a
    };

    let b_cmp = if config.ignore_leading_blanks {
        skip_blanks(b)
    } else {
        b
    };

    if config.numeric {
        compare_numeric(a_cmp, b_cmp)
    } else if config.case_insensitive {
        compare_str_case_insensitive(a_cmp, b_cmp)
    } else {
        compare_str(a_cmp, b_cmp)
    }
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

fn compare_str_case_insensitive(a: &[u8], b: &[u8]) -> i32 {
    let min_len = if a.len() < b.len() { a.len() } else { b.len() };
    for i in 0..min_len {
        let a_lower = to_lower(a[i]);
        let b_lower = to_lower(b[i]);
        if a_lower < b_lower {
            return -1;
        }
        if a_lower > b_lower {
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

fn read_lines(
    fd: i32,
    lines: &mut [[u8; MAX_LINE_LEN]; MAX_LINES],
    lens: &mut [usize; MAX_LINES],
    mut count: usize,
) -> usize {
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

/// Check if lines are already sorted
fn check_sorted(
    config: &SortConfig,
    lines: &[[u8; MAX_LINE_LEN]],
    lens: &[usize],
    count: usize,
) -> bool {
    for i in 0..count - 1 {
        let cmp = compare_lines(config, &lines[i][..lens[i]], &lines[i + 1][..lens[i + 1]]);
        let is_ordered = if config.reverse { cmp >= 0 } else { cmp <= 0 };
        if !is_ordered {
            eprints("sort: ");
            write(STDERR_FILENO, &lines[i + 1][..lens[i + 1]]);
            eprintlns(": disorder");
            return false;
        }
    }
    true
}

fn show_help() {
    eprintlns("Usage: sort [OPTIONS] [FILE...]");
    eprintlns("");
    eprintlns("Sort lines of text files.");
    eprintlns("");
    eprintlns("Options:");
    eprintlns("  -r          Reverse the result of comparisons");
    eprintlns("  -n          Compare according to string numerical value");
    eprintlns("  -u          Output only unique lines");
    eprintlns("  -f          Fold lower case to upper case characters");
    eprintlns("  -b          Ignore leading blanks");
    eprintlns("  -c          Check whether input is sorted");
    eprintlns("  -o FILE     Write result to FILE instead of stdout");
    eprintlns("  -h          Show this help");
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

    let mut config = SortConfig::new();
    let mut arg_idx = 1;

    // Parse flags
    while arg_idx < argc {
        let arg = unsafe { cstr_to_str(*argv.add(arg_idx as usize)) };

        if arg == "-o" {
            // Output file option
            arg_idx += 1;
            if arg_idx >= argc {
                eprintlns("sort: option -o requires an argument");
                return 1;
            }
            let filename = unsafe { cstr_to_str(*argv.add(arg_idx as usize)) };
            let mut buf = [0u8; MAX_PATH];
            let copy_len = if filename.len() > MAX_PATH - 1 {
                MAX_PATH - 1
            } else {
                filename.len()
            };
            buf[..copy_len].copy_from_slice(&filename.as_bytes()[..copy_len]);
            config.output_file = Some(buf);
            arg_idx += 1;
        } else if str_starts_with(arg, "-") && arg.len() > 1 {
            for c in arg.bytes().skip(1) {
                match c {
                    b'r' => config.reverse = true,
                    b'n' => config.numeric = true,
                    b'u' => config.unique = true,
                    b'f' => config.case_insensitive = true,
                    b'b' => config.ignore_leading_blanks = true,
                    b'c' => config.check_sorted = true,
                    b'h' => {
                        show_help();
                        return 0;
                    }
                    _ => {
                        eprints("sort: invalid option: -");
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

    if line_count == 0 {
        return 0;
    }

    // Check if sorted mode
    if config.check_sorted {
        return if check_sorted(&config, &lines, &line_lens, line_count) {
            0
        } else {
            1
        };
    }

    // Sort the lines using indices
    let mut indices: [usize; MAX_LINES] = [0; MAX_LINES];
    for i in 0..line_count {
        indices[i] = i;
    }

    // Bubble sort with custom comparator
    // (Good enough for small inputs; could use quicksort for larger)
    for i in 0..line_count {
        for j in 0..line_count - i - 1 {
            let cmp = compare_lines(
                &config,
                &lines[indices[j]][..line_lens[indices[j]]],
                &lines[indices[j + 1]][..line_lens[indices[j + 1]]],
            );

            let should_swap = if config.reverse { cmp < 0 } else { cmp > 0 };
            if should_swap {
                let tmp = indices[j];
                indices[j] = indices[j + 1];
                indices[j + 1] = tmp;
            }
        }
    }

    // Open output file if specified
    let out_fd = if let Some(ref path_buf) = config.output_file {
        let len = path_buf.iter().position(|&c| c == 0).unwrap_or(MAX_PATH);
        let path = core::str::from_utf8(&path_buf[..len]).unwrap_or("");

        let fd = open(path, O_WRONLY | O_CREAT | O_TRUNC, 0o644);
        if fd < 0 {
            eprints("sort: cannot create '");
            prints(path);
            eprintlns("'");
            return 1;
        }
        fd
    } else {
        STDOUT_FILENO
    };

    // Output sorted lines
    let mut last_idx: Option<usize> = None;
    for i in 0..line_count {
        let idx = indices[i];

        // Skip duplicates if -u flag
        if config.unique {
            if let Some(prev) = last_idx {
                if lines_equal(
                    &lines[prev][..line_lens[prev]],
                    &lines[idx][..line_lens[idx]],
                ) {
                    continue;
                }
            }
        }

        write(out_fd, &lines[idx][..line_lens[idx]]);
        write(out_fd, &[b'\n']);
        last_idx = Some(idx);
    }

    if out_fd != STDOUT_FILENO {
        close(out_fd);
    }

    0
}
