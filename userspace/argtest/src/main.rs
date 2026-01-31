//! Argument testing utility - prints all arguments received

#![no_std]
#![no_main]

use libc::*;

#[unsafe(no_mangle)]
pub extern "C" fn main(argc: i32, argv: *const *const u8) -> i32 {
    prints("=== ARGTEST ===\n");

    prints("argc = ");
    if argc == 0 { prints("0"); }
    else if argc == 1 { prints("1"); }
    else if argc == 2 { prints("2"); }
    else if argc == 3 { prints("3"); }
    else if argc == 4 { prints("4"); }
    else if argc == 5 { prints("5"); }
    else { prints("?"); }
    prints("\n");

    prints("argv ptr = 0x");
    print_hex(argv as u64);
    prints("\n");

    // Print each argument
    for i in 0..argc {
        prints("argv[");
        if i == 0 { prints("0"); }
        else if i == 1 { prints("1"); }
        else if i == 2 { prints("2"); }
        else if i == 3 { prints("3"); }
        else if i == 4 { prints("4"); }
        else { prints("?"); }
        prints("] = ");

        let arg_ptr = unsafe { *argv.add(i as usize) };
        prints("0x");
        print_hex(arg_ptr as u64);
        prints(" -> \"");

        if arg_ptr.is_null() {
            prints("(null)");
        } else {
            // Print the string
            let mut j = 0;
            unsafe {
                while j < 256 {
                    let ch = *arg_ptr.add(j);
                    if ch == 0 {
                        break;
                    }
                    putchar(ch as u8);
                    j += 1;
                }
            }
        }
        prints("\"\n");
    }

    prints("===============\n");
    0
}

fn print_hex(mut val: u64) {
    let hex_chars = b"0123456789abcdef";
    let mut buf = [0u8; 16];
    for i in 0..16 {
        buf[15 - i] = hex_chars[(val & 0xF) as usize];
        val >>= 4;
    }
    for &ch in &buf {
        putchar(ch);
    }
}
