//! rm - remove files or directories
//!
//! Enhanced implementation with:
//! - Recursive directory removal (-r, -R)
//! - Force mode (-f) - ignore nonexistent files
//! - Interactive mode (-i) - prompt before each removal
//! - Verbose mode (-v) - show what's being removed
//! - Multiple file/directory arguments
//! - Directory removal
//! - Error handling

#![no_std]
#![no_main]

use libc::*;

const MAX_PATH: usize = 256;
const MAX_DEPTH: usize = 32;

struct RmConfig {
    recursive: bool,
    force: bool,
    interactive: bool,
    verbose: bool,
}

impl RmConfig {
    fn new() -> Self {
        RmConfig {
            recursive: false,
            force: false,
            interactive: false,
            verbose: false,
        }
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

/// Check if path is a directory
fn is_directory(path: &str) -> bool {
    let fd = open(path, O_RDONLY | O_DIRECTORY, 0);
    if fd >= 0 {
        close(fd);
        true
    } else {
        false
    }
}

/// Prompt user for confirmation
fn confirm(message: &str) -> bool {
    prints(message);
    prints("? ");

    let mut buf = [0u8; 16];
    let n = read(STDIN_FILENO, &mut buf);

    n > 0 && (buf[0] == b'y' || buf[0] == b'Y')
}

/// Remove a single file
fn remove_file(path: &str, config: &RmConfig) -> i32 {
    if config.interactive {
        prints("rm: remove file '");
        prints(path);
        if !confirm("'") {
            return 0;
        }
    }

    if sys_unlink(path) < 0 {
        if !config.force {
            eprints("rm: cannot remove '");
            prints(path);
            eprintlns("'");
            return 1;
        }
        return 0;
    }

    if config.verbose {
        prints("removed '");
        prints(path);
        printlns("'");
    }

    0
}

/// Remove directory recursively
fn remove_directory(path: &str, config: &RmConfig, depth: usize) -> i32 {
    if depth >= MAX_DEPTH {
        eprints("rm: directory nesting too deep: '");
        prints(path);
        eprintlns("'");
        return 1;
    }

    if !config.recursive {
        eprints("rm: cannot remove '");
        prints(path);
        eprintlns("': Is a directory");
        return 1;
    }

    if config.interactive {
        prints("rm: descend into directory '");
        prints(path);
        if !confirm("'") {
            return 0;
        }
    }

    // Open directory
    let fd = open(path, O_RDONLY | O_DIRECTORY, 0);
    if fd < 0 {
        if !config.force {
            eprints("rm: cannot open directory '");
            prints(path);
            eprintlns("'");
            return 1;
        }
        return 0;
    }

    // Read directory entries
    let mut buf = [0u8; 4096];
    let mut ret = 0;
    let mut entries: [[u8; MAX_PATH]; 64] = [[0; MAX_PATH]; 64];
    let mut entry_types: [u8; 64] = [0; 64];
    let mut entry_count = 0;

    loop {
        let n = sys_getdents(fd, &mut buf);
        if n <= 0 {
            break;
        }

        let mut offset = 0;
        while offset < n as usize {
            // Parse dirent structure
            let d_reclen = u16::from_ne_bytes([buf[offset + 16], buf[offset + 17]]);
            let d_type = buf[offset + 18];

            // Name starts after header
            let name_start = offset + 19;
            let mut name_len = 0;
            while name_start + name_len < buf.len() && buf[name_start + name_len] != 0 {
                name_len += 1;
            }

            if name_len > 0 {
                let name = &buf[name_start..name_start + name_len];

                // Skip . and ..
                if !(name[0] == b'.' && (name_len == 1 || (name_len == 2 && name[1] == b'.'))) {
                    if entry_count < 64 {
                        // Store entry for processing after closing directory
                        let copy_len = if name_len > MAX_PATH - 1 {
                            MAX_PATH - 1
                        } else {
                            name_len
                        };
                        entries[entry_count][..copy_len].copy_from_slice(&name[..copy_len]);
                        entry_types[entry_count] = d_type;
                        entry_count += 1;
                    }
                }
            }

            offset += d_reclen as usize;
        }
    }

    close(fd);

    // Process entries
    for i in 0..entry_count {
        let entry_len = entries[i].iter().position(|&c| c == 0).unwrap_or(MAX_PATH);
        let entry_name = core::str::from_utf8(&entries[i][..entry_len]).unwrap_or("");

        // Build full path
        let mut full_path = [0u8; MAX_PATH];
        let mut pos = 0;

        for &b in path.as_bytes() {
            if pos < MAX_PATH - 1 {
                full_path[pos] = b;
                pos += 1;
            }
        }
        if pos > 0 && full_path[pos - 1] != b'/' {
            if pos < MAX_PATH - 1 {
                full_path[pos] = b'/';
                pos += 1;
            }
        }
        for &b in entry_name.as_bytes() {
            if pos < MAX_PATH - 1 {
                full_path[pos] = b;
                pos += 1;
            }
        }

        let full_path_str = core::str::from_utf8(&full_path[..pos]).unwrap_or("");

        // Remove entry
        if entry_types[i] == 4 {
            // DT_DIR
            if remove_directory(full_path_str, config, depth + 1) != 0 {
                ret = 1;
            }
        } else {
            if remove_file(full_path_str, config) != 0 {
                ret = 1;
            }
        }
    }

    // Remove the now-empty directory
    if config.interactive {
        prints("rm: remove directory '");
        prints(path);
        if !confirm("'") {
            return ret;
        }
    }

    if sys_rmdir(path) < 0 {
        if !config.force {
            eprints("rm: cannot remove directory '");
            prints(path);
            eprintlns("'");
            return 1;
        }
    } else if config.verbose {
        prints("removed directory '");
        prints(path);
        printlns("'");
    }

    ret
}

#[unsafe(no_mangle)]
fn main(argc: i32, argv: *const *const u8) -> i32 {
    if argc < 2 {
        eprintlns("usage: rm [options] FILE...");
        eprintlns("Options:");
        eprintlns("  -r, -R    Remove directories recursively");
        eprintlns("  -f        Force (ignore nonexistent files, no prompt)");
        eprintlns("  -i        Interactive (prompt before removal)");
        eprintlns("  -v        Verbose (show what's being removed)");
        return 1;
    }

    let mut config = RmConfig::new();
    let mut arg_idx = 1;

    // Parse options
    while arg_idx < argc {
        let arg_ptr = unsafe { *argv.add(arg_idx as usize) };
        let arg = cstr_to_str(arg_ptr);

        if arg.starts_with('-') && arg.len() > 1 {
            for c in arg[1..].bytes() {
                match c {
                    b'r' | b'R' => config.recursive = true,
                    b'f' => {
                        config.force = true;
                        config.interactive = false;
                    }
                    b'i' => {
                        config.interactive = true;
                        config.force = false;
                    }
                    b'v' => config.verbose = true,
                    _ => {
                        eprints("rm: unknown option: -");
                        putchar(c);
                        printlns("");
                        return 1;
                    }
                }
            }
            arg_idx += 1;
        } else {
            break;
        }
    }

    if arg_idx >= argc {
        if !config.force {
            eprintlns("rm: missing operand");
        }
        return 1;
    }

    let mut status = 0;

    // Remove each specified file/directory
    for i in arg_idx..argc {
        let path = cstr_to_str(unsafe { *argv.add(i as usize) });

        if is_directory(path) {
            if remove_directory(path, &config, 0) != 0 {
                status = 1;
            }
        } else {
            if remove_file(path, &config) != 0 {
                status = 1;
            }
        }
    }

    status
}
