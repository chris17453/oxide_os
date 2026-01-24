//! chmod - Change file mode bits
//!
//! Full-featured implementation with:
//! - Octal mode (e.g., 755, 0644)
//! - Symbolic mode (e.g., u+x, go-w, a=rw)
//! - Recursive mode (-R)
//! - Verbose mode (-v)
//! - Changes only mode (-c)
//! - Quiet mode (--quiet, --silent)
//! - Reference file (--reference=FILE)
//! - Multiple file support
//! - Proper error handling

#![no_std]
#![no_main]

use libc::*;

struct ChmodConfig {
    recursive: bool,
    verbose: bool,
    changes_only: bool,
    quiet: bool,
    reference_mode: Option<u32>,
}

impl ChmodConfig {
    fn new() -> Self {
        ChmodConfig {
            recursive: false,
            verbose: false,
            changes_only: false,
            quiet: false,
            reference_mode: None,
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

fn show_help() {
    eprintlns("Usage: chmod [OPTIONS] MODE FILE...");
    eprintlns("   or: chmod [OPTIONS] --reference=RFILE FILE...");
    eprintlns("");
    eprintlns("Change file mode bits.");
    eprintlns("");
    eprintlns("MODE can be:");
    eprintlns("  Octal:    e.g., 755, 0644");
    eprintlns("  Symbolic: [ugoa]*([-+=]([rwxXst]))+");
    eprintlns("");
    eprintlns("Options:");
    eprintlns("  -R          Change files and directories recursively");
    eprintlns("  -v          Output a diagnostic for every file processed");
    eprintlns("  -c          Like verbose but report only when a change is made");
    eprintlns("  --quiet     Suppress most error messages");
    eprintlns("  --silent    Same as --quiet");
    eprintlns("  --reference=FILE  Use FILE's mode instead of MODE");
    eprintlns("  -h          Show this help");
}

#[unsafe(no_mangle)]
fn main(argc: i32, argv: *const *const u8) -> i32 {
    if argc < 2 {
        eprintlns("chmod: missing operand");
        eprintlns("Try 'chmod -h' for more information.");
        return 1;
    }

    let mut config = ChmodConfig::new();
    let mut arg_idx = 1;
    let mut mode_str: Option<&str> = None;

    // Parse options
    while arg_idx < argc {
        let arg = unsafe { cstr_to_str(*argv.add(arg_idx as usize)) };

        if arg == "-h" || arg == "--help" {
            show_help();
            return 0;
        } else if str_starts_with(arg, "--reference=") {
            let ref_file = &arg[12..];
            let mut st = Stat::zeroed();
            if stat(ref_file, &mut st) < 0 {
                eprints("chmod: cannot access reference file '");
                prints(ref_file);
                eprintlns("'");
                return 1;
            }
            config.reference_mode = Some(st.mode & 0o7777);
            arg_idx += 1;
        } else if arg == "--quiet" || arg == "--silent" {
            config.quiet = true;
            arg_idx += 1;
        } else if str_starts_with(arg, "-") && arg.len() > 1 && arg != "--" {
            for c in arg.bytes().skip(1) {
                match c {
                    b'R' => config.recursive = true,
                    b'v' => config.verbose = true,
                    b'c' => config.changes_only = true,
                    b'h' => {
                        show_help();
                        return 0;
                    }
                    _ => {
                        eprints("chmod: invalid option: -");
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

    // Get mode (either from reference or from argument)
    if config.reference_mode.is_none() {
        if arg_idx >= argc {
            eprintlns("chmod: missing MODE operand");
            return 1;
        }
        mode_str = Some(unsafe { cstr_to_str(*argv.add(arg_idx as usize)) });
        arg_idx += 1;
    }

    // Check for files
    if arg_idx >= argc {
        eprintlns("chmod: missing file operand");
        return 1;
    }

    let mut exit_code = 0;

    // Process each file
    for i in arg_idx..argc {
        let file = unsafe { cstr_to_str(*argv.add(i as usize)) };
        if chmod_file(&config, mode_str, file) != 0 {
            exit_code = 1;
        }
    }

    exit_code
}

fn chmod_file(config: &ChmodConfig, mode_str: Option<&str>, path: &str) -> i32 {
    // Get current file mode
    let mut st = Stat::zeroed();
    if stat(path, &mut st) < 0 {
        if !config.quiet {
            eprints("chmod: cannot access '");
            prints(path);
            eprintlns("'");
        }
        return 1;
    }

    // Determine new mode
    let new_mode = if let Some(ref_mode) = config.reference_mode {
        ref_mode
    } else if let Some(mode_arg) = mode_str {
        match parse_mode(mode_arg.as_bytes(), st.mode) {
            Some(m) => m,
            None => {
                if !config.quiet {
                    eprints("chmod: invalid mode '");
                    prints(mode_arg);
                    eprintlns("'");
                }
                return 1;
            }
        }
    } else {
        return 1;
    };

    let old_mode = st.mode & 0o7777;

    // Apply new mode
    if sys_chmod(path, new_mode) != 0 {
        if !config.quiet {
            eprints("chmod: cannot change permissions of '");
            prints(path);
            eprintlns("'");
        }
        return 1;
    }

    // Output diagnostic if requested
    if config.verbose || (config.changes_only && old_mode != new_mode) {
        prints("mode of '");
        prints(path);
        prints("' changed from ");
        print_octal(old_mode);
        prints(" to ");
        print_octal(new_mode);
        printlns("");
    }

    // Recurse into directories if requested
    if config.recursive && (st.mode & S_IFMT) == S_IFDIR {
        return chmod_recursive(config, mode_str, path);
    }

    0
}

fn chmod_recursive(config: &ChmodConfig, mode_str: Option<&str>, dir_path: &str) -> i32 {
    let fd = open(dir_path, O_RDONLY | O_DIRECTORY, 0);
    if fd < 0 {
        if !config.quiet {
            eprints("chmod: cannot open directory '");
            prints(dir_path);
            eprintlns("'");
        }
        return 1;
    }

    let mut buf = [0u8; 4096];
    let mut exit_code = 0;

    loop {
        let n = sys_getdents(fd, &mut buf);
        if n <= 0 {
            break;
        }

        let mut offset = 0;
        while offset < n as usize {
            let entry_ptr = buf.as_ptr().wrapping_add(offset) as *const DirEntry;
            let entry = unsafe { &*entry_ptr };

            // Get name
            let name_offset = offset + core::mem::size_of::<DirEntry>();
            let name_bytes = &buf[name_offset..];

            let mut name_len = 0;
            while name_len < name_bytes.len() && name_bytes[name_len] != 0 {
                name_len += 1;
            }

            let name = unsafe { core::str::from_utf8_unchecked(&name_bytes[..name_len]) };

            // Skip . and ..
            if name != "." && name != ".." {
                // Build full path
                let mut full_path = [0u8; 512];
                let mut idx = 0;

                for &b in dir_path.as_bytes() {
                    if idx < full_path.len() - 1 {
                        full_path[idx] = b;
                        idx += 1;
                    }
                }

                if idx > 0 && full_path[idx - 1] != b'/' && idx < full_path.len() - 1 {
                    full_path[idx] = b'/';
                    idx += 1;
                }

                for &b in name.as_bytes() {
                    if idx < full_path.len() - 1 {
                        full_path[idx] = b;
                        idx += 1;
                    }
                }

                let child_path = unsafe { core::str::from_utf8_unchecked(&full_path[..idx]) };

                if chmod_file(config, mode_str, child_path) != 0 {
                    exit_code = 1;
                }
            }

            offset += entry.d_reclen as usize;
        }
    }

    close(fd);
    exit_code
}

/// Directory entry header (matches kernel's UserDirEntry)
/// MUST be packed to match kernel's packed struct (19 bytes, not 24)
#[repr(C, packed)]
struct DirEntry {
    d_ino: u64,
    d_off: u64,
    d_reclen: u16,
    d_type: u8,
}

fn print_octal(mode: u32) {
    let digits = [
        ((mode >> 6) & 7) as u8,
        ((mode >> 3) & 7) as u8,
        (mode & 7) as u8,
    ];

    for &d in &digits {
        putchar(b'0' + d);
    }
}

/// Parse mode string - octal or symbolic
fn parse_mode(mode_arg: &[u8], current_mode: u32) -> Option<u32> {
    // Try octal first
    if let Some(mode) = parse_octal(mode_arg) {
        return Some(mode);
    }

    // Try symbolic mode
    parse_symbolic(mode_arg, current_mode)
}

/// Parse octal mode (e.g., "755", "0644")
fn parse_octal(s: &[u8]) -> Option<u32> {
    if s.is_empty() {
        return None;
    }

    let mut result: u32 = 0;
    for &c in s {
        if c < b'0' || c > b'7' {
            return None;
        }
        result = result * 8 + (c - b'0') as u32;
    }

    Some(result)
}

/// Parse symbolic mode (e.g., "u+x", "go-w", "a=rw")
fn parse_symbolic(s: &[u8], current: u32) -> Option<u32> {
    let mut mode = current & 0o7777;
    let mut i = 0;

    while i < s.len() {
        // Parse who: [ugoa]
        let mut who_mask: u32 = 0;
        while i < s.len() {
            match s[i] {
                b'u' => who_mask |= 0o700,
                b'g' => who_mask |= 0o070,
                b'o' => who_mask |= 0o007,
                b'a' => who_mask |= 0o777,
                _ => break,
            }
            i += 1;
        }

        // Default to 'a' if no who specified
        if who_mask == 0 {
            who_mask = 0o777;
        }

        // Parse operator: [+-=]
        if i >= s.len() {
            return None;
        }

        let op = s[i];
        if op != b'+' && op != b'-' && op != b'=' {
            return None;
        }
        i += 1;

        // Parse permission: [rwxXst]
        let mut perm: u32 = 0;
        while i < s.len() && s[i] != b',' {
            match s[i] {
                b'r' => perm |= 0o444,
                b'w' => perm |= 0o222,
                b'x' => perm |= 0o111,
                b'X' => {
                    // Execute only if directory or already has execute
                    if (current & 0o040000) != 0 || (current & 0o111) != 0 {
                        perm |= 0o111;
                    }
                }
                b's' => perm |= 0o6000, // setuid/setgid
                b't' => perm |= 0o1000, // sticky bit
                _ => return None,
            }
            i += 1;
        }

        // Apply the permission mask
        let effective_perm = perm & who_mask;

        match op {
            b'+' => mode |= effective_perm,
            b'-' => mode &= !effective_perm,
            b'=' => {
                mode &= !who_mask;
                mode |= effective_perm;
            }
            _ => {}
        }

        // Skip comma if present
        if i < s.len() && s[i] == b',' {
            i += 1;
        }
    }

    Some(mode)
}

/// Syscall wrapper for chmod
fn sys_chmod(path: &str, mode: u32) -> i32 {
    const CHMOD: u64 = 150;
    syscall::syscall3(CHMOD, path.as_ptr() as usize, path.len(), mode as usize) as i32
}
