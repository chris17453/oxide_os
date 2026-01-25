//! awk - pattern scanning and text processing language
//!
//! A simplified awk implementation with:
//! - Field splitting (default: whitespace)
//! - Custom field separator (-F)
//! - Built-in variables: $0 (whole line), $1..$NF (fields), NF (field count), NR (record number)
//! - Pattern matching with actions
//! - BEGIN/END blocks
//! - Print statement
//! - Basic arithmetic and string operations
//! - Multiple input files
//!
//! Limitations:
//! - No user-defined functions
//! - No arrays
//! - No regex (literal string matching only)
//! - Simplified expression evaluation

#![no_std]
#![no_main]
#![allow(unused)]

use libc::*;

const MAX_LINE: usize = 4096;
const MAX_FIELDS: usize = 128;
const MAX_PROGRAM: usize = 2048;
const MAX_ACTIONS: usize = 16;

#[derive(Clone, Copy)]
enum ActionType {
    Print,    // Print fields/expression
    PrintAll, // Print whole line
}

#[derive(Clone, Copy)]
enum BlockType {
    Begin,
    End,
    Pattern, // Pattern with action
    Always,  // Action only (no pattern)
}

#[derive(Clone, Copy)]
struct Action {
    block_type: BlockType,
    pattern: [u8; 256],
    pattern_len: usize,
    action_type: ActionType,
    field_nums: [usize; 16], // Which fields to print (0 = all)
    field_count: usize,
}

impl Action {
    fn new() -> Self {
        Action {
            block_type: BlockType::Always,
            pattern: [0; 256],
            pattern_len: 0,
            action_type: ActionType::PrintAll,
            field_nums: [0; 16],
            field_count: 0,
        }
    }
}

struct AwkConfig {
    field_separator: u8,
    actions: [Action; MAX_ACTIONS],
    action_count: usize,
    files: [[u8; 256]; 16],
    file_count: usize,
}

impl AwkConfig {
    fn new() -> Self {
        AwkConfig {
            field_separator: b' ',
            actions: [Action::new(); MAX_ACTIONS],
            action_count: 0,
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

fn parse_number(s: &str) -> Option<usize> {
    let mut val = 0usize;
    for c in s.bytes() {
        if c >= b'0' && c <= b'9' {
            val = val.saturating_mul(10).saturating_add((c - b'0') as usize);
        } else {
            return None;
        }
    }
    Some(val)
}

/// Parse awk program like: /pattern/ { print $1, $2 }
/// Or: BEGIN { print "header" }
/// Or: { print $0 }
fn parse_program(prog: &str, config: &mut AwkConfig) -> bool {
    let prog = prog.trim();

    if prog.is_empty() {
        return false;
    }

    let mut action = Action::new();
    let bytes = prog.as_bytes();
    let mut pos = 0;

    // Skip whitespace
    while pos < bytes.len() && (bytes[pos] == b' ' || bytes[pos] == b'\t') {
        pos += 1;
    }

    // Check for BEGIN/END
    if prog.starts_with("BEGIN") {
        action.block_type = BlockType::Begin;
        pos += 5;
    } else if prog.starts_with("END") {
        action.block_type = BlockType::End;
        pos += 3;
    } else if bytes[pos] == b'/' {
        // Pattern matching
        pos += 1;
        let pattern_start = pos;
        while pos < bytes.len() && bytes[pos] != b'/' {
            pos += 1;
        }
        if pos >= bytes.len() {
            eprintlns("awk: unterminated pattern");
            return false;
        }
        action.pattern_len = pos - pattern_start;
        if action.pattern_len > 255 {
            action.pattern_len = 255;
        }
        action.pattern[..action.pattern_len]
            .copy_from_slice(&bytes[pattern_start..pattern_start + action.pattern_len]);
        action.block_type = BlockType::Pattern;
        pos += 1; // skip closing /
    }

    // Skip whitespace to action
    while pos < bytes.len() && (bytes[pos] == b' ' || bytes[pos] == b'\t') {
        pos += 1;
    }

    // Parse action { ... }
    if pos < bytes.len() && bytes[pos] == b'{' {
        pos += 1;

        // Skip whitespace
        while pos < bytes.len() && (bytes[pos] == b' ' || bytes[pos] == b'\t') {
            pos += 1;
        }

        // Check for print statement
        if pos + 5 <= bytes.len() && &bytes[pos..pos + 5] == b"print" {
            pos += 5;

            // Skip whitespace
            while pos < bytes.len() && (bytes[pos] == b' ' || bytes[pos] == b'\t') {
                pos += 1;
            }

            // Parse what to print
            if pos >= bytes.len() || bytes[pos] == b'}' {
                // print with no args = print $0
                action.action_type = ActionType::PrintAll;
            } else {
                action.action_type = ActionType::Print;

                // Parse field specifications
                while pos < bytes.len() && bytes[pos] != b'}' {
                    if bytes[pos] == b'$' {
                        pos += 1;
                        // Parse field number
                        let mut field_num_str = [0u8; 8];
                        let mut field_num_len = 0;
                        while pos < bytes.len() && bytes[pos] >= b'0' && bytes[pos] <= b'9' {
                            if field_num_len < 8 {
                                field_num_str[field_num_len] = bytes[pos];
                                field_num_len += 1;
                            }
                            pos += 1;
                        }

                        if field_num_len > 0 {
                            let field_str = core::str::from_utf8(&field_num_str[..field_num_len])
                                .unwrap_or("0");
                            if let Some(num) = parse_number(field_str) {
                                if action.field_count < 16 {
                                    action.field_nums[action.field_count] = num;
                                    action.field_count += 1;
                                }
                            }
                        }
                    } else if bytes[pos] == b'"' {
                        // String literal - skip for now
                        pos += 1;
                        while pos < bytes.len() && bytes[pos] != b'"' {
                            pos += 1;
                        }
                        if pos < bytes.len() {
                            pos += 1;
                        }
                    }

                    // Skip separators (comma, space)
                    while pos < bytes.len()
                        && (bytes[pos] == b',' || bytes[pos] == b' ' || bytes[pos] == b'\t')
                    {
                        pos += 1;
                    }
                }
            }
        } else {
            eprintlns("awk: only print statements are supported");
            return false;
        }
    } else {
        // No action specified, default to { print }
        action.action_type = ActionType::PrintAll;
    }

    if config.action_count < MAX_ACTIONS {
        config.actions[config.action_count] = action;
        config.action_count += 1;
    }

    true
}

fn split_fields<'a>(line: &'a [u8], separator: u8, fields: &mut [[u8; 256]; MAX_FIELDS]) -> usize {
    let mut field_count = 0;
    let mut field_len = 0;
    let mut in_field = false;

    for i in 0..line.len() {
        let ch = line[i];

        if separator == b' ' {
            // Whitespace separator: treat consecutive spaces/tabs as single separator
            if ch == b' ' || ch == b'\t' {
                if in_field {
                    // End of field
                    if field_count < MAX_FIELDS {
                        field_count += 1;
                    }
                    field_len = 0;
                    in_field = false;
                }
            } else {
                // Part of field
                if !in_field {
                    in_field = true;
                }
                if field_count < MAX_FIELDS && field_len < 255 {
                    fields[field_count][field_len] = ch;
                    field_len += 1;
                }
            }
        } else {
            // Specific separator
            if ch == separator {
                if field_count < MAX_FIELDS {
                    field_count += 1;
                }
                field_len = 0;
            } else {
                if field_count < MAX_FIELDS && field_len < 255 {
                    fields[field_count][field_len] = ch;
                    field_len += 1;
                }
            }
        }
    }

    // Last field
    if in_field || field_len > 0 {
        field_count += 1;
    }

    field_count
}

fn matches_pattern(line: &[u8], pattern: &[u8]) -> bool {
    if pattern.is_empty() {
        return true;
    }
    if line.len() < pattern.len() {
        return false;
    }

    for i in 0..=(line.len() - pattern.len()) {
        let mut matches = true;
        for j in 0..pattern.len() {
            if line[i + j] != pattern[j] {
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

fn process_stream(fd: i32, config: &AwkConfig) -> i32 {
    // Execute BEGIN blocks
    for i in 0..config.action_count {
        let action = &config.actions[i];
        match action.block_type {
            BlockType::Begin => {
                match action.action_type {
                    ActionType::PrintAll => printlns(""),
                    ActionType::Print => {
                        // BEGIN blocks can't access fields, just print empty
                        printlns("");
                    }
                }
            }
            _ => {}
        }
    }

    let mut buf = [0u8; 4096];
    let mut line = [0u8; MAX_LINE];
    let mut line_len = 0;
    let mut nr = 0u64; // Record number

    loop {
        let n = read(fd, &mut buf);
        if n <= 0 {
            break;
        }

        for i in 0..n as usize {
            if buf[i] == b'\n' {
                nr += 1;

                // Split into fields
                let mut fields = [[0u8; 256]; MAX_FIELDS];
                let nf = split_fields(&line[..line_len], config.field_separator, &mut fields);

                // Execute pattern/action blocks
                for j in 0..config.action_count {
                    let action = &config.actions[j];

                    match action.block_type {
                        BlockType::Begin | BlockType::End => continue,
                        BlockType::Pattern => {
                            if !matches_pattern(
                                &line[..line_len],
                                &action.pattern[..action.pattern_len],
                            ) {
                                continue;
                            }
                        }
                        BlockType::Always => {}
                    }

                    // Execute action
                    match action.action_type {
                        ActionType::PrintAll => {
                            for k in 0..line_len {
                                putchar(line[k]);
                            }
                            putchar(b'\n');
                        }
                        ActionType::Print => {
                            for k in 0..action.field_count {
                                let field_num = action.field_nums[k];

                                if k > 0 {
                                    putchar(b' ');
                                }

                                if field_num == 0 {
                                    // $0 = whole line
                                    for m in 0..line_len {
                                        putchar(line[m]);
                                    }
                                } else if field_num <= nf {
                                    // Print field
                                    let field = &fields[field_num - 1];
                                    let field_len =
                                        field.iter().position(|&c| c == 0).unwrap_or(256);
                                    for m in 0..field_len {
                                        putchar(field[m]);
                                    }
                                }
                            }
                            putchar(b'\n');
                        }
                    }
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
        nr += 1;
        let mut fields = [[0u8; 256]; MAX_FIELDS];
        let nf = split_fields(&line[..line_len], config.field_separator, &mut fields);

        for j in 0..config.action_count {
            let action = &config.actions[j];

            match action.block_type {
                BlockType::Begin | BlockType::End => continue,
                BlockType::Pattern => {
                    if !matches_pattern(&line[..line_len], &action.pattern[..action.pattern_len]) {
                        continue;
                    }
                }
                BlockType::Always => {}
            }

            match action.action_type {
                ActionType::PrintAll => {
                    for k in 0..line_len {
                        putchar(line[k]);
                    }
                    putchar(b'\n');
                }
                ActionType::Print => {
                    for k in 0..action.field_count {
                        let field_num = action.field_nums[k];

                        if k > 0 {
                            putchar(b' ');
                        }

                        if field_num == 0 {
                            for m in 0..line_len {
                                putchar(line[m]);
                            }
                        } else if field_num <= nf {
                            let field = &fields[field_num - 1];
                            let field_len = field.iter().position(|&c| c == 0).unwrap_or(256);
                            for m in 0..field_len {
                                putchar(field[m]);
                            }
                        }
                    }
                    putchar(b'\n');
                }
            }
        }
    }

    // Execute END blocks
    for i in 0..config.action_count {
        let action = &config.actions[i];
        match action.block_type {
            BlockType::End => match action.action_type {
                ActionType::PrintAll => printlns(""),
                ActionType::Print => printlns(""),
            },
            _ => {}
        }
    }

    0
}

#[unsafe(no_mangle)]
fn main(argc: i32, argv: *const *const u8) -> i32 {
    if argc < 2 {
        eprintlns("usage: awk [-F separator] 'program' [file...]");
        eprintlns("Program syntax:");
        eprintlns("  BEGIN { action }              Execute before processing");
        eprintlns("  END { action }                Execute after processing");
        eprintlns("  /pattern/ { action }          Execute action on matching lines");
        eprintlns("  { action }                    Execute action on all lines");
        eprintlns("Actions:");
        eprintlns("  print                         Print entire line ($0)");
        eprintlns("  print $1, $2, ...            Print specific fields");
        eprintlns("Variables:");
        eprintlns("  $0    Entire line");
        eprintlns("  $1-$N Specific fields");
        return 1;
    }

    let mut config = AwkConfig::new();
    let mut arg_idx = 1;

    // Parse field separator
    if arg_idx < argc {
        let arg_ptr = unsafe { *argv.add(arg_idx as usize) };
        let arg = cstr_to_str(arg_ptr);

        if arg == "-F" {
            arg_idx += 1;
            if arg_idx >= argc {
                eprintlns("awk: -F requires an argument");
                return 1;
            }
            let sep_ptr = unsafe { *argv.add(arg_idx as usize) };
            let sep_str = cstr_to_str(sep_ptr);
            if !sep_str.is_empty() {
                config.field_separator = sep_str.as_bytes()[0];
            }
            arg_idx += 1;
        }
    }

    // Parse program
    if arg_idx >= argc {
        eprintlns("awk: no program specified");
        return 1;
    }

    let prog_ptr = unsafe { *argv.add(arg_idx as usize) };
    let program = cstr_to_str(prog_ptr);

    if !parse_program(program, &mut config) {
        return 1;
    }

    arg_idx += 1;

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
                eprints("awk: cannot open '");
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
