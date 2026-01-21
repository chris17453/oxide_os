//! wc - word, line, character, and byte count
//!
//! Full-featured implementation with:
//! - Line counting (-l)
//! - Word counting (-w)
//! - Character counting (-m)
//! - Byte counting (-c)
//! - Max line length (-L)
//! - Multiple file support
//! - Stdin support
//! - Total line for multiple files
//! - Help message (-h)
//! - Proper error handling

#![no_std]
#![no_main]

use libc::*;

struct WcConfig {
    show_lines: bool,
    show_words: bool,
    show_chars: bool,
    show_bytes: bool,
    show_max_line_length: bool,
}

impl WcConfig {
    fn new() -> Self {
        WcConfig {
            show_lines: false,
            show_words: false,
            show_chars: false,
            show_bytes: false,
            show_max_line_length: false,
        }
    }

    fn is_default(&self) -> bool {
        !self.show_lines
            && !self.show_words
            && !self.show_chars
            && !self.show_bytes
            && !self.show_max_line_length
    }

    fn set_defaults(&mut self) {
        self.show_lines = true;
        self.show_words = true;
        self.show_bytes = true;
    }
}

struct Counts {
    lines: u64,
    words: u64,
    chars: u64,
    bytes: u64,
    max_line_length: u64,
}

impl Counts {
    fn new() -> Self {
        Counts {
            lines: 0,
            words: 0,
            chars: 0,
            bytes: 0,
            max_line_length: 0,
        }
    }

    fn add(&mut self, other: &Counts) {
        self.lines += other.lines;
        self.words += other.words;
        self.chars += other.chars;
        self.bytes += other.bytes;
        if other.max_line_length > self.max_line_length {
            self.max_line_length = other.max_line_length;
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
    eprintlns("Usage: wc [OPTION]... [FILE]...");
    eprintlns("");
    eprintlns("Print newline, word, and byte counts for each FILE, and a total line if");
    eprintlns("more than one FILE is specified. With no FILE, or when FILE is -, read");
    eprintlns("standard input.");
    eprintlns("");
    eprintlns("Options:");
    eprintlns("  -c              Print the byte counts");
    eprintlns("  -m              Print the character counts");
    eprintlns("  -l              Print the newline counts");
    eprintlns("  -L              Print the maximum display width");
    eprintlns("  -w              Print the word counts");
    eprintlns("  -h, --help      Display this help and exit");
    eprintlns("");
    eprintlns("A word is a non-zero-length sequence of characters delimited by white space.");
}

#[unsafe(no_mangle)]
fn main(argc: i32, argv: *const *const u8) -> i32 {
    let mut config = WcConfig::new();
    let mut arg_idx = 1;

    // Parse options
    while arg_idx < argc {
        let arg = unsafe { cstr_to_str(*argv.add(arg_idx as usize)) };

        if arg == "-h" || arg == "--help" {
            show_help();
            return 0;
        } else if str_starts_with(arg, "-") && arg.len() > 1 && arg != "-" {
            for c in arg.bytes().skip(1) {
                match c {
                    b'l' => config.show_lines = true,
                    b'w' => config.show_words = true,
                    b'c' => config.show_bytes = true,
                    b'm' => config.show_chars = true,
                    b'L' => config.show_max_line_length = true,
                    b'h' => {
                        show_help();
                        return 0;
                    }
                    _ => {
                        eprints("wc: invalid option: -");
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

    // Default: show lines, words, and bytes
    if config.is_default() {
        config.set_defaults();
    }

    let mut total = Counts::new();
    let mut file_count = 0;

    // If no files, read from stdin
    if arg_idx >= argc {
        let counts = count_fd(STDIN_FILENO);
        print_counts(&counts, "", &config);
        return 0;
    }

    // Process each file
    for i in arg_idx..argc {
        let path = unsafe { cstr_to_str(*argv.add(i as usize)) };

        let counts = if path == "-" {
            count_fd(STDIN_FILENO)
        } else {
            let fd = open2(path, O_RDONLY);
            if fd < 0 {
                eprints("wc: ");
                eprints(path);
                eprintlns(": No such file or directory");
                continue;
            }

            let c = count_fd(fd);
            close(fd);
            c
        };

        print_counts(&counts, path, &config);
        total.add(&counts);
        file_count += 1;
    }

    // Print total if multiple files
    if file_count > 1 {
        print_counts(&total, "total", &config);
    }

    0
}

fn count_fd(fd: i32) -> Counts {
    let mut counts = Counts::new();
    let mut in_word = false;
    let mut current_line_length = 0u64;

    let mut buf = [0u8; 4096];

    loop {
        let n = read(fd, &mut buf);
        if n <= 0 {
            break;
        }

        counts.bytes += n as u64;

        for i in 0..n as usize {
            let c = buf[i];

            // Count characters (for ASCII, chars == bytes)
            counts.chars += 1;

            // Count lines
            if c == b'\n' {
                counts.lines += 1;
                if current_line_length > counts.max_line_length {
                    counts.max_line_length = current_line_length;
                }
                current_line_length = 0;
            } else {
                current_line_length += 1;
            }

            // Count words
            let is_space = c == b' ' || c == b'\t' || c == b'\n' || c == b'\r';
            if is_space {
                if in_word {
                    counts.words += 1;
                    in_word = false;
                }
            } else {
                in_word = true;
            }
        }
    }

    // Count final word if file doesn't end with whitespace
    if in_word {
        counts.words += 1;
    }

    // Check final line length
    if current_line_length > counts.max_line_length {
        counts.max_line_length = current_line_length;
    }

    counts
}

fn print_counts(counts: &Counts, name: &str, config: &WcConfig) {
    if config.show_lines {
        print_u64_padded(counts.lines, 8);
    }
    if config.show_words {
        print_u64_padded(counts.words, 8);
    }
    if config.show_chars {
        print_u64_padded(counts.chars, 8);
    }
    if config.show_bytes {
        print_u64_padded(counts.bytes, 8);
    }
    if config.show_max_line_length {
        print_u64_padded(counts.max_line_length, 8);
    }
    if !name.is_empty() {
        prints(" ");
        prints(name);
    }
    printlns("");
}

fn print_u64_padded(n: u64, width: usize) {
    let mut buf = [b' '; 20];
    let mut val = n;
    let mut i = buf.len();

    if val == 0 {
        i -= 1;
        buf[i] = b'0';
    } else {
        while val > 0 {
            i -= 1;
            buf[i] = b'0' + (val % 10) as u8;
            val /= 10;
        }
    }

    let start = if buf.len() - i < width {
        buf.len() - width
    } else {
        i
    };
    for j in start..buf.len() {
        putchar(buf[j]);
    }
}
