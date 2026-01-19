//! touch - create file or update timestamp

#![no_std]
#![no_main]

use libc::*;

#[unsafe(no_mangle)]
fn main(argc: i32, argv: *const *const u8) -> i32 {
    if argc < 2 {
        eprintln("usage: touch <file>...");
        return 1;
    }

    let mut status = 0;

    for i in 1..argc {
        let path = unsafe { cstr_to_str(*argv.add(i as usize)) };

        // Try to open existing file, or create new one
        let fd = open(path, O_WRONLY | O_CREAT, 0o644);
        if fd < 0 {
            eprint("touch: cannot touch '");
            print(path);
            eprintln("'");
            status = 1;
        } else {
            close(fd);
        }
    }

    status
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
