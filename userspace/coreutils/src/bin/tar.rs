//! tar - tape archive utility

#![no_std]
#![no_main]

use libc::*;

// TAR header format (POSIX ustar)
#[repr(C)]
struct TarHeader {
    name: [u8; 100],
    mode: [u8; 8],
    uid: [u8; 8],
    gid: [u8; 8],
    size: [u8; 12],
    mtime: [u8; 12],
    checksum: [u8; 8],
    typeflag: u8,
    linkname: [u8; 100],
    magic: [u8; 6],
    version: [u8; 2],
    uname: [u8; 32],
    gname: [u8; 32],
    devmajor: [u8; 8],
    devminor: [u8; 8],
    prefix: [u8; 155],
    padding: [u8; 12],
}

#[unsafe(no_mangle)]
fn main(argc: i32, argv: *const *const u8) -> i32 {
    if argc < 2 {
        eprintlns("usage: tar [-c|-x|-t] [-f archive] [files...]");
        eprintlns("  -c  create archive");
        eprintlns("  -x  extract archive");
        eprintlns("  -t  list contents");
        eprintlns("  -f  archive file (default: stdout/stdin)");
        return 1;
    }

    let mut create = false;
    let mut extract = false;
    let mut list = false;
    let mut archive_file: Option<&str> = None;
    let mut file_start = argc;

    // Parse options
    let mut i = 1;
    while i < argc {
        let arg = unsafe { cstr_to_str(*argv.add(i as usize)) };
        
        if arg.starts_with('-') && !arg.starts_with("--") {
            // Parse flag characters
            for &c in arg.as_bytes()[1..].iter() {
                match c {
                    b'c' => create = true,
                    b'x' => extract = true,
                    b't' => list = true,
                    b'f' => {
                        i += 1;
                        if i < argc {
                            archive_file = Some(unsafe { cstr_to_str(*argv.add(i as usize)) });
                        }
                    }
                    b'v' => {} // Verbose - ignored for now
                    b'z' => {} // Gzip - ignored for now
                    _ => {
                        eprints("tar: invalid option: ");
                        putchar(c);
                        eprintlns("");
                        return 1;
                    }
                }
            }
        } else {
            // First non-option argument
            file_start = i;
            break;
        }
        
        i += 1;
    }

    // Validate options
    let mode_count = (create as i32) + (extract as i32) + (list as i32);
    if mode_count != 1 {
        eprintlns("tar: must specify exactly one of -c, -x, or -t");
        return 1;
    }

    if create {
        prints("tar: create mode - archive='");
        if let Some(f) = archive_file {
            prints(f);
        } else {
            prints("stdout");
        }
        printlns("'");
        
        if file_start < argc {
            prints("tar: would add files: ");
            for j in file_start..argc {
                let file = unsafe { cstr_to_str(*argv.add(j as usize)) };
                prints(file);
                prints(" ");
            }
            printlns("");
        }
        
        eprintlns("tar: create mode not yet fully implemented");
    } else if extract {
        prints("tar: extract mode - archive='");
        if let Some(f) = archive_file {
            prints(f);
        } else {
            prints("stdin");
        }
        printlns("'");
        
        eprintlns("tar: extract mode not yet fully implemented");
    } else if list {
        prints("tar: list mode - archive='");
        if let Some(f) = archive_file {
            prints(f);
        } else {
            prints("stdin");
        }
        printlns("'");
        
        eprintlns("tar: list mode not yet fully implemented");
    }

    1
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

trait StrExt {
    fn starts_with(&self, prefix: &str) -> bool;
}

impl StrExt for &str {
    fn starts_with(&self, prefix: &str) -> bool {
        if self.len() < prefix.len() {
            return false;
        }
        let self_bytes = self.as_bytes();
        let prefix_bytes = prefix.as_bytes();
        for i in 0..prefix.len() {
            if self_bytes[i] != prefix_bytes[i] {
                return false;
            }
        }
        true
    }
}
