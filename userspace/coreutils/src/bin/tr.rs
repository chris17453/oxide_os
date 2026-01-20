//! tr - translate or delete characters

#![no_std]
#![no_main]

use libc::*;

#[unsafe(no_mangle)]
fn main(argc: i32, argv: *const *const u8) -> i32 {
    let mut delete = false;
    let mut squeeze = false;
    let mut arg_idx = 1;

    // Parse flags
    while arg_idx < argc {
        let arg = unsafe { cstr_to_str(*argv.add(arg_idx as usize)) };
        if arg.starts_with('-') && arg.len() > 1 {
            for c in arg.bytes().skip(1) {
                match c {
                    b'd' => delete = true,
                    b's' => squeeze = true,
                    _ => {}
                }
            }
            arg_idx += 1;
        } else {
            break;
        }
    }

    if arg_idx >= argc {
        eprintlns("usage: tr [-ds] set1 [set2]");
        return 1;
    }

    let set1 = unsafe { cstr_to_str(*argv.add(arg_idx as usize)) };
    let set1_expanded = expand_set(set1);
    arg_idx += 1;

    let set2 = if arg_idx < argc {
        unsafe { cstr_to_str(*argv.add(arg_idx as usize)) }
    } else {
        ""
    };
    let set2_expanded = expand_set(set2);

    // Build translation table
    let mut table: [u8; 256] = [0; 256];
    for i in 0..256 {
        table[i] = i as u8;
    }

    if delete {
        // Mark characters to delete with special value
        for &c in &set1_expanded {
            table[c as usize] = 0xFF; // Mark for deletion
        }
    } else if !set2_expanded.is_empty() {
        // Build translation mapping
        let set2_bytes = &set2_expanded;
        for (i, &c) in set1_expanded.iter().enumerate() {
            let replacement = if i < set2_bytes.len() {
                set2_bytes[i]
            } else {
                set2_bytes[set2_bytes.len() - 1]
            };
            table[c as usize] = replacement;
        }
    }

    // Build squeeze set
    let mut squeeze_set = [false; 256];
    if squeeze {
        let squeeze_chars = if delete { &set2_expanded } else { &set2_expanded };
        for &c in squeeze_chars {
            squeeze_set[c as usize] = true;
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

            if delete && translated == 0xFF {
                // Skip deleted characters
                continue;
            }

            // Handle squeeze
            if squeeze && squeeze_set[translated as usize] {
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

fn expand_set(s: &str) -> [u8; 256] {
    let mut result = [0u8; 256];
    let mut count = 0;
    let bytes = s.as_bytes();
    let mut i = 0;

    while i < bytes.len() && count < 256 {
        if i + 2 < bytes.len() && bytes[i + 1] == b'-' {
            // Range: a-z
            let start = bytes[i];
            let end = bytes[i + 2];
            let (lo, hi) = if start <= end { (start, end) } else { (end, start) };
            for c in lo..=hi {
                if count < 256 {
                    result[count] = c;
                    count += 1;
                }
            }
            i += 3;
        } else if bytes[i] == b'\\' && i + 1 < bytes.len() {
            // Escape sequence
            let escaped = match bytes[i + 1] {
                b'n' => b'\n',
                b't' => b'\t',
                b'r' => b'\r',
                b'\\' => b'\\',
                c => c,
            };
            result[count] = escaped;
            count += 1;
            i += 2;
        } else if bytes[i] == b'[' && i + 3 < bytes.len() && bytes[i + 1] == b':' {
            // Character class [:class:]
            let class_end = find_class_end(&bytes[i..]);
            if class_end > 0 {
                let class_name = &bytes[i + 2..i + class_end - 2];
                expand_class(class_name, &mut result, &mut count);
                i += class_end;
            } else {
                result[count] = bytes[i];
                count += 1;
                i += 1;
            }
        } else {
            result[count] = bytes[i];
            count += 1;
            i += 1;
        }
    }

    // Truncate to actual count
    let mut final_result = [0u8; 256];
    for i in 0..count {
        final_result[i] = result[i];
    }
    // Store count in last position (hacky but works)
    if count < 256 {
        final_result[255] = count as u8;
    }
    final_result
}

fn find_class_end(s: &[u8]) -> usize {
    for i in 2..s.len() - 1 {
        if s[i] == b':' && s[i + 1] == b']' {
            return i + 2;
        }
    }
    0
}

fn expand_class(class: &[u8], result: &mut [u8; 256], count: &mut usize) {
    match class {
        b"lower" => {
            for c in b'a'..=b'z' {
                if *count < 256 {
                    result[*count] = c;
                    *count += 1;
                }
            }
        }
        b"upper" => {
            for c in b'A'..=b'Z' {
                if *count < 256 {
                    result[*count] = c;
                    *count += 1;
                }
            }
        }
        b"digit" => {
            for c in b'0'..=b'9' {
                if *count < 256 {
                    result[*count] = c;
                    *count += 1;
                }
            }
        }
        b"alpha" => {
            for c in b'a'..=b'z' {
                if *count < 256 {
                    result[*count] = c;
                    *count += 1;
                }
            }
            for c in b'A'..=b'Z' {
                if *count < 256 {
                    result[*count] = c;
                    *count += 1;
                }
            }
        }
        b"alnum" => {
            for c in b'0'..=b'9' {
                if *count < 256 {
                    result[*count] = c;
                    *count += 1;
                }
            }
            for c in b'a'..=b'z' {
                if *count < 256 {
                    result[*count] = c;
                    *count += 1;
                }
            }
            for c in b'A'..=b'Z' {
                if *count < 256 {
                    result[*count] = c;
                    *count += 1;
                }
            }
        }
        b"space" => {
            for &c in b" \t\n\r\x0b\x0c" {
                if *count < 256 {
                    result[*count] = c;
                    *count += 1;
                }
            }
        }
        _ => {}
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
