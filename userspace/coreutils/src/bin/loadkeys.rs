//! loadkeys - load and manage keyboard layouts
//!
//! Usage:
//!   loadkeys <layout>  - Set keyboard layout (us, uk, de, fr)
//!   loadkeys -l        - List available layouts
//!   loadkeys           - Show current layout

#![no_std]
#![no_main]

use libc::*;

#[unsafe(no_mangle)]
fn main(argc: i32, argv: *const *const u8) -> i32 {
    if argc < 2 {
        // Show current layout
        show_current_layout();
        return 0;
    }

    // Get first argument
    let arg1 = unsafe { *argv.offset(1) };
    let arg1_str = unsafe { cstr_to_str(arg1) };

    if arg1_str == "-l" || arg1_str == "--list" {
        // List available layouts
        list_layouts();
        return 0;
    } else if arg1_str == "-h" || arg1_str == "--help" {
        print_help();
        return 0;
    } else {
        // Set keyboard layout
        let layout = arg1_str;
        if set_layout(layout) {
            puts("Keyboard layout set to: ");
            puts(layout);
            puts("\n");
            return 0;
        } else {
            puts("Error: Unknown layout '");
            puts(layout);
            puts("'\n");
            puts("Use 'loadkeys -l' to list available layouts.\n");
            return 1;
        }
    }
}

fn print_help() {
    puts("loadkeys - load keyboard layout\n\n");
    puts("Usage:\n");
    puts("  loadkeys <layout>  Set keyboard layout\n");
    puts("  loadkeys -l        List available layouts\n");
    puts("  loadkeys -h        Show this help\n");
    puts("  loadkeys           Show current layout\n\n");
    puts("Available layouts: us, uk, de, fr\n");
}

fn show_current_layout() {
    let mut buf = [0u8; 32];
    let result = syscall::syscall2(
        syscall::SYS_GETKEYMAP,
        buf.as_mut_ptr() as usize,
        buf.len(),
    );

    if result >= 0 {
        puts("Current keyboard layout: ");
        // Print until null terminator
        for &b in buf.iter() {
            if b == 0 {
                break;
            }
            putchar(b as i32);
        }
        puts("\n");
    } else {
        puts("Error getting keyboard layout\n");
    }
}

fn list_layouts() {
    puts("Available keyboard layouts:\n");
    puts("  us   - US QWERTY (default)\n");
    puts("  uk   - UK QWERTY\n");
    puts("  de   - German QWERTZ\n");
    puts("  fr   - French AZERTY\n");
}

fn set_layout(name: &str) -> bool {
    let result = syscall::syscall2(
        syscall::SYS_SETKEYMAP,
        name.as_ptr() as usize,
        name.len(),
    );
    result == 0
}

/// Convert C string to &str (unsafe - assumes valid UTF-8)
unsafe fn cstr_to_str(s: *const u8) -> &'static str {
    if s.is_null() {
        return "";
    }
    let mut len = 0;
    unsafe {
        while *s.add(len) != 0 {
            len += 1;
        }
        core::str::from_utf8_unchecked(core::slice::from_raw_parts(s, len))
    }
}

fn putchar(c: i32) {
    let ch = c as u8;
    write(STDOUT_FILENO, &[ch]);
}
