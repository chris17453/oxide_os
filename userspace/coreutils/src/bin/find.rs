//! find - search for files in a directory hierarchy

#![no_std]
#![no_main]

use libc::*;

const MAX_PATH: usize = 512;
const MAX_DEPTH: usize = 32;

#[unsafe(no_mangle)]
fn main(argc: i32, argv: *const *const u8) -> i32 {
    let mut start_path = ".";
    let mut name_pattern: Option<&str> = None;
    let mut type_filter: Option<u8> = None; // b'f' for file, b'd' for directory
    let mut arg_idx = 1;

    // First argument might be path
    if arg_idx < argc {
        let arg = unsafe { cstr_to_str(*argv.add(arg_idx as usize)) };
        if !arg.starts_with('-') {
            start_path = arg;
            arg_idx += 1;
        }
    }

    // Parse options
    while arg_idx < argc {
        let arg = unsafe { cstr_to_str(*argv.add(arg_idx as usize)) };
        if arg == "-name" {
            arg_idx += 1;
            if arg_idx < argc {
                name_pattern = Some(unsafe { cstr_to_str(*argv.add(arg_idx as usize)) });
            }
            arg_idx += 1;
        } else if arg == "-type" {
            arg_idx += 1;
            if arg_idx < argc {
                let type_arg = unsafe { cstr_to_str(*argv.add(arg_idx as usize)) };
                if !type_arg.is_empty() {
                    type_filter = Some(type_arg.as_bytes()[0]);
                }
            }
            arg_idx += 1;
        } else {
            arg_idx += 1;
        }
    }

    // Start traversal
    let mut path_buf = [0u8; MAX_PATH];
    let start_len = start_path.len().min(MAX_PATH - 1);
    path_buf[..start_len].copy_from_slice(&start_path.as_bytes()[..start_len]);

    find_recursive(&mut path_buf, start_len, name_pattern, type_filter, 0);

    0
}

fn find_recursive(path: &mut [u8; MAX_PATH], path_len: usize,
                  name_pattern: Option<&str>, type_filter: Option<u8>,
                  depth: usize) {
    if depth >= MAX_DEPTH {
        return;
    }

    // Open directory
    let path_str = bytes_to_str(&path[..path_len]);
    let fd = open2(path_str, O_RDONLY | O_DIRECTORY);
    if fd < 0 {
        // Not a directory, might be a file - check if it matches
        let fd = open2(path_str, O_RDONLY);
        if fd >= 0 {
            close(fd);
            // It's a file, check filters
            if check_match(&path[..path_len], name_pattern, type_filter, false) {
                print_path(&path[..path_len]);
            }
        }
        return;
    }

    // Check if directory itself matches
    if check_match(&path[..path_len], name_pattern, type_filter, true) {
        print_path(&path[..path_len]);
    }

    // Read directory entries
    let mut buf = [0u8; 4096];
    loop {
        let n = sys_getdents(fd, &mut buf);
        if n <= 0 {
            break;
        }

        let mut offset = 0;
        while offset < n as usize {
            // Parse dirent structure
            // struct dirent { u64 ino, u64 off, u16 reclen, u8 type, char name[] }
            if offset + 19 > n as usize {
                break;
            }

            let reclen = u16::from_ne_bytes([buf[offset + 16], buf[offset + 17]]) as usize;
            if reclen == 0 {
                break;
            }

            let dtype = buf[offset + 18];

            // Get name (null-terminated after d_type)
            let name_start = offset + 19;
            let mut name_len = 0;
            while name_start + name_len < offset + reclen && buf[name_start + name_len] != 0 {
                name_len += 1;
            }

            let name = &buf[name_start..name_start + name_len];

            // Skip . and ..
            if !(name == b"." || name == b"..") {
                // Build full path
                let new_len = path_len + 1 + name_len;
                if new_len < MAX_PATH {
                    let mut new_path = *path;
                    new_path[path_len] = b'/';
                    new_path[path_len + 1..path_len + 1 + name_len].copy_from_slice(name);

                    let is_dir = dtype == 4; // DT_DIR

                    if is_dir {
                        find_recursive(&mut new_path, new_len, name_pattern, type_filter, depth + 1);
                    } else {
                        // Check if file matches
                        if check_match(&new_path[..new_len], name_pattern, type_filter, false) {
                            print_path(&new_path[..new_len]);
                        }
                    }
                }
            }

            offset += reclen;
        }
    }

    close(fd);
}

fn check_match(path: &[u8], name_pattern: Option<&str>, type_filter: Option<u8>, is_dir: bool) -> bool {
    // Check type filter
    if let Some(t) = type_filter {
        match t {
            b'f' if is_dir => return false,
            b'd' if !is_dir => return false,
            _ => {}
        }
    }

    // Check name pattern
    if let Some(pattern) = name_pattern {
        let name = get_basename(path);
        if !matches_pattern(name, pattern) {
            return false;
        }
    }

    true
}

fn get_basename(path: &[u8]) -> &[u8] {
    let mut last_slash = 0;
    for i in 0..path.len() {
        if path[i] == b'/' {
            last_slash = i + 1;
        }
    }
    &path[last_slash..]
}

fn matches_pattern(name: &[u8], pattern: &str) -> bool {
    let pat = pattern.as_bytes();

    // Simple glob matching with * wildcard
    if pat.is_empty() {
        return name.is_empty();
    }

    let mut ni = 0;
    let mut pi = 0;
    let mut star_pi: Option<usize> = None;
    let mut star_ni = 0;

    while ni < name.len() {
        if pi < pat.len() && (pat[pi] == b'?' || pat[pi] == name[ni]) {
            ni += 1;
            pi += 1;
        } else if pi < pat.len() && pat[pi] == b'*' {
            star_pi = Some(pi);
            star_ni = ni;
            pi += 1;
        } else if let Some(sp) = star_pi {
            pi = sp + 1;
            star_ni += 1;
            ni = star_ni;
        } else {
            return false;
        }
    }

    while pi < pat.len() && pat[pi] == b'*' {
        pi += 1;
    }

    pi == pat.len()
}

fn print_path(path: &[u8]) {
    for &b in path {
        putchar(b);
    }
    putchar(b'\n');
}

fn bytes_to_str(bytes: &[u8]) -> &str {
    unsafe { core::str::from_utf8_unchecked(bytes) }
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
