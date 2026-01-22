//! stat - display file status

#![no_std]
#![no_main]

use libc::*;

#[repr(C)]
struct Stat {
    st_dev: u64,
    st_ino: u64,
    st_mode: u32,
    st_nlink: u32,
    st_uid: u32,
    st_gid: u32,
    st_rdev: u64,
    st_size: i64,
    st_blksize: i64,
    st_blocks: i64,
    st_atime: i64,
    st_mtime: i64,
    st_ctime: i64,
}

#[unsafe(no_mangle)]
fn main(argc: i32, argv: *const *const u8) -> i32 {
    if argc < 2 {
        eprintlns("usage: stat <file>...");
        return 1;
    }

    let mut status = 0;

    for i in 1..argc {
        let path = unsafe { cstr_to_str(*argv.add(i as usize)) };

        let mut st = Stat {
            st_dev: 0,
            st_ino: 0,
            st_mode: 0,
            st_nlink: 0,
            st_uid: 0,
            st_gid: 0,
            st_rdev: 0,
            st_size: 0,
            st_blksize: 0,
            st_blocks: 0,
            st_atime: 0,
            st_mtime: 0,
            st_ctime: 0,
        };

        let result = sys_stat(path, &mut st);
        if result < 0 {
            eprints("stat: cannot stat '");
            prints(path);
            eprintlns("': No such file or directory");
            status = 1;
            continue;
        }

        prints("  File: ");
        printlns(path);

        prints("  Size: ");
        print_i64(st.st_size);
        prints("\t\tBlocks: ");
        print_i64(st.st_blocks);
        prints("\t\tIO Block: ");
        print_i64(st.st_blksize);
        prints("\t");
        print_file_type(st.st_mode);
        printlns("");

        prints("Device: ");
        print_u64(st.st_dev);
        prints("\tInode: ");
        print_u64(st.st_ino);
        prints("\tLinks: ");
        print_u64(st.st_nlink as u64);
        printlns("");

        prints("Access: (");
        print_octal(st.st_mode & 0o7777);
        prints("/");
        print_perms(st.st_mode);
        prints(")  Uid: (");
        print_u64(st.st_uid as u64);
        prints(")   Gid: (");
        print_u64(st.st_gid as u64);
        printlns(")");

        prints("Access: ");
        print_time(st.st_atime);
        printlns("");

        prints("Modify: ");
        print_time(st.st_mtime);
        printlns("");

        prints("Change: ");
        print_time(st.st_ctime);
        printlns("");

        if i < argc - 1 {
            printlns("");
        }
    }

    status
}

fn sys_stat(path: &str, st: &mut Stat) -> i32 {
    use libc::syscall::{nr, syscall4};
    syscall4(
        nr::STAT,
        path.as_ptr() as usize,
        path.len(),
        st as *mut Stat as usize,
        0,
    ) as i32
}

fn print_file_type(mode: u32) {
    let file_type = mode & 0o170000;
    match file_type {
        0o140000 => prints("socket"),
        0o120000 => prints("symbolic link"),
        0o100000 => prints("regular file"),
        0o060000 => prints("block special file"),
        0o040000 => prints("directory"),
        0o020000 => prints("character special file"),
        0o010000 => prints("FIFO"),
        _ => prints("unknown"),
    }
}

fn print_perms(mode: u32) {
    // User
    putchar(if mode & 0o400 != 0 { b'r' } else { b'-' });
    putchar(if mode & 0o200 != 0 { b'w' } else { b'-' });
    if mode & 0o4000 != 0 {
        putchar(if mode & 0o100 != 0 { b's' } else { b'S' });
    } else {
        putchar(if mode & 0o100 != 0 { b'x' } else { b'-' });
    }

    // Group
    putchar(if mode & 0o040 != 0 { b'r' } else { b'-' });
    putchar(if mode & 0o020 != 0 { b'w' } else { b'-' });
    if mode & 0o2000 != 0 {
        putchar(if mode & 0o010 != 0 { b's' } else { b'S' });
    } else {
        putchar(if mode & 0o010 != 0 { b'x' } else { b'-' });
    }

    // Other
    putchar(if mode & 0o004 != 0 { b'r' } else { b'-' });
    putchar(if mode & 0o002 != 0 { b'w' } else { b'-' });
    if mode & 0o1000 != 0 {
        putchar(if mode & 0o001 != 0 { b't' } else { b'T' });
    } else {
        putchar(if mode & 0o001 != 0 { b'x' } else { b'-' });
    }
}

fn print_octal(n: u32) {
    let mut buf = [b'0'; 4];
    let mut val = n;
    let mut pos = 3;

    loop {
        buf[pos] = b'0' + (val % 8) as u8;
        val /= 8;
        if val == 0 || pos == 0 {
            break;
        }
        pos -= 1;
    }

    for i in pos..4 {
        putchar(buf[i]);
    }
}

fn print_time(timestamp: i64) {
    // Simple timestamp display - just show the raw value
    // A full implementation would convert to date/time string
    print_i64(timestamp);
}

fn print_i64(n: i64) {
    if n < 0 {
        putchar(b'-');
        print_u64((-n) as u64);
    } else {
        print_u64(n as u64);
    }
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
