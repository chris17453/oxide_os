//! # Termcap/Terminfo Parser
//!
//! Parses termcap text entries and terminfo binary files.
//!
//! -- WireSaint: File format parsing, reading terminal databases from disk

use crate::TerminalEntry;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

/// Parse a termcap entry from a text string
///
/// Termcap format:
/// ```text
/// name|alias1|alias2:capability1=value1:capability2#number:capability3:
/// ```
pub fn parse_termcap_entry(text: &str) -> Result<TerminalEntry, &'static str> {
    // Split on colons
    let parts: Vec<&str> = text.split(':').collect();
    if parts.is_empty() {
        return Err("Empty termcap entry");
    }

    // First part is name and aliases
    let names: Vec<&str> = parts[0].split('|').collect();
    if names.is_empty() {
        return Err("No terminal name");
    }

    let mut entry = TerminalEntry::new(names[0].trim());

    // Add aliases
    for alias in &names[1..] {
        entry.aliases.push(alias.trim().to_string());
    }

    // Parse capabilities
    for cap_str in &parts[1..] {
        let cap_str = cap_str.trim();
        if cap_str.is_empty() {
            continue;
        }

        parse_capability(&mut entry, cap_str)?;
    }

    Ok(entry)
}

/// Parse a single capability
fn parse_capability(entry: &mut TerminalEntry, cap: &str) -> Result<(), &'static str> {
    if cap.is_empty() {
        return Ok(());
    }

    // String capability: name=value
    if let Some(eq_pos) = cap.find('=') {
        let name = &cap[..eq_pos];
        let value = &cap[eq_pos + 1..];

        // Unescape the value
        let unescaped = unescape_termcap(value);
        entry.set_string(name, &unescaped);
        return Ok(());
    }

    // Numeric capability: name#number
    if let Some(hash_pos) = cap.find('#') {
        let name = &cap[..hash_pos];
        let value_str = &cap[hash_pos + 1..];

        if let Ok(num) = value_str.parse::<i32>() {
            entry.set_number(name, num);
            return Ok(());
        } else {
            return Err("Invalid numeric capability");
        }
    }

    // Boolean capability: just name
    entry.set_flag(cap, true);
    Ok(())
}

/// Unescape termcap escape sequences
fn unescape_termcap(s: &str) -> String {
    let mut result = String::new();
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '\\' && i + 1 < chars.len() {
            i += 1;
            match chars[i] {
                'E' | 'e' => result.push('\x1b'), // ESC
                'n' => result.push('\n'),
                'r' => result.push('\r'),
                't' => result.push('\t'),
                'b' => result.push('\x08'), // Backspace
                'f' => result.push('\x0c'), // Form feed
                's' => result.push(' '),
                '^' => result.push('^'),
                '\\' => result.push('\\'),
                ':' => result.push(':'),
                // Octal: \123
                '0'..='7' => {
                    let mut octal = (chars[i] as u8 - b'0') as u32;
                    i += 1;
                    if i < chars.len() && chars[i].is_ascii_digit() && chars[i] <= '7' {
                        octal = octal * 8 + (chars[i] as u8 - b'0') as u32;
                        i += 1;
                        if i < chars.len() && chars[i].is_ascii_digit() && chars[i] <= '7' {
                            octal = octal * 8 + (chars[i] as u8 - b'0') as u32;
                        } else {
                            i -= 1;
                        }
                    } else {
                        i -= 1;
                    }
                    if octal <= 255 {
                        result.push(octal as u8 as char);
                    }
                }
                _ => {
                    result.push('\\');
                    result.push(chars[i]);
                }
            }
            i += 1;
        } else if chars[i] == '^' && i + 1 < chars.len() {
            // Control character: ^A = 0x01
            i += 1;
            let ch = chars[i];
            if ch.is_ascii_alphabetic() {
                let ctrl = ((ch.to_ascii_uppercase() as u8) - b'A' + 1) as char;
                result.push(ctrl);
            } else if ch == '?' {
                result.push('\x7f');
            } else {
                result.push('^');
                result.push(ch);
            }
            i += 1;
        } else {
            result.push(chars[i]);
            i += 1;
        }
    }

    result
}

/// Search for a termcap file and load the entry
///
/// Standard search paths:
/// - $TERMCAP (if set and is a file)
/// - /etc/termcap
/// - /usr/share/misc/termcap
pub fn load_termcap_file(_name: &str) -> Result<TerminalEntry, &'static str> {
    // In a real implementation, this would:
    // 1. Check $TERMCAP environment variable
    // 2. Search standard locations
    // 3. Parse the file and find the matching entry
    //
    // For now, return error - use built-in database instead
    Err("Termcap file loading not yet implemented - use built-in database")
}
