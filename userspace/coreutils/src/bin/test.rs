//! test/[ - Evaluate conditional expression
//!
//! Returns exit code 0 (true) or 1 (false) based on expression evaluation.

#![no_std]
#![no_main]

use libc::*;

// Store argv globally for get_arg function
static mut ARGV: *const *const u8 = core::ptr::null();
static mut ARGC: usize = 0;

#[unsafe(no_mangle)]
fn main(argc: i32, argv: *const *const u8) -> i32 {
    unsafe {
        ARGC = argc as usize;
        ARGV = argv;
    }

    if argc < 1 {
        return 1;
    }

    // Get program name
    let prog = get_arg(0);
    let is_bracket = prog.ends_with(b"[");

    let argc = argc as usize;

    // Determine expression arguments
    let (start, end) = if is_bracket {
        // Must have closing ']'
        if argc < 2 {
            eputs("[: missing ']'\n");
            return 2;
        }
        let last = get_arg(argc - 1);
        if last != b"]" {
            eputs("[: missing ']'\n");
            return 2;
        }
        (1, argc - 1)
    } else {
        (1, argc)
    };

    if start >= end {
        return 1; // Empty test is false
    }

    evaluate(start, end)
}

/// Get argument at index as byte slice
fn get_arg(idx: usize) -> &'static [u8] {
    unsafe {
        if ARGV.is_null() || idx >= ARGC {
            return b"";
        }
        let ptr = *ARGV.add(idx);
        if ptr.is_null() {
            return b"";
        }
        let mut len = 0;
        while *ptr.add(len) != 0 {
            len += 1;
        }
        core::slice::from_raw_parts(ptr, len)
    }
}

/// Evaluate test expression
fn evaluate(start: usize, end: usize) -> i32 {
    let count = end - start;

    if count == 0 {
        return 1;
    }

    // Handle negation
    let first = get_arg(start);
    if first == b"!" && count > 1 {
        let result = evaluate(start + 1, end);
        return if result == 0 { 1 } else { 0 };
    }

    // Unary operators
    if count == 2 {
        let op = get_arg(start);
        let arg = get_arg(start + 1);
        return evaluate_unary(op, arg);
    }

    // Binary operators
    if count == 3 {
        let left = get_arg(start);
        let op = get_arg(start + 1);
        let right = get_arg(start + 2);
        return evaluate_binary(left, op, right);
    }

    // Single argument: true if non-empty string
    if count == 1 {
        return if first.is_empty() { 1 } else { 0 };
    }

    // Unknown expression
    eputs("test: unknown expression\n");
    2
}

/// Evaluate unary operator
fn evaluate_unary(op: &[u8], arg: &[u8]) -> i32 {
    match op {
        // String tests
        b"-n" => if !arg.is_empty() { 0 } else { 1 },
        b"-z" => if arg.is_empty() { 0 } else { 1 },

        // File tests
        b"-e" | b"-a" => file_exists(arg),
        b"-f" => is_regular_file(arg),
        b"-d" => is_directory(arg),
        b"-r" => is_readable(arg),
        b"-w" => is_writable(arg),
        b"-x" => is_executable(arg),
        b"-s" => file_not_empty(arg),
        b"-L" | b"-h" => is_symlink(arg),
        b"-b" => is_block_device(arg),
        b"-c" => is_char_device(arg),
        b"-p" => is_fifo(arg),
        b"-S" => is_socket(arg),

        _ => {
            // Not a unary operator - treat as single argument test
            if !op.is_empty() { 0 } else { 1 }
        }
    }
}

/// Evaluate binary operator
fn evaluate_binary(left: &[u8], op: &[u8], right: &[u8]) -> i32 {
    match op {
        // String comparison
        b"=" | b"==" => if left == right { 0 } else { 1 },
        b"!=" => if left != right { 0 } else { 1 },

        // Numeric comparison
        b"-eq" => int_cmp(left, right, |a, b| a == b),
        b"-ne" => int_cmp(left, right, |a, b| a != b),
        b"-lt" => int_cmp(left, right, |a, b| a < b),
        b"-le" => int_cmp(left, right, |a, b| a <= b),
        b"-gt" => int_cmp(left, right, |a, b| a > b),
        b"-ge" => int_cmp(left, right, |a, b| a >= b),

        // File comparison
        b"-nt" => file_newer(left, right),
        b"-ot" => file_older(left, right),
        b"-ef" => same_file(left, right),

        _ => {
            eputs("test: unknown binary operator\n");
            2
        }
    }
}

/// Integer comparison helper
fn int_cmp<F: Fn(i64, i64) -> bool>(left: &[u8], right: &[u8], cmp: F) -> i32 {
    match (parse_int(left), parse_int(right)) {
        (Some(a), Some(b)) => if cmp(a, b) { 0 } else { 1 },
        _ => {
            eputs("test: integer expression expected\n");
            2
        }
    }
}

/// Parse integer from bytes
fn parse_int(s: &[u8]) -> Option<i64> {
    if s.is_empty() {
        return None;
    }

    let mut i = 0;
    // Skip whitespace
    while i < s.len() && (s[i] == b' ' || s[i] == b'\t') {
        i += 1;
    }

    if i >= s.len() {
        return None;
    }

    let negative = if s[i] == b'-' {
        i += 1;
        true
    } else if s[i] == b'+' {
        i += 1;
        false
    } else {
        false
    };

    if i >= s.len() {
        return None;
    }

    let mut result: i64 = 0;
    while i < s.len() {
        let c = s[i];
        if c < b'0' || c > b'9' {
            return None;
        }
        result = result.checked_mul(10)?.checked_add((c - b'0') as i64)?;
        i += 1;
    }

    Some(if negative { -result } else { result })
}

/// Convert byte slice to str for libc calls
fn to_str(s: &[u8]) -> &str {
    unsafe { core::str::from_utf8_unchecked(s) }
}

// File test functions

fn get_stat(path: &[u8]) -> Option<Stat> {
    let mut st = Stat::zeroed();
    if stat(to_str(path), &mut st) == 0 {
        Some(st)
    } else {
        None
    }
}

fn file_exists(path: &[u8]) -> i32 {
    if get_stat(path).is_some() { 0 } else { 1 }
}

fn is_regular_file(path: &[u8]) -> i32 {
    match get_stat(path) {
        Some(st) if st.is_file() => 0,
        _ => 1,
    }
}

fn is_directory(path: &[u8]) -> i32 {
    match get_stat(path) {
        Some(st) if st.is_dir() => 0,
        _ => 1,
    }
}

fn is_symlink(path: &[u8]) -> i32 {
    let mut st = Stat::zeroed();
    if lstat(to_str(path), &mut st) == 0 && st.is_symlink() {
        0
    } else {
        1
    }
}

fn is_readable(path: &[u8]) -> i32 {
    let fd = open(to_str(path), O_RDONLY, 0);
    if fd >= 0 {
        close(fd);
        0
    } else {
        1
    }
}

fn is_writable(path: &[u8]) -> i32 {
    match get_stat(path) {
        Some(st) if (st.mode & 0o222) != 0 => 0,
        _ => 1,
    }
}

fn is_executable(path: &[u8]) -> i32 {
    match get_stat(path) {
        Some(st) if (st.mode & 0o111) != 0 => 0,
        _ => 1,
    }
}

fn file_not_empty(path: &[u8]) -> i32 {
    match get_stat(path) {
        Some(st) if st.size > 0 => 0,
        _ => 1,
    }
}

fn is_block_device(path: &[u8]) -> i32 {
    match get_stat(path) {
        Some(st) if st.is_block_device() => 0,
        _ => 1,
    }
}

fn is_char_device(path: &[u8]) -> i32 {
    match get_stat(path) {
        Some(st) if st.is_char_device() => 0,
        _ => 1,
    }
}

fn is_fifo(path: &[u8]) -> i32 {
    match get_stat(path) {
        Some(st) if st.is_fifo() => 0,
        _ => 1,
    }
}

fn is_socket(path: &[u8]) -> i32 {
    match get_stat(path) {
        Some(st) if st.is_socket() => 0,
        _ => 1,
    }
}

fn file_newer(file1: &[u8], file2: &[u8]) -> i32 {
    let st1 = match get_stat(file1) {
        Some(st) => st,
        None => return 1,
    };
    let st2 = match get_stat(file2) {
        Some(st) => st,
        None => return 0, // file1 exists, file2 doesn't -> file1 is newer
    };
    if st1.mtime > st2.mtime { 0 } else { 1 }
}

fn file_older(file1: &[u8], file2: &[u8]) -> i32 {
    file_newer(file2, file1)
}

fn same_file(file1: &[u8], file2: &[u8]) -> i32 {
    let st1 = match get_stat(file1) {
        Some(st) => st,
        None => return 1,
    };
    let st2 = match get_stat(file2) {
        Some(st) => st,
        None => return 1,
    };
    if st1.dev == st2.dev && st1.ino == st2.ino { 0 } else { 1 }
}
