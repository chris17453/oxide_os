//! xargs - build and execute command lines from standard input
//!
//! Full-featured implementation with:
//! - Null-separated input (-0)
//! - Custom delimiter (-d)
//! - Replace string (-I)
//! - Max arguments per command (-n)
//! - Max chars per command (-s)
//! - No run if empty (-r)
//! - Verbose mode (-t)
//! - Interactive prompt (-p)
//! - Help message (-h)
//! - Proper error handling

#![no_std]
#![no_main]

use libc::*;

const MAX_ARGS: usize = 128;
const MAX_ARG_LEN: usize = 2048;
const MAX_CMD_LEN: usize = 8192;
const MAX_REPLACE_LEN: usize = 128;

struct XargsConfig {
    null_separated: bool,
    delimiter: u8,
    replace_str: Option<[u8; MAX_REPLACE_LEN]>,
    replace_len: usize,
    max_args: usize,
    max_chars: usize,
    no_run_if_empty: bool,
    verbose: bool,
    interactive: bool,
}

impl XargsConfig {
    fn new() -> Self {
        XargsConfig {
            null_separated: false,
            delimiter: 0, // 0 means whitespace
            replace_str: None,
            replace_len: 0,
            max_args: MAX_ARGS,
            max_chars: MAX_CMD_LEN,
            no_run_if_empty: false,
            verbose: false,
            interactive: false,
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

fn parse_number(s: &str) -> Option<usize> {
    let mut result = 0usize;
    for b in s.bytes() {
        if b >= b'0' && b <= b'9' {
            result = result * 10 + (b - b'0') as usize;
        } else {
            return None;
        }
    }
    Some(result)
}

fn show_help() {
    eprintlns("Usage: xargs [OPTION]... [COMMAND [INITIAL-ARGS]...]");
    eprintlns("");
    eprintlns("Build and execute command lines from standard input.");
    eprintlns("");
    eprintlns("Options:");
    eprintlns("  -0                  Input items are terminated by null, not whitespace");
    eprintlns("  -d DELIM            Input items are terminated by DELIM");
    eprintlns("  -I REPLACE          Replace REPLACE in INITIAL-ARGS with names read from stdin");
    eprintlns("  -n MAX-ARGS         Use at most MAX-ARGS arguments per command line");
    eprintlns("  -s MAX-CHARS        Limit command line length to MAX-CHARS");
    eprintlns("  -r                  Do not run command if input is empty");
    eprintlns("  -t                  Print commands before executing them");
    eprintlns("  -p                  Prompt user before executing each command");
    eprintlns("  -h                  Show this help");
    eprintlns("");
    eprintlns("Default command is 'echo' if not specified.");
}

#[unsafe(no_mangle)]
fn main(argc: i32, argv: *const *const u8) -> i32 {
    let mut config = XargsConfig::new();
    let mut arg_idx = 1;

    // Parse options
    while arg_idx < argc {
        let arg = unsafe { cstr_to_str(*argv.add(arg_idx as usize)) };

        if arg == "-h" || arg == "--help" {
            show_help();
            return 0;
        } else if arg == "-0" || arg == "--null" {
            config.null_separated = true;
            arg_idx += 1;
        } else if arg == "-d" {
            arg_idx += 1;
            if arg_idx >= argc {
                eprintlns("xargs: option -d requires an argument");
                return 1;
            }
            let delim_str = unsafe { cstr_to_str(*argv.add(arg_idx as usize)) };
            if delim_str.is_empty() {
                eprintlns("xargs: delimiter must not be empty");
                return 1;
            }
            config.delimiter = delim_str.as_bytes()[0];
            arg_idx += 1;
        } else if arg == "-I" {
            arg_idx += 1;
            if arg_idx >= argc {
                eprintlns("xargs: option -I requires an argument");
                return 1;
            }
            let replace = unsafe { cstr_to_str(*argv.add(arg_idx as usize)) };
            let mut buf = [0u8; MAX_REPLACE_LEN];
            let copy_len = replace.len().min(MAX_REPLACE_LEN);
            buf[..copy_len].copy_from_slice(&replace.as_bytes()[..copy_len]);
            config.replace_str = Some(buf);
            config.replace_len = copy_len;
            config.max_args = 1; // -I implies -n 1
            arg_idx += 1;
        } else if arg == "-n" {
            arg_idx += 1;
            if arg_idx >= argc {
                eprintlns("xargs: option -n requires an argument");
                return 1;
            }
            let n_str = unsafe { cstr_to_str(*argv.add(arg_idx as usize)) };
            match parse_number(n_str) {
                Some(n) if n > 0 => config.max_args = n.min(MAX_ARGS),
                _ => {
                    eprintlns("xargs: invalid number for -n");
                    return 1;
                }
            }
            arg_idx += 1;
        } else if arg == "-s" {
            arg_idx += 1;
            if arg_idx >= argc {
                eprintlns("xargs: option -s requires an argument");
                return 1;
            }
            let s_str = unsafe { cstr_to_str(*argv.add(arg_idx as usize)) };
            match parse_number(s_str) {
                Some(s) if s > 0 => config.max_chars = s.min(MAX_CMD_LEN),
                _ => {
                    eprintlns("xargs: invalid number for -s");
                    return 1;
                }
            }
            arg_idx += 1;
        } else if arg == "-r" || arg == "--no-run-if-empty" {
            config.no_run_if_empty = true;
            arg_idx += 1;
        } else if arg == "-t" || arg == "--verbose" {
            config.verbose = true;
            arg_idx += 1;
        } else if arg == "-p" {
            config.interactive = true;
            arg_idx += 1;
        } else if str_starts_with(arg, "-") && arg.len() > 1 && arg != "--" {
            for c in arg.bytes().skip(1) {
                match c {
                    b'0' => config.null_separated = true,
                    b'r' => config.no_run_if_empty = true,
                    b't' => config.verbose = true,
                    b'p' => config.interactive = true,
                    b'h' => {
                        show_help();
                        return 0;
                    }
                    _ => {
                        eprints("xargs: invalid option: -");
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

    // Get command to execute (default: echo)
    let mut cmd_parts: [[u8; MAX_ARG_LEN]; MAX_ARGS] = [[0; MAX_ARG_LEN]; MAX_ARGS];
    let mut cmd_lens: [usize; MAX_ARGS] = [0; MAX_ARGS];
    let mut cmd_count = 0;

    if arg_idx >= argc {
        // Default command is echo
        cmd_parts[0][..4].copy_from_slice(b"echo");
        cmd_lens[0] = 4;
        cmd_count = 1;
    } else {
        while arg_idx < argc && cmd_count < MAX_ARGS {
            let arg = unsafe { cstr_to_str(*argv.add(arg_idx as usize)) };
            let len = arg.len().min(MAX_ARG_LEN - 1);
            cmd_parts[cmd_count][..len].copy_from_slice(&arg.as_bytes()[..len]);
            cmd_lens[cmd_count] = len;
            cmd_count += 1;
            arg_idx += 1;
        }
    }

    // Read arguments from stdin
    let mut args: [[u8; MAX_ARG_LEN]; MAX_ARGS] = [[0; MAX_ARG_LEN]; MAX_ARGS];
    let mut arg_lens: [usize; MAX_ARGS] = [0; MAX_ARGS];
    let mut arg_count = 0;

    let delimiter = if config.null_separated {
        b'\0'
    } else if config.delimiter != 0 {
        config.delimiter
    } else {
        0 // Whitespace mode
    };

    if !read_args(&config, delimiter, &mut args, &mut arg_lens, &mut arg_count) {
        return 1;
    }

    // Check for empty input
    if config.no_run_if_empty && arg_count == 0 {
        return 0;
    }

    // Execute commands
    execute_all(
        &config, &cmd_parts, &cmd_lens, cmd_count, &args, &arg_lens, arg_count,
    )
}

fn read_args(
    config: &XargsConfig,
    delimiter: u8,
    args: &mut [[u8; MAX_ARG_LEN]; MAX_ARGS],
    arg_lens: &mut [usize; MAX_ARGS],
    arg_count: &mut usize,
) -> bool {
    let mut buf = [0u8; 4096];
    let mut current_arg = [0u8; MAX_ARG_LEN];
    let mut current_len = 0;

    loop {
        let n = read(STDIN_FILENO, &mut buf);
        if n <= 0 {
            break;
        }

        for i in 0..n as usize {
            let c = buf[i];

            let is_delimiter = if delimiter == 0 {
                // Whitespace mode
                c == b' ' || c == b'\t' || c == b'\n'
            } else {
                c == delimiter
            };

            if is_delimiter {
                if current_len > 0 {
                    // Store completed argument
                    if *arg_count < MAX_ARGS {
                        args[*arg_count][..current_len]
                            .copy_from_slice(&current_arg[..current_len]);
                        arg_lens[*arg_count] = current_len;
                        *arg_count += 1;
                    } else {
                        eprintlns("xargs: too many arguments");
                        return false;
                    }
                    current_len = 0;
                }
            } else if current_len < MAX_ARG_LEN - 1 {
                current_arg[current_len] = c;
                current_len += 1;
            }
        }
    }

    // Handle last argument
    if current_len > 0 {
        if *arg_count < MAX_ARGS {
            args[*arg_count][..current_len].copy_from_slice(&current_arg[..current_len]);
            arg_lens[*arg_count] = current_len;
            *arg_count += 1;
        } else {
            eprintlns("xargs: too many arguments");
            return false;
        }
    }

    true
}

fn execute_all(
    config: &XargsConfig,
    cmd_parts: &[[u8; MAX_ARG_LEN]; MAX_ARGS],
    cmd_lens: &[usize; MAX_ARGS],
    cmd_count: usize,
    args: &[[u8; MAX_ARG_LEN]; MAX_ARGS],
    arg_lens: &[usize; MAX_ARGS],
    arg_count: usize,
) -> i32 {
    let mut status = 0;

    if config.replace_str.is_some() {
        // Replace mode: execute once for each argument
        for i in 0..arg_count {
            if execute_with_replace(
                config,
                cmd_parts,
                cmd_lens,
                cmd_count,
                &args[i][..arg_lens[i]],
            ) != 0
            {
                status = 1;
            }
        }
    } else {
        // Batch mode: execute with groups of arguments
        let mut i = 0;
        while i < arg_count {
            let batch_size = (arg_count - i).min(config.max_args);
            if execute_batch(
                config, cmd_parts, cmd_lens, cmd_count, args, arg_lens, i, batch_size,
            ) != 0
            {
                status = 1;
            }
            i += batch_size;
        }

        // Handle case where there are no arguments but we should run anyway
        if arg_count == 0 && !config.no_run_if_empty {
            if execute_batch(config, cmd_parts, cmd_lens, cmd_count, args, arg_lens, 0, 0) != 0 {
                status = 1;
            }
        }
    }

    status
}

fn execute_with_replace(
    config: &XargsConfig,
    cmd_parts: &[[u8; MAX_ARG_LEN]; MAX_ARGS],
    cmd_lens: &[usize; MAX_ARGS],
    cmd_count: usize,
    replacement: &[u8],
) -> i32 {
    let replace_pattern = match &config.replace_str {
        Some(buf) => &buf[..config.replace_len],
        None => return 1,
    };

    // Build command with replacements
    let mut final_cmd: [[u8; MAX_ARG_LEN]; MAX_ARGS] = [[0; MAX_ARG_LEN]; MAX_ARGS];
    let mut final_lens: [usize; MAX_ARGS] = [0; MAX_ARGS];
    let mut final_count = 0;

    for i in 0..cmd_count {
        if final_count >= MAX_ARGS {
            break;
        }

        let part = &cmd_parts[i][..cmd_lens[i]];

        // Check if this part contains the replace pattern
        if contains_pattern(part, replace_pattern) {
            // Replace pattern with argument
            let new_len = replacement.len().min(MAX_ARG_LEN);
            final_cmd[final_count][..new_len].copy_from_slice(&replacement[..new_len]);
            final_lens[final_count] = new_len;
        } else {
            // Copy as-is
            final_cmd[final_count][..cmd_lens[i]].copy_from_slice(part);
            final_lens[final_count] = cmd_lens[i];
        }
        final_count += 1;
    }

    execute_command(config, &final_cmd, &final_lens, final_count)
}

fn contains_pattern(haystack: &[u8], needle: &[u8]) -> bool {
    if needle.len() > haystack.len() {
        return false;
    }
    for i in 0..=(haystack.len() - needle.len()) {
        let mut matches = true;
        for j in 0..needle.len() {
            if haystack[i + j] != needle[j] {
                matches = false;
                break;
            }
        }
        if matches {
            return true;
        }
    }
    false
}

fn execute_batch(
    config: &XargsConfig,
    cmd_parts: &[[u8; MAX_ARG_LEN]; MAX_ARGS],
    cmd_lens: &[usize; MAX_ARGS],
    cmd_count: usize,
    args: &[[u8; MAX_ARG_LEN]; MAX_ARGS],
    arg_lens: &[usize; MAX_ARGS],
    start_idx: usize,
    count: usize,
) -> i32 {
    // Build combined command
    let mut final_cmd: [[u8; MAX_ARG_LEN]; MAX_ARGS] = [[0; MAX_ARG_LEN]; MAX_ARGS];
    let mut final_lens: [usize; MAX_ARGS] = [0; MAX_ARGS];
    let mut final_count = 0;

    // Copy command parts
    for i in 0..cmd_count {
        if final_count >= MAX_ARGS {
            break;
        }
        final_cmd[final_count][..cmd_lens[i]].copy_from_slice(&cmd_parts[i][..cmd_lens[i]]);
        final_lens[final_count] = cmd_lens[i];
        final_count += 1;
    }

    // Add arguments
    for i in 0..count {
        if final_count >= MAX_ARGS {
            break;
        }
        let arg_idx = start_idx + i;
        final_cmd[final_count][..arg_lens[arg_idx]]
            .copy_from_slice(&args[arg_idx][..arg_lens[arg_idx]]);
        final_lens[final_count] = arg_lens[arg_idx];
        final_count += 1;
    }

    execute_command(config, &final_cmd, &final_lens, final_count)
}

fn execute_command(
    config: &XargsConfig,
    cmd_parts: &[[u8; MAX_ARG_LEN]; MAX_ARGS],
    cmd_lens: &[usize; MAX_ARGS],
    cmd_count: usize,
) -> i32 {
    // Build command string for display
    let mut cmd_str = [0u8; MAX_CMD_LEN];
    let mut pos = 0;

    for i in 0..cmd_count {
        if i > 0 && pos < MAX_CMD_LEN {
            cmd_str[pos] = b' ';
            pos += 1;
        }
        let len = cmd_lens[i].min(MAX_CMD_LEN - pos);
        cmd_str[pos..pos + len].copy_from_slice(&cmd_parts[i][..len]);
        pos += len;
    }

    // Verbose or interactive mode
    if config.verbose || config.interactive {
        write(STDERR_FILENO, &cmd_str[..pos]);
        write(STDERR_FILENO, b"\n");
    }

    // Interactive prompt
    if config.interactive {
        write(STDERR_FILENO, b"?...");
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

    // Fork and exec
    let pid = fork();
    if pid == 0 {
        // Child process
        let cmd_cstr = unsafe { core::str::from_utf8_unchecked(&cmd_str[..pos]) };
        exec(cmd_cstr);
        exit(127);
    } else if pid > 0 {
        // Parent process
        let mut status = 0;
        waitpid(pid, &mut status, 0);
        return status;
    }

    1
}
