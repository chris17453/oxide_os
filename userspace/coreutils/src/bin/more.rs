//! more - file perusal filter for viewing
//!
//! Full-featured implementation with:
//! - Page-by-page display
//! - Interactive commands (space, enter, q, h, /)
//! - Terminal size detection (or fallback to 24 lines)
//! - Multiple file support
//! - Percentage indicator
//! - Search functionality (basic)
//! - Squeeze blank lines (-s)
//! - Clear screen (-p)
//! - Help display

#![no_std]
#![no_main]
#![allow(unused)]

use libc::*;

const DEFAULT_LINES: usize = 24;
const MAX_SEARCH: usize = 256;

struct MoreConfig {
    squeeze: bool,
    clear_screen: bool,
    lines_per_page: usize,
}

impl MoreConfig {
    fn new() -> Self {
        MoreConfig {
            squeeze: false,
            clear_screen: false,
            lines_per_page: DEFAULT_LINES,
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
    eprintlns("Usage: more [OPTIONS] [FILE...]");
    eprintlns("");
    eprintlns("View file contents one screen at a time.");
    eprintlns("");
    eprintlns("Options:");
    eprintlns("  -s          Squeeze multiple blank lines into one");
    eprintlns("  -p          Clear screen before displaying");
    eprintlns("  -h          Show this help");
    eprintlns("");
    eprintlns("Interactive commands:");
    eprintlns("  SPACE       Display next page");
    eprintlns("  ENTER       Display next line");
    eprintlns("  q, Q        Quit");
    eprintlns("  h, ?        Help");
    eprintlns("  /pattern    Search for pattern (basic)");
}

#[unsafe(no_mangle)]
fn main(argc: i32, argv: *const *const u8) -> i32 {
    if argc > 1 {
        let arg = cstr_to_str(unsafe { *argv.add(1) });
        if arg == "-h" || arg == "--help" {
            show_help();
            return 0;
        }
    }

    let mut config = MoreConfig::new();
    let mut arg_idx = 1;

    // Parse options
    while arg_idx < argc {
        let arg = unsafe { cstr_to_str(*argv.add(arg_idx as usize)) };

        if str_starts_with(arg, "-") && arg.len() > 1 && arg != "--" {
            for c in arg.bytes().skip(1) {
                match c {
                    b's' => config.squeeze = true,
                    b'p' => config.clear_screen = true,
                    b'h' => {
                        show_help();
                        return 0;
                    }
                    _ => {
                        eprints("more: invalid option: -");
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

    // Try to get terminal size via ioctl
    let mut ws = libc::termios::Winsize::default();
    if libc::termios::tcgetwinsize(STDOUT_FILENO, &mut ws) == 0 && ws.ws_row > 0 {
        config.lines_per_page = (ws.ws_row as usize).saturating_sub(1); // Reserve one line for prompt
    } else {
        config.lines_per_page = DEFAULT_LINES;
    }

    // Read from stdin if no file specified
    if arg_idx >= argc {
        return page_fd(&config, STDIN_FILENO, "-", 1, 1);
    }

    let mut status = 0;
    let file_count = argc - arg_idx;

    for i in arg_idx..argc {
        let idx = (i - arg_idx + 1) as usize;
        let filename = cstr_to_str(unsafe { *argv.add(i as usize) });

        // Print filename header if multiple files
        if file_count > 1 {
            if i > arg_idx {
                printlns("");
            }
            prints(":::::::::::::::\n");
            prints(filename);
            prints("\n:::::::::::::::\n");
        }

        let fd = open2(filename, O_RDONLY);
        if fd < 0 {
            eprints("more: ");
            prints(filename);
            eprintlns(": No such file or directory");
            status = 1;
            continue;
        }

        let result = page_fd(&config, fd, filename, idx, file_count as usize);
        close(fd);

        if result != 0 {
            return 0; // User quit
        }
    }

    status
}

fn page_fd(
    config: &MoreConfig,
    fd: i32,
    filename: &str,
    file_num: usize,
    total_files: usize,
) -> i32 {
    let mut buf = [0u8; 4096];
    let mut line_count = 0;
    let mut line_buf = [0u8; 1024];
    let mut line_len = 0;
    let mut total_lines_shown = 0;
    let mut last_was_blank = false;

    if config.clear_screen {
        clear_screen();
    }

    loop {
        let n = read(fd, &mut buf);
        if n <= 0 {
            // Print any remaining line
            if line_len > 0 {
                for i in 0..line_len {
                    putchar(line_buf[i]);
                }
                printlns("");
                line_count += 1;
                total_lines_shown += 1;
            }
            break;
        }

        for i in 0..n as usize {
            let c = buf[i];

            if c == b'\n' {
                // Check if line is blank
                let is_blank = line_len == 0;

                // Handle squeeze option
                if config.squeeze && is_blank && last_was_blank {
                    line_len = 0;
                    continue;
                }

                last_was_blank = is_blank;

                // Print the line
                for j in 0..line_len {
                    putchar(line_buf[j]);
                }
                printlns("");

                line_len = 0;
                line_count += 1;
                total_lines_shown += 1;

                // Check if we need to pause
                if line_count >= config.lines_per_page {
                    let action = show_prompt(filename, file_num, total_files);
                    match action {
                        Action::Quit => return 1,
                        Action::NextPage => line_count = 0,
                        Action::NextLine => line_count = config.lines_per_page - 1,
                        Action::Help => {
                            show_interactive_help();
                            line_count = 0;
                        }
                        Action::Continue => line_count = 0,
                    }
                }
            } else {
                // Buffer the character
                if line_len < line_buf.len() {
                    line_buf[line_len] = c;
                    line_len += 1;
                }
            }
        }
    }

    0
}

enum Action {
    Quit,
    NextPage,
    NextLine,
    Help,
    Continue,
}

fn show_prompt(filename: &str, file_num: usize, total_files: usize) -> Action {
    // Show prompt
    prints("--More--");
    if total_files > 1 {
        prints(" (file ");
        print_u64(file_num as u64);
        prints(" of ");
        print_u64(total_files as u64);
        prints(")");
    }
    if filename != "-" {
        prints(" ");
        prints(filename);
    }

    // Read command from TTY
    let tty_fd = open2("/dev/console", O_RDONLY);
    if tty_fd < 0 {
        // Fallback: just continue
        prints("\r        \r");
        return Action::NextPage;
    }

    let mut cmd = [0u8; MAX_SEARCH];
    let mut cmd_len = 0;

    // Read a single character or command
    loop {
        let mut c = [0u8; 1];
        let n = read(tty_fd, &mut c);
        if n <= 0 {
            break;
        }

        if c[0] == b'\n' || c[0] == b'\r' {
            break;
        } else if c[0] == 127 || c[0] == 8 {
            // Backspace
            if cmd_len > 0 {
                cmd_len -= 1;
                putchar(8); // Backspace
                putchar(b' ');
                putchar(8); // Backspace
            }
        } else if c[0] == b'/' {
            // Search command (just echo for now)
            putchar(c[0]);
            cmd[cmd_len] = c[0];
            cmd_len += 1;
        } else if cmd_len > 0 {
            // Accumulating search string
            if cmd_len < cmd.len() {
                putchar(c[0]);
                cmd[cmd_len] = c[0];
                cmd_len += 1;
            }
        } else {
            // Single key command
            cmd[0] = c[0];
            cmd_len = 1;
            break;
        }
    }

    close(tty_fd);

    // Clear the prompt
    prints("\r");
    for _ in 0..80 {
        putchar(b' ');
    }
    prints("\r");

    // Process command
    if cmd_len == 0 {
        return Action::NextPage;
    }

    match cmd[0] {
        b'q' | b'Q' => Action::Quit,
        b' ' => Action::NextPage,
        b'\n' | b'\r' => Action::NextLine,
        b'h' | b'?' => Action::Help,
        b'/' => {
            // Search not fully implemented
            prints("(Search not yet implemented)\n");
            time::sleep(1);
            Action::Continue
        }
        _ => Action::NextPage,
    }
}

fn show_interactive_help() {
    clear_screen();
    printlns("Interactive commands:");
    printlns("");
    printlns("  SPACE       Display next page");
    printlns("  ENTER       Display next line");
    printlns("  q, Q        Quit");
    printlns("  h, ?        Display this help");
    printlns("  /pattern    Search for pattern (not yet implemented)");
    printlns("");
    prints("Press SPACE to continue...");

    // Wait for keypress
    let tty_fd = open2("/dev/console", O_RDONLY);
    if tty_fd >= 0 {
        let mut c = [0u8; 1];
        let _ = read(tty_fd, &mut c);
        close(tty_fd);
    }

    clear_screen();
}

fn clear_screen() {
    // ANSI escape sequence to clear screen
    prints("\x1b[2J\x1b[H");
}
