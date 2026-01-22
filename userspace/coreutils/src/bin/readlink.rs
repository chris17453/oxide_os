//! readlink - Print resolved symbolic links or canonical file names
//!
//! Print the value of a symbolic link or canonical file name.

#![no_std]
#![no_main]

use libc::syscall::sys_readlink;
use libc::*;

/// Convert a C string pointer to a Rust str slice
fn ptr_to_str(ptr: *const u8) -> &'static str {
    unsafe {
        let mut len = 0;
        while *ptr.add(len) != 0 {
            len += 1;
        }
        core::str::from_utf8_unchecked(core::slice::from_raw_parts(ptr, len))
    }
}

#[unsafe(no_mangle)]
fn main(argc: i32, argv: *const *const u8) -> i32 {
    if argc < 2 {
        eprintlns("usage: readlink [-f] file...");
        return 1;
    }

    let mut canonicalize = false;
    let mut file_start = 1i32;

    // Parse -f option (canonicalize)
    let arg1 = ptr_to_str(unsafe { *argv.add(1) });
    if arg1 == "-f" {
        canonicalize = true;
        file_start = 2;
    }

    let mut status = 0;

    for i in file_start..argc {
        let filename = ptr_to_str(unsafe { *argv.add(i as usize) });

        // Read the symlink target
        let mut buf = [0u8; 1024];
        let n = sys_readlink(filename, &mut buf);

        if n < 0 {
            if !canonicalize {
                // readlink without -f fails silently for non-links
                status = 1;
            } else {
                // With -f, just print the original path for non-links
                prints(filename);
                printlns("");
            }
        } else {
            // Print the link target
            for j in 0..n as usize {
                putchar(buf[j]);
            }
            printlns("");
        }
    }

    status
}
