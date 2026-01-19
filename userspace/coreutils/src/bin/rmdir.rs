//! rmdir - remove empty directories

#![no_std]
#![no_main]

use efflux_libc::*;

#[unsafe(no_mangle)]
fn main(argc: i32, argv: *const *const u8) -> i32 {
    if argc < 2 {
        eprintln("usage: rmdir <directory>...");
        return 1;
    }

    let mut status = 0;

    for i in 1..argc {
        let path = unsafe { cstr_to_str(*argv.add(i as usize)) };

        if sys_rmdir(path) < 0 {
            eprint("rmdir: failed to remove '");
            print(path);
            eprintln("'");
            status = 1;
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
