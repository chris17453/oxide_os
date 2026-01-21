//! expr - evaluate expressions
//!
//! Full-featured implementation with:
//! - Arithmetic operations (+, -, *, /, %)
//! - Comparison operations (=, !=, <, <=, >, >=)
//! - Boolean operations (&, |)
//! - String operations (length, substr, index, match)
//! - Parentheses for grouping
//! - Help message (-h)
//! - Proper error handling

#![no_std]
#![no_main]

use libc::*;

#[derive(Debug, Clone, Copy)]
enum Value {
    Integer(i64),
    String(&'static str),
}

impl Value {
    fn as_int(&self) -> Option<i64> {
        match self {
            Value::Integer(i) => Some(*i),
            Value::String(s) => parse_int(s.as_bytes()),
        }
    }

    fn as_bool(&self) -> bool {
        match self {
            Value::Integer(i) => *i != 0,
            Value::String(s) => !s.is_empty(),
        }
    }

    fn as_str(&self) -> &'static str {
        match self {
            Value::String(s) => s,
            Value::Integer(_) => "",
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

fn show_help() {
    eprintlns("Usage: expr EXPRESSION");
    eprintlns("   or: expr OPTION");
    eprintlns("");
    eprintlns("Print the value of EXPRESSION to standard output.");
    eprintlns("");
    eprintlns("Operators (in order of increasing precedence):");
    eprintlns("  |             Boolean OR");
    eprintlns("  &             Boolean AND");
    eprintlns("  <, <=, =, !=, >=, >   Comparison operators");
    eprintlns("  +, -          Addition, subtraction");
    eprintlns("  *, /, %       Multiplication, division, modulo");
    eprintlns("");
    eprintlns("String operations:");
    eprintlns("  length STRING         Length of STRING");
    eprintlns("  substr STRING POS LENGTH   Substring of STRING");
    eprintlns("  index STRING CHARS    Index of first CHARS in STRING (1-based)");
    eprintlns("  match STRING PATTERN  Match STRING against PATTERN");
    eprintlns("");
    eprintlns("Options:");
    eprintlns("  -h, --help    Show this help");
}

#[unsafe(no_mangle)]
fn main(argc: i32, argv: *const *const u8) -> i32 {
    if argc < 2 {
        eprintlns("expr: missing operand");
        eprintlns("Try 'expr --help' for more information.");
        return 1;
    }

    // Check for help
    let first_arg = unsafe { cstr_to_str(*argv.add(1)) };
    if first_arg == "-h" || first_arg == "--help" {
        show_help();
        return 0;
    }

    // Parse expression from argv
    let mut args = Vec::new();
    for i in 1..argc {
        let arg = unsafe { cstr_to_str(*argv.add(i as usize)) };
        args.push(arg);
    }

    match eval_expr(&args) {
        Ok(Value::Integer(i)) => {
            print_i64(i);
            printlns("");
            0
        }
        Ok(Value::String(s)) => {
            printlns(s);
            0
        }
        Err(msg) => {
            eprints("expr: ");
            eprintlns(msg);
            1
        }
    }
}

struct Vec<T> {
    data: [Option<T>; 64],
    len: usize,
}

impl<T: Copy> Vec<T> {
    fn new() -> Self {
        Vec {
            data: [None; 64],
            len: 0,
        }
    }

    fn push(&mut self, item: T) {
        if self.len < 64 {
            self.data[self.len] = Some(item);
            self.len += 1;
        }
    }

    fn get(&self, index: usize) -> Option<&T> {
        if index < self.len {
            self.data[index].as_ref()
        } else {
            None
        }
    }

    fn len(&self) -> usize {
        self.len
    }
}

fn eval_expr(args: &Vec<&'static str>) -> Result<Value, &'static str> {
    if args.len() == 0 {
        return Err("missing operand");
    }

    eval_or(args, &mut 0)
}

fn eval_or(args: &Vec<&'static str>, pos: &mut usize) -> Result<Value, &'static str> {
    let mut left = eval_and(args, pos)?;

    while *pos < args.len() {
        if let Some(&op) = args.get(*pos) {
            if op == "|" {
                *pos += 1;
                let right = eval_and(args, pos)?;
                let result = if left.as_bool() { left } else { right };
                left = Value::Integer(if result.as_bool() { 1 } else { 0 });
            } else {
                break;
            }
        } else {
            break;
        }
    }

    Ok(left)
}

fn eval_and(args: &Vec<&'static str>, pos: &mut usize) -> Result<Value, &'static str> {
    let mut left = eval_comparison(args, pos)?;

    while *pos < args.len() {
        if let Some(&op) = args.get(*pos) {
            if op == "&" {
                *pos += 1;
                let right = eval_comparison(args, pos)?;
                let result = if left.as_bool() && right.as_bool() {
                    left
                } else {
                    Value::Integer(0)
                };
                left = result;
            } else {
                break;
            }
        } else {
            break;
        }
    }

    Ok(left)
}

fn eval_comparison(args: &Vec<&'static str>, pos: &mut usize) -> Result<Value, &'static str> {
    let mut left = eval_additive(args, pos)?;

    while *pos < args.len() {
        if let Some(&op) = args.get(*pos) {
            let cmp_result = match op {
                "=" => {
                    *pos += 1;
                    let right = eval_additive(args, pos)?;
                    Some(compare_eq(&left, &right))
                }
                "!=" => {
                    *pos += 1;
                    let right = eval_additive(args, pos)?;
                    Some(!compare_eq(&left, &right))
                }
                "<" => {
                    *pos += 1;
                    let right = eval_additive(args, pos)?;
                    Some(compare_lt(&left, &right))
                }
                "<=" => {
                    *pos += 1;
                    let right = eval_additive(args, pos)?;
                    Some(!compare_lt(&right, &left))
                }
                ">" => {
                    *pos += 1;
                    let right = eval_additive(args, pos)?;
                    Some(compare_lt(&right, &left))
                }
                ">=" => {
                    *pos += 1;
                    let right = eval_additive(args, pos)?;
                    Some(!compare_lt(&left, &right))
                }
                _ => None,
            };

            if let Some(result) = cmp_result {
                left = Value::Integer(if result { 1 } else { 0 });
            } else {
                break;
            }
        } else {
            break;
        }
    }

    Ok(left)
}

fn eval_additive(args: &Vec<&'static str>, pos: &mut usize) -> Result<Value, &'static str> {
    let mut left = eval_multiplicative(args, pos)?;

    while *pos < args.len() {
        if let Some(&op) = args.get(*pos) {
            match op {
                "+" => {
                    *pos += 1;
                    let right = eval_multiplicative(args, pos)?;
                    let l = left.as_int().ok_or("non-numeric argument")?;
                    let r = right.as_int().ok_or("non-numeric argument")?;
                    left = Value::Integer(l + r);
                }
                "-" => {
                    *pos += 1;
                    let right = eval_multiplicative(args, pos)?;
                    let l = left.as_int().ok_or("non-numeric argument")?;
                    let r = right.as_int().ok_or("non-numeric argument")?;
                    left = Value::Integer(l - r);
                }
                _ => break,
            }
        } else {
            break;
        }
    }

    Ok(left)
}

fn eval_multiplicative(args: &Vec<&'static str>, pos: &mut usize) -> Result<Value, &'static str> {
    let mut left = eval_primary(args, pos)?;

    while *pos < args.len() {
        if let Some(&op) = args.get(*pos) {
            match op {
                "*" | "\\*" => {
                    *pos += 1;
                    let right = eval_primary(args, pos)?;
                    let l = left.as_int().ok_or("non-numeric argument")?;
                    let r = right.as_int().ok_or("non-numeric argument")?;
                    left = Value::Integer(l * r);
                }
                "/" => {
                    *pos += 1;
                    let right = eval_primary(args, pos)?;
                    let l = left.as_int().ok_or("non-numeric argument")?;
                    let r = right.as_int().ok_or("non-numeric argument")?;
                    if r == 0 {
                        return Err("division by zero");
                    }
                    left = Value::Integer(l / r);
                }
                "%" => {
                    *pos += 1;
                    let right = eval_primary(args, pos)?;
                    let l = left.as_int().ok_or("non-numeric argument")?;
                    let r = right.as_int().ok_or("non-numeric argument")?;
                    if r == 0 {
                        return Err("division by zero");
                    }
                    left = Value::Integer(l % r);
                }
                _ => break,
            }
        } else {
            break;
        }
    }

    Ok(left)
}

fn eval_primary(args: &Vec<&'static str>, pos: &mut usize) -> Result<Value, &'static str> {
    if *pos >= args.len() {
        return Err("missing operand");
    }

    let arg = args.get(*pos).ok_or("missing operand")?;

    // Check for parentheses
    if *arg == "(" {
        *pos += 1;
        let result = eval_or(args, pos)?;
        if *pos >= args.len() || args.get(*pos) != Some(&")") {
            return Err("missing ')'");
        }
        *pos += 1;
        return Ok(result);
    }

    // Check for string operations
    match *arg {
        "length" => {
            *pos += 1;
            let s = args.get(*pos).ok_or("missing operand")?;
            *pos += 1;
            return Ok(Value::Integer(s.len() as i64));
        }
        "substr" => {
            *pos += 1;
            let s = args.get(*pos).ok_or("missing operand")?;
            *pos += 1;
            let start_arg = args.get(*pos).ok_or("missing operand")?;
            let start = parse_int(start_arg.as_bytes()).ok_or("invalid position")? as usize;
            *pos += 1;
            let len_arg = args.get(*pos).ok_or("missing operand")?;
            let length = parse_int(len_arg.as_bytes()).ok_or("invalid length")? as usize;
            *pos += 1;

            // substr uses 1-based indexing
            if start == 0 || start > s.len() {
                return Ok(Value::String(""));
            }
            let start_idx = start - 1;
            let end_idx = (start_idx + length).min(s.len());
            let result = &s[start_idx..end_idx];
            return Ok(Value::String(result));
        }
        "index" => {
            *pos += 1;
            let s = args.get(*pos).ok_or("missing operand")?;
            *pos += 1;
            let chars = args.get(*pos).ok_or("missing operand")?;
            *pos += 1;

            // Find first occurrence of any char in chars
            for (i, c) in s.bytes().enumerate() {
                for ch in chars.bytes() {
                    if c == ch {
                        return Ok(Value::Integer((i + 1) as i64)); // 1-based
                    }
                }
            }
            return Ok(Value::Integer(0));
        }
        "match" => {
            *pos += 1;
            let s = args.get(*pos).ok_or("missing operand")?;
            *pos += 1;
            let pattern = args.get(*pos).ok_or("missing operand")?;
            *pos += 1;

            // Simple pattern matching: check if pattern is at start of string
            if str_starts_with(s, pattern) {
                return Ok(Value::Integer(pattern.len() as i64));
            }
            return Ok(Value::Integer(0));
        }
        _ => {}
    }

    // Try to parse as integer
    if let Some(i) = parse_int(arg.as_bytes()) {
        *pos += 1;
        return Ok(Value::Integer(i));
    }

    // Otherwise treat as string
    *pos += 1;
    Ok(Value::String(arg))
}

fn compare_eq(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Integer(l), Value::Integer(r)) => l == r,
        (Value::String(l), Value::String(r)) => l == r,
        _ => false,
    }
}

fn compare_lt(left: &Value, right: &Value) -> bool {
    // Try numeric comparison first
    if let (Some(l), Some(r)) = (left.as_int(), right.as_int()) {
        return l < r;
    }
    // Fall back to string comparison
    str_compare(left.as_str(), right.as_str()) < 0
}

fn str_compare(a: &str, b: &str) -> i32 {
    let a_bytes = a.as_bytes();
    let b_bytes = b.as_bytes();
    let min_len = a_bytes.len().min(b_bytes.len());

    for i in 0..min_len {
        if a_bytes[i] < b_bytes[i] {
            return -1;
        } else if a_bytes[i] > b_bytes[i] {
            return 1;
        }
    }

    if a_bytes.len() < b_bytes.len() {
        -1
    } else if a_bytes.len() > b_bytes.len() {
        1
    } else {
        0
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

    if start >= s.len() {
        return None;
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
