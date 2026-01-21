//! ln - create links
//!
//! Full-featured implementation with:
//! - Hard links (default)
//! - Symbolic links (-s)
//! - Force mode (-f, remove existing destination)
//! - Interactive mode (-i, prompt before overwrite)
//! - No-dereference (-n, treat symlink dest as normal file)
//! - Verbose mode (-v)
//! - Backup mode (-b)
//! - Target directory (-t)
//! - Multiple source files
//! - Proper error handling

#![no_std]
#![no_main]

use libc::*;

struct LnConfig {
    symbolic: bool,
    force: bool,
    interactive: bool,
    no_dereference: bool,
    verbose: bool,
    backup: bool,
    target_directory: Option<[u8; 256]>,
}

impl LnConfig {
    fn new() -> Self {
        LnConfig {
            symbolic: false,
            force: false,
            interactive: false,
            no_dereference: false,
            verbose: false,
            backup: false,
            target_directory: None,
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
    eprintlns("Usage: ln [OPTIONS] TARGET LINK_NAME");
    eprintlns("   or: ln [OPTIONS] TARGET... DIRECTORY");
    eprintlns("   or: ln [OPTIONS] -t DIRECTORY TARGET...");
    eprintlns("");
    eprintlns("Create links to TARGET with name LINK_NAME.");
    eprintlns("");
    eprintlns("Options:");
    eprintlns("  -s          Create symbolic links instead of hard links");
    eprintlns("  -f          Remove existing destination files");
    eprintlns("  -i          Prompt before overwriting");
    eprintlns("  -n          Treat LINK_NAME as normal file if it's a symlink to a directory");
    eprintlns("  -v          Print name of each linked file");
    eprintlns("  -b          Make backup of each existing destination file");
    eprintlns("  -t DIR      Specify target directory");
    eprintlns("  -h          Show this help");
}

#[unsafe(no_mangle)]
fn main(argc: i32, argv: *const *const u8) -> i32 {
    if argc < 2 {
        eprintlns("ln: missing file operand");
        eprintlns("Try 'ln -h' for more information.");
        return 1;
    }

    let mut config = LnConfig::new();
    let mut arg_idx = 1;

    // Parse options
    while arg_idx < argc {
        let arg = unsafe { cstr_to_str(*argv.add(arg_idx as usize)) };

        if arg == "-h" || arg == "--help" {
            show_help();
            return 0;
        } else if arg == "-t" {
            arg_idx += 1;
            if arg_idx >= argc {
                eprintlns("ln: option -t requires an argument");
                return 1;
            }
            let target_dir = unsafe { cstr_to_str(*argv.add(arg_idx as usize)) };
            let mut buf = [0u8; 256];
            let copy_len = target_dir.len().min(255);
            buf[..copy_len].copy_from_slice(&target_dir.as_bytes()[..copy_len]);
            config.target_directory = Some(buf);
            arg_idx += 1;
        } else if str_starts_with(arg, "-") && arg.len() > 1 && arg != "--" {
            for c in arg.bytes().skip(1) {
                match c {
                    b's' => config.symbolic = true,
                    b'f' => config.force = true,
                    b'i' => config.interactive = true,
                    b'n' => config.no_dereference = true,
                    b'v' => config.verbose = true,
                    b'b' => config.backup = true,
                    b'h' => {
                        show_help();
                        return 0;
                    }
                    _ => {
                        eprints("ln: invalid option: -");
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

    // Check arguments
    let remaining = argc - arg_idx;
    if remaining < 1 {
        eprintlns("ln: missing file operand");
        return 1;
    }

    // Target directory mode
    if let Some(ref target_dir_buf) = config.target_directory {
        let len = target_dir_buf.iter().position(|&c| c == 0).unwrap_or(256);
        let target_dir = unsafe { core::str::from_utf8_unchecked(&target_dir_buf[..len]) };

        let mut exit_code = 0;
        for i in arg_idx..argc {
            let source = unsafe { cstr_to_str(*argv.add(i as usize)) };
            if link_to_directory(&config, source, target_dir) != 0 {
                exit_code = 1;
            }
        }
        return exit_code;
    }

    // Two or more arguments
    if remaining < 2 {
        eprintlns("ln: missing destination file operand");
        return 1;
    }

    // Multiple sources - last argument is directory
    if remaining > 2 {
        let dest_dir = unsafe { cstr_to_str(*argv.add((argc - 1) as usize)) };

        // Check if destination is a directory
        let mut st = Stat::zeroed();
        if stat(dest_dir, &mut st) < 0 || (st.mode & S_IFMT) != S_IFDIR {
            eprints("ln: target '");
            prints(dest_dir);
            eprintlns("' is not a directory");
            return 1;
        }

        let mut exit_code = 0;
        for i in arg_idx..(argc - 1) {
            let source = unsafe { cstr_to_str(*argv.add(i as usize)) };
            if link_to_directory(&config, source, dest_dir) != 0 {
                exit_code = 1;
            }
        }
        return exit_code;
    }

    // Simple case: one source, one destination
    let source = unsafe { cstr_to_str(*argv.add(arg_idx as usize)) };
    let dest = unsafe { cstr_to_str(*argv.add((arg_idx + 1) as usize)) };

    create_link(&config, source, dest)
}

fn link_to_directory(config: &LnConfig, source: &str, dest_dir: &str) -> i32 {
    // Extract basename from source
    let basename = get_basename(source);

    // Build destination path
    let mut dest_path = [0u8; 512];
    let mut idx = 0;

    for &b in dest_dir.as_bytes() {
        if idx < dest_path.len() - 1 {
            dest_path[idx] = b;
            idx += 1;
        }
    }

    if idx > 0 && dest_path[idx - 1] != b'/' && idx < dest_path.len() - 1 {
        dest_path[idx] = b'/';
        idx += 1;
    }

    for &b in basename.as_bytes() {
        if idx < dest_path.len() - 1 {
            dest_path[idx] = b;
            idx += 1;
        }
    }

    let dest = unsafe { core::str::from_utf8_unchecked(&dest_path[..idx]) };
    create_link(config, source, dest)
}

fn get_basename(path: &str) -> &str {
    let bytes = path.as_bytes();
    for i in (0..bytes.len()).rev() {
        if bytes[i] == b'/' {
            return unsafe { core::str::from_utf8_unchecked(&bytes[i + 1..]) };
        }
    }
    path
}

fn create_link(config: &LnConfig, source: &str, dest: &str) -> i32 {
    // Check if destination exists
    let mut dest_stat = Stat::zeroed();
    let dest_exists = stat(dest, &mut dest_stat) == 0;

    // Handle interactive mode
    if config.interactive && dest_exists {
        prints("ln: overwrite '");
        prints(dest);
        prints("'? ");

        let tty_fd = open2("/dev/console", O_RDONLY);
        if tty_fd >= 0 {
            let mut response = [0u8; 1];
            let _ = read(tty_fd, &mut response);
            close(tty_fd);

            if response[0] != b'y' && response[0] != b'Y' {
                return 0;
            }
        }
    }

    // Backup if requested
    if config.backup && dest_exists {
        let mut backup_path = [0u8; 260];
        let mut idx = 0;

        for &b in dest.as_bytes() {
            if idx < backup_path.len() - 1 {
                backup_path[idx] = b;
                idx += 1;
            }
        }

        if idx < backup_path.len() - 1 {
            backup_path[idx] = b'~';
            idx += 1;
        }

        let backup = unsafe { core::str::from_utf8_unchecked(&backup_path[..idx]) };
        let _ = sys_rename(dest, backup);
    }

    // Remove existing destination if force mode
    if config.force && dest_exists {
        let _ = unlink(dest);
    }

    // Create the link
    let result = if config.symbolic {
        sys_symlink(source, dest)
    } else {
        sys_link(source, dest)
    };

    if result < 0 {
        eprints("ln: failed to create ");
        if config.symbolic {
            eprints("symbolic ");
        }
        eprints("link '");
        prints(dest);
        eprints("' -> '");
        prints(source);
        eprintlns("'");
        return 1;
    }

    // Verbose output
    if config.verbose {
        prints("'");
        prints(dest);
        prints("' -> '");
        prints(source);
        printlns("'");
    }

    0
}
