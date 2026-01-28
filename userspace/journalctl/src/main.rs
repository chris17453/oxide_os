//! OXIDE Journal Log Viewer (journalctl)
//!
//! Reads and displays log entries from /var/log/journal with filtering.
//!
//! Usage:
//!   journalctl              Show all log entries
//!   journalctl -u <service> Filter by service tag
//!   journalctl -p <0-7>     Filter by priority (and higher)
//!   journalctl -n <count>   Show last N entries
//!   journalctl -f           Follow mode (poll for new entries)

#![no_std]
#![no_main]
#![allow(unused)]

extern crate alloc;

use libc::poll::{PollFd, events, poll};
use libc::*;

/// Path to the journal file
const JOURNAL_PATH: &str = "/var/log/journal";

/// Read buffer size
const BUF_SIZE: usize = 8192;

/// Maximum entries to buffer for -n mode
const MAX_TAIL_ENTRIES: usize = 4096;

/// Filter options
struct Filter {
    /// Service tag to filter by (empty = no filter)
    tag: [u8; 32],
    tag_len: usize,
    /// Maximum priority to show (7 = all, 0 = only EMERG)
    max_priority: u8,
    /// Number of last entries to show (0 = all)
    tail_count: usize,
    /// Follow mode
    follow: bool,
}

impl Filter {
    fn new() -> Self {
        Filter {
            tag: [0; 32],
            tag_len: 0,
            max_priority: 7,
            tail_count: 0,
            follow: false,
        }
    }

    fn set_tag(&mut self, tag: &str) {
        let len = tag.len().min(31);
        self.tag[..len].copy_from_slice(&tag.as_bytes()[..len]);
        self.tag_len = len;
    }

    fn tag_str(&self) -> &str {
        core::str::from_utf8(&self.tag[..self.tag_len]).unwrap_or("")
    }
}

/// Parse a log entry line and extract fields
/// Format: <priority>,<sec>.<ms>,<pid>,<tag>;<message>\n
struct LogEntry<'a> {
    priority: u8,
    timestamp: &'a [u8],
    pid: &'a [u8],
    tag: &'a [u8],
    message: &'a [u8],
}

fn parse_entry(line: &[u8]) -> Option<LogEntry<'_>> {
    // Strip trailing newline
    let line = if line.last() == Some(&b'\n') {
        &line[..line.len() - 1]
    } else {
        line
    };

    if line.is_empty() {
        return None;
    }

    // Find semicolon (separates header from message)
    let semi_pos = line.iter().position(|&b| b == b';')?;
    let header = &line[..semi_pos];
    let message = if semi_pos + 1 < line.len() {
        &line[semi_pos + 1..]
    } else {
        &[]
    };

    // Parse header: priority,timestamp,pid,tag
    let mut parts = header.splitn(4, |&b| b == b',');

    let prio_bytes = parts.next()?;
    let priority = if prio_bytes.len() == 1 && prio_bytes[0].is_ascii_digit() {
        prio_bytes[0] - b'0'
    } else {
        6 // Default to INFO
    };

    let timestamp = parts.next().unwrap_or(&[]);
    let pid = parts.next().unwrap_or(&[]);
    let tag = parts.next().unwrap_or(&[]);

    Some(LogEntry {
        priority,
        timestamp,
        pid,
        tag,
        message,
    })
}

/// Priority level names
const PRIORITY_NAMES: [&str; 8] = [
    "emerg", "alert", "crit", "err", "warn", "notice", "info", "debug",
];

/// Display a parsed log entry
/// Format: [  142.037] tag[pid]: message
fn display_entry(entry: &LogEntry) {
    // Timestamp
    prints("[");
    // Right-align timestamp in 10 chars
    let ts = core::str::from_utf8(entry.timestamp).unwrap_or("0.000");
    let padding = if ts.len() < 10 { 10 - ts.len() } else { 0 };
    for _ in 0..padding {
        prints(" ");
    }
    prints(ts);
    prints("] ");

    // Tag
    let tag = core::str::from_utf8(entry.tag).unwrap_or("?");
    prints(tag);

    // PID
    prints("[");
    let pid = core::str::from_utf8(entry.pid).unwrap_or("?");
    prints(pid);
    prints("]: ");

    // Message
    let msg = core::str::from_utf8(entry.message).unwrap_or("");
    prints(msg);
    prints("\n");
}

/// Check if an entry matches the filter
fn matches_filter(entry: &LogEntry, filter: &Filter) -> bool {
    // Priority filter
    if entry.priority > filter.max_priority {
        return false;
    }

    // Tag filter
    if filter.tag_len > 0 {
        let filter_tag = &filter.tag[..filter.tag_len];
        if entry.tag != filter_tag {
            return false;
        }
    }

    true
}

/// Helper to convert C string to str
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

/// Parse a decimal integer from a string
fn parse_usize(s: &str) -> Option<usize> {
    let mut val: usize = 0;
    let mut any = false;
    for b in s.bytes() {
        if b.is_ascii_digit() {
            val = val.checked_mul(10)?;
            val = val.checked_add((b - b'0') as usize)?;
            any = true;
        } else if any {
            break;
        }
    }
    if any { Some(val) } else { None }
}

/// Read the journal file and process entries
fn read_journal(filter: &Filter) {
    let fd = open2(JOURNAL_PATH, O_RDONLY);
    if fd < 0 {
        prints("journalctl: no journal entries found\n");
        return;
    }

    let mut buf = [0u8; BUF_SIZE];
    let mut line_buf = [0u8; 1024];
    let mut line_pos = 0;

    // For -n mode, we need to count total entries first then re-read
    if filter.tail_count > 0 {
        // First pass: count matching entries
        let mut total_entries = 0;
        loop {
            let n = read(fd, &mut buf);
            if n <= 0 {
                break;
            }
            for i in 0..n as usize {
                if buf[i] == b'\n' {
                    if line_pos > 0 {
                        if let Some(entry) = parse_entry(&line_buf[..line_pos]) {
                            if matches_filter(&entry, filter) {
                                total_entries += 1;
                            }
                        }
                        line_pos = 0;
                    }
                } else if line_pos < line_buf.len() {
                    line_buf[line_pos] = buf[i];
                    line_pos += 1;
                }
            }
        }
        // Handle last line without trailing newline
        if line_pos > 0 {
            if let Some(entry) = parse_entry(&line_buf[..line_pos]) {
                if matches_filter(&entry, filter) {
                    total_entries += 1;
                }
            }
        }

        // Second pass: skip entries until we're at (total - count)
        let skip = if total_entries > filter.tail_count {
            total_entries - filter.tail_count
        } else {
            0
        };

        // Seek back to start
        lseek(fd, 0, SEEK_SET);
        line_pos = 0;
        let mut entry_idx = 0;

        loop {
            let n = read(fd, &mut buf);
            if n <= 0 {
                break;
            }
            for i in 0..n as usize {
                if buf[i] == b'\n' {
                    if line_pos > 0 {
                        if let Some(entry) = parse_entry(&line_buf[..line_pos]) {
                            if matches_filter(&entry, filter) {
                                if entry_idx >= skip {
                                    display_entry(&entry);
                                }
                                entry_idx += 1;
                            }
                        }
                        line_pos = 0;
                    }
                } else if line_pos < line_buf.len() {
                    line_buf[line_pos] = buf[i];
                    line_pos += 1;
                }
            }
        }
        if line_pos > 0 {
            if let Some(entry) = parse_entry(&line_buf[..line_pos]) {
                if matches_filter(&entry, filter) && entry_idx >= skip {
                    display_entry(&entry);
                }
            }
        }
    } else {
        // Show all matching entries
        loop {
            let n = read(fd, &mut buf);
            if n <= 0 {
                break;
            }
            for i in 0..n as usize {
                if buf[i] == b'\n' {
                    if line_pos > 0 {
                        if let Some(entry) = parse_entry(&line_buf[..line_pos]) {
                            if matches_filter(&entry, filter) {
                                display_entry(&entry);
                            }
                        }
                        line_pos = 0;
                    }
                } else if line_pos < line_buf.len() {
                    line_buf[line_pos] = buf[i];
                    line_pos += 1;
                }
            }
        }
        // Handle last line without trailing newline
        if line_pos > 0 {
            if let Some(entry) = parse_entry(&line_buf[..line_pos]) {
                if matches_filter(&entry, filter) {
                    display_entry(&entry);
                }
            }
        }
    }

    close(fd);
}

/// Follow mode: poll journal file for new entries
fn follow_journal(filter: &Filter) {
    // First, display existing entries
    read_journal(filter);

    // Then open for polling
    let fd = open2(JOURNAL_PATH, O_RDONLY);
    if fd < 0 {
        return;
    }

    // Seek to end
    lseek(fd, 0, SEEK_END);

    let mut buf = [0u8; BUF_SIZE];
    let mut line_buf = [0u8; 1024];
    let mut line_pos = 0;

    loop {
        // Try to read new data
        let n = read(fd, &mut buf);
        if n > 0 {
            for i in 0..n as usize {
                if buf[i] == b'\n' {
                    if line_pos > 0 {
                        if let Some(entry) = parse_entry(&line_buf[..line_pos]) {
                            if matches_filter(&entry, filter) {
                                display_entry(&entry);
                            }
                        }
                        line_pos = 0;
                    }
                } else if line_pos < line_buf.len() {
                    line_buf[line_pos] = buf[i];
                    line_pos += 1;
                }
            }
        } else {
            // No data — sleep and retry
            libc::time::usleep(500_000); // 500ms
        }
    }
}

fn show_usage() {
    prints("Usage: journalctl [OPTIONS]\n");
    prints("\n");
    prints("Options:\n");
    prints("  -u <service>  Filter by service tag\n");
    prints("  -p <0-7>      Filter by priority (show this level and higher)\n");
    prints("  -n <count>    Show last N entries\n");
    prints("  -f            Follow mode (like tail -f)\n");
    prints("  -h, --help    Show this help\n");
    prints("\n");
    prints("Priority levels:\n");
    prints("  0=emerg  1=alert  2=crit  3=err\n");
    prints("  4=warn   5=notice 6=info  7=debug\n");
}

/// Main entry point
#[unsafe(no_mangle)]
fn main(argc: i32, argv: *const *const u8) -> i32 {
    let mut filter = Filter::new();

    // Parse arguments
    let mut i = 1;
    while i < argc as usize {
        let arg = cstr_to_str(unsafe { *argv.add(i) });

        match arg {
            "-u" => {
                i += 1;
                if i < argc as usize {
                    let val = cstr_to_str(unsafe { *argv.add(i) });
                    filter.set_tag(val);
                } else {
                    prints("journalctl: -u requires a service name\n");
                    return 1;
                }
            }
            "-p" => {
                i += 1;
                if i < argc as usize {
                    let val = cstr_to_str(unsafe { *argv.add(i) });
                    if let Some(p) = parse_usize(val) {
                        filter.max_priority = if p > 7 { 7 } else { p as u8 };
                    } else {
                        prints("journalctl: invalid priority\n");
                        return 1;
                    }
                } else {
                    prints("journalctl: -p requires a priority level\n");
                    return 1;
                }
            }
            "-n" => {
                i += 1;
                if i < argc as usize {
                    let val = cstr_to_str(unsafe { *argv.add(i) });
                    if let Some(n) = parse_usize(val) {
                        filter.tail_count = n;
                    } else {
                        prints("journalctl: invalid count\n");
                        return 1;
                    }
                } else {
                    prints("journalctl: -n requires a count\n");
                    return 1;
                }
            }
            "-f" => {
                filter.follow = true;
            }
            "-h" | "--help" | "help" => {
                show_usage();
                return 0;
            }
            _ => {
                prints("journalctl: unknown option: ");
                prints(arg);
                prints("\n");
                show_usage();
                return 1;
            }
        }

        i += 1;
    }

    if filter.follow {
        follow_journal(&filter);
    } else {
        read_journal(&filter);
    }

    0
}
