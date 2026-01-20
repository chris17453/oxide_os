//! ln - create links

#![no_std]
#![no_main]

use libc::*;

#[unsafe(no_mangle)]
fn main(argc: i32, argv: *const *const u8) -> i32 {
    if argc < 3 {
        eprintlns("usage: ln [-s] <target> <link_name>");
        return 1;
    }

    let mut symbolic = false;
    let mut arg_start = 1;

    // Check for -s flag
    let arg1 = unsafe { cstr_to_str(*argv.add(1)) };
    if arg1 == "-s" {
        symbolic = true;
        arg_start = 2;
        if argc < 4 {
            eprintlns("usage: ln [-s] <target> <link_name>");
            return 1;
        }
    }

    let target = unsafe { cstr_to_str(*argv.add(arg_start as usize)) };
    let link_name = unsafe { cstr_to_str(*argv.add((arg_start + 1) as usize)) };

    let result = if symbolic {
        sys_symlink(target, link_name)
    } else {
        sys_link(target, link_name)
    };

    if result < 0 {
        eprints("ln: failed to create ");
        if symbolic {
            eprints("symbolic ");
        }
        eprints("link '");
        prints(link_name);
        eprints("' -> '");
        prints(target);
        eprintlns("'");
        return 1;
    }

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
