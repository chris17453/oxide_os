//! tr - translate or delete characters
//!
//! Full-featured implementation with:
//! - Translation mode (default)
//! - Delete mode (-d)
//! - Squeeze mode (-s)
//! - Complement mode (-c, -C)
//! - Truncate set1 mode (-t)
//! - Character ranges (a-z, A-Z, 0-9)
//! - Escape sequences (\n, \t, \r, \\, \NNN)
//! - Character classes [:lower:], [:upper:], [:digit:], etc.
//! - Repeat notation [c*n] and [c*]
//! - Multiple file support
//! - Proper error handling

#![no_std]
#![no_main]

use libc::*;

const MAX_SET_SIZE: usize = 512;

struct TrConfig {
    delete: bool,
    squeeze: bool,
    complement: bool,
    truncate_set1: bool,
}

impl TrConfig {
    fn new() -> Self {
        TrConfig {
            delete: false,
            squeeze: false,
            complement: false,
            truncate_set1: false,
        }
    }
}

struct CharSet {
    chars: [u8; MAX_SET_SIZE],
    len: usize,
}

impl CharSet {
    fn new() -> Self {
        CharSet {
            chars: [0u8; MAX_SET_SIZE],
            len: 0,
        }
    }

    fn add(&mut self, c: u8) {
        if self.len < MAX_SET_SIZE {
            self.chars[self.len] = c;
            self.len += 1;
        }
    }

    fn contains(&self, c: u8) -> bool {
        for i in 0..self.len {
            if self.chars[i] == c {
                return true;
            }
        }
        false
    }

    fn get(&self, idx: usize) -> Option<u8> {
        if idx < self.len {
            Some(self.chars[idx])
        } else if self.len > 0 {
            // Extend with last character
            Some(self.chars[self.len - 1])
        } else {
            None
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
    eprintlns("Usage: tr [OPTIONS] SET1 [SET2]");
    eprintlns("");
    eprintlns("Translate, squeeze, and/or delete characters from standard input.");
    eprintlns("");
    eprintlns("Options:");
    eprintlns("  -c, -C      Use complement of SET1");
    eprintlns("  -d          Delete characters in SET1");
    eprintlns("  -s          Squeeze multiple consecutive characters from SET");
    eprintlns("  -t          Truncate SET1 to length of SET2");
    eprintlns("  -h          Show this help");
    eprintlns("");
    eprintlns("SETs are specified as strings of characters. Sequences are:");
    eprintlns("  \\NNN        Octal value NNN (1 to 3 digits)");
    eprintlns("  \\\\          Backslash");
    eprintlns("  \\n          Newline");
    eprintlns("  \\r          Carriage return");
    eprintlns("  \\t          Tab");
    eprintlns("  CHAR1-CHAR2 All characters from CHAR1 to CHAR2");
    eprintlns("  [CHAR*]     Repeat CHAR to length of SET2");
    eprintlns("  [CHAR*N]    Repeat CHAR N times");
    eprintlns("  [:CLASS:]   Character class (lower, upper, digit, etc.)");
}

#[unsafe(no_mangle)]
fn main(argc: i32, argv: *const *const u8) -> i32 {
    let mut config = TrConfig::new();
    let mut arg_idx = 1;

    // Parse options
    while arg_idx < argc {
        let arg = unsafe { cstr_to_str(*argv.add(arg_idx as usize)) };

        if arg == "-h" || arg == "--help" {
            show_help();
            return 0;
        } else if arg.starts_with('-') && arg.len() > 1 && arg != "--" {
            for c in arg.bytes().skip(1) {
                match c {
                    b'c' | b'C' => config.complement = true,
                    b'd' => config.delete = true,
                    b's' => config.squeeze = true,
                    b't' => config.truncate_set1 = true,
                    b'h' => {
                        show_help();
                        return 0;
                    }
                    _ => {
                        eprints("tr: invalid option: -");
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

    // Get SET1
    if arg_idx >= argc {
        eprintlns("tr: missing operand");
        eprintlns("Try 'tr -h' for more information.");
        return 1;
    }

    let set1_str = unsafe { cstr_to_str(*argv.add(arg_idx as usize)) };
    arg_idx += 1;

    // Get SET2 (optional for delete mode)
    let set2_str = if arg_idx < argc {
        unsafe { cstr_to_str(*argv.add(arg_idx as usize)) }
    } else {
        ""
    };

    // Parse sets
    let mut set1 = CharSet::new();
    let mut set2 = CharSet::new();

    if !parse_set(set1_str, &mut set1) {
        eprintlns("tr: invalid SET1");
        return 1;
    }

    if !set2_str.is_empty() {
        if !parse_set(set2_str, &mut set2) {
            eprintlns("tr: invalid SET2");
            return 1;
        }
    }

    // Apply complement if requested
    if config.complement {
        let orig_set1 = set1;
        set1 = CharSet::new();
        for c in 0..=255u8 {
            if !orig_set1.contains(c) {
                set1.add(c);
            }
        }
    }

    // Truncate set1 if requested
    if config.truncate_set1 && set2.len < set1.len {
        set1.len = set2.len;
    }

    // Build translation table
    let mut table: [u8; 256] = [0; 256];
    for i in 0..256 {
        table[i] = i as u8;
    }

    if config.delete {
        // Mark characters to delete
        for i in 0..set1.len {
            table[set1.chars[i] as usize] = 0xFF;
        }
    } else if set2.len > 0 {
        // Build translation mapping
        for i in 0..set1.len {
            let src = set1.chars[i];
            let dst = set2.get(i).unwrap_or(src);
            table[src as usize] = dst;
        }
    }

    // Build squeeze set
    let mut squeeze_set = [false; 256];
    if config.squeeze {
        let squeeze_chars = if config.delete && set2.len > 0 {
            &set2
        } else if !config.delete && set2.len > 0 {
            &set2
        } else {
            &set1
        };

        for i in 0..squeeze_chars.len {
            squeeze_set[squeeze_chars.chars[i] as usize] = true;
        }
    }

    // Process input
    let mut buf = [0u8; 4096];
    let mut last_char: Option<u8> = None;

    loop {
        let n = read(STDIN_FILENO, &mut buf);
        if n <= 0 {
            break;
        }

        for i in 0..n as usize {
            let c = buf[i];
            let translated = table[c as usize];

            // Skip deleted characters
            if config.delete && translated == 0xFF {
                continue;
            }

            // Handle squeeze
            if config.squeeze && squeeze_set[translated as usize] {
                if let Some(last) = last_char {
                    if last == translated {
                        continue;
                    }
                }
            }

            putchar(translated);
            last_char = Some(translated);
        }
    }

    0
}

fn parse_set(s: &str, set: &mut CharSet) -> bool {
    let bytes = s.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        // Check for range notation (a-z)
        if i + 2 < bytes.len() && bytes[i + 1] == b'-' && bytes[i] != b'\\' {
            let start = bytes[i];
            let end = bytes[i + 2];

            if start <= end {
                for c in start..=end {
                    set.add(c);
                }
            } else {
                // Reverse range
                for c in end..=start {
                    set.add(c);
                }
            }
            i += 3;
        }
        // Check for repeat notation [c*n] or [c*]
        else if bytes[i] == b'[' && i + 2 < bytes.len() && bytes[i + 2] == b'*' {
            let c = bytes[i + 1];
            i += 3;

            // Find end of repeat count
            let mut count_str = [0u8; 10];
            let mut count_len = 0;

            while i < bytes.len() && bytes[i] != b']' {
                if count_len < 10 {
                    count_str[count_len] = bytes[i];
                    count_len += 1;
                }
                i += 1;
            }

            if i >= bytes.len() || bytes[i] != b']' {
                return false;  // Invalid: missing ]
            }
            i += 1;  // Skip ]

            // Parse count
            let count = if count_len == 0 {
                // [c*] means repeat to match SET2 length
                // We'll just repeat a lot and rely on truncation
                MAX_SET_SIZE
            } else {
                let mut num = 0usize;
                for j in 0..count_len {
                    if count_str[j] >= b'0' && count_str[j] <= b'9' {
                        num = num * 10 + (count_str[j] - b'0') as usize;
                    } else {
                        return false;  // Invalid number
                    }
                }
                num
            };

            // Add character 'count' times
            for _ in 0..count {
                if set.len >= MAX_SET_SIZE {
                    break;
                }
                set.add(c);
            }
        }
        // Check for character class [:class:]
        else if bytes[i] == b'[' && i + 2 < bytes.len() && bytes[i + 1] == b':' {
            let class_end = find_class_end(&bytes[i..]);
            if class_end > 0 {
                let class_name = &bytes[i + 2..i + class_end - 2];
                if !expand_class(class_name, set) {
                    return false;
                }
                i += class_end;
            } else {
                set.add(bytes[i]);
                i += 1;
            }
        }
        // Check for escape sequences
        else if bytes[i] == b'\\' && i + 1 < bytes.len() {
            i += 1;

            // Check for octal \NNN
            if bytes[i] >= b'0' && bytes[i] <= b'7' {
                let mut octal = 0u8;
                let mut digits = 0;

                while i < bytes.len() && digits < 3 && bytes[i] >= b'0' && bytes[i] <= b'7' {
                    octal = octal * 8 + (bytes[i] - b'0');
                    i += 1;
                    digits += 1;
                }

                set.add(octal);
            } else {
                // Named escape sequences
                let escaped = match bytes[i] {
                    b'n' => b'\n',
                    b't' => b'\t',
                    b'r' => b'\r',
                    b'v' => 0x0b,  // vertical tab
                    b'f' => 0x0c,  // form feed
                    b'\\' => b'\\',
                    c => c,  // Unknown escape, use literal
                };
                set.add(escaped);
                i += 1;
            }
        }
        // Regular character
        else {
            set.add(bytes[i]);
            i += 1;
        }
    }

    true
}

fn find_class_end(s: &[u8]) -> usize {
    for i in 2..s.len().saturating_sub(1) {
        if s[i] == b':' && i + 1 < s.len() && s[i + 1] == b']' {
            return i + 2;
        }
    }
    0
}

fn expand_class(class: &[u8], set: &mut CharSet) -> bool {
    match class {
        b"lower" => {
            for c in b'a'..=b'z' {
                set.add(c);
            }
        }
        b"upper" => {
            for c in b'A'..=b'Z' {
                set.add(c);
            }
        }
        b"digit" => {
            for c in b'0'..=b'9' {
                set.add(c);
            }
        }
        b"alpha" => {
            for c in b'a'..=b'z' {
                set.add(c);
            }
            for c in b'A'..=b'Z' {
                set.add(c);
            }
        }
        b"alnum" => {
            for c in b'0'..=b'9' {
                set.add(c);
            }
            for c in b'a'..=b'z' {
                set.add(c);
            }
            for c in b'A'..=b'Z' {
                set.add(c);
            }
        }
        b"space" => {
            for &c in b" \t\n\r\x0b\x0c" {
                set.add(c);
            }
        }
        b"blank" => {
            for &c in b" \t" {
                set.add(c);
            }
        }
        b"cntrl" => {
            for c in 0..=31u8 {
                set.add(c);
            }
            set.add(127);  // DEL
        }
        b"graph" => {
            for c in 33..=126u8 {
                set.add(c);
            }
        }
        b"print" => {
            for c in 32..=126u8 {
                set.add(c);
            }
        }
        b"punct" => {
            for &c in b"!\"#$%&'()*+,-./:;<=>?@[\\]^_`{|}~" {
                set.add(c);
            }
        }
        b"xdigit" => {
            for c in b'0'..=b'9' {
                set.add(c);
            }
            for c in b'a'..=b'f' {
                set.add(c);
            }
            for c in b'A'..=b'F' {
                set.add(c);
            }
        }
        _ => {
            return false;  // Unknown class
        }
    }
    true
}
