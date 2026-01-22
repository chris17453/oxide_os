//! sed - stream editor
//!
//! A functional sed implementation with:
//! - Substitution command (s/pattern/replacement/flags)
//! - Line deletion (d)
//! - Print command (p)
//! - Line addressing (number, $, ranges)
//! - Multiple commands (-e)
//! - Quiet mode (-n)
//! - Multiple input files
//!
//! Limitations:
//! - No regex (literal string matching only)
//! - No in-place editing (-i) - would need rename syscall
//! - No advanced addressing patterns

#![no_std]
#![no_main]

use libc::*;

const MAX_LINE: usize = 4096;
const MAX_PATTERN: usize = 256;
const MAX_REPLACEMENT: usize = 256;
const MAX_COMMANDS: usize = 16;

#[derive(Clone, Copy)]
enum CommandType {
    Substitute, // s/pattern/replacement/flags
    Delete,     // d
    Print,      // p
}

#[derive(Clone, Copy)]
struct Command {
    cmd_type: CommandType,
    pattern: [u8; MAX_PATTERN],
    pattern_len: usize,
    replacement: [u8; MAX_REPLACEMENT],
    replacement_len: usize,
    global: bool,      // g flag for substitute
    print: bool,       // p flag for substitute
    ignore_case: bool, // i flag for substitute
    start_line: u64,   // 0 = all lines
    end_line: u64,     // 0 = single line
}

impl Command {
    fn new() -> Self {
        Command {
            cmd_type: CommandType::Substitute,
            pattern: [0; MAX_PATTERN],
            pattern_len: 0,
            replacement: [0; MAX_REPLACEMENT],
            replacement_len: 0,
            global: false,
            print: false,
            ignore_case: false,
            start_line: 0,
            end_line: 0,
        }
    }
}

struct SedConfig {
    commands: [Command; MAX_COMMANDS],
    command_count: usize,
    quiet: bool,
    files: [[u8; 256]; 16],
    file_count: usize,
}

impl SedConfig {
    fn new() -> Self {
        SedConfig {
            commands: [Command::new(); MAX_COMMANDS],
            command_count: 0,
            quiet: false,
            files: [[0; 256]; 16],
            file_count: 0,
        }
    }

    fn add_file(&mut self, path: &str) {
        if self.file_count < 16 {
            let bytes = path.as_bytes();
            let len = if bytes.len() > 255 { 255 } else { bytes.len() };
            self.files[self.file_count][..len].copy_from_slice(&bytes[..len]);
            self.file_count += 1;
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

fn parse_number(s: &str) -> Option<u64> {
    let mut val = 0u64;
    for c in s.bytes() {
        if c >= b'0' && c <= b'9' {
            val = val.saturating_mul(10).saturating_add((c - b'0') as u64);
        } else {
            return None;
        }
    }
    Some(val)
}

/// Parse a sed command string like "s/pattern/replacement/flags" or "3,5d"
fn parse_command(cmd_str: &str) -> Option<Command> {
    let mut cmd = Command::new();
    let bytes = cmd_str.as_bytes();

    if bytes.is_empty() {
        return None;
    }

    // Check for line addressing (e.g., "3s", "1,5d", "$d")
    let mut pos = 0;
    let mut start_line = 0u64;
    let mut end_line = 0u64;

    // Parse optional start line number
    while pos < bytes.len() && bytes[pos] >= b'0' && bytes[pos] <= b'9' {
        start_line = start_line * 10 + (bytes[pos] - b'0') as u64;
        pos += 1;
    }

    // Check for range (comma)
    if pos < bytes.len() && bytes[pos] == b',' {
        pos += 1;
        // Parse end line (could be number or $)
        if pos < bytes.len() && bytes[pos] == b'$' {
            end_line = u64::MAX; // $ means last line
            pos += 1;
        } else {
            while pos < bytes.len() && bytes[pos] >= b'0' && bytes[pos] <= b'9' {
                end_line = end_line * 10 + (bytes[pos] - b'0') as u64;
                pos += 1;
            }
        }
    }

    if pos >= bytes.len() {
        return None;
    }

    // Parse command type
    match bytes[pos] {
        b's' => {
            // Parse s/pattern/replacement/flags
            pos += 1;
            if pos >= bytes.len() || bytes[pos] != b'/' {
                eprintlns("sed: s command requires /pattern/replacement/ format");
                return None;
            }
            pos += 1;

            // Extract pattern
            let pattern_start = pos;
            while pos < bytes.len() && bytes[pos] != b'/' {
                pos += 1;
            }
            if pos >= bytes.len() {
                eprintlns("sed: unterminated s command");
                return None;
            }
            cmd.pattern_len = pos - pattern_start;
            if cmd.pattern_len > MAX_PATTERN - 1 {
                cmd.pattern_len = MAX_PATTERN - 1;
            }
            cmd.pattern[..cmd.pattern_len]
                .copy_from_slice(&bytes[pattern_start..pattern_start + cmd.pattern_len]);
            pos += 1; // skip /

            // Extract replacement
            let replacement_start = pos;
            while pos < bytes.len() && bytes[pos] != b'/' {
                pos += 1;
            }
            cmd.replacement_len = pos - replacement_start;
            if cmd.replacement_len > MAX_REPLACEMENT - 1 {
                cmd.replacement_len = MAX_REPLACEMENT - 1;
            }
            if cmd.replacement_len > 0 {
                cmd.replacement[..cmd.replacement_len].copy_from_slice(
                    &bytes[replacement_start..replacement_start + cmd.replacement_len],
                );
            }
            if pos < bytes.len() {
                pos += 1; // skip /
            }

            // Parse flags
            while pos < bytes.len() {
                match bytes[pos] {
                    b'g' => cmd.global = true,
                    b'p' => cmd.print = true,
                    b'i' => cmd.ignore_case = true,
                    _ => {}
                }
                pos += 1;
            }

            cmd.cmd_type = CommandType::Substitute;
        }
        b'd' => {
            cmd.cmd_type = CommandType::Delete;
        }
        b'p' => {
            cmd.cmd_type = CommandType::Print;
        }
        _ => {
            eprints("sed: unknown command: ");
            putchar(bytes[pos]);
            printlns("");
            return None;
        }
    }

    cmd.start_line = start_line;
    cmd.end_line = end_line;
    Some(cmd)
}

fn matches_line(line_num: u64, start: u64, end: u64, is_last: bool) -> bool {
    if start == 0 && end == 0 {
        return true; // No addressing = all lines
    }
    if end == 0 {
        // Single line addressing
        return line_num == start;
    }
    if end == u64::MAX {
        // Range to end
        return is_last || line_num >= start;
    }
    // Normal range
    line_num >= start && line_num <= end
}

fn to_lower(c: u8) -> u8 {
    if c >= b'A' && c <= b'Z' {
        c + (b'a' - b'A')
    } else {
        c
    }
}

fn find_pattern(line: &[u8], pattern: &[u8], ignore_case: bool) -> Option<usize> {
    if pattern.is_empty() {
        return Some(0);
    }
    if line.len() < pattern.len() {
        return None;
    }

    for i in 0..=(line.len() - pattern.len()) {
        let mut matches = true;
        for j in 0..pattern.len() {
            let l = if ignore_case {
                to_lower(line[i + j])
            } else {
                line[i + j]
            };
            let p = if ignore_case {
                to_lower(pattern[j])
            } else {
                pattern[j]
            };
            if l != p {
                matches = false;
                break;
            }
        }
        if matches {
            return Some(i);
        }
    }
    None
}

fn substitute_line(
    line: &[u8],
    pattern: &[u8],
    replacement: &[u8],
    global: bool,
    ignore_case: bool,
    output: &mut [u8],
) -> usize {
    let mut out_len = 0;
    let mut pos = 0;

    while pos < line.len() && out_len < MAX_LINE - 1 {
        if let Some(match_pos) = find_pattern(&line[pos..], pattern, ignore_case) {
            let actual_pos = pos + match_pos;

            // Copy everything before match
            for i in pos..actual_pos {
                if out_len < MAX_LINE - 1 {
                    output[out_len] = line[i];
                    out_len += 1;
                }
            }

            // Copy replacement
            for i in 0..replacement.len() {
                if out_len < MAX_LINE - 1 {
                    output[out_len] = replacement[i];
                    out_len += 1;
                }
            }

            pos = actual_pos + pattern.len();

            if !global {
                // Copy rest of line if not global
                for i in pos..line.len() {
                    if out_len < MAX_LINE - 1 {
                        output[out_len] = line[i];
                        out_len += 1;
                    }
                }
                break;
            }
        } else {
            // No match found, copy rest
            for i in pos..line.len() {
                if out_len < MAX_LINE - 1 {
                    output[out_len] = line[i];
                    out_len += 1;
                }
            }
            break;
        }
    }

    out_len
}

fn process_stream(fd: i32, config: &SedConfig) -> i32 {
    let mut buf = [0u8; 4096];
    let mut line = [0u8; MAX_LINE];
    let mut line_len = 0;
    let mut line_num = 0u64;

    loop {
        let n = read(fd, &mut buf);
        if n <= 0 {
            break;
        }

        for i in 0..n as usize {
            if buf[i] == b'\n' {
                line_num += 1;

                let mut output_line = [0u8; MAX_LINE];
                let mut temp_line = [0u8; MAX_LINE];
                let mut output_len = line_len;
                output_line[..line_len].copy_from_slice(&line[..line_len]);

                let mut should_print = !config.quiet;
                let mut deleted = false;

                // Apply all commands to this line
                for c in 0..config.command_count {
                    let cmd = &config.commands[c];

                    if !matches_line(line_num, cmd.start_line, cmd.end_line, false) {
                        continue;
                    }

                    match cmd.cmd_type {
                        CommandType::Substitute => {
                            output_len = substitute_line(
                                &output_line[..output_len],
                                &cmd.pattern[..cmd.pattern_len],
                                &cmd.replacement[..cmd.replacement_len],
                                cmd.global,
                                cmd.ignore_case,
                                &mut temp_line,
                            );
                            output_line[..output_len].copy_from_slice(&temp_line[..output_len]);
                            if cmd.print {
                                should_print = true;
                            }
                        }
                        CommandType::Delete => {
                            deleted = true;
                            should_print = false;
                        }
                        CommandType::Print => {
                            should_print = true;
                        }
                    }
                }

                // Output the line if not deleted
                if !deleted && should_print {
                    for j in 0..output_len {
                        putchar(output_line[j]);
                    }
                    putchar(b'\n');
                }

                line_len = 0;
            } else if line_len < MAX_LINE - 1 {
                line[line_len] = buf[i];
                line_len += 1;
            }
        }
    }

    // Handle last line without newline
    if line_len > 0 {
        line_num += 1;
        let mut output_line = [0u8; MAX_LINE];
        let mut temp_line = [0u8; MAX_LINE];
        let mut output_len = line_len;
        output_line[..line_len].copy_from_slice(&line[..line_len]);

        let mut should_print = !config.quiet;
        let mut deleted = false;

        for c in 0..config.command_count {
            let cmd = &config.commands[c];
            if !matches_line(line_num, cmd.start_line, cmd.end_line, true) {
                continue;
            }

            match cmd.cmd_type {
                CommandType::Substitute => {
                    output_len = substitute_line(
                        &output_line[..output_len],
                        &cmd.pattern[..cmd.pattern_len],
                        &cmd.replacement[..cmd.replacement_len],
                        cmd.global,
                        cmd.ignore_case,
                        &mut temp_line,
                    );
                    output_line[..output_len].copy_from_slice(&temp_line[..output_len]);
                    if cmd.print {
                        should_print = true;
                    }
                }
                CommandType::Delete => {
                    deleted = true;
                    should_print = false;
                }
                CommandType::Print => {
                    should_print = true;
                }
            }
        }

        if !deleted && should_print {
            for j in 0..output_len {
                putchar(output_line[j]);
            }
            putchar(b'\n');
        }
    }

    0
}

#[unsafe(no_mangle)]
fn main(argc: i32, argv: *const *const u8) -> i32 {
    if argc < 2 {
        eprintlns("usage: sed [-n] [-e command] [command] [file...]");
        eprintlns("Commands:");
        eprintlns("  s/pattern/replacement/[flags]  Substitute");
        eprintlns("    Flags: g (global), p (print), i (ignore case)");
        eprintlns("  [addr]d                         Delete line(s)");
        eprintlns("  [addr]p                         Print line(s)");
        eprintlns("Addresses:");
        eprintlns("  N        Line number N");
        eprintlns("  N,M      Lines N through M");
        eprintlns("  N,$      Lines N through end");
        return 1;
    }

    let mut config = SedConfig::new();
    let mut arg_idx = 1;

    // Parse options and commands
    while arg_idx < argc {
        let arg_ptr = unsafe { *argv.add(arg_idx as usize) };
        let arg = cstr_to_str(arg_ptr);

        if arg.starts_with('-') && arg.len() > 1 {
            if arg == "-n" {
                config.quiet = true;
                arg_idx += 1;
            } else if arg == "-e" {
                // Next arg is a command
                arg_idx += 1;
                if arg_idx >= argc {
                    eprintlns("sed: -e requires an argument");
                    return 1;
                }
                let cmd_ptr = unsafe { *argv.add(arg_idx as usize) };
                let cmd_str = cstr_to_str(cmd_ptr);
                if let Some(cmd) = parse_command(cmd_str) {
                    if config.command_count < MAX_COMMANDS {
                        config.commands[config.command_count] = cmd;
                        config.command_count += 1;
                    }
                } else {
                    return 1;
                }
                arg_idx += 1;
            } else {
                eprints("sed: unknown option: ");
                printlns(arg);
                return 1;
            }
        } else {
            // First non-option is the command (if we haven't parsed any yet)
            if config.command_count == 0 {
                if let Some(cmd) = parse_command(arg) {
                    config.commands[config.command_count] = cmd;
                    config.command_count += 1;
                } else {
                    return 1;
                }
                arg_idx += 1;
            } else {
                // Rest are files
                break;
            }
        }
    }

    if config.command_count == 0 {
        eprintlns("sed: no command specified");
        return 1;
    }

    // Collect file arguments
    while arg_idx < argc {
        let path_ptr = unsafe { *argv.add(arg_idx as usize) };
        let path = cstr_to_str(path_ptr);
        config.add_file(path);
        arg_idx += 1;
    }

    // Process files or stdin
    if config.file_count == 0 {
        process_stream(STDIN_FILENO, &config)
    } else {
        let mut ret = 0;
        for i in 0..config.file_count {
            let path_len = config.files[i].iter().position(|&c| c == 0).unwrap_or(256);
            let path = core::str::from_utf8(&config.files[i][..path_len]).unwrap_or("");

            let fd = open2(path, O_RDONLY);
            if fd < 0 {
                eprints("sed: cannot open '");
                prints(path);
                eprintlns("'");
                ret = 1;
                continue;
            }

            if process_stream(fd, &config) != 0 {
                ret = 1;
            }
            close(fd);
        }
        ret
    }
}
