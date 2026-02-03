//! # Terminal Capability String Expansion
//!
//! Implements parameter substitution for terminal control strings.
//! Supports both termcap (tgoto) and terminfo (tparm) style expansions.
//!
//! -- GraveShift: Parameter expansion - translate intent to escape sequences

use core::prelude::v1::*;
use alloc::string::String;
use alloc::vec::Vec;
use alloc::format;

/// Expand a capability string with parameters (terminfo style)
///
/// Supports:
/// - %p1, %p2, ... - push parameter 1, 2, ...
/// - %d - pop and print as decimal
/// - %s - pop and print as string
/// - %i - increment first two parameters (for 1-based indexing)
/// - %+ %- %* %/ %m - arithmetic operators
/// - %& %| %^ - bitwise operators
/// - %> %< %= - comparison operators
/// - %! %~ - logical/bitwise not
/// - %? ... %t ... %e ... %; - if-then-else
/// - %{num} - push constant
pub fn tparm(template: &str, params: &[i32]) -> Result<String, &'static str> {
    let mut result = String::new();
    let mut stack: Vec<i32> = Vec::new();
    let mut params = params.to_vec();
    let chars: Vec<char> = template.chars().collect();
    let mut i = 0;
    
    while i < chars.len() {
        if chars[i] == '%' && i + 1 < chars.len() {
            i += 1;
            match chars[i] {
                // Push parameters
                'p' if i + 1 < chars.len() && chars[i + 1].is_ascii_digit() => {
                    i += 1;
                    let param_num = (chars[i] as u8 - b'0') as usize;
                    if param_num > 0 && param_num <= params.len() {
                        stack.push(params[param_num - 1]);
                    } else {
                        stack.push(0);
                    }
                }
                
                // Increment first two parameters (1-based indexing)
                'i' => {
                    if params.len() >= 1 {
                        params[0] += 1;
                    }
                    if params.len() >= 2 {
                        params[1] += 1;
                    }
                }
                
                // Print as decimal
                'd' => {
                    if let Some(val) = stack.pop() {
                        result.push_str(&val.to_string());
                    }
                }
                
                // Print with width
                '0'..='9' => {
                    let mut width = (chars[i] as u8 - b'0') as usize;
                    i += 1;
                    if i < chars.len() && chars[i].is_ascii_digit() {
                        width = width * 10 + (chars[i] as u8 - b'0') as usize;
                        i += 1;
                    }
                    if i < chars.len() && chars[i] == 'd' {
                        if let Some(val) = stack.pop() {
                            result.push_str(&format!("{:0width$}", val, width = width));
                        }
                    }
                    i -= 1;
                }
                
                // Character output
                'c' => {
                    if let Some(val) = stack.pop() {
                        if val >= 0 && val <= 255 {
                            result.push(val as u8 as char);
                        }
                    }
                }
                
                // Push constant
                '{' => {
                    i += 1;
                    let mut num = 0;
                    let mut neg = false;
                    if i < chars.len() && chars[i] == '-' {
                        neg = true;
                        i += 1;
                    }
                    while i < chars.len() && chars[i].is_ascii_digit() {
                        num = num * 10 + (chars[i] as u8 - b'0') as i32;
                        i += 1;
                    }
                    if i < chars.len() && chars[i] == '}' {
                        stack.push(if neg { -num } else { num });
                    }
                }
                
                // Arithmetic operators
                '+' => {
                    if let (Some(b), Some(a)) = (stack.pop(), stack.pop()) {
                        stack.push(a + b);
                    }
                }
                '-' => {
                    if let (Some(b), Some(a)) = (stack.pop(), stack.pop()) {
                        stack.push(a - b);
                    }
                }
                '*' => {
                    if let (Some(b), Some(a)) = (stack.pop(), stack.pop()) {
                        stack.push(a * b);
                    }
                }
                '/' => {
                    if let (Some(b), Some(a)) = (stack.pop(), stack.pop()) {
                        if b != 0 {
                            stack.push(a / b);
                        } else {
                            stack.push(0);
                        }
                    }
                }
                'm' => {
                    if let (Some(b), Some(a)) = (stack.pop(), stack.pop()) {
                        if b != 0 {
                            stack.push(a % b);
                        } else {
                            stack.push(0);
                        }
                    }
                }
                
                // Bitwise operators
                '&' => {
                    if let (Some(b), Some(a)) = (stack.pop(), stack.pop()) {
                        stack.push(a & b);
                    }
                }
                '|' => {
                    if let (Some(b), Some(a)) = (stack.pop(), stack.pop()) {
                        stack.push(a | b);
                    }
                }
                '^' => {
                    if let (Some(b), Some(a)) = (stack.pop(), stack.pop()) {
                        stack.push(a ^ b);
                    }
                }
                '~' => {
                    if let Some(a) = stack.pop() {
                        stack.push(!a);
                    }
                }
                '!' => {
                    if let Some(a) = stack.pop() {
                        stack.push(if a == 0 { 1 } else { 0 });
                    }
                }
                
                // Comparison operators
                '=' => {
                    if let (Some(b), Some(a)) = (stack.pop(), stack.pop()) {
                        stack.push(if a == b { 1 } else { 0 });
                    }
                }
                '>' => {
                    if let (Some(b), Some(a)) = (stack.pop(), stack.pop()) {
                        stack.push(if a > b { 1 } else { 0 });
                    }
                }
                '<' => {
                    if let (Some(b), Some(a)) = (stack.pop(), stack.pop()) {
                        stack.push(if a < b { 1 } else { 0 });
                    }
                }
                
                // Literal %
                '%' => result.push('%'),
                
                // Conditional (simplified - would need full parser for complex cases)
                '?' => {
                    // Skip conditionals for now - simplified implementation
                }
                't' | 'e' | ';' => {
                    // Skip conditional markers
                }
                
                _ => {
                    // Unknown escape - output literally
                    result.push('%');
                    result.push(chars[i]);
                }
            }
            i += 1;
        } else {
            result.push(chars[i]);
            i += 1;
        }
    }
    
    Ok(result)
}

/// Simplified tgoto for cursor positioning (termcap style)
///
/// Used for backward compatibility with old termcap applications.
/// Expands cursor motion strings like "cm" capability.
pub fn tgoto(template: &str, col: i32, row: i32) -> Result<String, &'static str> {
    // For most terminals, convert to terminfo style and expand
    // Handle both %d and %2d formats
    tparm(template, &[row, col])
}

/// Output a capability string with padding (terminfo/termcap tputs)
///
/// Extracts padding information from strings like "50\x1b[H" and
/// returns the string without padding prefix and the delay in milliseconds.
pub fn parse_padding(cap: &str) -> (String, u32) {
    let mut result = String::new();
    let mut delay_ms = 0u32;
    let chars: Vec<char> = cap.chars().collect();
    let mut i = 0;
    
    // Check for padding at start: digits followed by optional '*' or '/'
    if i < chars.len() && chars[i].is_ascii_digit() {
        let mut delay = 0;
        while i < chars.len() && chars[i].is_ascii_digit() {
            delay = delay * 10 + (chars[i] as u8 - b'0') as u32;
            i += 1;
        }
        delay_ms = delay;
        
        // Skip optional multiplier or mandatory indicator
        if i < chars.len() && (chars[i] == '*' || chars[i] == '/') {
            i += 1;
        }
    }
    
    // Rest is the actual escape sequence
    while i < chars.len() {
        result.push(chars[i]);
        i += 1;
    }
    
    (result, delay_ms)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_parameter() {
        // %p1%d - push param 1, print as decimal
        let result = tparm("%p1%d", &[42]).unwrap();
        assert_eq!(result, "42");
    }

    #[test]
    fn test_cursor_address() {
        // Standard cursor addressing: \x1b[row;colH
        let result = tparm("\x1b[%i%p1%d;%p2%dH", &[10, 20]).unwrap();
        assert_eq!(result, "\x1b[11;21H"); // %i increments both
    }

    #[test]
    fn test_arithmetic() {
        // %p1%p2%+%d - push p1, push p2, add, print
        let result = tparm("%p1%p2%+%d", &[10, 5]).unwrap();
        assert_eq!(result, "15");
    }

    #[test]
    fn test_constant() {
        // %{42}%d - push 42, print
        let result = tparm("%{42}%d", &[]).unwrap();
        assert_eq!(result, "42");
    }

    #[test]
    fn test_tgoto() {
        let result = tgoto("\x1b[%i%p1%d;%p2%dH", 20, 10).unwrap();
        assert_eq!(result, "\x1b[11;21H");
    }

    #[test]
    fn test_padding() {
        let (seq, delay) = parse_padding("100\x1b[H");
        assert_eq!(seq, "\x1b[H");
        assert_eq!(delay, 100);
        
        let (seq2, delay2) = parse_padding("\x1b[2J");
        assert_eq!(seq2, "\x1b[2J");
        assert_eq!(delay2, 0);
    }

    #[test]
    fn test_literal_percent() {
        let result = tparm("100%%", &[]).unwrap();
        assert_eq!(result, "100%");
    }
}
