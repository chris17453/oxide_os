//! pgrep - find processes by name

#![no_std]
#![no_main]

use libc::*;

#[unsafe(no_mangle)]
fn main(argc: i32, argv: *const *const u8) -> i32 {
    if argc < 2 {
        eprintlns("usage: pgrep <pattern>");
        return 1;
    }

    let pattern = unsafe { cstr_to_str(*argv.add(1)) };
    
    // In a full implementation, we would:
    // 1. Read /proc directory
    // 2. For each process, read its cmdline
    // 3. Match against pattern
    // 4. Print matching PIDs
    
    // For now, this is a placeholder implementation
    // that demonstrates the interface
    
    prints("pgrep: searching for processes matching '");
    prints(pattern);
    printlns("'");
    
    // Note: This requires /proc filesystem support
    eprintlns("pgrep: /proc filesystem not yet fully implemented");
    
    1
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
