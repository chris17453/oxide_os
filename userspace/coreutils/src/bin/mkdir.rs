//! mkdir - make directories
//!
//! Enhanced implementation with:
//! - Parent directory creation (-p)
//! - Mode specification (-m)
//! - Verbose output (-v)
//! - Multiple directory arguments
//! - Proper error handling

#![no_std]
#![no_main]

use libc::*;

const MAX_PATH: usize = 256;

struct MkdirConfig {
    parents: bool,
    mode: u32,
    verbose: bool,
}

impl MkdirConfig {
    fn new() -> Self {
        MkdirConfig {
            parents: false,
            mode: 0o755,
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

/// Parse octal mode string (e.g., "0755", "755")
fn parse_mode(s: &str) -> Option<u32> {
    let s = s.strip_prefix("0").unwrap_or(s);
    let mut mode = 0u32;

    for c in s.bytes() {
        if c >= b'0' && c <= b'7' {
            mode = mode * 8 + (c - b'0') as u32;
        } else {
            return None;
        }
    }

    Some(mode)
}

/// Check if directory exists
fn dir_exists(path: &str) -> bool {
    let fd = open(path, O_RDONLY | O_DIRECTORY, 0);
    if fd >= 0 {
        close(fd);
        true
    } else {
        false
    }
}

/// Create directory with optional parent creation
fn create_directory(path: &str, config: &MkdirConfig) -> i32 {
    if dir_exists(path) {
        // Directory already exists
        if !config.parents {
            eprints("mkdir: cannot create directory '");
            prints(path);
            eprintlns("': File exists");
            return 1;
        }
        return 0;
    }

    // Try to create directory
    if sys_mkdir(path, config.mode) == 0 {
        if config.verbose {
            prints("mkdir: created directory '");
            prints(path);
            printlns("'");
        }
        return 0;
    }

    // If -p flag, try creating parent directories
    if config.parents {
        // Find the last slash
        let bytes = path.as_bytes();
        let mut last_slash = None;

        for i in (0..bytes.len()).rev() {
            if bytes[i] == b'/' {
                last_slash = Some(i);
                break;
            }
        }

        if let Some(slash_pos) = last_slash {
            if slash_pos > 0 {
                // Create parent directory first
                let mut parent = [0u8; MAX_PATH];
                let parent_len = if slash_pos > MAX_PATH - 1 {
                    MAX_PATH - 1
                } else {
                    slash_pos
                };
                parent[..parent_len].copy_from_slice(&bytes[..parent_len]);
                let parent_str = core::str::from_utf8(&parent[..parent_len]).unwrap_or("");

                // Recursively create parent
                if create_directory(parent_str, config) != 0 {
                    return 1;
                }

                // Now try creating this directory again
                if sys_mkdir(path, config.mode) == 0 {
                    if config.verbose {
                        prints("mkdir: created directory '");
                        prints(path);
                        printlns("'");
                    }
                    return 0;
                }
            }
        }
    }

    eprints("mkdir: cannot create directory '");
    prints(path);
    eprintlns("'");
    1
}

#[unsafe(no_mangle)]
fn main(argc: i32, argv: *const *const u8) -> i32 {
    if argc < 2 {
        eprintlns("usage: mkdir [options] DIRECTORY...");
        eprintlns("Options:");
        eprintlns("  -p        Create parent directories as needed");
        eprintlns("  -m MODE   Set file mode (octal, default 755)");
        eprintlns("  -v        Verbose output");
        return 1;
    }

    let mut config = MkdirConfig::new();
    let mut arg_idx = 1;

    // Parse options
    while arg_idx < argc {
        let arg_ptr = unsafe { *argv.add(arg_idx as usize) };
        let arg = cstr_to_str(arg_ptr);

        if arg.starts_with('-') && arg.len() > 1 && arg != "--" {
            if arg == "-m" {
                // Mode specification
                arg_idx += 1;
                if arg_idx >= argc {
                    eprintlns("mkdir: option -m requires an argument");
                    return 1;
                }
                let mode_str = cstr_to_str(unsafe { *argv.add(arg_idx as usize) });
                match parse_mode(mode_str) {
                    Some(mode) => config.mode = mode,
                    None => {
                        eprints("mkdir: invalid mode: '");
                        prints(mode_str);
                        eprintlns("'");
                        return 1;
                    }
                }
                arg_idx += 1;
            } else {
                // Parse character flags
                for c in arg[1..].bytes() {
                    match c {
                        b'p' => config.parents = true,
                        b'v' => config.verbose = true,
                        b'm' => {
                            // -m might be combined like -m755
                            let rest = &arg[(arg.as_bytes().iter().position(|&x| x == b'm').unwrap() + 1)..];
                            if !rest.is_empty() {
                                match parse_mode(rest) {
                                    Some(mode) => config.mode = mode,
                                    None => {
                                        eprints("mkdir: invalid mode: '");
                                        prints(rest);
                                        eprintlns("'");
                                        return 1;
                                    }
                                }
                            } else {
                                // Next argument is mode
                                arg_idx += 1;
                                if arg_idx >= argc {
                                    eprintlns("mkdir: option -m requires an argument");
                                    return 1;
                                }
                                let mode_str = cstr_to_str(unsafe { *argv.add(arg_idx as usize) });
                                match parse_mode(mode_str) {
                                    Some(mode) => config.mode = mode,
                                    None => {
                                        eprints("mkdir: invalid mode: '");
                                        prints(mode_str);
                                        eprintlns("'");
                                        return 1;
                                    }
                                }
                            }
                            break; // Stop processing flags after -m
                        }
                        _ => {
                            eprints("mkdir: unknown option: -");
                            putchar(c);
                            printlns("");
                            return 1;
                        }
                    }
                }
                arg_idx += 1;
            }
        } else {
            break;
        }
    }

    if arg_idx >= argc {
        eprintlns("mkdir: missing operand");
        return 1;
    }

    let mut status = 0;

    // Create each specified directory
    for i in arg_idx..argc {
        let path = cstr_to_str(unsafe { *argv.add(i as usize) });

        if create_directory(path, &config) != 0 {
            status = 1;
        }
    }

    status
}
