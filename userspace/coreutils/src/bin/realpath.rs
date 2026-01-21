//! realpath - print resolved absolute path

#![no_std]
#![no_main]

use libc::*;

#[unsafe(no_mangle)]
fn main(argc: i32, argv: *const *const u8) -> i32 {
    if argc < 2 {
        eprintlns("usage: realpath <path>");
        return 1;
    }

    let path = unsafe { cstr_to_str(*argv.add(1)) };
    
    // For now, we'll do a simple implementation
    // In a full implementation, we'd:
    // 1. Resolve symlinks
    // 2. Handle . and ..
    // 3. Make path absolute
    
    let mut resolved = [0u8; 4096];
    let mut pos = 0;

    // If path doesn't start with /, prepend cwd
    if !path.starts_with('/') {
        // Get current working directory
        let cwd_len = getcwd(&mut resolved);
        if cwd_len < 0 {
            eprintlns("realpath: cannot get current directory");
            return 1;
        }
        pos = cwd_len as usize;
        
        // Add separator if needed
        if pos > 0 && resolved[pos - 1] != b'/' {
            resolved[pos] = b'/';
            pos += 1;
        }
    }

    // Copy path
    let path_bytes = path.as_bytes();
    let mut i = 0;
    
    // Skip leading / if absolute path
    if !path_bytes.is_empty() && path_bytes[0] == b'/' {
        resolved[0] = b'/';
        pos = 1;
        i = 1;
    }

    // Process path components
    while i < path_bytes.len() && pos < resolved.len() {
        // Skip multiple slashes
        if path_bytes[i] == b'/' {
            if pos > 0 && resolved[pos - 1] != b'/' {
                resolved[pos] = b'/';
                pos += 1;
            }
            i += 1;
            continue;
        }

        // Find end of component
        let start = i;
        while i < path_bytes.len() && path_bytes[i] != b'/' {
            i += 1;
        }
        
        let component = &path_bytes[start..i];
        
        // Handle . (current directory) - skip it
        if component == b"." {
            continue;
        }
        
        // Handle .. (parent directory)
        if component == b".." {
            // Remove last component
            if pos > 1 {
                pos -= 1; // Remove trailing /
                while pos > 0 && resolved[pos - 1] != b'/' {
                    pos -= 1;
                }
            }
            continue;
        }
        
        // Copy component
        for &byte in component {
            if pos >= resolved.len() {
                eprintlns("realpath: path too long");
                return 1;
            }
            resolved[pos] = byte;
            pos += 1;
        }
    }

    // Ensure we have at least /
    if pos == 0 {
        resolved[0] = b'/';
        pos = 1;
    }

    // Remove trailing / unless it's root
    if pos > 1 && resolved[pos - 1] == b'/' {
        pos -= 1;
    }

    // Print result
    for i in 0..pos {
        putchar(resolved[i]);
    }
    printlns("");

    0
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
