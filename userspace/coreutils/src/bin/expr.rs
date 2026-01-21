//! expr - evaluate expressions

#![no_std]
#![no_main]

use libc::*;

#[unsafe(no_mangle)]
fn main(argc: i32, argv: *const *const u8) -> i32 {
    if argc < 2 {
        eprintlns("usage: expr <expression>");
        return 1;
    }

    // For simplicity, we'll support basic arithmetic operations
    // expr 5 + 3
    // expr 10 - 2
    // expr 4 \* 5
    // expr 20 / 4
    // expr 10 % 3

    if argc < 4 {
        // Single value
        let arg = unsafe { cstr_to_str(*argv.add(1)) };
        if let Some(val) = parse_int(arg.as_bytes()) {
            print_i64(val);
            printlns("");
            return 0;
        }
        eprintlns("expr: syntax error");
        return 1;
    }

    let left = unsafe { cstr_to_str(*argv.add(1)) };
    let op = unsafe { cstr_to_str(*argv.add(2)) };
    let right = unsafe { cstr_to_str(*argv.add(3)) };

    let left_val = match parse_int(left.as_bytes()) {
        Some(v) => v,
        None => {
            eprintlns("expr: invalid operand");
            return 1;
        }
    };

    let right_val = match parse_int(right.as_bytes()) {
        Some(v) => v,
        None => {
            eprintlns("expr: invalid operand");
            return 1;
        }
    };

    let result = match op {
        "+" => left_val + right_val,
        "-" => left_val - right_val,
        "*" | "\\*" => left_val * right_val,
        "/" => {
            if right_val == 0 {
                eprintlns("expr: division by zero");
                return 1;
            }
            left_val / right_val
        }
        "%" => {
            if right_val == 0 {
                eprintlns("expr: division by zero");
                return 1;
            }
            left_val % right_val
        }
        "=" => if left_val == right_val { 1 } else { 0 },
        "!=" => if left_val != right_val { 1 } else { 0 },
        "<" => if left_val < right_val { 1 } else { 0 },
        "<=" => if left_val <= right_val { 1 } else { 0 },
        ">" => if left_val > right_val { 1 } else { 0 },
        ">=" => if left_val >= right_val { 1 } else { 0 },
        _ => {
            eprintlns("expr: unknown operator");
            return 1;
        }
    };

    print_i64(result);
    printlns("");

    0
}

fn parse_int(s: &[u8]) -> Option<i64> {
    if s.is_empty() {
        return None;
    }

    let mut result: i64 = 0;
    let mut negative = false;
    let mut start = 0;

    if s[0] == b'-' {
        negative = true;
        start = 1;
    } else if s[0] == b'+' {
        start = 1;
    }

    for i in start..s.len() {
        let c = s[i];
        if c < b'0' || c > b'9' {
            return None;
        }
        result = result * 10 + (c - b'0') as i64;
    }

    if negative {
        result = -result;
    }

    Some(result)
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
