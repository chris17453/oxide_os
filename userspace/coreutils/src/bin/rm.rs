//! rm - remove files

#![no_std]
#![no_main]

use libc::*;

#[unsafe(no_mangle)]
fn main(argc: i32, argv: *const *const u8) -> i32 {
    if argc < 2 {
        eprintlns("Usage: rm FILE...");
        return 1;
    }

    let mut success = true;

    // Remove each file specified
    for i in 1..argc {
        let arg_ptr = unsafe { *argv.offset(i as isize) };

        // Find the length of the null-terminated string
        let mut len = 0;
        while unsafe { *arg_ptr.offset(len as isize) != 0 } {
            len += 1;
        }

        // Try to unlink (remove file)
        let result = unlink(arg_ptr, len as usize);

        if result < 0 {
            eprints("rm: cannot remove '");

            // Print the file name
            for j in 0..len {
                let ch = unsafe { *arg_ptr.offset(j as isize) };
                eputchar(ch as i32);
            }

            eprintlns("'");
            success = false;
        }
    }

    if success { 0 } else { 1 }
}
