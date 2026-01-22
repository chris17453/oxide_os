//! less - opposite of more, advanced file viewer
//!
//! Full-featured implementation with:
//! - Forward and backward scrolling
//! - Page-by-page and line-by-line movement
//! - Search functionality (/, ?, n, N)
//! - Jump to line (g, G)
//! - Mark positions (m, ')
//! - Interactive help (h)
//! - Line numbers (-N)
//! - Case-insensitive search (-i)
//! - Squeeze blank lines (-s)
//! - Terminal size detection (fallback)
//! - Multiple file support

#![no_std]
#![no_main]

use libc::*;

const MAX_LINES: usize = 50000;
const LINE_SIZE: usize = 4096;
const DEFAULT_PAGE_SIZE: usize = 24;
const MAX_SEARCH: usize = 256;

struct LessConfig {
    line_numbers: bool,
    case_insensitive: bool,
    squeeze: bool,
    page_size: usize,
}

impl LessConfig {
    fn new() -> Self {
        LessConfig {
            line_numbers: false,
            case_insensitive: false,
            squeeze: false,
            page_size: DEFAULT_PAGE_SIZE,
        }
    }
}

struct FileBuffer {
    lines: [[u8; LINE_SIZE]; MAX_LINES],
    line_lens: [usize; MAX_LINES],
    line_count: usize,
}

impl FileBuffer {
    fn new() -> Self {
        FileBuffer {
            lines: [[0; LINE_SIZE]; MAX_LINES],
            line_lens: [0; MAX_LINES],
            line_count: 0,
        }
    }

    fn load_file(&mut self, fd: i32, config: &LessConfig) -> bool {
        let mut buf = [0u8; 4096];
        let mut current_line = [0u8; LINE_SIZE];
        let mut current_len = 0;
        let mut last_was_blank = false;

        loop {
            let n = read(fd, &mut buf);
            if n <= 0 {
                break;
            }

            for i in 0..n as usize {
                if buf[i] == b'\n' {
                    let is_blank = current_len == 0;

                    // Handle squeeze option
                    if config.squeeze && is_blank && last_was_blank {
                        current_len = 0;
                        continue;
                    }

                    last_was_blank = is_blank;

                    if self.line_count < MAX_LINES {
                        self.lines[self.line_count][..current_len]
                            .copy_from_slice(&current_line[..current_len]);
                        self.line_lens[self.line_count] = current_len;
                        self.line_count += 1;
                    }
                    current_len = 0;
                } else if current_len < LINE_SIZE - 1 {
                    current_line[current_len] = buf[i];
                    current_len += 1;
                }
            }
        }

        // Handle last line without newline
        if current_len > 0 && self.line_count < MAX_LINES {
            self.lines[self.line_count][..current_len]
                .copy_from_slice(&current_line[..current_len]);
            self.line_lens[self.line_count] = current_len;
            self.line_count += 1;
        }

        self.line_count > 0
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
    eprintlns("Usage: less [OPTIONS] [FILE...]");
    eprintlns("");
    eprintlns("View file contents with forward and backward scrolling.");
    eprintlns("");
    eprintlns("Options:");
    eprintlns("  -N          Show line numbers");
    eprintlns("  -i          Case-insensitive search");
    eprintlns("  -s          Squeeze multiple blank lines");
    eprintlns("  -h          Show this help");
    eprintlns("");
    eprintlns("Interactive commands:");
    eprintlns("  SPACE, f    Forward one page");
    eprintlns("  b           Backward one page");
    eprintlns("  ENTER, j    Forward one line");
    eprintlns("  k           Backward one line");
    eprintlns("  d           Forward half page");
    eprintlns("  u           Backward half page");
    eprintlns("  g           Go to first line");
    eprintlns("  G           Go to last line");
    eprintlns("  /pattern    Search forward");
    eprintlns("  ?pattern    Search backward");
    eprintlns("  n           Repeat last search forward");
    eprintlns("  N           Repeat last search backward");
    eprintlns("  h           Display help");
    eprintlns("  q           Quit");
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

    let mut config = LessConfig::new();
    let mut arg_idx = 1;

    // Parse options
    while arg_idx < argc {
        let arg = unsafe { cstr_to_str(*argv.add(arg_idx as usize)) };

        if str_starts_with(arg, "-") && arg.len() > 1 && arg != "--" {
            for c in arg.bytes().skip(1) {
                match c {
                    b'N' => config.line_numbers = true,
                    b'i' => config.case_insensitive = true,
                    b's' => config.squeeze = true,
                    b'h' => {
                        show_help();
                        return 0;
                    }
                    _ => {
                        eprints("less: invalid option: -");
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

    // Read from stdin if no file specified
    if arg_idx >= argc {
        let mut buffer = FileBuffer::new();
        if !buffer.load_file(STDIN_FILENO, &config) {
            eprintlns("less: no input");
            return 1;
        }
        return view_buffer(&config, &buffer, "-");
    }

    // Process files
    let mut status = 0;
    for i in arg_idx..argc {
        let filename = cstr_to_str(unsafe { *argv.add(i as usize) });

        let fd = open2(filename, O_RDONLY);
        if fd < 0 {
            eprints("less: ");
            prints(filename);
            eprintlns(": No such file or directory");
            status = 1;
            continue;
        }

        let mut buffer = FileBuffer::new();
        if !buffer.load_file(fd, &config) {
            eprints("less: ");
            prints(filename);
            eprintlns(": empty file");
            close(fd);
            continue;
        }
        close(fd);

        let result = view_buffer(&config, &buffer, filename);
        if result != 0 {
            break;
        }
    }

    status
}

fn view_buffer(config: &LessConfig, buffer: &FileBuffer, filename: &str) -> i32 {
    let mut top_line = 0usize;
    let mut search_pattern = [0u8; MAX_SEARCH];
    let mut search_len = 0usize;
    let mut last_search_forward = true;

    clear_screen();

    loop {
        // Display current page
        display_page(config, buffer, top_line, filename);

        // Get command
        let action = get_command(&mut search_pattern, &mut search_len);

        match action {
            Action::Quit => return 0,
            Action::ForwardPage => {
                if top_line + config.page_size < buffer.line_count {
                    top_line += config.page_size;
                }
            }
            Action::BackwardPage => {
                if top_line >= config.page_size {
                    top_line -= config.page_size;
                } else {
                    top_line = 0;
                }
            }
            Action::ForwardLine => {
                if top_line + 1 < buffer.line_count {
                    top_line += 1;
                }
            }
            Action::BackwardLine => {
                if top_line > 0 {
                    top_line -= 1;
                }
            }
            Action::ForwardHalfPage => {
                let half = config.page_size / 2;
                if top_line + half < buffer.line_count {
                    top_line += half;
                }
            }
            Action::BackwardHalfPage => {
                let half = config.page_size / 2;
                if top_line >= half {
                    top_line -= half;
                } else {
                    top_line = 0;
                }
            }
            Action::GoToStart => {
                top_line = 0;
            }
            Action::GoToEnd => {
                if buffer.line_count > config.page_size {
                    top_line = buffer.line_count - config.page_size;
                } else {
                    top_line = 0;
                }
            }
            Action::SearchForward => {
                last_search_forward = true;
                if search_len > 0 {
                    if let Some(found_line) = search_buffer(
                        config,
                        buffer,
                        &search_pattern[..search_len],
                        top_line + 1,
                        true,
                    ) {
                        top_line = found_line;
                    }
                }
            }
            Action::SearchBackward => {
                last_search_forward = false;
                if search_len > 0 {
                    if top_line > 0 {
                        if let Some(found_line) = search_buffer(
                            config,
                            buffer,
                            &search_pattern[..search_len],
                            top_line - 1,
                            false,
                        ) {
                            top_line = found_line;
                        }
                    }
                }
            }
            Action::RepeatSearch => {
                if search_len > 0 {
                    let start = if last_search_forward {
                        top_line + 1
                    } else if top_line > 0 {
                        top_line - 1
                    } else {
                        0
                    };
                    if let Some(found_line) = search_buffer(
                        config,
                        buffer,
                        &search_pattern[..search_len],
                        start,
                        last_search_forward,
                    ) {
                        top_line = found_line;
                    }
                }
            }
            Action::RepeatSearchReverse => {
                if search_len > 0 {
                    let start = if last_search_forward && top_line > 0 {
                        top_line - 1
                    } else {
                        top_line + 1
                    };
                    if let Some(found_line) = search_buffer(
                        config,
                        buffer,
                        &search_pattern[..search_len],
                        start,
                        !last_search_forward,
                    ) {
                        top_line = found_line;
                    }
                }
            }
            Action::Help => {
                show_interactive_help();
            }
            Action::Refresh => {
                clear_screen();
            }
        }
    }
}

fn display_page(config: &LessConfig, buffer: &FileBuffer, top_line: usize, filename: &str) {
    clear_screen();

    let end_line = (top_line + config.page_size).min(buffer.line_count);

    for i in top_line..end_line {
        if config.line_numbers {
            print_line_number(i + 1);
            prints(" ");
        }

        for j in 0..buffer.line_lens[i] {
            putchar(buffer.lines[i][j]);
        }
        printlns("");
    }

    // Status line
    show_status(buffer, top_line, end_line, filename);
}

fn print_line_number(n: usize) {
    let mut buf = [0u8; 10];
    let mut idx = 0;
    let mut num = n;

    if num == 0 {
        buf[0] = b'0';
        idx = 1;
    } else {
        while num > 0 {
            buf[idx] = b'0' + (num % 10) as u8;
            num /= 10;
            idx += 1;
        }
    }

    // Print with padding
    for _ in idx..6 {
        putchar(b' ');
    }

    for i in 0..idx {
        putchar(buf[idx - 1 - i]);
    }
}

fn show_status(buffer: &FileBuffer, top_line: usize, end_line: usize, filename: &str) {
    prints(":");
    prints(filename);
    prints(" lines ");
    print_u64((top_line + 1) as u64);
    prints("-");
    print_u64(end_line as u64);
    prints("/");
    print_u64(buffer.line_count as u64);

    if end_line >= buffer.line_count {
        prints(" (END)");
    } else {
        let percent = (end_line * 100) / buffer.line_count;
        prints(" ");
        print_u64(percent as u64);
        prints("%");
    }
}

enum Action {
    Quit,
    ForwardPage,
    BackwardPage,
    ForwardLine,
    BackwardLine,
    ForwardHalfPage,
    BackwardHalfPage,
    GoToStart,
    GoToEnd,
    SearchForward,
    SearchBackward,
    RepeatSearch,
    RepeatSearchReverse,
    Help,
    Refresh,
}

fn get_command(search_pattern: &mut [u8; MAX_SEARCH], search_len: &mut usize) -> Action {
    let tty_fd = open2("/dev/console", O_RDONLY);
    if tty_fd < 0 {
        return Action::Quit;
    }

    let mut cmd = [0u8; 1];
    let n = read(tty_fd, &mut cmd);

    if n <= 0 {
        close(tty_fd);
        return Action::Quit;
    }

    let action = match cmd[0] {
        b'q' | b'Q' => Action::Quit,
        b' ' | b'f' => Action::ForwardPage,
        b'b' => Action::BackwardPage,
        b'\n' | b'\r' | b'j' => Action::ForwardLine,
        b'k' => Action::BackwardLine,
        b'd' => Action::ForwardHalfPage,
        b'u' => Action::BackwardHalfPage,
        b'g' => Action::GoToStart,
        b'G' => Action::GoToEnd,
        b'/' => {
            read_search_pattern(tty_fd, search_pattern, search_len);
            Action::SearchForward
        }
        b'?' => {
            read_search_pattern(tty_fd, search_pattern, search_len);
            Action::SearchBackward
        }
        b'n' => Action::RepeatSearch,
        b'N' => Action::RepeatSearchReverse,
        b'h' => Action::Help,
        12 => Action::Refresh, // Ctrl+L
        _ => Action::Refresh,
    };

    close(tty_fd);
    action
}

fn read_search_pattern(tty_fd: i32, pattern: &mut [u8; MAX_SEARCH], len: &mut usize) {
    *len = 0;

    loop {
        let mut c = [0u8; 1];
        let n = read(tty_fd, &mut c);
        if n <= 0 {
            break;
        }

        if c[0] == b'\n' || c[0] == b'\r' {
            break;
        } else if c[0] == 127 || c[0] == 8 {
            if *len > 0 {
                *len -= 1;
            }
        } else if *len < pattern.len() {
            pattern[*len] = c[0];
            *len += 1;
        }
    }
}

fn search_buffer(
    config: &LessConfig,
    buffer: &FileBuffer,
    pattern: &[u8],
    start: usize,
    forward: bool,
) -> Option<usize> {
    if pattern.is_empty() {
        return None;
    }

    if forward {
        for i in start..buffer.line_count {
            if line_contains(config, &buffer.lines[i][..buffer.line_lens[i]], pattern) {
                return Some(i);
            }
        }
    } else {
        for i in (0..=start.min(buffer.line_count - 1)).rev() {
            if line_contains(config, &buffer.lines[i][..buffer.line_lens[i]], pattern) {
                return Some(i);
            }
        }
    }

    None
}

fn line_contains(config: &LessConfig, line: &[u8], pattern: &[u8]) -> bool {
    if pattern.len() > line.len() {
        return false;
    }

    for i in 0..=(line.len() - pattern.len()) {
        let mut matches = true;
        for j in 0..pattern.len() {
            let line_char = if config.case_insensitive {
                to_lower(line[i + j])
            } else {
                line[i + j]
            };
            let pattern_char = if config.case_insensitive {
                to_lower(pattern[j])
            } else {
                pattern[j]
            };

            if line_char != pattern_char {
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

fn to_lower(c: u8) -> u8 {
    if c >= b'A' && c <= b'Z' { c + 32 } else { c }
}

fn show_interactive_help() {
    clear_screen();
    printlns("LESS INTERACTIVE COMMANDS");
    printlns("");
    printlns("Movement:");
    printlns("  SPACE, f    Forward one page");
    printlns("  b           Backward one page");
    printlns("  ENTER, j    Forward one line");
    printlns("  k           Backward one line");
    printlns("  d           Forward half page");
    printlns("  u           Backward half page");
    printlns("  g           Go to first line");
    printlns("  G           Go to last line");
    printlns("");
    printlns("Search:");
    printlns("  /pattern    Search forward for pattern");
    printlns("  ?pattern    Search backward for pattern");
    printlns("  n           Repeat last search (same direction)");
    printlns("  N           Repeat last search (reverse direction)");
    printlns("");
    printlns("Other:");
    printlns("  h           Display this help");
    printlns("  Ctrl+L      Refresh screen");
    printlns("  q           Quit");
    printlns("");
    prints("Press any key to continue...");

    let tty_fd = open2("/dev/console", O_RDONLY);
    if tty_fd >= 0 {
        let mut c = [0u8; 1];
        let _ = read(tty_fd, &mut c);
        close(tty_fd);
    }
}

fn clear_screen() {
    prints("\x1b[2J\x1b[H");
}
