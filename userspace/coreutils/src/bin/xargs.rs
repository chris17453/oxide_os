//! xargs - build and execute command lines from standard input
//!
//! ╔═══════════════════════════════════════════════════════════════╗
//! ║  OXIDE OS - XARGS v2.0 - PARALLEL COMMAND EXECUTION ENGINE   ║
//! ║                                                                ║
//! ║  "In the neon-lit depths of the command pipeline,             ║
//! ║   where data flows like rain on chrome streets..."           ║
//! ║                                        -- GraveShift          ║
//! ╚═══════════════════════════════════════════════════════════════╝
//!
//! Full-featured implementation with GNU xargs compatibility:
//! - Null-separated input (-0/--null)
//! - Custom delimiter (-d/--delimiter)
//! - Replace string (-I/-i/--replace)
//! - Max arguments per command (-n/--max-args)
//! - Max chars per command (-s/--max-chars)
//! - Max lines per command (-L/--max-lines)
//! - No run if empty (-r/--no-run-if-empty)
//! - Verbose mode (-t/--verbose)
//! - Interactive prompt (-p/--interactive)
//! - Parallel execution (-P/--max-procs)
//! - Exit on error (-x/--exit)
//! - File input (-a/--arg-file)
//! - Show limits (--show-limits)
//! - EOF string (-e/--eof)
//! - Open TTY for prompts (-o/--open-tty)
//! - Process substitution support
//! - Proper error handling and exit codes
//! - Statistics tracking (--verbose-stats)
//!
//! Exit codes:
//!   0 - Success
//!   1-125 - Child process exit code
//!   123 - Any child exited with 1-125
//!   124 - Max lines reached (with -L)
//!   125 - Child process exit code 255
//!   126 - Command cannot be run
//!   127 - Command not found
//!   255 - Internal error

#![no_std]
#![no_main]
#![allow(unused)]

use libc::*;

// --- Constants for the grid of data flowing through the system ---
const MAX_ARGS: usize = 256; // Doubled for high-throughput scenarios
const MAX_ARG_LEN: usize = 4096; // Larger buffers for modern workloads
const MAX_CMD_LEN: usize = 131072; // 128KB command line limit (Linux default)
const MAX_REPLACE_LEN: usize = 256;
const MAX_PROCS: usize = 64; // Max parallel processes
const MAX_PATH_LEN: usize = 4096;

// --- Configuration struct: The control panel in our chrome cockpit ---
// Signed by: WireSaint - "Configuration is destiny"
struct XargsConfig {
    null_separated: bool,
    delimiter: u8,
    replace_str: Option<[u8; MAX_REPLACE_LEN]>,
    replace_len: usize,
    max_args: usize,
    max_chars: usize,
    max_lines: usize, // NEW: -L flag
    no_run_if_empty: bool,
    verbose: bool,
    verbose_stats: bool, // NEW: --verbose-stats
    interactive: bool,
    max_procs: usize,                     // NEW: -P flag for parallel execution
    exit_on_error: bool,                  // NEW: -x flag
    open_tty: bool,                       // NEW: -o flag
    show_limits: bool,                    // NEW: --show-limits
    arg_file: Option<[u8; MAX_PATH_LEN]>, // NEW: -a flag
    arg_file_len: usize,
    eof_string: Option<[u8; MAX_REPLACE_LEN]>, // NEW: -e flag
    eof_len: usize,
    insert_mode: bool, // NEW: -i flag (like -I but with {})
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
            max_lines: 0, // 0 means no line limit
            no_run_if_empty: false,
            verbose: false,
            verbose_stats: false,
            interactive: false,
            max_procs: 1, // Sequential by default
            exit_on_error: false,
            open_tty: false,
            show_limits: false,
            arg_file: None,
            arg_file_len: 0,
            eof_string: None,
            eof_len: 0,
            insert_mode: false,
        }
    }
}

// --- Statistics tracking: Quantifying the data flow ---
// Signed by: StaticRiot - "Metrics tell no lies"
struct ExecutionStats {
    commands_executed: usize,
    commands_failed: usize,
    max_parallel: usize,
    total_args_processed: usize,
    bytes_read: usize,
}

// --- Helper functions: The tools in our cyberpunk toolkit ---

// Signed by: NeonRoot - "String manipulation in the void"
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

fn str_equals(s: &str, other: &str) -> bool {
    if s.len() != other.len() {
        return false;
    }
    let s_bytes = s.as_bytes();
    let o_bytes = other.as_bytes();
    for i in 0..s.len() {
        if s_bytes[i] != o_bytes[i] {
            return false;
        }
    }
    true
}

fn parse_number(s: &str) -> Option<usize> {
    if s.is_empty() {
        return None;
    }
    let mut result = 0usize;
    for b in s.bytes() {
        if b >= b'0' && b <= b'9' {
            result = result
                .saturating_mul(10)
                .saturating_add((b - b'0') as usize);
        } else {
            return None;
        }
    }
    Some(result)
}

// Signed by: GraveShift - "Help text is the map through the maze"
fn show_help() {
    eprintlns("╔════════════════════════════════════════════════════════════════╗");
    eprintlns("║              OXIDE XARGS - Command Execution Engine            ║");
    eprintlns("╚════════════════════════════════════════════════════════════════╝");
    eprintlns("");
    eprintlns("Usage: xargs [OPTION]... [COMMAND [INITIAL-ARGS]...]");
    eprintlns("");
    eprintlns("Build and execute command lines from standard input.");
    eprintlns("");
    eprintlns("Input Control:");
    eprintlns("  -0, --null              Input items terminated by null, not whitespace");
    eprintlns("  -d, --delimiter=DELIM   Input items terminated by DELIM");
    eprintlns("  -e, --eof[=STR]         Logical EOF string (default: none)");
    eprintlns("  -a, --arg-file=FILE     Read arguments from FILE, not stdin");
    eprintlns("");
    eprintlns("Argument Batching:");
    eprintlns("  -n, --max-args=MAX      Use at most MAX arguments per command");
    eprintlns("  -L, --max-lines=MAX     Use at most MAX non-blank input lines");
    eprintlns("  -s, --max-chars=MAX     Limit command line length to MAX");
    eprintlns("  -I, --replace[=STR]     Replace STR in args with input (implies -n 1)");
    eprintlns("  -i, --replace-i         Same as -I{} for compatibility");
    eprintlns("");
    eprintlns("Execution Control:");
    eprintlns("  -P, --max-procs=MAX     Run up to MAX processes in parallel");
    eprintlns("  -x, --exit              Exit if size exceeded");
    eprintlns("  -r, --no-run-if-empty   Don't run if input is empty");
    eprintlns("");
    eprintlns("Interactive & Display:");
    eprintlns("  -t, --verbose           Print commands before executing");
    eprintlns("  -p, --interactive       Prompt user before executing");
    eprintlns("  -o, --open-tty          Open tty for child processes");
    eprintlns("      --show-limits       Display system limits and exit");
    eprintlns("      --verbose-stats     Show execution statistics");
    eprintlns("");
    eprintlns("Other:");
    eprintlns("  -h, --help              Show this help and exit");
    eprintlns("");
    eprintlns("Default command is 'echo' if not specified.");
    eprintlns("");
    eprintlns("Exit status:");
    eprintlns("  0    All commands succeeded");
    eprintlns("  123  Any command exit code 1-125");
    eprintlns("  124  Command line too long");
    eprintlns("  125  Command exit code 255");
    eprintlns("  126  Command cannot be run");
    eprintlns("  127  Command not found");
    eprintlns("");
    eprintlns("Examples:");
    eprintlns("  find . -name '*.rs' | xargs grep 'TODO'");
    eprintlns("  echo '1 2 3' | xargs -n1 echo 'Number:'");
    eprintlns("  ls | xargs -I{} echo 'File: {}'");
    eprintlns("  find . -print0 | xargs -0 -P4 process_file");
}

// Signed by: CrashBloom - "Show me the limits of this reality"
fn show_system_limits() {
    eprintlns("╔════════════════════════════════════════════════════════════════╗");
    eprintlns("║                    XARGS System Limits                         ║");
    eprintlns("╚════════════════════════════════════════════════════════════════╝");
    eprints("Max arguments:        ");
    print_number(MAX_ARGS);
    eprintlns("");
    eprints("Max arg length:       ");
    print_number(MAX_ARG_LEN);
    eprintlns(" bytes");
    eprints("Max command length:   ");
    print_number(MAX_CMD_LEN);
    eprintlns(" bytes");
    eprints("Max parallel procs:   ");
    print_number(MAX_PROCS);
    eprintlns("");
    eprints("Max replace length:   ");
    print_number(MAX_REPLACE_LEN);
    eprintlns(" bytes");
}

fn print_number(n: usize) {
    let mut buf = [0u8; 32];
    let mut pos = 0;
    let mut num = n;

    if num == 0 {
        eprints("0");
        return;
    }

    while num > 0 {
        buf[pos] = b'0' + (num % 10) as u8;
        num /= 10;
        pos += 1;
    }

    // Reverse
    for i in 0..pos {
        let c = buf[pos - 1 - i];
        putchar(c);
    }
}

// Signed by: BlackLatch - "The entry point where chrome meets code"
#[unsafe(no_mangle)]
fn main(argc: i32, argv: *const *const u8) -> i32 {
    let mut config = XargsConfig::new();
    let mut arg_idx = 1;

    // Parse options - supporting both short and long forms
    while arg_idx < argc {
        let arg = unsafe { cstr_to_str(*argv.add(arg_idx as usize)) };

        // Help
        if str_equals(arg, "-h") || str_equals(arg, "--help") {
            show_help();
            return 0;
        }
        // Show limits
        else if str_equals(arg, "--show-limits") {
            show_system_limits();
            return 0;
        }
        // Null separator
        else if str_equals(arg, "-0") || str_equals(arg, "--null") {
            config.null_separated = true;
            arg_idx += 1;
        }
        // Delimiter
        else if str_starts_with(arg, "--delimiter=") {
            let delim_str = &arg[12..];
            if delim_str.is_empty() {
                eprintlns("xargs: delimiter must not be empty");
                return 1;
            }
            config.delimiter = delim_str.as_bytes()[0];
            arg_idx += 1;
        } else if str_equals(arg, "-d") {
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
        }
        // Replace string
        else if str_starts_with(arg, "--replace=") {
            let replace = &arg[10..];
            if !set_replace_string(&mut config, replace) {
                return 1;
            }
            arg_idx += 1;
        } else if str_equals(arg, "-I") || str_equals(arg, "--replace") {
            arg_idx += 1;
            if arg_idx >= argc {
                eprintlns("xargs: option -I requires an argument");
                return 1;
            }
            let replace = unsafe { cstr_to_str(*argv.add(arg_idx as usize)) };
            if !set_replace_string(&mut config, replace) {
                return 1;
            }
            arg_idx += 1;
        } else if str_equals(arg, "-i") || str_equals(arg, "--replace-i") {
            // -i is same as -I{} for compatibility
            if !set_replace_string(&mut config, "{}") {
                return 1;
            }
            config.insert_mode = true;
            arg_idx += 1;
        }
        // Max args
        else if str_starts_with(arg, "--max-args=") {
            let n_str = &arg[11..];
            match parse_number(n_str) {
                Some(n) if n > 0 => config.max_args = n.min(MAX_ARGS),
                _ => {
                    eprintlns("xargs: invalid number for --max-args");
                    return 1;
                }
            }
            arg_idx += 1;
        } else if str_equals(arg, "-n") {
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
        }
        // Max lines
        else if str_starts_with(arg, "--max-lines=") {
            let l_str = &arg[12..];
            match parse_number(l_str) {
                Some(l) if l > 0 => config.max_lines = l,
                _ => {
                    eprintlns("xargs: invalid number for --max-lines");
                    return 1;
                }
            }
            arg_idx += 1;
        } else if str_equals(arg, "-L") {
            arg_idx += 1;
            if arg_idx >= argc {
                eprintlns("xargs: option -L requires an argument");
                return 1;
            }
            let l_str = unsafe { cstr_to_str(*argv.add(arg_idx as usize)) };
            match parse_number(l_str) {
                Some(l) if l > 0 => config.max_lines = l,
                _ => {
                    eprintlns("xargs: invalid number for -L");
                    return 1;
                }
            }
            arg_idx += 1;
        }
        // Max chars
        else if str_starts_with(arg, "--max-chars=") {
            let s_str = &arg[12..];
            match parse_number(s_str) {
                Some(s) if s > 0 => config.max_chars = s.min(MAX_CMD_LEN),
                _ => {
                    eprintlns("xargs: invalid number for --max-chars");
                    return 1;
                }
            }
            arg_idx += 1;
        } else if str_equals(arg, "-s") {
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
        }
        // Max processes (parallel execution)
        else if str_starts_with(arg, "--max-procs=") {
            let p_str = &arg[12..];
            match parse_number(p_str) {
                Some(p) => config.max_procs = if p == 0 { MAX_PROCS } else { p.min(MAX_PROCS) },
                _ => {
                    eprintlns("xargs: invalid number for --max-procs");
                    return 1;
                }
            }
            arg_idx += 1;
        } else if str_equals(arg, "-P") {
            arg_idx += 1;
            if arg_idx >= argc {
                eprintlns("xargs: option -P requires an argument");
                return 1;
            }
            let p_str = unsafe { cstr_to_str(*argv.add(arg_idx as usize)) };
            match parse_number(p_str) {
                Some(p) => config.max_procs = if p == 0 { MAX_PROCS } else { p.min(MAX_PROCS) },
                _ => {
                    eprintlns("xargs: invalid number for -P");
                    return 1;
                }
            }
            arg_idx += 1;
        }
        // EOF string
        else if str_starts_with(arg, "--eof=") {
            let eof_str = &arg[6..];
            if !set_eof_string(&mut config, eof_str) {
                return 1;
            }
            arg_idx += 1;
        } else if str_equals(arg, "-e") || str_equals(arg, "--eof") {
            arg_idx += 1;
            if arg_idx < argc {
                let next_arg = unsafe { cstr_to_str(*argv.add(arg_idx as usize)) };
                if !str_starts_with(next_arg, "-") {
                    if !set_eof_string(&mut config, next_arg) {
                        return 1;
                    }
                    arg_idx += 1;
                } else {
                    // Default EOF string
                    if !set_eof_string(&mut config, "_") {
                        return 1;
                    }
                }
            } else {
                if !set_eof_string(&mut config, "_") {
                    return 1;
                }
            }
        }
        // Arg file
        else if str_starts_with(arg, "--arg-file=") {
            let file_str = &arg[11..];
            if !set_arg_file(&mut config, file_str) {
                return 1;
            }
            arg_idx += 1;
        } else if str_equals(arg, "-a") {
            arg_idx += 1;
            if arg_idx >= argc {
                eprintlns("xargs: option -a requires an argument");
                return 1;
            }
            let file_str = unsafe { cstr_to_str(*argv.add(arg_idx as usize)) };
            if !set_arg_file(&mut config, file_str) {
                return 1;
            }
            arg_idx += 1;
        }
        // Boolean flags
        else if str_equals(arg, "-r") || str_equals(arg, "--no-run-if-empty") {
            config.no_run_if_empty = true;
            arg_idx += 1;
        } else if str_equals(arg, "-t") || str_equals(arg, "--verbose") {
            config.verbose = true;
            arg_idx += 1;
        } else if str_equals(arg, "--verbose-stats") {
            config.verbose_stats = true;
            arg_idx += 1;
        } else if str_equals(arg, "-p") || str_equals(arg, "--interactive") {
            config.interactive = true;
            config.verbose = true; // -p implies -t
            arg_idx += 1;
        } else if str_equals(arg, "-x") || str_equals(arg, "--exit") {
            config.exit_on_error = true;
            arg_idx += 1;
        } else if str_equals(arg, "-o") || str_equals(arg, "--open-tty") {
            config.open_tty = true;
            arg_idx += 1;
        }
        // Combined short options
        else if str_starts_with(arg, "-") && arg.len() > 1 && arg != "--" {
            let mut parse_ok = true;
            for c in arg.bytes().skip(1) {
                match c {
                    b'0' => config.null_separated = true,
                    b'r' => config.no_run_if_empty = true,
                    b't' => config.verbose = true,
                    b'p' => {
                        config.interactive = true;
                        config.verbose = true;
                    }
                    b'x' => config.exit_on_error = true,
                    b'o' => config.open_tty = true,
                    b'h' => {
                        show_help();
                        return 0;
                    }
                    _ => {
                        eprints("xargs: invalid option: -");
                        putchar(c);
                        eprintlns("");
                        parse_ok = false;
                        break;
                    }
                }
            }
            if !parse_ok {
                return 1;
            }
            arg_idx += 1;
        }
        // End of options or command starts
        else {
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

    // Read arguments from stdin or file
    let mut args: [[u8; MAX_ARG_LEN]; MAX_ARGS] = [[0; MAX_ARG_LEN]; MAX_ARGS];
    let mut arg_lens: [usize; MAX_ARGS] = [0; MAX_ARGS];
    let mut arg_count = 0;
    let mut stats = ExecutionStats {
        commands_executed: 0,
        commands_failed: 0,
        max_parallel: 0,
        total_args_processed: 0,
        bytes_read: 0,
    };

    let delimiter = if config.null_separated {
        b'\0'
    } else if config.delimiter != 0 {
        config.delimiter
    } else {
        0 // Whitespace mode
    };

    if !read_args(
        &config,
        delimiter,
        &mut args,
        &mut arg_lens,
        &mut arg_count,
        &mut stats,
    ) {
        return 1;
    }

    stats.total_args_processed = arg_count;

    // Check for empty input
    if config.no_run_if_empty && arg_count == 0 {
        return 0;
    }

    // Execute commands
    let exit_code = execute_all(
        &config, &cmd_parts, &cmd_lens, cmd_count, &args, &arg_lens, arg_count, &mut stats,
    );

    // Print statistics if requested
    if config.verbose_stats {
        print_stats(&stats);
    }

    exit_code
}

// Signed by: TorqueJax - "Helper functions for configuration setup"
fn set_replace_string(config: &mut XargsConfig, replace: &str) -> bool {
    if replace.len() > MAX_REPLACE_LEN {
        eprintlns("xargs: replace string too long");
        return false;
    }
    let mut buf = [0u8; MAX_REPLACE_LEN];
    let copy_len = replace.len();
    buf[..copy_len].copy_from_slice(&replace.as_bytes()[..copy_len]);
    config.replace_str = Some(buf);
    config.replace_len = copy_len;
    config.max_args = 1; // -I implies -n 1
    true
}

fn set_eof_string(config: &mut XargsConfig, eof: &str) -> bool {
    if eof.len() > MAX_REPLACE_LEN {
        eprintlns("xargs: EOF string too long");
        return false;
    }
    let mut buf = [0u8; MAX_REPLACE_LEN];
    let copy_len = eof.len();
    buf[..copy_len].copy_from_slice(&eof.as_bytes()[..copy_len]);
    config.eof_string = Some(buf);
    config.eof_len = copy_len;
    true
}

fn set_arg_file(config: &mut XargsConfig, file: &str) -> bool {
    if file.len() > MAX_PATH_LEN {
        eprintlns("xargs: file path too long");
        return false;
    }
    let mut buf = [0u8; MAX_PATH_LEN];
    let copy_len = file.len();
    buf[..copy_len].copy_from_slice(&file.as_bytes()[..copy_len]);
    config.arg_file = Some(buf);
    config.arg_file_len = copy_len;
    true
}

// Signed by: WireSaint - "Reading from the data streams"
fn read_args(
    config: &XargsConfig,
    delimiter: u8,
    args: &mut [[u8; MAX_ARG_LEN]; MAX_ARGS],
    arg_lens: &mut [usize; MAX_ARGS],
    arg_count: &mut usize,
    stats: &mut ExecutionStats,
) -> bool {
    // Choose input source
    let input_fd = if let Some(ref file_buf) = config.arg_file {
        let file_path = unsafe { core::str::from_utf8_unchecked(&file_buf[..config.arg_file_len]) };
        let fd = open2(file_path, O_RDONLY);
        if fd < 0 {
            eprints("xargs: cannot open file: ");
            eprintlns(file_path);
            return false;
        }
        fd
    } else {
        STDIN_FILENO
    };

    let mut buf = [0u8; 8192]; // Larger read buffer for efficiency
    let mut current_arg = [0u8; MAX_ARG_LEN];
    let mut current_len = 0;
    let mut line_count = 0;

    loop {
        let n = read(input_fd, &mut buf);
        if n <= 0 {
            break;
        }

        stats.bytes_read += n as usize;

        for i in 0..n as usize {
            let c = buf[i];

            // Check for EOF string
            if let Some(ref eof_buf) = config.eof_string {
                if current_len >= config.eof_len {
                    let matches =
                        check_eof_match(&current_arg[..current_len], &eof_buf[..config.eof_len]);
                    if matches {
                        // Hit EOF string, stop reading
                        if input_fd != STDIN_FILENO {
                            close(input_fd);
                        }
                        return true;
                    }
                }
            }

            let is_delimiter = if delimiter == 0 {
                // Whitespace mode
                c == b' ' || c == b'\t' || c == b'\n'
            } else {
                c == delimiter
            };

            let is_newline = c == b'\n';

            if is_delimiter {
                if current_len > 0 {
                    // Store completed argument
                    if *arg_count < MAX_ARGS {
                        args[*arg_count][..current_len]
                            .copy_from_slice(&current_arg[..current_len]);
                        arg_lens[*arg_count] = current_len;
                        *arg_count += 1;

                        // Check line limit
                        if config.max_lines > 0 && is_newline {
                            line_count += 1;
                            if line_count >= config.max_lines {
                                if input_fd != STDIN_FILENO {
                                    close(input_fd);
                                }
                                return true;
                            }
                        }
                    } else {
                        eprintlns("xargs: too many arguments");
                        if input_fd != STDIN_FILENO {
                            close(input_fd);
                        }
                        return false;
                    }
                    current_len = 0;
                } else if is_newline && config.max_lines > 0 {
                    // Empty line counts for max_lines
                    line_count += 1;
                    if line_count >= config.max_lines {
                        if input_fd != STDIN_FILENO {
                            close(input_fd);
                        }
                        return true;
                    }
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
            if input_fd != STDIN_FILENO {
                close(input_fd);
            }
            return false;
        }
    }

    if input_fd != STDIN_FILENO {
        close(input_fd);
    }

    true
}

fn check_eof_match(haystack: &[u8], needle: &[u8]) -> bool {
    if haystack.len() < needle.len() {
        return false;
    }
    let start = haystack.len() - needle.len();
    for i in 0..needle.len() {
        if haystack[start + i] != needle[i] {
            return false;
        }
    }
    true
}

// Signed by: ShadePacket - "Orchestrating parallel command execution"
fn execute_all(
    config: &XargsConfig,
    cmd_parts: &[[u8; MAX_ARG_LEN]; MAX_ARGS],
    cmd_lens: &[usize; MAX_ARGS],
    cmd_count: usize,
    args: &[[u8; MAX_ARG_LEN]; MAX_ARGS],
    arg_lens: &[usize; MAX_ARGS],
    arg_count: usize,
    stats: &mut ExecutionStats,
) -> i32 {
    let mut overall_status = 0;

    if config.replace_str.is_some() {
        // Replace mode: execute once for each argument
        if config.max_procs > 1 {
            // Parallel replace mode
            overall_status = execute_parallel_replace(
                config, cmd_parts, cmd_lens, cmd_count, args, arg_lens, arg_count, stats,
            );
        } else {
            // Sequential replace mode
            for i in 0..arg_count {
                let status = execute_with_replace(
                    config,
                    cmd_parts,
                    cmd_lens,
                    cmd_count,
                    &args[i][..arg_lens[i]],
                );
                stats.commands_executed += 1;
                if status != 0 {
                    stats.commands_failed += 1;
                    overall_status = map_exit_status(status);
                    if config.exit_on_error {
                        break;
                    }
                }
            }
        }
    } else {
        // Batch mode: execute with groups of arguments
        let mut i = 0;

        if config.max_procs > 1 {
            // Parallel batch mode
            overall_status = execute_parallel_batch(
                config, cmd_parts, cmd_lens, cmd_count, args, arg_lens, arg_count, stats,
            );
        } else {
            // Sequential batch mode
            while i < arg_count {
                let batch_size = (arg_count - i).min(config.max_args);
                let status = execute_batch(
                    config, cmd_parts, cmd_lens, cmd_count, args, arg_lens, i, batch_size,
                );
                stats.commands_executed += 1;
                if status != 0 {
                    stats.commands_failed += 1;
                    overall_status = map_exit_status(status);
                    if config.exit_on_error {
                        break;
                    }
                }
                i += batch_size;
            }

            // Handle case where there are no arguments but we should run anyway
            if arg_count == 0 && !config.no_run_if_empty {
                let status =
                    execute_batch(config, cmd_parts, cmd_lens, cmd_count, args, arg_lens, 0, 0);
                stats.commands_executed += 1;
                if status != 0 {
                    stats.commands_failed += 1;
                    overall_status = map_exit_status(status);
                }
            }
        }
    }

    overall_status
}

// Signed by: NeonRoot - "Mapping exit codes to xargs conventions"
fn map_exit_status(status: i32) -> i32 {
    match status {
        0 => 0,
        1..=125 => 123, // Any child exit 1-125 returns 123
        126 => 126,     // Command cannot be run
        127 => 127,     // Command not found
        255 => 125,     // Map 255 to 125
        _ => 1,         // Other errors
    }
}

// Signed by: ThreadRogue - "Parallel execution with process pool"
fn execute_parallel_replace(
    config: &XargsConfig,
    cmd_parts: &[[u8; MAX_ARG_LEN]; MAX_ARGS],
    cmd_lens: &[usize; MAX_ARGS],
    cmd_count: usize,
    args: &[[u8; MAX_ARG_LEN]; MAX_ARGS],
    arg_lens: &[usize; MAX_ARGS],
    arg_count: usize,
    stats: &mut ExecutionStats,
) -> i32 {
    let mut pids: [i32; MAX_PROCS] = [0; MAX_PROCS];
    let mut active_count = 0;
    let mut next_arg = 0;
    let mut overall_status = 0;

    while next_arg < arg_count || active_count > 0 {
        // Start new processes up to max_procs
        while active_count < config.max_procs && next_arg < arg_count {
            let pid = fork();
            if pid == 0 {
                // Child: execute command
                let status = execute_with_replace(
                    config,
                    cmd_parts,
                    cmd_lens,
                    cmd_count,
                    &args[next_arg][..arg_lens[next_arg]],
                );
                exit(status);
            } else if pid > 0 {
                pids[active_count] = pid;
                active_count += 1;
                stats.commands_executed += 1;
                if stats.max_parallel < active_count {
                    stats.max_parallel = active_count;
                }
                next_arg += 1;
            } else {
                eprintlns("xargs: fork failed");
                return 255;
            }
        }

        // Wait for any child to complete
        if active_count > 0 {
            let mut status = 0;
            let finished_pid = waitpid(-1, &mut status, 0);
            if finished_pid > 0 {
                // Remove finished PID from active list
                for i in 0..active_count {
                    if pids[i] == finished_pid {
                        // Shift remaining PIDs
                        for j in i..active_count - 1 {
                            pids[j] = pids[j + 1];
                        }
                        active_count -= 1;
                        break;
                    }
                }

                if status != 0 {
                    stats.commands_failed += 1;
                    overall_status = map_exit_status(status);
                    if config.exit_on_error {
                        // Kill remaining processes
                        for i in 0..active_count {
                            kill(pids[i], SIGTERM);
                        }
                        // Wait for them to finish
                        while active_count > 0 {
                            waitpid(-1, &mut status, 0);
                            active_count -= 1;
                        }
                        return overall_status;
                    }
                }
            }
        }
    }

    overall_status
}

fn execute_parallel_batch(
    config: &XargsConfig,
    cmd_parts: &[[u8; MAX_ARG_LEN]; MAX_ARGS],
    cmd_lens: &[usize; MAX_ARGS],
    cmd_count: usize,
    args: &[[u8; MAX_ARG_LEN]; MAX_ARGS],
    arg_lens: &[usize; MAX_ARGS],
    arg_count: usize,
    stats: &mut ExecutionStats,
) -> i32 {
    let mut pids: [i32; MAX_PROCS] = [0; MAX_PROCS];
    let mut active_count = 0;
    let mut next_idx = 0;
    let mut overall_status = 0;

    // Handle empty case
    if arg_count == 0 {
        if !config.no_run_if_empty {
            let status =
                execute_batch(config, cmd_parts, cmd_lens, cmd_count, args, arg_lens, 0, 0);
            stats.commands_executed += 1;
            if status != 0 {
                stats.commands_failed += 1;
                return map_exit_status(status);
            }
        }
        return 0;
    }

    while next_idx < arg_count || active_count > 0 {
        // Start new processes
        while active_count < config.max_procs && next_idx < arg_count {
            let batch_size = (arg_count - next_idx).min(config.max_args);

            let pid = fork();
            if pid == 0 {
                // Child: execute batch
                let status = execute_batch(
                    config, cmd_parts, cmd_lens, cmd_count, args, arg_lens, next_idx, batch_size,
                );
                exit(status);
            } else if pid > 0 {
                pids[active_count] = pid;
                active_count += 1;
                stats.commands_executed += 1;
                if stats.max_parallel < active_count {
                    stats.max_parallel = active_count;
                }
                next_idx += batch_size;
            } else {
                eprintlns("xargs: fork failed");
                return 255;
            }
        }

        // Wait for completion
        if active_count > 0 {
            let mut status = 0;
            let finished_pid = waitpid(-1, &mut status, 0);
            if finished_pid > 0 {
                for i in 0..active_count {
                    if pids[i] == finished_pid {
                        for j in i..active_count - 1 {
                            pids[j] = pids[j + 1];
                        }
                        active_count -= 1;
                        break;
                    }
                }

                if status != 0 {
                    stats.commands_failed += 1;
                    overall_status = map_exit_status(status);
                    if config.exit_on_error {
                        for i in 0..active_count {
                            kill(pids[i], SIGTERM);
                        }
                        while active_count > 0 {
                            waitpid(-1, &mut status, 0);
                            active_count -= 1;
                        }
                        return overall_status;
                    }
                }
            }
        }
    }

    overall_status
}

// Signed by: IronGhost - "Replace mode execution with pattern matching"
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
    let mut total_len = 0;

    for i in 0..cmd_count {
        if final_count >= MAX_ARGS {
            break;
        }

        let part = &cmd_parts[i][..cmd_lens[i]];

        // Check if this part contains the replace pattern
        if contains_pattern(part, replace_pattern) {
            // Replace pattern with argument
            let new_len = replacement.len().min(MAX_ARG_LEN);

            // Check command length limit
            if config.exit_on_error && total_len + new_len > config.max_chars {
                eprintlns("xargs: command line too long");
                return 124;
            }

            final_cmd[final_count][..new_len].copy_from_slice(&replacement[..new_len]);
            final_lens[final_count] = new_len;
            total_len += new_len + 1; // +1 for space
        } else {
            // Copy as-is
            if config.exit_on_error && total_len + cmd_lens[i] > config.max_chars {
                eprintlns("xargs: command line too long");
                return 124;
            }

            final_cmd[final_count][..cmd_lens[i]].copy_from_slice(part);
            final_lens[final_count] = cmd_lens[i];
            total_len += cmd_lens[i] + 1;
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
    let mut total_len = 0;

    // Copy command parts
    for i in 0..cmd_count {
        if final_count >= MAX_ARGS {
            break;
        }
        final_cmd[final_count][..cmd_lens[i]].copy_from_slice(&cmd_parts[i][..cmd_lens[i]]);
        final_lens[final_count] = cmd_lens[i];
        total_len += cmd_lens[i] + 1;
        final_count += 1;
    }

    // Add arguments
    for i in 0..count {
        if final_count >= MAX_ARGS {
            break;
        }
        let arg_idx = start_idx + i;

        // Check command length limit
        if config.exit_on_error && total_len + arg_lens[arg_idx] > config.max_chars {
            eprintlns("xargs: command line too long");
            return 124;
        }

        final_cmd[final_count][..arg_lens[arg_idx]]
            .copy_from_slice(&args[arg_idx][..arg_lens[arg_idx]]);
        final_lens[final_count] = arg_lens[arg_idx];
        total_len += arg_lens[arg_idx] + 1;
        final_count += 1;
    }

    execute_command(config, &final_cmd, &final_lens, final_count)
}

// Signed by: EmberLock - "The execution chamber where commands come alive"
fn execute_command(
    config: &XargsConfig,
    cmd_parts: &[[u8; MAX_ARG_LEN]; MAX_ARGS],
    cmd_lens: &[usize; MAX_ARGS],
    cmd_count: usize,
) -> i32 {
    if cmd_count == 0 {
        return 0;
    }

    // Build command string for display
    let mut cmd_str = [0u8; MAX_CMD_LEN];
    let mut pos = 0;

    for i in 0..cmd_count {
        if i > 0 && pos < MAX_CMD_LEN {
            cmd_str[pos] = b' ';
            pos += 1;
        }
        let len = cmd_lens[i].min(MAX_CMD_LEN - pos);
        if len > 0 {
            cmd_str[pos..pos + len].copy_from_slice(&cmd_parts[i][..len]);
            pos += len;
        }
    }

    // Verbose or interactive mode
    if config.verbose || config.interactive {
        write(STDERR_FILENO, &cmd_str[..pos]);
        write(STDERR_FILENO, b"\n");
    }

    // Interactive prompt
    if config.interactive {
        write(STDERR_FILENO, b"?...");
        let tty_fd = if config.open_tty {
            open2("/dev/tty", O_RDWR)
        } else {
            open2("/dev/console", O_RDONLY)
        };

        if tty_fd >= 0 {
            let mut response = [0u8; 2];
            let n = read(tty_fd, &mut response);
            close(tty_fd);

            if n > 0 && response[0] != b'y' && response[0] != b'Y' {
                return 0;
            }
        } else {
            // Can't read response, assume yes
            eprintlns("y");
        }
    }

    // Fork and exec
    let pid = fork();
    if pid == 0 {
        // Child process
        let cmd_cstr = unsafe { core::str::from_utf8_unchecked(&cmd_str[..pos]) };
        exec(cmd_cstr);
        // If exec fails
        exit(127);
    } else if pid > 0 {
        // Parent process - wait for child
        let mut status = 0;
        waitpid(pid, &mut status, 0);
        return status;
    } else {
        eprintlns("xargs: fork failed");
        return 255;
    }
}

// Signed by: StaticRiot - "Statistics reporting for the performance conscious"
fn print_stats(stats: &ExecutionStats) {
    eprintlns("");
    eprintlns("╔════════════════════════════════════════════════════════════════╗");
    eprintlns("║                    Execution Statistics                        ║");
    eprintlns("╚════════════════════════════════════════════════════════════════╝");

    eprints("Commands executed:    ");
    print_number(stats.commands_executed);
    eprintlns("");

    eprints("Commands failed:      ");
    print_number(stats.commands_failed);
    eprintlns("");

    eprints("Max parallel:         ");
    print_number(stats.max_parallel);
    eprintlns("");

    eprints("Arguments processed:  ");
    print_number(stats.total_args_processed);
    eprintlns("");

    eprints("Bytes read:           ");
    print_number(stats.bytes_read);
    eprintlns("");
}
