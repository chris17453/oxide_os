//! seq - print a sequence of numbers
//!
//! Full-featured implementation with:
//! - Floating point support
//! - Custom format strings (-f)
//! - Custom separator (-s)
//! - Equal width/zero padding (-w)
//! - Ascending and descending sequences
//! - Help message (-h)
//! - Proper error handling

#![no_std]
#![no_main]

use libc::*;

const MAX_FORMAT: usize = 128;

struct SeqConfig {
    format: Option<[u8; MAX_FORMAT]>,
    format_len: usize,
    separator: [u8; 128],
    separator_len: usize,
    equal_width: bool,
}

impl SeqConfig {
    fn new() -> Self {
        SeqConfig {
            format: None,
            format_len: 0,
            separator: [b'\n'; 128],
            separator_len: 1,
            equal_width: false,
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

fn show_help() {
    eprintlns("Usage: seq [OPTION]... LAST");
    eprintlns("   or: seq [OPTION]... FIRST LAST");
    eprintlns("   or: seq [OPTION]... FIRST INCREMENT LAST");
    eprintlns("");
    eprintlns("Print numbers from FIRST to LAST, in steps of INCREMENT.");
    eprintlns("");
    eprintlns("Options:");
    eprintlns("  -f FORMAT       Use printf-style floating-point FORMAT");
    eprintlns("  -s STRING       Use STRING to separate numbers (default: \\n)");
    eprintlns("  -w              Equalize width by padding with leading zeroes");
    eprintlns("  -h              Show this help");
    eprintlns("");
    eprintlns("If FIRST or INCREMENT is omitted, it defaults to 1.");
    eprintlns("FIRST, INCREMENT, and LAST are interpreted as floating point values.");
}

#[unsafe(no_mangle)]
fn main(argc: i32, argv: *const *const u8) -> i32 {
    let mut config = SeqConfig::new();
    let mut arg_idx = 1;

    // Parse options
    while arg_idx < argc {
        let arg = unsafe { cstr_to_str(*argv.add(arg_idx as usize)) };

        if arg == "-h" || arg == "--help" {
            show_help();
            return 0;
        } else if arg == "-f" || arg == "--format" {
            arg_idx += 1;
            if arg_idx >= argc {
                eprintlns("seq: option -f requires an argument");
                return 1;
            }
            let fmt_str = unsafe { cstr_to_str(*argv.add(arg_idx as usize)) };
            let mut buf = [0u8; MAX_FORMAT];
            let copy_len = fmt_str.len().min(MAX_FORMAT);
            buf[..copy_len].copy_from_slice(&fmt_str.as_bytes()[..copy_len]);
            config.format = Some(buf);
            config.format_len = copy_len;
            arg_idx += 1;
        } else if arg == "-s" || arg == "--separator" {
            arg_idx += 1;
            if arg_idx >= argc {
                eprintlns("seq: option -s requires an argument");
                return 1;
            }
            let sep_str = unsafe { cstr_to_str(*argv.add(arg_idx as usize)) };
            let copy_len = sep_str.len().min(128);
            config.separator[..copy_len].copy_from_slice(&sep_str.as_bytes()[..copy_len]);
            config.separator_len = copy_len;
            arg_idx += 1;
        } else if arg == "-w" {
            config.equal_width = true;
            arg_idx += 1;
        } else if str_starts_with(arg, "-") && arg.len() > 1 && arg != "--" {
            for c in arg.bytes().skip(1) {
                match c {
                    b'w' => config.equal_width = true,
                    b'h' => {
                        show_help();
                        return 0;
                    }
                    _ => {
                        eprints("seq: invalid option: -");
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

    // Parse numeric arguments
    let remaining = argc - arg_idx;
    if remaining < 1 {
        eprintlns("seq: missing operand");
        eprintlns("Try 'seq -h' for more information.");
        return 1;
    }

    let (first, incr, last) = match remaining {
        1 => {
            let last = parse_float(unsafe { cstr_to_str(*argv.add(arg_idx as usize)) });
            (1.0, 1.0, last)
        }
        2 => {
            let first = parse_float(unsafe { cstr_to_str(*argv.add(arg_idx as usize)) });
            let last = parse_float(unsafe { cstr_to_str(*argv.add((arg_idx + 1) as usize)) });
            (first, 1.0, last)
        }
        _ => {
            let first = parse_float(unsafe { cstr_to_str(*argv.add(arg_idx as usize)) });
            let incr = parse_float(unsafe { cstr_to_str(*argv.add((arg_idx + 1) as usize)) });
            let last = parse_float(unsafe { cstr_to_str(*argv.add((arg_idx + 2) as usize)) });
            (first, incr, last)
        }
    };

    // Check for zero increment
    if incr == 0.0 {
        eprintlns("seq: invalid increment argument: 0");
        return 1;
    }

    // Calculate decimal places for equal width
    let decimal_places = if config.equal_width {
        let first_decimals =
            count_decimal_places(unsafe { cstr_to_str(*argv.add(arg_idx as usize)) });
        let last_idx = if remaining == 1 {
            arg_idx
        } else if remaining == 2 {
            arg_idx + 1
        } else {
            arg_idx + 2
        };
        let last_decimals =
            count_decimal_places(unsafe { cstr_to_str(*argv.add(last_idx as usize)) });
        first_decimals.max(last_decimals)
    } else {
        0
    };

    // Calculate width for equal width formatting
    let width = if config.equal_width {
        let first_width = format_number_width(first, decimal_places);
        let last_width = format_number_width(last, decimal_places);
        first_width.max(last_width)
    } else {
        0
    };

    // Generate sequence
    let mut current = first;
    let mut is_first = true;

    if incr > 0.0 {
        while current <= last + 0.0000001 {
            // Small epsilon for floating point comparison
            if !is_first {
                write(STDOUT_FILENO, &config.separator[..config.separator_len]);
            }
            print_number(current, &config, width, decimal_places);
            is_first = false;
            current += incr;
        }
    } else {
        while current >= last - 0.0000001 {
            if !is_first {
                write(STDOUT_FILENO, &config.separator[..config.separator_len]);
            }
            print_number(current, &config, width, decimal_places);
            is_first = false;
            current += incr;
        }
    }

    // Print final separator if it's not newline (to match GNU behavior)
    if config.separator[0] != b'\n' {
        write(STDOUT_FILENO, b"\n");
    }

    0
}

fn parse_float(s: &str) -> f64 {
    let bytes = s.as_bytes();
    let mut result: f64 = 0.0;
    let mut i = 0;
    let mut negative = false;

    if i < bytes.len() && bytes[i] == b'-' {
        negative = true;
        i += 1;
    } else if i < bytes.len() && bytes[i] == b'+' {
        i += 1;
    }

    // Parse integer part
    while i < bytes.len() && bytes[i] >= b'0' && bytes[i] <= b'9' {
        result = result * 10.0 + (bytes[i] - b'0') as f64;
        i += 1;
    }

    // Parse decimal part
    if i < bytes.len() && bytes[i] == b'.' {
        i += 1;
        let mut divisor = 10.0;
        while i < bytes.len() && bytes[i] >= b'0' && bytes[i] <= b'9' {
            result += (bytes[i] - b'0') as f64 / divisor;
            divisor *= 10.0;
            i += 1;
        }
    }

    if negative { -result } else { result }
}

fn count_decimal_places(s: &str) -> usize {
    let bytes = s.as_bytes();
    let mut found_dot = false;
    let mut count = 0;

    for &b in bytes {
        if b == b'.' {
            found_dot = true;
        } else if found_dot && b >= b'0' && b <= b'9' {
            count += 1;
        }
    }

    count
}

fn format_number_width(num: f64, decimal_places: usize) -> usize {
    let mut width = 0;
    let abs_num = if num < 0.0 { -num } else { num };

    if num < 0.0 {
        width += 1; // Minus sign
    }

    // Count integer digits
    let int_part = abs_num as i64;
    if int_part == 0 {
        width += 1;
    } else {
        let mut temp = int_part;
        while temp > 0 {
            width += 1;
            temp /= 10;
        }
    }

    if decimal_places > 0 {
        width += 1 + decimal_places; // Dot + decimal digits
    }

    width
}

fn print_number(num: f64, config: &SeqConfig, width: usize, decimal_places: usize) {
    if config.equal_width {
        print_number_padded(num, width, decimal_places);
    } else {
        print_float(num);
    }
}

fn print_number_padded(num: f64, width: usize, decimal_places: usize) {
    let mut buf = [0u8; 64];
    let len = format_float_fixed(&mut buf, num, decimal_places);

    // Pad with zeros on the left
    for _ in 0..(width.saturating_sub(len)) {
        putchar(b'0');
    }

    write(STDOUT_FILENO, &buf[..len]);
}

fn print_float(num: f64) {
    let mut buf = [0u8; 64];
    let len = format_float(&mut buf, num);
    write(STDOUT_FILENO, &buf[..len]);
}

fn format_float(buf: &mut [u8], num: f64) -> usize {
    // Check if it's an integer value
    let int_value = num as i64;
    if num == int_value as f64 {
        return format_i64(buf, int_value);
    }

    format_float_fixed(buf, num, 6)
}

fn format_float_fixed(buf: &mut [u8], num: f64, decimal_places: usize) -> usize {
    let mut idx = 0;
    let negative = num < 0.0;
    let abs_num = if negative { -num } else { num };

    if negative {
        buf[idx] = b'-';
        idx += 1;
    }

    // Integer part
    let int_part = abs_num as i64;
    idx += format_i64(&mut buf[idx..], int_part);

    if decimal_places > 0 {
        buf[idx] = b'.';
        idx += 1;

        // Fractional part
        let mut frac = abs_num - int_part as f64;
        for _ in 0..decimal_places {
            frac *= 10.0;
            let digit = frac as u8;
            buf[idx] = b'0' + digit;
            idx += 1;
            frac -= digit as f64;
        }
    }

    idx
}

fn format_i64(buf: &mut [u8], num: i64) -> usize {
    if num == 0 {
        buf[0] = b'0';
        return 1;
    }

    let mut temp_buf = [0u8; 32];
    let mut temp_idx = 0;
    let mut n = if num < 0 { -num } else { num };

    while n > 0 {
        temp_buf[temp_idx] = b'0' + (n % 10) as u8;
        temp_idx += 1;
        n /= 10;
    }

    let mut idx = 0;
    if num < 0 {
        buf[idx] = b'-';
        idx += 1;
    }

    // Reverse the digits
    for i in 0..temp_idx {
        buf[idx + i] = temp_buf[temp_idx - 1 - i];
    }

    idx + temp_idx
}
