//! grep - search for patterns in files
//!
//! Enhanced grep implementation with:
//! - Pattern matching (literal strings)
//! - Case-insensitive search (-i)
//! - Invert match (-v)
//! - Line numbers (-n)
//! - Count matches (-c)
//! - Files-only output (-l, -L)
//! - Suppress/force filename (-h, -H)
//! - Quiet mode (-q)
//! - Max count (-m)
//! - Context lines (-A, -B, -C)

#![no_std]
#![no_main]

use libc::*;

const MAX_LINE: usize = 4096;
const MAX_CONTEXT: usize = 100;

struct GrepConfig {
    pattern: [u8; 256],
    pattern_len: usize,
    ignore_case: bool,
    invert: bool,
    line_numbers: bool,
    count_only: bool,
    files_with_matches: bool,
    files_without_matches: bool,
    suppress_filename: bool,
    force_filename: bool,
    quiet: bool,
    max_count: u32,
    after_context: usize,
    before_context: usize,
}

impl GrepConfig {
    fn new() -> Self {
        GrepConfig {
            pattern: [0; 256],
            pattern_len: 0,
            ignore_case: false,
            invert: false,
            line_numbers: false,
            count_only: false,
            files_with_matches: false,
            files_without_matches: false,
            suppress_filename: false,
            force_filename: false,
            quiet: false,
            max_count: u32::MAX,
            after_context: 0,
            before_context: 0,
        }
    }

    fn set_pattern(&mut self, s: &str) {
        let bytes = s.as_bytes();
        let len = if bytes.len() > 255 { 255 } else { bytes.len() };
        self.pattern[..len].copy_from_slice(&bytes[..len]);
        self.pattern_len = len;
    }

    fn pattern_str(&self) -> &str {
        core::str::from_utf8(&self.pattern[..self.pattern_len]).unwrap_or("")
    }
}

fn parse_u32(s: &str) -> Option<u32> {
    let mut val = 0u32;
    for ch in s.bytes() {
        if ch >= b'0' && ch <= b'9' {
            val = val.checked_mul(10)?;
            val = val.checked_add((ch - b'0') as u32)?;
        } else {
            return None;
        }
    }
    Some(val)
}

fn parse_u32_from_ptr(ptr: *const u8) -> u32 {
    let mut val = 0u32;
    let mut i = 0;
    loop {
        let c = unsafe { *ptr.add(i) };
        if c == 0 || c < b'0' || c > b'9' {
            break;
        }
        val = val.saturating_mul(10).saturating_add((c - b'0') as u32);
        i += 1;
    }
    val
}

#[unsafe(no_mangle)]
fn main(argc: i32, argv: *const *const u8) -> i32 {
    if argc < 2 {
        eprintlns("usage: grep [options] pattern [file...]");
        eprintlns("Options:");
        eprintlns("  -i        Ignore case");
        eprintlns("  -v        Invert match (select non-matching lines)");
        eprintlns("  -n        Show line numbers");
        eprintlns("  -c        Count matches only");
        eprintlns("  -l        Show only filenames with matches");
        eprintlns("  -L        Show only filenames without matches");
        eprintlns("  -h        Suppress filename prefix");
        eprintlns("  -H        Always show filename prefix");
        eprintlns("  -q        Quiet mode (exit status only)");
        eprintlns("  -m NUM    Stop after NUM matches");
        eprintlns("  -A NUM    Print NUM lines after match");
        eprintlns("  -B NUM    Print NUM lines before match");
        eprintlns("  -C NUM    Print NUM lines before and after match");
        return 2;
    }

    let mut config = GrepConfig::new();
    let mut arg_idx = 1;

    // Parse flags
    while arg_idx < argc {
        let arg_ptr = unsafe { *argv.add(arg_idx as usize) };
        if arg_ptr.is_null() {
            arg_idx += 1;
            continue;
        }

        let first = unsafe { *arg_ptr };
        if first != b'-' {
            break;
        }

        // Convert to string for easier parsing
        let mut arg_buf = [0u8; 256];
        let mut arg_len = 0;
        while arg_len < 255 {
            let c = unsafe { *arg_ptr.add(arg_len) };
            if c == 0 { break; }
            arg_buf[arg_len] = c;
            arg_len += 1;
        }
        let arg = core::str::from_utf8(&arg_buf[..arg_len]).unwrap_or("");

        if arg.len() > 1 {
            let rest = &arg[1..];

            // Check for options with arguments
            if rest.starts_with('m') {
                let val = if rest.len() > 1 {
                    parse_u32(&rest[1..]).unwrap_or(u32::MAX)
                } else if arg_idx + 1 < argc {
                    arg_idx += 1;
                    let val_ptr = unsafe { *argv.add(arg_idx as usize) };
                    parse_u32_from_ptr(val_ptr)
                } else {
                    eprintlns("grep: option requires an argument -- 'm'");
                    return 2;
                };
                config.max_count = val;
                arg_idx += 1;
                continue;
            } else if rest.starts_with('A') {
                let val = if rest.len() > 1 {
                    parse_u32(&rest[1..]).unwrap_or(0)
                } else if arg_idx + 1 < argc {
                    arg_idx += 1;
                    let val_ptr = unsafe { *argv.add(arg_idx as usize) };
                    parse_u32_from_ptr(val_ptr)
                } else {
                    eprintlns("grep: option requires an argument -- 'A'");
                    return 2;
                };
                config.after_context = val as usize;
                if config.after_context > MAX_CONTEXT {
                    config.after_context = MAX_CONTEXT;
                }
                arg_idx += 1;
                continue;
            } else if rest.starts_with('B') {
                let val = if rest.len() > 1 {
                    parse_u32(&rest[1..]).unwrap_or(0)
                } else if arg_idx + 1 < argc {
                    arg_idx += 1;
                    let val_ptr = unsafe { *argv.add(arg_idx as usize) };
                    parse_u32_from_ptr(val_ptr)
                } else {
                    eprintlns("grep: option requires an argument -- 'B'");
                    return 2;
                };
                config.before_context = val as usize;
                if config.before_context > MAX_CONTEXT {
                    config.before_context = MAX_CONTEXT;
                }
                arg_idx += 1;
                continue;
            } else if rest.starts_with('C') {
                let val = if rest.len() > 1 {
                    parse_u32(&rest[1..]).unwrap_or(0)
                } else if arg_idx + 1 < argc {
                    arg_idx += 1;
                    let val_ptr = unsafe { *argv.add(arg_idx as usize) };
                    parse_u32_from_ptr(val_ptr)
                } else {
                    eprintlns("grep: option requires an argument -- 'C'");
                    return 2;
                };
                let ctx = val as usize;
                let ctx = if ctx > MAX_CONTEXT { MAX_CONTEXT } else { ctx };
                config.after_context = ctx;
                config.before_context = ctx;
                arg_idx += 1;
                continue;
            }

            // Parse character flags
            for c in rest.bytes() {
                match c {
                    b'i' => config.ignore_case = true,
                    b'v' => config.invert = true,
                    b'n' => config.line_numbers = true,
                    b'c' => config.count_only = true,
                    b'l' => config.files_with_matches = true,
                    b'L' => config.files_without_matches = true,
                    b'h' => config.suppress_filename = true,
                    b'H' => config.force_filename = true,
                    b'q' => config.quiet = true,
                    _ => {
                        eprints("grep: unknown option: -");
                        putchar(c);
                        printlns("");
                        return 2;
                    }
                }
            }
            arg_idx += 1;
        } else {
            break;
        }
    }

    if arg_idx >= argc {
        eprintlns("grep: no pattern specified");
        return 2;
    }

    let pattern_ptr = unsafe { *argv.add(arg_idx as usize) };
    let mut pattern_buf = [0u8; 256];
    let mut pattern_len = 0;
    while pattern_len < 255 {
        let c = unsafe { *pattern_ptr.add(pattern_len) };
        if c == 0 { break; }
        pattern_buf[pattern_len] = c;
        pattern_len += 1;
    }
    let pattern = core::str::from_utf8(&pattern_buf[..pattern_len]).unwrap_or("");
    config.set_pattern(pattern);
    arg_idx += 1;

    let mut found_any = false;

    // If no files, read from stdin
    if arg_idx >= argc {
        found_any = grep_fd(STDIN_FILENO, "", &config, false);
    } else {
        let multiple = (argc - arg_idx) > 1;
        let show_filename = if config.force_filename {
            true
        } else if config.suppress_filename {
            false
        } else {
            multiple
        };

        for i in arg_idx..argc {
            let path_ptr = unsafe { *argv.add(i as usize) };
            let mut path_buf = [0u8; 256];
            let mut path_len = 0;
            while path_len < 255 {
                let c = unsafe { *path_ptr.add(path_len) };
                if c == 0 { break; }
                path_buf[path_len] = c;
                path_len += 1;
            }
            let path = core::str::from_utf8(&path_buf[..path_len]).unwrap_or("");
            let fd = open2(path, O_RDONLY);
            if fd < 0 {
                if !config.quiet {
                    eprints("grep: ");
                    prints(path);
                    eprintlns(": No such file or directory");
                }
                continue;
            }

            if grep_fd(fd, path, &config, show_filename) {
                found_any = true;
            }
            close(fd);
        }
    }

    if found_any { 0 } else { 1 }
}

fn grep_fd(fd: i32, filename: &str, config: &GrepConfig, show_filename: bool) -> bool {
    let mut buf = [0u8; 4096];
    let mut line = [0u8; MAX_LINE];
    let mut line_len = 0;
    let mut line_num = 0u64;
    let mut match_count = 0u32;
    let mut found = false;

    // Circular buffer for before-context
    let mut context_buf: [[u8; MAX_LINE]; MAX_CONTEXT] = [[0; MAX_LINE]; MAX_CONTEXT];
    let mut context_lens: [usize; MAX_CONTEXT] = [0; MAX_CONTEXT];
    let mut context_nums: [u64; MAX_CONTEXT] = [0; MAX_CONTEXT];
    let mut context_idx = 0;
    let mut after_lines = 0usize;

    loop {
        let n = read(fd, &mut buf);
        if n <= 0 {
            break;
        }

        for i in 0..n as usize {
            if buf[i] == b'\n' {
                line_num += 1;
                let matches = contains(&line[..line_len], config.pattern_str(), config.ignore_case);
                let should_match = if config.invert { !matches } else { matches };

                if should_match {
                    match_count += 1;
                    found = true;

                    // Check max count
                    if match_count > config.max_count {
                        return found;
                    }

                    // Files-only modes
                    if config.files_with_matches {
                        if !config.quiet {
                            printlns(filename);
                        }
                        return true;
                    }

                    // Print before context
                    if !config.count_only && !config.quiet && config.before_context > 0 {
                        let start = if context_idx < config.before_context {
                            MAX_CONTEXT - (config.before_context - context_idx)
                        } else {
                            context_idx - config.before_context
                        };

                        for j in 0..config.before_context {
                            let idx = (start + j) % MAX_CONTEXT;
                            if context_lens[idx] > 0 && context_nums[idx] < line_num {
                                if show_filename {
                                    prints(filename);
                                    prints("-");
                                }
                                print_u64(context_nums[idx]);
                                prints("-");
                                for k in 0..context_lens[idx] {
                                    putchar(context_buf[idx][k]);
                                }
                                putchar(b'\n');
                            }
                        }
                    }

                    // Print matching line
                    if !config.count_only && !config.quiet {
                        if show_filename {
                            prints(filename);
                            prints(":");
                        }
                        if config.line_numbers {
                            print_u64(line_num);
                            prints(":");
                        }
                        for j in 0..line_len {
                            putchar(line[j]);
                        }
                        putchar(b'\n');
                    }

                    after_lines = config.after_context;
                } else if after_lines > 0 {
                    // Print after context
                    if !config.count_only && !config.quiet {
                        if show_filename {
                            prints(filename);
                            prints("-");
                        }
                        if config.line_numbers {
                            print_u64(line_num);
                            prints("-");
                        }
                        for j in 0..line_len {
                            putchar(line[j]);
                        }
                        putchar(b'\n');
                    }
                    after_lines -= 1;
                }

                // Store in context buffer for before-context
                if config.before_context > 0 && line_len < MAX_LINE {
                    context_lens[context_idx] = line_len;
                    context_nums[context_idx] = line_num;
                    context_buf[context_idx][..line_len].copy_from_slice(&line[..line_len]);
                    context_idx = (context_idx + 1) % MAX_CONTEXT;
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
        let matches = contains(&line[..line_len], config.pattern_str(), config.ignore_case);
        let should_match = if config.invert { !matches } else { matches };

        if should_match {
            match_count += 1;
            found = true;

            if config.files_with_matches && !config.quiet {
                printlns(filename);
                return true;
            }

            if !config.count_only && !config.quiet {
                if show_filename {
                    prints(filename);
                    prints(":");
                }
                if config.line_numbers {
                    print_u64(line_num);
                    prints(":");
                }
                for j in 0..line_len {
                    putchar(line[j]);
                }
                putchar(b'\n');
            }
        }
    }

    // Handle files-without-matches
    if config.files_without_matches && !found && !config.quiet {
        printlns(filename);
    }

    // Print count if requested
    if config.count_only && !config.quiet {
        if show_filename {
            prints(filename);
            prints(":");
        }
        print_u64(match_count as u64);
        putchar(b'\n');
    }

    found
}

fn contains(haystack: &[u8], needle: &str, ignore_case: bool) -> bool {
    let needle_bytes = needle.as_bytes();
    if needle_bytes.is_empty() {
        return true;
    }
    if haystack.len() < needle_bytes.len() {
        return false;
    }

    for i in 0..=(haystack.len() - needle_bytes.len()) {
        let mut matches = true;
        for j in 0..needle_bytes.len() {
            let h = if ignore_case { to_lower(haystack[i + j]) } else { haystack[i + j] };
            let n = if ignore_case { to_lower(needle_bytes[j]) } else { needle_bytes[j] };
            if h != n {
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
    if c >= b'A' && c <= b'Z' {
        c + (b'a' - b'A')
    } else {
        c
    }
}
