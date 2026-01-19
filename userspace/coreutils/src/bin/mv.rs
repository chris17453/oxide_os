//! mv - move/rename files

#![no_std]
#![no_main]

use libc::*;

#[unsafe(no_mangle)]
fn main(argc: i32, argv: *const *const u8) -> i32 {
    if argc < 3 {
        eprintln("usage: mv <source> <dest>");
        return 1;
    }

    let src = unsafe { cstr_to_str(*argv.add(1)) };
    let dst = unsafe { cstr_to_str(*argv.add(2)) };

    // Try rename syscall first
    if sys_rename(src, dst) == 0 {
        return 0;
    }

    // If rename fails (cross-device), fall back to copy + delete
    // Open source file
    let src_fd = open2(src, O_RDONLY);
    if src_fd < 0 {
        eprint("mv: cannot stat '");
        print(src);
        eprintln("'");
        return 1;
    }

    // Open/create destination file
    let dst_fd = open(dst, O_WRONLY | O_CREAT | O_TRUNC, 0o644);
    if dst_fd < 0 {
        eprint("mv: cannot create '");
        print(dst);
        eprintln("'");
        close(src_fd);
        return 1;
    }

    // Copy contents
    let mut buf = [0u8; 4096];
    loop {
        let n = read(src_fd, &mut buf);
        if n <= 0 {
            break;
        }
        write(dst_fd, &buf[..n as usize]);
    }

    close(src_fd);
    close(dst_fd);

    // Delete source
    sys_unlink(src);

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
