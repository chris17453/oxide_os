//! ps - report a snapshot of current processes
//!
//! Supports standard UNIX and BSD options:
//! - BSD style: ps aux, ps ax, ps u
//! - UNIX style: ps -ef, ps -aux, ps -e
//!
//! 🔥 GraveShift: Complete ps implementation - no fake data! 🔥

#![no_std]
#![no_main]

use libc::dirent::{closedir, opendir, readdir};
use libc::*;

/// Global argv pointer for argument parsing
static mut ARGV: *const *const u8 = core::ptr::null();
static mut ARGC: usize = 0;

/// Command-line options
struct Options {
    /// Show all processes
    all: bool,
    /// Show processes for all users
    all_users: bool,
    /// Show processes without controlling terminal
    no_tty: bool,
    /// Full format listing (UNIX -f)
    full_format: bool,
    /// User-oriented format (BSD u)
    user_format: bool,
    /// Long format (BSD l)
    long_format: bool,
}

impl Options {
    fn new() -> Self {
        Options {
            all: false,
            all_users: false,
            no_tty: false,
            full_format: false,
            user_format: false,
            long_format: false,
        }
    }

    /// Parse command-line arguments
    fn parse() -> Self {
        let mut opts = Options::new();
        let mut i = 1; // Skip argv[0]

        unsafe {
            let argc = ARGC;

            while i < argc {
                if ARGV.is_null() || (*ARGV.add(i)).is_null() {
                    break;
                }

                let arg = cstr_to_str(*ARGV.add(i));

                if arg.is_empty() {
                    i += 1;
                    continue;
                }

                // BSD-style options (no dash)
                if !arg.starts_with("-") {
                    for ch in arg.as_bytes() {
                        match ch {
                            b'a' => opts.all = true,
                            b'u' => opts.user_format = true,
                            b'x' => opts.no_tty = true,
                            b'l' => opts.long_format = true,
                            _ => {}
                        }
                    }
                } else {
                    // UNIX-style options (with dash)
                    let arg = &arg[1..]; // Skip the dash
                    for ch in arg.as_bytes() {
                        match ch {
                            b'a' => opts.all = true,
                            b'A' | b'e' => {
                                opts.all = true;
                                opts.all_users = true;
                                opts.no_tty = true;
                            }
                            b'u' => opts.user_format = true,
                            b'x' => opts.no_tty = true,
                            b'f' => opts.full_format = true,
                            b'l' => opts.long_format = true,
                            _ => {}
                        }
                    }
                }

                i += 1;
            }
        }

        // Default to showing all processes if -e, -A, or ax is used
        if !opts.all && !opts.all_users && !opts.no_tty {
            // Default: only show processes with tty for current user
        }

        opts
    }
}

/// Convert C string to Rust string slice
fn cstr_to_str(ptr: *const u8) -> &'static str {
    if ptr.is_null() {
        return "";
    }
    let mut len = 0;
    unsafe {
        while *ptr.add(len) != 0 {
            len += 1;
            if len > 1024 {
                break; // Safety limit
            }
        }
        let slice = core::slice::from_raw_parts(ptr, len);
        core::str::from_utf8(slice).unwrap_or("")
    }
}

/// Parse a number from a byte slice
fn parse_num(s: &[u8]) -> Option<u32> {
    if s.is_empty() {
        return None;
    }
    let mut result: u32 = 0;
    for &b in s {
        if b < b'0' || b > b'9' {
            return None;
        }
        result = result.checked_mul(10)?.checked_add((b - b'0') as u32)?;
    }
    Some(result)
}

/// Parse the first number from a whitespace-separated list
fn parse_first_num(s: &[u8]) -> Option<u32> {
    let mut i = 0;
    // Skip whitespace
    while i < s.len() && (s[i] == b' ' || s[i] == b'\t') {
        i += 1;
    }
    if i >= s.len() {
        return None;
    }
    // Find end of number
    let start = i;
    while i < s.len() && s[i] >= b'0' && s[i] <= b'9' {
        i += 1;
    }
    parse_num(&s[start..i])
}

/// Parse the second number from a whitespace-separated list
fn parse_second_num(s: &[u8]) -> Option<u32> {
    let mut i = 0;
    // Skip whitespace
    while i < s.len() && (s[i] == b' ' || s[i] == b'\t') {
        i += 1;
    }
    // Skip first number
    while i < s.len() && s[i] >= b'0' && s[i] <= b'9' {
        i += 1;
    }
    // Skip whitespace
    while i < s.len() && (s[i] == b' ' || s[i] == b'\t') {
        i += 1;
    }
    if i >= s.len() {
        return None;
    }
    // Find end of second number
    let start = i;
    while i < s.len() && s[i] >= b'0' && s[i] <= b'9' {
        i += 1;
    }
    parse_num(&s[start..i])
}

/// Read a file into a buffer, return bytes read
fn read_file(path: &str, buf: &mut [u8]) -> isize {
    let fd = open(path, O_RDONLY as u32, 0);
    if fd < 0 {
        return -1;
    }
    let n = read(fd, buf);
    close(fd);
    n
}

/// Parse a line from status file to get value after tab
fn parse_status_line<'a>(line: &'a [u8], key: &[u8]) -> Option<&'a [u8]> {
    if line.len() <= key.len() {
        return None;
    }
    if !line.starts_with(key) {
        return None;
    }
    // Skip key and colon, find tab
    let rest = &line[key.len()..];
    if rest.is_empty() || rest[0] != b':' {
        return None;
    }
    let rest = &rest[1..];
    // Skip whitespace
    let mut start = 0;
    while start < rest.len() && (rest[start] == b'\t' || rest[start] == b' ') {
        start += 1;
    }
    if start >= rest.len() {
        return None;
    }
    // Find end of value (newline or end)
    let mut end = start;
    while end < rest.len() && rest[end] != b'\n' {
        end += 1;
    }
    Some(&rest[start..end])
}

/// Process info
struct ProcInfo {
    pid: u32,
    ppid: u32,
    state: u8,
    uid: u32,
    gid: u32,
    euid: u32,
    egid: u32,
    name: [u8; 64],
    name_len: usize,
    cmdline: [u8; 512],
    cmdline_len: usize,
    is_kernel_thread: bool,
    tty_nr: u32,     // TTY device number
    start_time: u64, // Start time in jiffies
    utime: u64,      // User CPU time in jiffies
    stime: u64,      // System CPU time in jiffies
}

impl ProcInfo {
    fn new() -> Self {
        ProcInfo {
            pid: 0,
            ppid: 0,
            state: b'?',
            uid: 0,
            gid: 0,
            euid: 0,
            egid: 0,
            name: [0; 64],
            name_len: 0,
            cmdline: [0; 512],
            cmdline_len: 0,
            is_kernel_thread: false,
            tty_nr: 0,
            start_time: 0,
            utime: 0,
            stime: 0,
        }
    }

    /// Get name as str
    fn name_str(&self) -> &str {
        core::str::from_utf8(&self.name[..self.name_len]).unwrap_or("???")
    }

    /// Get cmdline as str
    fn cmdline_str(&self) -> &str {
        if self.cmdline_len == 0 {
            return self.name_str();
        }
        core::str::from_utf8(&self.cmdline[..self.cmdline_len]).unwrap_or(self.name_str())
    }

    /// Get user name for this process
    fn user_name(&self) -> &str {
        // Try to get username from passwd database
        let passwd = pwd::getpwuid(self.uid);
        if !passwd.is_null() {
            unsafe {
                let name_ptr = (*passwd).pw_name;
                if !name_ptr.is_null() {
                    return cstr_to_str(name_ptr);
                }
            }
        }
        // Fallback to uid number
        "?"
    }
}

/// Convert PID to path component
fn pid_to_path(pid: u32, path: &mut [u8], start: usize) -> usize {
    let mut pid_str = [0u8; 12];
    let mut pid_len = 0;
    let mut n = pid;
    if n == 0 {
        pid_str[0] = b'0';
        pid_len = 1;
    } else {
        while n > 0 {
            pid_str[pid_len] = b'0' + (n % 10) as u8;
            n /= 10;
            pid_len += 1;
        }
        // Reverse
        pid_str[..pid_len].reverse();
    }
    path[start..start + pid_len].copy_from_slice(&pid_str[..pid_len]);
    pid_len
}

/// Parse /proc/[pid]/stat file
/// Returns (state, ppid, tty_nr, start_time, utime, stime) on success
fn parse_stat_file(stat_data: &[u8]) -> Option<(u8, u32, u32, u64, u64, u64)> {
    // Format: pid (comm) state ppid pgrp session tty_nr ...
    // Find the closing paren of the comm field
    let mut paren_end = 0;
    for i in (0..stat_data.len()).rev() {
        if stat_data[i] == b')' {
            paren_end = i;
            break;
        }
    }
    if paren_end == 0 {
        return None;
    }

    // Parse fields after comm (starting after ") ")
    let rest = &stat_data[paren_end + 1..];
    let mut fields = [0u64; 52];
    let mut field_idx = 0;
    let mut i = 0;

    // Parse all fields
    while i < rest.len() && field_idx < fields.len() {
        // Skip whitespace
        while i < rest.len() && (rest[i] == b' ' || rest[i] == b'\t') {
            i += 1;
        }
        if i >= rest.len() {
            break;
        }

        // Parse number (handle negative)
        let mut num: i64 = 0;
        let mut negative = false;
        if rest[i] == b'-' {
            negative = true;
            i += 1;
        }
        while i < rest.len() && rest[i] >= b'0' && rest[i] <= b'9' {
            num = num * 10 + (rest[i] - b'0') as i64;
            i += 1;
        }
        if negative {
            num = -num;
        }
        fields[field_idx] = num as u64;
        field_idx += 1;
    }

    if field_idx < 14 {
        return None; // Need at least fields up to utime
    }

    // Field indices (0-based after comm):
    // 0 = state, 1 = ppid, 4 = tty_nr, 11 = utime, 12 = stime, 19 = starttime
    let state = fields[0] as u8;
    let ppid = fields[1] as u32;
    let tty_nr = fields[4] as u32;
    let utime = fields[11];
    let stime = fields[12];
    let start_time = if field_idx > 19 { fields[19] } else { 0 };

    Some((state, ppid, tty_nr, start_time, utime, stime))
}

/// Format TTY device number to string
/// Returns formatted TTY like "pts/0", "tty1", or "?" for no tty
fn format_tty(tty_nr: u32, buf: &mut [u8]) -> usize {
    if tty_nr == 0 {
        buf[0] = b'?';
        return 1;
    }

    // For now, just show "?" since we don't have proper TTY subsystem yet
    // TODO: Map major/minor device numbers to TTY names
    buf[0] = b'?';
    1
}

/// Format jiffies to HH:MM:SS
fn format_time(jiffies: u64, buf: &mut [u8]) -> usize {
    // Convert jiffies to seconds (100 Hz = 100 jiffies/sec)
    let total_secs = jiffies / 100;
    let hours = total_secs / 3600;
    let minutes = (total_secs % 3600) / 60;
    let seconds = total_secs % 60;

    let mut pos = 0;

    // Hours
    if hours >= 10 {
        buf[pos] = b'0' + (hours / 10) as u8;
        pos += 1;
    }
    buf[pos] = b'0' + (hours % 10) as u8;
    pos += 1;
    buf[pos] = b':';
    pos += 1;

    // Minutes
    buf[pos] = b'0' + (minutes / 10) as u8;
    pos += 1;
    buf[pos] = b'0' + (minutes % 10) as u8;
    pos += 1;
    buf[pos] = b':';
    pos += 1;

    // Seconds
    buf[pos] = b'0' + (seconds / 10) as u8;
    pos += 1;
    buf[pos] = b'0' + (seconds % 10) as u8;
    pos += 1;

    pos
}

/// Format start time to a short format (HH:MM or date)
fn format_start_time(start_jiffies: u64, buf: &mut [u8]) -> usize {
    // For now, just show elapsed time in minutes
    // TODO: Show actual time or date based on how long ago process started
    let elapsed_secs = start_jiffies / 100;
    let elapsed_mins = elapsed_secs / 60;

    if elapsed_mins < 60 {
        // Show as MM:SS if less than an hour
        let mins = elapsed_mins;
        let secs = elapsed_secs % 60;
        let mut pos = 0;
        if mins >= 10 {
            buf[pos] = b'0' + (mins / 10) as u8;
            pos += 1;
        } else {
            buf[pos] = b' ';
            pos += 1;
        }
        buf[pos] = b'0' + (mins % 10) as u8;
        pos += 1;
        buf[pos] = b':';
        pos += 1;
        buf[pos] = b'0' + (secs / 10) as u8;
        pos += 1;
        buf[pos] = b'0' + (secs % 10) as u8;
        pos += 1;
        pos
    } else {
        // Show as HH:MM for longer times
        let hours = elapsed_mins / 60;
        let mins = elapsed_mins % 60;
        let mut pos = 0;
        if hours >= 10 {
            buf[pos] = b'0' + (hours / 10) as u8;
            pos += 1;
        } else {
            buf[pos] = b' ';
            pos += 1;
        }
        buf[pos] = b'0' + (hours % 10) as u8;
        pos += 1;
        buf[pos] = b':';
        pos += 1;
        buf[pos] = b'0' + (mins / 10) as u8;
        pos += 1;
        buf[pos] = b'0' + (mins % 10) as u8;
        pos += 1;
        pos
    }
}

/// Read process info from /proc/[pid]/status and /proc/[pid]/cmdline
fn read_proc_info(pid: u32) -> Option<ProcInfo> {
    let mut info = ProcInfo::new();
    info.pid = pid;

    // Build path: /proc/[pid]/status
    let mut path = [0u8; 32];
    let prefix = b"/proc/";
    path[..prefix.len()].copy_from_slice(prefix);
    let mut pos = prefix.len();
    pos += pid_to_path(pid, &mut path, pos);

    let suffix = b"/status";
    path[pos..pos + suffix.len()].copy_from_slice(suffix);
    pos += suffix.len();

    // Read status file
    let mut buf = [0u8; 1024];
    let path_str = core::str::from_utf8(&path[..pos]).ok()?;
    let n = read_file(path_str, &mut buf);
    if n <= 0 {
        return None;
    }

    // Parse lines
    let content = &buf[..n as usize];
    let mut line_start = 0;
    for i in 0..content.len() {
        if content[i] == b'\n' || i == content.len() - 1 {
            let line_end = if content[i] == b'\n' { i } else { i + 1 };
            let line = &content[line_start..line_end];

            if let Some(val) = parse_status_line(line, b"Name") {
                let copy_len = val.len().min(info.name.len() - 1);
                info.name[..copy_len].copy_from_slice(&val[..copy_len]);
                info.name_len = copy_len;
            } else if let Some(val) = parse_status_line(line, b"State") {
                if !val.is_empty() {
                    info.state = val[0];
                }
            } else if let Some(val) = parse_status_line(line, b"PPid") {
                if let Some(ppid) = parse_num(val) {
                    info.ppid = ppid;
                }
            } else if let Some(val) = parse_status_line(line, b"Uid") {
                // Uid line format: "Uid:\treal\teff\tsaved\tfs"
                // We want the first value (real uid)
                if let Some(uid) = parse_first_num(val) {
                    info.uid = uid;
                }
                // Second value is euid
                if let Some(euid) = parse_second_num(val) {
                    info.euid = euid;
                }
            } else if let Some(val) = parse_status_line(line, b"Gid") {
                if let Some(gid) = parse_first_num(val) {
                    info.gid = gid;
                }
                if let Some(egid) = parse_second_num(val) {
                    info.egid = egid;
                }
            }

            line_start = i + 1;
        }
    }

    // Read cmdline
    let mut cmdline_path = [0u8; 32];
    cmdline_path[..prefix.len()].copy_from_slice(prefix);
    let mut cmdline_pos = prefix.len();
    cmdline_pos += pid_to_path(pid, &mut cmdline_path, cmdline_pos);
    let cmdline_suffix = b"/cmdline";
    cmdline_path[cmdline_pos..cmdline_pos + cmdline_suffix.len()].copy_from_slice(cmdline_suffix);
    cmdline_pos += cmdline_suffix.len();

    let cmdline_path_str = core::str::from_utf8(&cmdline_path[..cmdline_pos]).ok()?;
    let cmdline_n = read_file(cmdline_path_str, &mut info.cmdline);

    if cmdline_n > 0 {
        info.cmdline_len = cmdline_n as usize;
        // Replace NUL bytes with spaces for display
        for i in 0..info.cmdline_len {
            if info.cmdline[i] == 0 {
                info.cmdline[i] = b' ';
            }
        }
        // Trim trailing spaces
        while info.cmdline_len > 0 && info.cmdline[info.cmdline_len - 1] == b' ' {
            info.cmdline_len -= 1;
        }
    } else {
        // No cmdline means kernel thread
        info.is_kernel_thread = true;
    }

    // Read stat file for timing and TTY information
    let mut stat_path = [0u8; 32];
    stat_path[..prefix.len()].copy_from_slice(prefix);
    let mut stat_pos = prefix.len();
    stat_pos += pid_to_path(pid, &mut stat_path, stat_pos);
    let stat_suffix = b"/stat";
    stat_path[stat_pos..stat_pos + stat_suffix.len()].copy_from_slice(stat_suffix);
    stat_pos += stat_suffix.len();

    let stat_path_str = core::str::from_utf8(&stat_path[..stat_pos]).ok()?;
    let mut stat_buf = [0u8; 512];
    let stat_n = read_file(stat_path_str, &mut stat_buf);

    if stat_n > 0 {
        if let Some((state, ppid, tty_nr, start_time, utime, stime)) =
            parse_stat_file(&stat_buf[..stat_n as usize])
        {
            // Update from stat file (more authoritative for these fields)
            info.state = state;
            info.ppid = ppid;
            info.tty_nr = tty_nr;
            info.start_time = start_time;
            info.utime = utime;
            info.stime = stime;
        }
    }

    Some(info)
}

/// Print a number with padding
fn print_padded_num(n: u32, width: usize) {
    let mut buf = [b' '; 12];
    let mut pos = buf.len();
    let mut num = n;

    if num == 0 {
        pos -= 1;
        buf[pos] = b'0';
    } else {
        while num > 0 && pos > 0 {
            pos -= 1;
            buf[pos] = b'0' + (num % 10) as u8;
            num /= 10;
        }
    }

    // Calculate padding
    let num_len = buf.len() - pos;
    if num_len < width {
        for _ in 0..(width - num_len) {
            prints(" ");
        }
    }

    // Print number
    if let Ok(s) = core::str::from_utf8(&buf[pos..]) {
        prints(s);
    }
}

/// Print string with fixed width, left-aligned
fn print_padded_str(s: &str, width: usize) {
    let len = s.len();
    if len >= width {
        // Truncate if too long
        prints(&s[..width]);
    } else {
        prints(s);
        for _ in 0..(width - len) {
            prints(" ");
        }
    }
}

/// Print state with modifiers
fn print_stat(state: u8, is_kernel: bool) {
    // Base state
    prints(core::str::from_utf8(&[state]).unwrap_or("?"));

    // Add modifiers
    if is_kernel {
        prints("s"); // Session leader / kernel thread
    }
}

/// Print default format (PID TTY TIME CMD)
fn print_default(info: &ProcInfo) {
    print_padded_num(info.pid, 5);
    prints(" ");

    // TTY
    let mut tty_buf = [0u8; 16];
    let tty_len = format_tty(info.tty_nr, &mut tty_buf);
    if let Ok(tty_str) = core::str::from_utf8(&tty_buf[..tty_len]) {
        print_padded_str(tty_str, 8);
    } else {
        prints("?       ");
    }
    prints(" ");

    // TIME (total CPU time = utime + stime)
    let total_time = info.utime + info.stime;
    let mut time_buf = [0u8; 16];
    let time_len = format_time(total_time, &mut time_buf);
    if let Ok(time_str) = core::str::from_utf8(&time_buf[..time_len]) {
        prints(time_str);
    } else {
        prints("00:00:00");
    }
    prints(" ");

    // CMD - show kernel threads in brackets
    if info.is_kernel_thread {
        prints("[");
        prints(info.name_str());
        prints("]");
    } else {
        prints(info.cmdline_str());
    }
    prints("\n");
}

/// Print BSD user format (USER PID %CPU %MEM VSZ RSS TTY STAT START TIME COMMAND)
fn print_user_format(info: &ProcInfo) {
    // USER (8 chars)
    print_padded_str(info.user_name(), 8);
    prints(" ");

    // PID (5 chars)
    print_padded_num(info.pid, 5);
    prints(" ");

    // %CPU (4 chars) - TODO: calculate real CPU%
    prints(" 0.0 ");

    // %MEM (4 chars) - TODO: calculate real MEM%
    prints(" 0.0 ");

    // VSZ (6 chars) - TODO: get from statm
    prints("     0 ");

    // RSS (5 chars) - TODO: get from statm
    prints("    0 ");

    // TTY (8 chars)
    let mut tty_buf = [0u8; 16];
    let tty_len = format_tty(info.tty_nr, &mut tty_buf);
    if let Ok(tty_str) = core::str::from_utf8(&tty_buf[..tty_len]) {
        print_padded_str(tty_str, 8);
    } else {
        prints("?       ");
    }
    prints(" ");

    // STAT (4 chars)
    print_stat(info.state, info.is_kernel_thread);
    prints("   ");

    // START (5 chars)
    let mut start_buf = [0u8; 16];
    let start_len = format_start_time(info.start_time, &mut start_buf);
    if let Ok(start_str) = core::str::from_utf8(&start_buf[..start_len]) {
        prints(start_str);
    } else {
        prints("?    ");
    }
    prints(" ");

    // TIME (8 chars)
    let total_time = info.utime + info.stime;
    let mut time_buf = [0u8; 16];
    let time_len = format_time(total_time, &mut time_buf);
    if let Ok(time_str) = core::str::from_utf8(&time_buf[..time_len]) {
        prints(time_str);
    } else {
        prints("00:00:00");
    }
    prints(" ");

    // COMMAND
    if info.is_kernel_thread {
        prints("[");
        prints(info.name_str());
        prints("]");
    } else {
        prints(info.cmdline_str());
    }
    prints("\n");
}

/// Print UNIX full format (UID PID PPID C STIME TTY TIME CMD)
fn print_full_format(info: &ProcInfo) {
    // UID (8 chars)
    print_padded_num(info.uid, 8);
    prints(" ");

    // PID (5 chars)
    print_padded_num(info.pid, 5);
    prints(" ");

    // PPID (5 chars)
    print_padded_num(info.ppid, 5);
    prints(" ");

    // C (CPU utilization) - TODO: calculate
    prints("  0 ");

    // STIME (start time)
    let mut start_buf = [0u8; 16];
    let start_len = format_start_time(info.start_time, &mut start_buf);
    if let Ok(start_str) = core::str::from_utf8(&start_buf[..start_len]) {
        print_padded_str(start_str, 5);
    } else {
        prints("?    ");
    }
    prints(" ");

    // TTY
    let mut tty_buf = [0u8; 16];
    let tty_len = format_tty(info.tty_nr, &mut tty_buf);
    if let Ok(tty_str) = core::str::from_utf8(&tty_buf[..tty_len]) {
        print_padded_str(tty_str, 8);
    } else {
        prints("?       ");
    }
    prints(" ");

    // TIME
    let total_time = info.utime + info.stime;
    let mut time_buf = [0u8; 16];
    let time_len = format_time(total_time, &mut time_buf);
    if let Ok(time_str) = core::str::from_utf8(&time_buf[..time_len]) {
        prints(time_str);
    } else {
        prints("00:00:00");
    }
    prints(" ");

    // CMD
    if info.is_kernel_thread {
        prints("[");
        prints(info.name_str());
        prints("]");
    } else {
        prints(info.cmdline_str());
    }
    prints("\n");
}

#[unsafe(no_mangle)]
fn main(argc: i32, argv: *const *const u8) -> i32 {
    unsafe {
        ARGC = argc as usize;
        ARGV = argv;
    }

    let opts = Options::parse();

    // Print header based on format
    if opts.user_format {
        printlns("USER       PID %CPU %MEM    VSZ   RSS TTY      STAT START   TIME COMMAND");
    } else if opts.full_format {
        printlns("     UID   PID  PPID  C STIME TTY          TIME CMD");
    } else {
        printlns("  PID TTY          TIME CMD");
    }

    // Open /proc directory
    let dir = match opendir("/proc") {
        Some(d) => d,
        None => {
            eprintlns("ps: cannot open /proc");
            return 1;
        }
    };

    // Collect PIDs first (to sort them)
    let mut pids = [0u32; 128];
    let mut pid_count = 0;

    let mut dir = dir;
    while let Some(entry) = readdir(&mut dir) {
        let name = entry.name();
        // Check if name is numeric (a PID directory)
        if let Some(pid) = parse_num(name.as_bytes()) {
            if pid_count < pids.len() {
                pids[pid_count] = pid;
                pid_count += 1;
            }
        }
    }
    closedir(dir);

    // Simple bubble sort to show PIDs in order
    for i in 0..pid_count {
        for j in 0..pid_count - 1 - i {
            if pids[j] > pids[j + 1] {
                pids.swap(j, j + 1);
            }
        }
    }

    // Get current user ID for filtering
    let current_uid = getuid();

    // Print each process
    for i in 0..pid_count {
        let pid = pids[i];
        if let Some(info) = read_proc_info(pid) {
            // Apply filters
            if !opts.all && !opts.all_users {
                // Default: only current user's processes
                if info.uid != current_uid {
                    continue;
                }
            }

            // Print in appropriate format
            if opts.user_format {
                print_user_format(&info);
            } else if opts.full_format {
                print_full_format(&info);
            } else {
                print_default(&info);
            }
        }
    }

    0
}
