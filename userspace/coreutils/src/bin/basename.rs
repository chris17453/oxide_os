//! basename - strip directory from filename
//!
//! Full-featured implementation with:
//! - Single file mode (default)
//! - Multiple file mode (-a)
//! - Suffix removal for all files (-s)
//! - Zero-terminated output (-z)
//! - Help message (-h)
//! - Proper error handling

#![no_std]
#![no_main]
#![allow(unused)]

use libc::*;

const MAX_PATH: usize = 4096;

struct BasenameConfig {
    multiple: bool,
    suffix: Option<[u8; MAX_PATH]>,
    suffix_len: usize,
    zero_terminated: bool,
}

impl BasenameConfig {
    fn new() -> Self {
        BasenameConfig {
            multiple: false,
            suffix: None,
            suffix_len: 0,
            zero_terminated: false,
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

fn str_starts_with(s: &str, prefix: &str) -> bool {
    if s.len() < prefix.len() {
        return false;
    }
    let s_bytes = s.as_bytes();
    let p_bytes = prefix.as_bytes();
    for i in 0..prefix.len() {
        if s_bytes[i] != p_bytes[i] {
            return false;
        }
    }
    true
}

fn basename_core<'a>(path: &'a str) -> &'a str {
    let bytes = path.as_bytes();
    let mut end = bytes.len();
    while end > 0 && bytes[end - 1] == b'/' {
        end -= 1;
    }
    if end == 0 {
        return "/";
    }
    let mut start = end;
    while start > 0 && bytes[start - 1] != b'/' {
        start -= 1;
    }
    &path[start..end]
}

fn strip_suffix<'a>(name: &'a str, suffix: Option<&'a str>) -> &'a str {
    if let Some(suf) = suffix {
        if name.ends_with(suf) {
            let new_len = name.len() - suf.len();
            return &name[..new_len];
        }
    }
    name
}

fn show_help() {
    eprintlns("Usage: basename NAME [SUFFIX]");
    eprintlns("   or: basename OPTION... NAME...");
    eprintlns("");
    eprintlns("Print NAME with any leading directory components removed.");
    eprintlns("If specified, also remove a trailing SUFFIX.");
    eprintlns("");
    eprintlns("Options:");
    eprintlns("  -a              Support multiple arguments and treat each as a NAME");
    eprintlns("  -s SUFFIX       Remove a trailing SUFFIX (implies -a)");
    eprintlns("  -z              End each output line with NUL, not newline");
    eprintlns("  -h              Show this help");
}

#[unsafe(no_mangle)]
fn main(argc: i32, argv: *const *const u8) -> i32 {
    let mut config = BasenameConfig::new();
    let mut arg_idx = 1;

    // Parse options
    while arg_idx < argc {
        let arg = unsafe { cstr_to_str(*argv.add(arg_idx as usize)) };

        if arg == "-h" || arg == "--help" {
            show_help();
            return 0;
        } else if arg == "-a" || arg == "--multiple" {
            config.multiple = true;
            arg_idx += 1;
        } else if arg == "-s" || arg == "--suffix" {
            arg_idx += 1;
            if arg_idx >= argc {
                eprintlns("basename: option -s requires an argument");
                return 1;
            }
            let suffix_str = unsafe { cstr_to_str(*argv.add(arg_idx as usize)) };
            let mut buf = [0u8; MAX_PATH];
            let copy_len = suffix_str.len().min(MAX_PATH);
            buf[..copy_len].copy_from_slice(&suffix_str.as_bytes()[..copy_len]);
            config.suffix = Some(buf);
            config.suffix_len = copy_len;
            config.multiple = true; // -s implies -a
            arg_idx += 1;
        } else if arg == "-z" || arg == "--zero" {
            config.zero_terminated = true;
            arg_idx += 1;
        } else if str_starts_with(arg, "-") && arg.len() > 1 && arg != "--" {
            // Handle combined short options
            let mut chars = arg.bytes().skip(1);
            let _valid = true;

            while let Some(c) = chars.next() {
                match c {
                    b'a' => config.multiple = true,
                    b'z' => config.zero_terminated = true,
                    b'h' => {
                        show_help();
                        return 0;
                    }
                    _ => {
                        eprints("basename: invalid option: -");
                        putchar(c);
                        eprintlns("");
                        return 1;
                    }
                }
            }
            arg_idx += 1;
        } else {
            break;
        }
    }

    // Check for arguments
    if arg_idx >= argc {
        eprintlns("basename: missing operand");
        eprintlns("Try 'basename -h' for more information.");
        return 1;
    }

    let delimiter = if config.zero_terminated { b'\0' } else { b'\n' };

    // Handle different modes
    if config.multiple {
        // Multiple file mode (-a or -s)
        for i in arg_idx..argc {
            let path = unsafe { cstr_to_str(*argv.add(i as usize)) };
            print_basename(path, &config);
            write(STDOUT_FILENO, &[delimiter]);
        }
    } else {
        // Traditional mode: basename NAME [SUFFIX]
        let path = unsafe { cstr_to_str(*argv.add(arg_idx as usize)) };

        // Check for optional suffix argument (only in non-multiple mode)
        if arg_idx + 1 < argc {
            let suffix_str = unsafe { cstr_to_str(*argv.add((arg_idx + 1) as usize)) };
            let mut buf = [0u8; MAX_PATH];
            let copy_len = suffix_str.len().min(MAX_PATH);
            buf[..copy_len].copy_from_slice(&suffix_str.as_bytes()[..copy_len]);
            config.suffix = Some(buf);
            config.suffix_len = copy_len;
        }

        print_basename(path, &config);
        write(STDOUT_FILENO, &[delimiter]);
    }

    0
}

fn print_basename(path: &str, config: &BasenameConfig) {
    let mut base = basename_core(path).as_bytes();

    // Remove suffix if specified
    if let Some(ref suffix_buf) = config.suffix {
        let suffix_bytes = &suffix_buf[..config.suffix_len];
        if base.len() > suffix_bytes.len() {
            let potential_start = base.len() - suffix_bytes.len();
            let mut matches = true;
            for i in 0..suffix_bytes.len() {
                if base[potential_start + i] != suffix_bytes[i] {
                    matches = false;
                    break;
                }
            }
            if matches {
                base = &base[..potential_start];
            }
        }
    }

    write(STDOUT_FILENO, base);
}
