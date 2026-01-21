//! mkdir - make directories

#![no_std]
#![no_main]

use libc::*;

#[unsafe(no_mangle)]
fn main(argc: i32, argv: *const *const u8) -> i32 {
    if argc < 2 {
        eprintlns("Usage: mkdir DIRECTORY...");
        return 1;
    }

    let mut success = true;

    // Create each directory specified
    for i in 1..argc {
        let arg_ptr = unsafe { *argv.offset(i as isize) };

        // Find the length of the null-terminated string
        let mut len = 0;
        while unsafe { *arg_ptr.offset(len as isize) != 0 } {
            len += 1;
        }

        // Create directory
        let result = mkdir(arg_ptr, len as usize, 0o755);

        if result < 0 {
            eprints("mkdir: cannot create directory '");

            // Print the directory name
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
