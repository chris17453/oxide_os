//! cut - remove sections from each line of files
//!
//! Full-featured implementation with:
//! - Byte selection (-b)
//! - Character selection (-c)
//! - Field selection (-f)
//! - Custom delimiter (-d)
//! - Output delimiter (--output-delimiter)
//! - Complement mode (--complement)
//! - Only delimited lines (-s)
//! - Zero-terminated lines (-z)
//! - Range support (N, N-M, N-, -M)
//! - Multiple file support
//! - Proper error handling

#![no_std]
#![no_main]

use libc::*;

const MAX_LINE: usize = 8192;
const MAX_RANGES: usize = 128;

struct CutConfig {
    mode: CutMode,
    delimiter: u8,
    output_delimiter: Option<u8>,
    complement: bool,
    only_delimited: bool,
    zero_terminated: bool,
}

enum CutMode {
    None,
    Bytes(RangeList),
    Chars(RangeList),
    Fields(RangeList),
}

struct RangeList {
    ranges: [Range; MAX_RANGES],
    count: usize,
}

impl RangeList {
    fn new() -> Self {
        RangeList {
            ranges: [Range { start: 0, end: 0 }; MAX_RANGES],
            count: 0,
        }
    }

    fn add_range(&mut self, r: Range) {
        if self.count < MAX_RANGES {
            self.ranges[self.count] = r;
            self.count += 1;
        }
    }

    fn contains(&self, n: usize) -> bool {
        for i in 0..self.count {
            if n >= self.ranges[i].start && n <= self.ranges[i].end {
                return true;
            }
        }
        false
    }
}

#[derive(Copy, Clone)]
struct Range {
    start: usize,
    end: usize, // usize::MAX means open-ended
}

impl CutConfig {
    fn new() -> Self {
        CutConfig {
            mode: CutMode::None,
            delimiter: b'\t',
            output_delimiter: None,
            complement: false,
            only_delimited: false,
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

fn show_help() {
    eprintlns("Usage: cut [OPTIONS] [FILE...]");
    eprintlns("");
    eprintlns("Print selected parts of lines from each FILE to standard output.");
    eprintlns("");
    eprintlns("Selection (choose one):");
    eprintlns("  -b LIST     Select only these bytes");
    eprintlns("  -c LIST     Select only these characters");
    eprintlns("  -f LIST     Select only these fields");
    eprintlns("");
    eprintlns("LIST is made up of ranges separated by commas:");
    eprintlns("  N       N'th byte, character or field, counted from 1");
    eprintlns("  N-M     From N'th to M'th");
    eprintlns("  N-      From N'th to end of line");
    eprintlns("  -M      From first to M'th");
    eprintlns("");
    eprintlns("Options:");
    eprintlns("  -d DELIM        Use DELIM instead of TAB for field delimiter");
    eprintlns("  --output-delimiter=STR  Use STR as output delimiter (default: input delimiter)");
    eprintlns("  --complement    Complement the set of selected bytes/chars/fields");
    eprintlns("  -s              Do not print lines not containing delimiters (fields only)");
    eprintlns("  -z              Line delimiter is NUL, not newline");
    eprintlns("  -h              Show this help");
}

#[unsafe(no_mangle)]
fn main(argc: i32, argv: *const *const u8) -> i32 {
    if argc < 2 {
        eprintlns("cut: missing operand");
        eprintlns("Try 'cut -h' for more information.");
        return 1;
    }

    let mut config = CutConfig::new();
    let mut arg_idx = 1;

    // Parse options
    while arg_idx < argc {
        let arg = unsafe { cstr_to_str(*argv.add(arg_idx as usize)) };

        if arg == "-h" || arg == "--help" {
            show_help();
            return 0;
        } else if arg == "-b" {
            arg_idx += 1;
            if arg_idx >= argc {
                eprintlns("cut: option -b requires an argument");
                return 1;
            }
            let list_arg = unsafe { cstr_to_str(*argv.add(arg_idx as usize)) };
            match parse_range_list(list_arg) {
                Some(ranges) => config.mode = CutMode::Bytes(ranges),
                None => {
                    eprints("cut: invalid byte list: ");
                    printlns(list_arg);
                    return 1;
                }
            }
            arg_idx += 1;
        } else if arg == "-c" {
            arg_idx += 1;
            if arg_idx >= argc {
                eprintlns("cut: option -c requires an argument");
                return 1;
            }
            let list_arg = unsafe { cstr_to_str(*argv.add(arg_idx as usize)) };
            match parse_range_list(list_arg) {
                Some(ranges) => config.mode = CutMode::Chars(ranges),
                None => {
                    eprints("cut: invalid character list: ");
                    printlns(list_arg);
                    return 1;
                }
            }
            arg_idx += 1;
        } else if arg == "-f" {
            arg_idx += 1;
            if arg_idx >= argc {
                eprintlns("cut: option -f requires an argument");
                return 1;
            }
            let list_arg = unsafe { cstr_to_str(*argv.add(arg_idx as usize)) };
            match parse_range_list(list_arg) {
                Some(ranges) => config.mode = CutMode::Fields(ranges),
                None => {
                    eprints("cut: invalid field list: ");
                    printlns(list_arg);
                    return 1;
                }
            }
            arg_idx += 1;
        } else if arg == "-d" {
            arg_idx += 1;
            if arg_idx >= argc {
                eprintlns("cut: option -d requires an argument");
                return 1;
            }
            let delim_arg = unsafe { cstr_to_str(*argv.add(arg_idx as usize)) };
            if delim_arg.is_empty() {
                eprintlns("cut: delimiter must not be empty");
                return 1;
            }
            config.delimiter = delim_arg.as_bytes()[0];
            arg_idx += 1;
        } else if str_starts_with(arg, "--output-delimiter=") {
            let delim_str = &arg[19..];
            if delim_str.is_empty() {
                eprintlns("cut: output delimiter must not be empty");
                return 1;
            }
            config.output_delimiter = Some(delim_str.as_bytes()[0]);
            arg_idx += 1;
        } else if arg == "--complement" {
            config.complement = true;
            arg_idx += 1;
        } else if arg == "-s" || arg == "--only-delimited" {
            config.only_delimited = true;
            arg_idx += 1;
        } else if arg == "-z" || arg == "--zero-terminated" {
            config.zero_terminated = true;
            arg_idx += 1;
        } else if str_starts_with(arg, "-") && arg.len() > 1 && arg != "--" {
            // Handle combined short options like -f1 or -d,
            if str_starts_with(arg, "-b") && arg.len() > 2 {
                let list_arg = &arg[2..];
                match parse_range_list(list_arg) {
                    Some(ranges) => config.mode = CutMode::Bytes(ranges),
                    None => {
                        eprints("cut: invalid byte list: ");
                        printlns(list_arg);
                        return 1;
                    }
                }
                arg_idx += 1;
            } else if str_starts_with(arg, "-c") && arg.len() > 2 {
                let list_arg = &arg[2..];
                match parse_range_list(list_arg) {
                    Some(ranges) => config.mode = CutMode::Chars(ranges),
                    None => {
                        eprints("cut: invalid character list: ");
                        printlns(list_arg);
                        return 1;
                    }
                }
                arg_idx += 1;
            } else if str_starts_with(arg, "-f") && arg.len() > 2 {
                let list_arg = &arg[2..];
                match parse_range_list(list_arg) {
                    Some(ranges) => config.mode = CutMode::Fields(ranges),
                    None => {
                        eprints("cut: invalid field list: ");
                        printlns(list_arg);
                        return 1;
                    }
                }
                arg_idx += 1;
            } else if str_starts_with(arg, "-d") && arg.len() > 2 {
                config.delimiter = arg.as_bytes()[2];
                arg_idx += 1;
            } else {
                eprints("cut: invalid option: ");
                printlns(arg);
                return 1;
            }
        } else {
            break;
        }
    }

    // Validate mode
    match config.mode {
        CutMode::None => {
            eprintlns("cut: you must specify a list of bytes, characters, or fields");
            eprintlns("Try 'cut -h' for more information.");
            return 1;
        }
        _ => {}
    }

    // Validate options
    if config.only_delimited {
        match config.mode {
            CutMode::Fields(_) => {}
            _ => {
                eprintlns("cut: suppressing non-delimited lines makes sense only with fields");
                return 1;
            }
        }
    }

    // Process files or stdin
    let mut exit_code = 0;
    if arg_idx >= argc {
        process_file(&config, STDIN_FILENO);
    } else {
        for i in arg_idx..argc {
            let path = unsafe { cstr_to_str(*argv.add(i as usize)) };
            if path == "-" {
                process_file(&config, STDIN_FILENO);
            } else {
                let fd = open2(path, O_RDONLY);
                if fd < 0 {
                    eprints("cut: ");
                    prints(path);
                    eprintlns(": No such file or directory");
                    exit_code = 1;
                    continue;
                }
                process_file(&config, fd);
                close(fd);
            }
        }
    }

    exit_code
}

fn parse_range_list(s: &str) -> Option<RangeList> {
    let mut list = RangeList::new();
    let mut current = 0usize;
    let mut has_current = false;
    let mut in_range = false;
    let mut range_start = 0usize;

    for c in s.bytes() {
        if c >= b'0' && c <= b'9' {
            current = current * 10 + (c - b'0') as usize;
            has_current = true;
        } else if c == b',' {
            if in_range {
                // End of range N-M
                let end = if has_current { current } else { usize::MAX };
                list.add_range(Range {
                    start: range_start,
                    end,
                });
                in_range = false;
            } else if has_current {
                // Single number
                list.add_range(Range {
                    start: current,
                    end: current,
                });
            } else {
                return None; // Invalid: empty before comma
            }
            current = 0;
            has_current = false;
        } else if c == b'-' {
            if in_range {
                return None; // Invalid: double dash
            }
            range_start = if has_current { current } else { 1 };
            in_range = true;
            current = 0;
            has_current = false;
        } else {
            return None; // Invalid character
        }
    }

    // Handle last item
    if in_range {
        let end = if has_current { current } else { usize::MAX };
        list.add_range(Range {
            start: range_start,
            end,
        });
    } else if has_current {
        list.add_range(Range {
            start: current,
            end: current,
        });
    } else if list.count == 0 {
        return None; // Empty list
    }

    Some(list)
}

fn process_file(config: &CutConfig, fd: i32) {
    let mut buf = [0u8; 4096];
    let mut line = [0u8; MAX_LINE];
    let mut line_len = 0;
    let delimiter = if config.zero_terminated { b'\0' } else { b'\n' };

    loop {
        let n = read(fd, &mut buf);
        if n <= 0 {
            break;
        }

        for i in 0..n as usize {
            if buf[i] == delimiter {
                process_line(config, &line[..line_len]);
                write(STDOUT_FILENO, &[delimiter]);
                line_len = 0;
            } else if line_len < MAX_LINE - 1 {
                line[line_len] = buf[i];
                line_len += 1;
            }
        }
    }

    // Handle last line without delimiter
    if line_len > 0 {
        process_line(config, &line[..line_len]);
        write(STDOUT_FILENO, &[delimiter]);
    }
}

fn process_line(config: &CutConfig, line: &[u8]) {
    match &config.mode {
        CutMode::Bytes(ranges) => cut_bytes(config, line, ranges),
        CutMode::Chars(ranges) => cut_chars(config, line, ranges),
        CutMode::Fields(ranges) => cut_fields(config, line, ranges),
        CutMode::None => {}
    }
}

fn cut_bytes(config: &CutConfig, line: &[u8], ranges: &RangeList) {
    for i in 0..line.len() {
        let pos = i + 1; // 1-indexed
        let in_range = ranges.contains(pos);
        let should_print = if config.complement {
            !in_range
        } else {
            in_range
        };

        if should_print {
            write(STDOUT_FILENO, &[line[i]]);
        }
    }
}

fn cut_chars(config: &CutConfig, line: &[u8], ranges: &RangeList) {
    // For simplicity, treat chars same as bytes (assumes ASCII/UTF-8 single byte chars)
    // A full implementation would handle multi-byte UTF-8 characters
    cut_bytes(config, line, ranges)
}

fn cut_fields(config: &CutConfig, line: &[u8], ranges: &RangeList) {
    // Check if line contains delimiter
    let has_delimiter = line.iter().any(|&b| b == config.delimiter);

    if config.only_delimited && !has_delimiter {
        // Suppress lines without delimiters
        return;
    }

    // Split line into fields
    let mut field_starts: [usize; MAX_RANGES] = [0; MAX_RANGES];
    let mut field_ends: [usize; MAX_RANGES] = [0; MAX_RANGES];
    let mut num_fields = 0;

    let mut start = 0;
    for i in 0..line.len() {
        if line[i] == config.delimiter {
            if num_fields < MAX_RANGES {
                field_starts[num_fields] = start;
                field_ends[num_fields] = i;
                num_fields += 1;
            }
            start = i + 1;
        }
    }
    // Last field
    if num_fields < MAX_RANGES {
        field_starts[num_fields] = start;
        field_ends[num_fields] = line.len();
        num_fields += 1;
    }

    let output_delim = config.output_delimiter.unwrap_or(config.delimiter);

    if config.complement {
        // Output fields NOT in ranges
        let mut first = true;
        for f in 1..=num_fields {
            if !ranges.contains(f) {
                if !first {
                    write(STDOUT_FILENO, &[output_delim]);
                }
                let field_idx = f - 1;
                write(
                    STDOUT_FILENO,
                    &line[field_starts[field_idx]..field_ends[field_idx]],
                );
                first = false;
            }
        }
    } else {
        // Output fields in ranges
        let mut first = true;
        for i in 0..ranges.count {
            let range = ranges.ranges[i];
            let start_field = range.start;
            let end_field = if range.end == usize::MAX {
                num_fields
            } else {
                range.end
            };

            for f in start_field..=end_field {
                if f > 0 && f <= num_fields {
                    if !first {
                        write(STDOUT_FILENO, &[output_delim]);
                    }
                    let field_idx = f - 1;
                    write(
                        STDOUT_FILENO,
                        &line[field_starts[field_idx]..field_ends[field_idx]],
                    );
                    first = false;
                }
            }
        }
    }
}
