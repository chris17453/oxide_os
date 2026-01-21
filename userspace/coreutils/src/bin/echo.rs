//! echo - display a line of text

#![no_std]
#![no_main]

use libc::*;

#[unsafe(no_mangle)]
fn main(argc: i32, argv: *const *const u8) -> i32 {
    let mut first = true;

    // Print each argument separated by spaces
    for i in 1..argc {
        if !first {
            putchar(b' ' as i32);
        }
        first = false;

        let arg_ptr = unsafe { *argv.offset(i as isize) };

        // Print the null-terminated string
        let mut offset = 0;
        loop {
            let ch = unsafe { *arg_ptr.offset(offset) };
            if ch == 0 {
                break;
            }
            putchar(ch as i32);
            offset += 1;
        }
    }

    putchar(b'\n' as i32);
    0
}
