//! top - display and update sorted information about processes
//!
//! Full-featured process monitor with ncurses UI supporting all standard flags:
//! - Real-time CPU and memory usage tracking
//! - Interactive sorting and filtering
//! - Color-coded display
//! - Batch mode for logging
//! - Multiple display modes
//!
//! -- ByteRiot: Performance monitoring - track every cycle, every byte

#![no_std]
#![no_main]

extern crate alloc;

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use alloc::format;
use libc::*;
use oxide_ncurses::{WINDOW, screen, input, output, color, attributes, attrs, window, colors, keys};

/// Process information structure
#[derive(Clone, Debug)]
struct ProcessInfo {
    pid: u32,
    ppid: u32,
    state: u8,
    name: [u8; 64],
    name_len: usize,
    user_time: u64,
    system_time: u64,
    vsize: u64,      // Virtual memory size in bytes
    rss: u64,        // Resident set size in pages
    cpu_percent: f32,
    mem_percent: f32,
    priority: i32,
    nice: i32,
}

impl ProcessInfo {
    fn new() -> Self {
        Self {
            pid: 0,
            ppid: 0,
            state: b'?',
            name: [0; 64],
            name_len: 0,
            user_time: 0,
            system_time: 0,
            vsize: 0,
            rss: 0,
            cpu_percent: 0.0,
            mem_percent: 0.0,
            priority: 0,
            nice: 0,
        }
    }

    fn name_str(&self) -> &str {
        core::str::from_utf8(&self.name[..self.name_len]).unwrap_or("???")
    }

    fn state_char(&self) -> char {
        self.state as char
    }
}

/// System statistics
struct SystemStats {
    uptime: u64,
    total_mem: u64,
    free_mem: u64,
    used_mem: u64,
    buffers: u64,
    cached: u64,
    total_swap: u64,
    free_swap: u64,
    num_tasks: usize,
    num_running: usize,
    num_sleeping: usize,
    num_stopped: usize,
    num_zombie: usize,
    load_avg_1: f32,
    load_avg_5: f32,
    load_avg_15: f32,
    cpu_user: u64,
    cpu_system: u64,
    cpu_idle: u64,
    /// — ByteRiot: Previous /proc/stat snapshot for delta-based CPU% display.
    /// Without deltas, cumulative counters show near-zero% forever since idle
    /// dominates from boot. Delta = current - previous over the refresh interval.
    prev_cpu_user: u64,
    prev_cpu_system: u64,
    prev_cpu_idle: u64,
}

impl SystemStats {
    fn new() -> Self {
        Self {
            uptime: 0,
            total_mem: 0,
            free_mem: 0,
            used_mem: 0,
            buffers: 0,
            cached: 0,
            total_swap: 0,
            free_swap: 0,
            num_tasks: 0,
            num_running: 0,
            num_sleeping: 0,
            num_stopped: 0,
            num_zombie: 0,
            load_avg_1: 0.0,
            load_avg_5: 0.0,
            load_avg_15: 0.0,
            cpu_user: 0,
            cpu_system: 0,
            cpu_idle: 0,
            prev_cpu_user: 0,
            prev_cpu_system: 0,
            prev_cpu_idle: 0,
        }
    }
}

/// Configuration options
struct TopConfig {
    delay: u32,           // Update delay in deciseconds (tenths of seconds)
    batch_mode: bool,     // Batch mode (non-interactive)
    iterations: i32,      // Number of iterations (-1 = infinite)
    show_threads: bool,   // Show individual threads
    show_idle: bool,      // Show idle processes
    case_sensitive: bool, // Case-sensitive filtering
    sort_field: SortField,
    reverse_sort: bool,
    color_mode: bool,
    highlight_running: bool,
    highlight_changes: bool,
    user_filter: Option<u32>, // Filter by UID
    pid_filter: Option<u32>,  // Filter by PID
    max_lines: i32,       // Number of process lines to display
}

impl TopConfig {
    fn new() -> Self {
        Self {
            delay: 30, // 3.0 seconds default
            batch_mode: false,
            iterations: -1,
            show_threads: false,
            show_idle: true,
            case_sensitive: false,
            sort_field: SortField::CpuPercent,
            reverse_sort: false,
            color_mode: true,
            highlight_running: true,
            highlight_changes: true,
            user_filter: None,
            pid_filter: None,
            max_lines: -1,
        }
    }
}

/// Sort field enumeration
#[derive(Clone, Copy, PartialEq)]
enum SortField {
    Pid,
    CpuPercent,
    MemPercent,
    Time,
    Command,
    User,
}

/// Parse command line arguments
fn parse_args(argc: i32, argv: *const *const u8, config: &mut TopConfig) {
    for i in 1..argc {
        let arg_ptr = unsafe { *argv.add(i as usize) };
        let mut arg_len = 0;
        while unsafe { *arg_ptr.add(arg_len) } != 0 {
            arg_len += 1;
        }
        let arg_bytes = unsafe { core::slice::from_raw_parts(arg_ptr, arg_len) };
        let arg = core::str::from_utf8(arg_bytes).unwrap_or("");

        match arg {
            "-h" | "--help" => {
                print_help();
                exit(0);
            }
            "-v" | "--version" => {
                printlns("top version 1.0.0 - OXIDE OS");
                exit(0);
            }
            "-b" | "--batch" => {
                config.batch_mode = true;
            }
            "-c" | "--command-line" => {
                // Show full command line (default in our implementation)
            }
            "-d" | "--delay" => {
                if i + 1 < argc {
                    let next_ptr = unsafe { *argv.add((i + 1) as usize) };
                    let mut next_len = 0;
                    while unsafe { *next_ptr.add(next_len) } != 0 {
                        next_len += 1;
                    }
                    let next_bytes = unsafe { core::slice::from_raw_parts(next_ptr, next_len) };
                    if let Ok(delay_str) = core::str::from_utf8(next_bytes) {
                        if let Some(delay_val) = parse_float(delay_str) {
                            config.delay = (delay_val * 10.0) as u32;
                        }
                    }
                }
            }
            "-H" | "--threads" => {
                config.show_threads = true;
            }
            "-i" | "--idle" => {
                config.show_idle = false;
            }
            "-n" | "--iterations" => {
                if i + 1 < argc {
                    let next_ptr = unsafe { *argv.add((i + 1) as usize) };
                    let mut next_len = 0;
                    while unsafe { *next_ptr.add(next_len) } != 0 {
                        next_len += 1;
                    }
                    let next_bytes = unsafe { core::slice::from_raw_parts(next_ptr, next_len) };
                    if let Some(n) = parse_num(next_bytes) {
                        config.iterations = n as i32;
                    }
                }
            }
            "-o" | "--sort-override" => {
                if i + 1 < argc {
                    let next_ptr = unsafe { *argv.add((i + 1) as usize) };
                    let mut next_len = 0;
                    while unsafe { *next_ptr.add(next_len) } != 0 {
                        next_len += 1;
                    }
                    let next_bytes = unsafe { core::slice::from_raw_parts(next_ptr, next_len) };
                    if let Ok(field) = core::str::from_utf8(next_bytes) {
                        config.sort_field = match field {
                            "PID" | "pid" => SortField::Pid,
                            "CPU" | "cpu" | "%CPU" => SortField::CpuPercent,
                            "MEM" | "mem" | "%MEM" => SortField::MemPercent,
                            "TIME" | "time" | "TIME+" => SortField::Time,
                            "COMMAND" | "command" => SortField::Command,
                            "USER" | "user" => SortField::User,
                            _ => SortField::CpuPercent,
                        };
                    }
                }
            }
            "-p" | "--pid" => {
                if i + 1 < argc {
                    let next_ptr = unsafe { *argv.add((i + 1) as usize) };
                    let mut next_len = 0;
                    while unsafe { *next_ptr.add(next_len) } != 0 {
                        next_len += 1;
                    }
                    let next_bytes = unsafe { core::slice::from_raw_parts(next_ptr, next_len) };
                    if let Some(pid) = parse_num(next_bytes) {
                        config.pid_filter = Some(pid);
                    }
                }
            }
            "-u" | "-U" | "--user" => {
                if i + 1 < argc {
                    let next_ptr = unsafe { *argv.add((i + 1) as usize) };
                    let mut next_len = 0;
                    while unsafe { *next_ptr.add(next_len) } != 0 {
                        next_len += 1;
                    }
                    let next_bytes = unsafe { core::slice::from_raw_parts(next_ptr, next_len) };
                    // Try to parse as UID number
                    if let Some(uid) = parse_num(next_bytes) {
                        config.user_filter = Some(uid);
                    }
                }
            }
            "-s" | "--secure-mode" => {
                // Secure mode - disable some commands
            }
            "-S" | "--cumulative" => {
                // Cumulative time mode
            }
            "-1" | "--single-cpu" => {
                // Show individual CPU stats
            }
            _ => {
                if arg.starts_with("-") {
                    eprintlns("top: unknown option");
                    print_help();
                    exit(1);
                }
            }
        }
    }
}

fn print_help() {
    printlns("Usage: top [options]");
    printlns("");
    printlns("Options:");
    printlns("  -h, --help              Show this help");
    printlns("  -v, --version           Show version");
    printlns("  -b, --batch             Batch mode (non-interactive)");
    printlns("  -c, --command-line      Show full command line");
    printlns("  -d, --delay=SECS        Delay between updates (default: 3.0)");
    printlns("  -H, --threads           Show threads");
    printlns("  -i, --idle              Don't show idle processes");
    printlns("  -n, --iterations=N      Number of iterations");
    printlns("  -o, --sort-override=FLD Sort by field (PID, CPU, MEM, TIME, COMMAND)");
    printlns("  -p, --pid=PID           Monitor only this PID");
    printlns("  -u, --user=USER         Monitor only this user (UID)");
    printlns("  -s, --secure-mode       Secure mode");
    printlns("  -S, --cumulative        Cumulative time mode");
    printlns("  -1, --single-cpu        Show individual CPU stats");
    printlns("");
    printlns("Interactive commands:");
    printlns("  h or ?  Show help");
    printlns("  q       Quit");
    printlns("  Space   Force update");
    printlns("  k       Kill a process");
    printlns("  r       Renice a process");
    printlns("  M       Sort by memory usage");
    printlns("  P       Sort by CPU usage");
    printlns("  T       Sort by time");
    printlns("  N       Sort by PID");
    printlns("  R       Reverse sort order");
    printlns("  i       Toggle idle processes");
    printlns("  u       Filter by user");
    printlns("  n or #  Set number of lines");
    printlns("  d or s  Set update delay");
}

/// Get current time in milliseconds
/// — ByteRiot: gettimeofday gives microsecond precision, convert to ms for sanity
fn get_time_ms() -> u64 {
    let mut tv = time::Timeval::default();
    time::gettimeofday(&mut tv, None);
    (tv.tv_sec as u64) * 1000 + (tv.tv_usec as u64) / 1000
}

/// Parse a number from bytes
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

/// — ByteRiot: Parse a signed integer from bytes. /proc/[pid]/stat has negative
/// nice values and we were silently dropping them like amateurs.
fn parse_signed_num(s: &[u8]) -> Option<i64> {
    if s.is_empty() {
        return None;
    }
    let (negative, digits) = if s[0] == b'-' {
        (true, &s[1..])
    } else {
        (false, s)
    };
    if digits.is_empty() {
        return None;
    }
    let mut result: i64 = 0;
    for &b in digits {
        if b < b'0' || b > b'9' {
            return None;
        }
        result = result.checked_mul(10)?.checked_add((b - b'0') as i64)?;
    }
    if negative { Some(-result) } else { Some(result) }
}

/// Parse a float from string
fn parse_float(s: &str) -> Option<f32> {
    let bytes = s.as_bytes();
    let mut result: f32 = 0.0;
    let mut decimal = false;
    let mut divisor: f32 = 1.0;
    
    for &b in bytes {
        if b == b'.' {
            if decimal {
                return None;
            }
            decimal = true;
        } else if b >= b'0' && b <= b'9' {
            let digit = (b - b'0') as f32;
            if decimal {
                divisor *= 10.0;
                result += digit / divisor;
            } else {
                result = result * 10.0 + digit;
            }
        } else {
            return None;
        }
    }
    Some(result)
}

/// Read a file into buffer
fn read_file(path: &str, buf: &mut [u8]) -> isize {
    let fd = open(path, O_RDONLY as u32, 0);
    if fd < 0 {
        return -1;
    }
    let n = read(fd, buf);
    close(fd);
    n
}

/// Parse /proc/uptime
fn read_uptime() -> u64 {
    let mut buf = [0u8; 64];
    let n = read_file("/proc/uptime", &mut buf);
    if n <= 0 {
        return 0;
    }

    let mut uptime: u64 = 0;
    for i in 0..(n as usize) {
        if buf[i] >= b'0' && buf[i] <= b'9' {
            uptime = uptime * 10 + (buf[i] - b'0') as u64;
        } else if buf[i] == b'.' || buf[i] == b' ' {
            break;
        }
    }
    uptime
}

/// Parse /proc/meminfo
fn read_meminfo(stats: &mut SystemStats) {
    let mut buf = [0u8; 1024];
    let n = read_file("/proc/meminfo", &mut buf);
    if n <= 0 {
        return;
    }

    let data = &buf[..n as usize];
    for line in data.split(|&c| c == b'\n') {
        if line.is_empty() {
            continue;
        }

        if line.starts_with(b"MemTotal:") {
            stats.total_mem = parse_meminfo_value(line);
        } else if line.starts_with(b"MemFree:") {
            stats.free_mem = parse_meminfo_value(line);
        } else if line.starts_with(b"Buffers:") {
            stats.buffers = parse_meminfo_value(line);
        } else if line.starts_with(b"Cached:") {
            stats.cached = parse_meminfo_value(line);
        } else if line.starts_with(b"SwapTotal:") {
            stats.total_swap = parse_meminfo_value(line);
        } else if line.starts_with(b"SwapFree:") {
            stats.free_swap = parse_meminfo_value(line);
        }
    }

    stats.used_mem = stats.total_mem.saturating_sub(stats.free_mem);
}

/// Parse value from meminfo line
fn parse_meminfo_value(line: &[u8]) -> u64 {
    let colon_pos = match line.iter().position(|&c| c == b':') {
        Some(p) => p,
        None => return 0,
    };

    let value_start = match line[colon_pos + 1..].iter().position(|&c| c >= b'0' && c <= b'9') {
        Some(p) => colon_pos + 1 + p,
        None => return 0,
    };

    let mut value: u64 = 0;
    for &c in &line[value_start..] {
        if c >= b'0' && c <= b'9' {
            value = value * 10 + (c - b'0') as u64;
        } else {
            break;
        }
    }
    value
}

/// Parse /proc/loadavg
fn read_loadavg(stats: &mut SystemStats) {
    let mut buf = [0u8; 128];
    let n = read_file("/proc/loadavg", &mut buf);
    if n <= 0 {
        return;
    }

    // Format: "0.00 0.01 0.05 1/123 456"
    let data = &buf[..n as usize];
    let mut field = 0;
    let mut start = 0;

    for i in 0..data.len() {
        if data[i] == b' ' || i == data.len() - 1 {
            let end = if i == data.len() - 1 { i + 1 } else { i };
            if start < end {
                let value_bytes = &data[start..end];
                if let Ok(s) = core::str::from_utf8(value_bytes) {
                    if let Some(val) = parse_float(s) {
                        match field {
                            0 => stats.load_avg_1 = val,
                            1 => stats.load_avg_5 = val,
                            2 => stats.load_avg_15 = val,
                            _ => break,
                        }
                    }
                }
                field += 1;
            }
            start = i + 1;
        }
    }
}

/// Parse /proc/stat for CPU statistics
fn read_stat(stats: &mut SystemStats) {
    let mut buf = [0u8; 512];
    let n = read_file("/proc/stat", &mut buf);
    if n <= 0 {
        return;
    }

    let data = &buf[..n as usize];
    for line in data.split(|&c| c == b'\n') {
        if line.starts_with(b"cpu ") {
            // Parse CPU line: cpu <user> <nice> <system> <idle> ...
            let mut values = [0u64; 7];
            let mut idx = 0;
            let mut start = 4; // Skip "cpu "

            for i in 4..line.len() {
                if line[i] == b' ' || i == line.len() - 1 {
                    let end = if i == line.len() - 1 { i + 1 } else { i };
                    if start < end && idx < values.len() {
                        if let Some(val) = parse_num(&line[start..end]) {
                            values[idx] = val as u64;
                            idx += 1;
                        }
                    }
                    start = i + 1;
                }
            }

            stats.cpu_user = values[0];
            stats.cpu_system = values[2];
            stats.cpu_idle = values[3];
            break;
        }
    }
}

/// Build path for /proc/[pid]/filename
fn build_proc_path(pid: u32, filename: &[u8], path: &mut [u8]) -> usize {
    let prefix = b"/proc/";
    path[..prefix.len()].copy_from_slice(prefix);
    let mut pos = prefix.len();

    // Convert PID to string
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
        pid_str[..pid_len].reverse();
    }
    path[pos..pos + pid_len].copy_from_slice(&pid_str[..pid_len]);
    pos += pid_len;

    path[pos] = b'/';
    pos += 1;

    path[pos..pos + filename.len()].copy_from_slice(filename);
    pos += filename.len();

    pos
}

/// Parse /proc/[pid]/stat
fn read_proc_stat(pid: u32, info: &mut ProcessInfo) -> bool {
    let mut path = [0u8; 64];
    let path_len = build_proc_path(pid, b"stat", &mut path);
    
    let path_str = match core::str::from_utf8(&path[..path_len]) {
        Ok(s) => s,
        Err(_) => return false,
    };

    let mut buf = [0u8; 512];
    let n = read_file(path_str, &mut buf);
    if n <= 0 {
        return false;
    }

    let data = &buf[..n as usize];
    
    // Parse stat format: pid (comm) state ppid ...
    // Find the last ')' to handle commands with spaces/parens
    let mut paren_end = 0;
    for i in (0..data.len()).rev() {
        if data[i] == b')' {
            paren_end = i;
            break;
        }
    }
    
    if paren_end == 0 {
        return false;
    }

    // Extract command name (between first '(' and last ')')
    let mut paren_start = 0;
    for i in 0..paren_end {
        if data[i] == b'(' {
            paren_start = i;
            break;
        }
    }
    
    if paren_start > 0 {
        let name_bytes = &data[paren_start + 1..paren_end];
        let copy_len = name_bytes.len().min(info.name.len() - 1);
        info.name[..copy_len].copy_from_slice(&name_bytes[..copy_len]);
        info.name_len = copy_len;
    }

    // — ByteRiot: Parse remaining fields after ')'. Use signed parser so
    // negative nice values don't get silently swallowed into oblivion.
    let rest = &data[paren_end + 1..];
    let mut fields: Vec<i64> = Vec::new();
    let mut field_start = 0;
    let mut in_field = false;

    for i in 0..rest.len() {
        if rest[i] != b' ' && !in_field {
            field_start = i;
            in_field = true;
        } else if rest[i] == b' ' && in_field {
            if let Some(val) = parse_signed_num(&rest[field_start..i]) {
                fields.push(val);
            } else if field_start < i {
                // Non-numeric field (like state)
                if fields.is_empty() && i - field_start == 1 {
                    info.state = rest[field_start];
                }
                fields.push(0);
            }
            in_field = false;
        }
    }

    // — ByteRiot: Field indices in /proc/[pid]/stat after extracting comm and state:
    // fields[0] = 0 (state placeholder), fields[1] = ppid, fields[2] = pgrp, ...
    // From proc(5) man page (1-indexed): ppid=4, utime=14, stime=15, priority=18,
    // nice=19, vsize=23, rss=24. After state extraction, indices shift by 3.
    // fields[1]=ppid, fields[11]=utime, fields[12]=stime, fields[15]=priority,
    // fields[16]=nice, fields[20]=vsize, fields[21]=rss
    if fields.len() > 1 {
        info.ppid = fields[1] as u32;
    }
    if fields.len() > 11 {
        info.user_time = fields[11] as u64;
    }
    if fields.len() > 12 {
        info.system_time = fields[12] as u64;
    }
    if fields.len() > 20 {
        info.vsize = fields[20] as u64;
    }
    if fields.len() > 21 {
        info.rss = fields[21] as u64;
    }
    if fields.len() > 15 {
        info.priority = fields[15] as i32;
    }
    if fields.len() > 16 {
        info.nice = fields[16] as i32;
    }

    true
}

/// Read all processes
fn read_processes(config: &TopConfig) -> Vec<ProcessInfo> {
    let mut processes = Vec::new();

    // Open /proc directory
    let dir = match dirent::opendir("/proc") {
        Some(d) => d,
        None => return processes,
    };

    let mut dir = dir;
    while let Some(entry) = dirent::readdir(&mut dir) {
        let name = entry.name();
        if let Some(pid) = parse_num(name.as_bytes()) {
            // Apply PID filter
            if let Some(filter_pid) = config.pid_filter {
                if pid != filter_pid {
                    continue;
                }
            }

            let mut info = ProcessInfo::new();
            info.pid = pid;

            if read_proc_stat(pid, &mut info) {
                // Apply idle filter
                if !config.show_idle && info.state == b'S' && info.user_time == 0 && info.system_time == 0 {
                    continue;
                }

                processes.push(info);
            }
        }
    }
    dirent::closedir(dir);

    processes
}

/// Calculate CPU percentages
/// — ByteRiot: elapsed_ms is in MILLISECONDS for sub-second precision.
/// CPU times from /proc are in jiffies (1/CLK_TCK seconds, typically 100 Hz).
/// Formula: CPU% = (delta_jiffies * 1000 * 100) / (elapsed_ms * CLK_TCK)
///        = (delta_jiffies * 100000) / (elapsed_ms * 100)
///        = delta_jiffies * 1000 / elapsed_ms
fn calculate_cpu_percentages(processes: &mut [ProcessInfo], prev_processes: &[ProcessInfo], elapsed_ms: u64) {
    if elapsed_ms == 0 {
        return;
    }

    for proc in processes.iter_mut() {
        // Find previous reading
        if let Some(prev) = prev_processes.iter().find(|p| p.pid == proc.pid) {
            let delta_jiffies = (proc.user_time + proc.system_time)
                .saturating_sub(prev.user_time + prev.system_time);
            // — ByteRiot: delta_jiffies are in 1/100th second units (CLK_TCK=100)
            // elapsed_ms is in milliseconds. Convert both to same base:
            // delta_jiffies * 10ms = delta_ms, then (delta_ms * 100) / elapsed_ms = %
            proc.cpu_percent = (delta_jiffies as f32 * 1000.0) / (elapsed_ms as f32);
        }
    }
}

/// Calculate memory percentages
fn calculate_mem_percentages(processes: &mut [ProcessInfo], total_mem: u64) {
    if total_mem == 0 {
        return;
    }

    let page_size = 4096u64; // 4KB pages
    for proc in processes.iter_mut() {
        let mem_kb = (proc.rss * page_size) / 1024;
        proc.mem_percent = (mem_kb as f32 * 100.0) / (total_mem as f32);
    }
}

/// Sort processes
fn sort_processes(processes: &mut [ProcessInfo], field: SortField, reverse: bool) {
    processes.sort_by(|a, b| {
        let cmp = match field {
            SortField::Pid => a.pid.cmp(&b.pid),
            SortField::CpuPercent => {
                b.cpu_percent.partial_cmp(&a.cpu_percent).unwrap_or(core::cmp::Ordering::Equal)
            }
            SortField::MemPercent => {
                b.mem_percent.partial_cmp(&a.mem_percent).unwrap_or(core::cmp::Ordering::Equal)
            }
            SortField::Time => {
                let a_time = a.user_time + a.system_time;
                let b_time = b.user_time + b.system_time;
                b_time.cmp(&a_time)
            }
            SortField::Command => {
                a.name_str().cmp(b.name_str())
            }
            SortField::User => a.pid.cmp(&b.pid), // TODO: implement user comparison
        };

        if reverse {
            cmp.reverse()
        } else {
            cmp
        }
    });
}

/// Update system statistics
fn update_system_stats(stats: &mut SystemStats, processes: &[ProcessInfo]) {
    // — ByteRiot: Snapshot previous CPU values before reading new ones.
    // Delta = new - old gives us the CPU activity over this refresh interval
    // instead of cumulative-since-boot (which is always ~0% user).
    stats.prev_cpu_user = stats.cpu_user;
    stats.prev_cpu_system = stats.cpu_system;
    stats.prev_cpu_idle = stats.cpu_idle;

    stats.uptime = read_uptime();
    read_meminfo(stats);
    read_loadavg(stats);
    read_stat(stats);

    stats.num_tasks = processes.len();
    stats.num_running = 0;
    stats.num_sleeping = 0;
    stats.num_stopped = 0;
    stats.num_zombie = 0;

    for proc in processes {
        match proc.state {
            b'R' => stats.num_running += 1,
            b'S' | b'D' => stats.num_sleeping += 1,
            b'T' => stats.num_stopped += 1,
            b'Z' => stats.num_zombie += 1,
            _ => {}
        }
    }
}

/// Display header in batch mode
fn display_batch_header(stats: &SystemStats, iteration: i32) {
    if iteration > 1 {
        prints("\n");
    }

    prints("top - ");

    // Current time (HH:MM:SS)
    let now = time::time(None);
    let hours = (now % 86400) / 3600;
    let mins = (now % 3600) / 60;
    let secs = now % 60;
    print_padded_num(hours as u32, 2);
    prints(":");
    print_padded_num(mins as u32, 2);
    prints(":");
    print_padded_num(secs as u32, 2);

    prints(" up ");
    let uptime_hours = stats.uptime / 3600;
    let uptime_mins = (stats.uptime % 3600) / 60;
    if uptime_hours > 0 {
        print_u64(uptime_hours);
        prints(":");
        if uptime_mins < 10 {
            prints("0");
        }
        print_u64(uptime_mins);
    } else {
        print_u64(uptime_mins);
        prints(" min");
    }

    prints(",  1 user,  load average: ");
    print_float(stats.load_avg_1, 2);
    prints(", ");
    print_float(stats.load_avg_5, 2);
    prints(", ");
    print_float(stats.load_avg_15, 2);
    prints("\n");

    // Tasks line
    prints("Tasks: ");
    print_u64(stats.num_tasks as u64);
    prints(" total,   ");
    print_u64(stats.num_running as u64);
    prints(" running,   ");
    print_u64(stats.num_sleeping as u64);
    prints(" sleeping,   ");
    print_u64(stats.num_stopped as u64);
    prints(" stopped,   ");
    print_u64(stats.num_zombie as u64);
    prints(" zombie\n");

    // CPU line — delta-based
    let delta_user = stats.cpu_user.saturating_sub(stats.prev_cpu_user);
    let delta_sys = stats.cpu_system.saturating_sub(stats.prev_cpu_system);
    let delta_idle = stats.cpu_idle.saturating_sub(stats.prev_cpu_idle);
    let total_cpu = delta_user + delta_sys + delta_idle;
    let cpu_user_pct = if total_cpu > 0 {
        (delta_user as f32 * 100.0) / total_cpu as f32
    } else {
        0.0
    };
    let cpu_sys_pct = if total_cpu > 0 {
        (delta_sys as f32 * 100.0) / total_cpu as f32
    } else {
        0.0
    };
    let cpu_idle_pct = if total_cpu > 0 {
        (delta_idle as f32 * 100.0) / total_cpu as f32
    } else {
        100.0
    };

    prints("%Cpu(s): ");
    print_float(cpu_user_pct, 1);
    prints(" us,  ");
    print_float(cpu_sys_pct, 1);
    prints(" sy,  0.0 ni,  ");
    print_float(cpu_idle_pct, 1);
    prints(" id,  0.0 wa,  0.0 hi,  0.0 si,  0.0 st\n");

    // Memory line
    prints("MiB Mem :  ");
    print_u64(stats.total_mem / 1024);
    prints(" total,  ");
    print_u64(stats.free_mem / 1024);
    prints(" free,  ");
    print_u64(stats.used_mem / 1024);
    prints(" used,  ");
    print_u64((stats.buffers + stats.cached) / 1024);
    prints(" buff/cache\n");

    // Swap line
    prints("MiB Swap:  ");
    print_u64(stats.total_swap / 1024);
    prints(" total,  ");
    print_u64(stats.free_swap / 1024);
    prints(" free,  ");
    print_u64((stats.total_swap - stats.free_swap) / 1024);
    prints(" used.\n");

    prints("\n");

    // Column headers
    prints("  PID USER      PR  NI    VIRT    RES  %CPU %MEM     TIME+ COMMAND\n");
}

/// Display process list in batch mode
fn display_batch_processes(processes: &[ProcessInfo], max_lines: i32) {
    let limit = if max_lines > 0 {
        max_lines as usize
    } else {
        processes.len()
    };

    for i in 0..limit.min(processes.len()) {
        let proc = &processes[i];
        
        // PID
        print_padded_num(proc.pid, 5);
        prints(" ");

        // USER (show UID for now)
        prints("root     ");

        // PR (priority)
        if proc.priority >= 0 {
            print_padded_num(proc.priority as u32, 3);
        } else {
            prints(" rt");
        }
        prints(" ");

        // NI (nice)
        if proc.nice >= 0 {
            prints(" ");
            print_padded_num(proc.nice as u32, 2);
        } else {
            print_i32(proc.nice);
        }
        prints(" ");

        // VIRT (virtual memory in KB)
        print_padded_num((proc.vsize / 1024) as u32, 7);
        prints(" ");

        // RES (resident memory in KB)
        print_padded_num(((proc.rss * 4096) / 1024) as u32, 6);
        prints(" ");

        // %CPU
        print_float(proc.cpu_percent, 1);
        prints(" ");

        // %MEM
        print_float(proc.mem_percent, 1);
        prints(" ");

        // TIME+
        let total_time = (proc.user_time + proc.system_time) / 100; // Convert to seconds
        let time_mins = total_time / 60;
        let time_secs = total_time % 60;
        print_padded_num(time_mins as u32, 4);
        prints(":");
        if time_secs < 10 {
            prints("0");
        }
        print_u64(time_secs);
        prints(" ");

        // COMMAND
        prints(proc.name_str());
        prints("\n");
    }
}

/// Main loop for batch mode
fn run_batch_mode(config: &TopConfig) {
    let mut prev_processes = Vec::new();
    let mut last_update_ms = get_time_ms();
    let mut stats = SystemStats::new();

    for iteration in 1..=config.iterations {
        if config.iterations > 0 && iteration > config.iterations {
            break;
        }

        let mut processes = read_processes(config);

        let now_ms = get_time_ms();
        let elapsed_ms = now_ms.saturating_sub(last_update_ms).max(100);

        calculate_cpu_percentages(&mut processes, &prev_processes, elapsed_ms);
        calculate_mem_percentages(&mut processes, stats.total_mem);
        sort_processes(&mut processes, config.sort_field, config.reverse_sort);
        update_system_stats(&mut stats, &processes);

        display_batch_header(&stats, iteration);
        display_batch_processes(&processes, config.max_lines);

        prev_processes = processes;
        last_update_ms = now_ms;

        // Sleep for delay
        if config.iterations < 0 || iteration < config.iterations {
            let sleep_time = (config.delay as u32) * 100_000; // deciseconds to microseconds
            time::usleep(sleep_time);
        }
    }
}

/// Run interactive mode with ncurses
fn run_interactive_mode(config: &mut TopConfig) {
    // Initialize ncurses
    let stdscr = screen::initscr();
    if stdscr.is_null() {
        eprintlns("Failed to initialize ncurses");
        return;
    }

    // Setup ncurses
    let _ = screen::cbreak();
    let _ = screen::noecho();

    // — ByteRiot: Use wtimeout instead of nodelay+usleep. wgetch now blocks
    // up to delay_ms then returns -1 on timeout OR returns immediately on
    // keypress. No more busy-polling — the kernel's poll() does the waiting.
    let delay_ms = (config.delay as i32) * 100;
    unsafe {
        (*stdscr).keypad = true;
        (*stdscr).scroll = false;
    }
    input::wtimeout(stdscr, delay_ms);

    // — ByteRiot: Standard ncurses pattern — start_color() initializes the
    // pairs table, then init_pair() populates it. has_colors() is set by
    // initscr() from termcap data.
    if config.color_mode && color::has_colors() {
        let _ = color::start_color();
        let _ = color::init_pair(1, colors::COLOR_GREEN, colors::COLOR_BLACK);   // Low CPU (< 10%)
        let _ = color::init_pair(2, colors::COLOR_YELLOW, colors::COLOR_BLACK);  // Medium CPU (10-50%)
        let _ = color::init_pair(3, colors::COLOR_RED, colors::COLOR_BLACK);     // High CPU (> 50%)
        let _ = color::init_pair(4, colors::COLOR_CYAN, colors::COLOR_BLACK);    // Sleeping processes
        let _ = color::init_pair(5, colors::COLOR_WHITE, colors::COLOR_BLUE);    // Headers
        let _ = color::init_pair(6, colors::COLOR_MAGENTA, colors::COLOR_BLACK); // Zombie processes
    }

    let mut prev_processes = Vec::new();
    let mut last_update_ms: u64 = 0;
    // — ByteRiot: Persistent stats struct holds prev_cpu_* across iterations
    // for delta-based CPU% in the header. Without this, cumulative counters
    // from /proc/stat always show ~0% user because idle dominates from boot.
    let mut stats = SystemStats::new();

    loop {
        let now_ms = get_time_ms();

        // Read data
        let mut processes = read_processes(config);

        let elapsed_ms = now_ms.saturating_sub(last_update_ms).max(100);
        calculate_cpu_percentages(&mut processes, &prev_processes, elapsed_ms);
        calculate_mem_percentages(&mut processes, stats.total_mem);
        sort_processes(&mut processes, config.sort_field, config.reverse_sort);
        update_system_stats(&mut stats, &processes);

        // Display
        display_interactive(&stats, &processes, config, stdscr);

        prev_processes = processes;
        last_update_ms = now_ms;

        // — ByteRiot: wgetch blocks up to delay_ms, then returns -1 (timeout)
        // or returns immediately with a key. Either way, we refresh next iteration.
        let ch = input::getch();
        if ch >= 0 {
            if ch == 27 {
                break;
            } else if ch == keys::KEY_UP || ch == keys::KEY_DOWN
                   || ch == keys::KEY_PPAGE || ch == keys::KEY_NPAGE
                   || ch == keys::KEY_HOME || ch == keys::KEY_END {
                // Scroll keys (future enhancement) — just refresh
            } else if ch >= 0x20 && ch < 0x7F {
                match ch as u8 as char {
                    'q' | 'Q' => break,
                    ' ' => {} // Force refresh (happens next iteration anyway)
                    'M' | 'm' => config.sort_field = SortField::MemPercent,
                    'P' | 'p' => config.sort_field = SortField::CpuPercent,
                    'T' | 't' => config.sort_field = SortField::Time,
                    'N' | 'n' => config.sort_field = SortField::Pid,
                    'R' | 'r' => config.reverse_sort = !config.reverse_sort,
                    'i' | 'I' => config.show_idle = !config.show_idle,
                    'h' | 'H' | '?' => {
                        display_help_screen(stdscr, delay_ms);
                    }
                    'd' | 's' => {
                        // — ByteRiot: Update delay — reconfigure wtimeout
                        // (future: prompt for value)
                    }
                    _ => {}
                }
            }
        }
    }

    // Cleanup
    let _ = screen::endwin();
}

/// Display interactive screen
fn display_interactive(stats: &SystemStats, processes: &[ProcessInfo], config: &TopConfig, win: WINDOW) {
    let _ = output::werase(win);
    let _ = output::wmove(win, 0, 0);

    // Get window size
    let (max_y, max_x) = unsafe {
        ((*win).lines, (*win).cols)
    };

    let mut row = 0;

    // Line 1: top - time, uptime, users, load average
    let _ = attributes::wattron(win, attrs::A_BOLD);
    let _ = output::waddstr(win, "top");
    let _ = attributes::wattroff(win, attrs::A_BOLD);
    let _ = output::waddstr(win, " - ");

    let now = time::time(None);
    let hours = (now % 86400) / 3600;
    let mins = (now % 3600) / 60;
    let secs = now % 60;
    
    let time_str = format!("{:02}:{:02}:{:02}", hours, mins, secs);
    let _ = output::waddstr(win, &time_str);
    
    let _ = output::waddstr(win, " up ");
    let uptime_hours = stats.uptime / 3600;
    let uptime_mins = (stats.uptime % 3600) / 60;
    let uptime_str = if uptime_hours > 0 {
        format!("{}:{:02}", uptime_hours, uptime_mins)
    } else {
        format!("{} min", uptime_mins)
    };
    let _ = output::waddstr(win, &uptime_str);
    
    let load_str = format!(",  1 user,  load average: {:.2}, {:.2}, {:.2}",
                          stats.load_avg_1, stats.load_avg_5, stats.load_avg_15);
    let _ = output::waddstr(win, &load_str);

    row += 1;
    let _ = output::wmove(win, row, 0);

    // Line 2: Tasks
    let tasks_str = format!("Tasks: {} total,   {} running,   {} sleeping,   {} stopped,   {} zombie",
                           stats.num_tasks, stats.num_running, stats.num_sleeping,
                           stats.num_stopped, stats.num_zombie);
    let _ = output::waddstr(win, &tasks_str);

    row += 1;
    let _ = output::wmove(win, row, 0);

    // Line 3: CPU — delta-based for meaningful percentages
    // — ByteRiot: Cumulative counters since boot are useless for display
    // (idle dominates). Delta = current - previous gives activity over the
    // last refresh interval, which is what htop/top actually show.
    let delta_user = stats.cpu_user.saturating_sub(stats.prev_cpu_user);
    let delta_sys = stats.cpu_system.saturating_sub(stats.prev_cpu_system);
    let delta_idle = stats.cpu_idle.saturating_sub(stats.prev_cpu_idle);
    let delta_total = delta_user + delta_sys + delta_idle;

    let cpu_user_pct = if delta_total > 0 {
        (delta_user as f32 * 100.0) / delta_total as f32
    } else {
        0.0
    };
    let cpu_sys_pct = if delta_total > 0 {
        (delta_sys as f32 * 100.0) / delta_total as f32
    } else {
        0.0
    };
    let cpu_idle_pct = if delta_total > 0 {
        (delta_idle as f32 * 100.0) / delta_total as f32
    } else {
        100.0
    };

    let cpu_str = format!("%Cpu(s): {:5.1} us, {:5.1} sy,  0.0 ni, {:5.1} id,  0.0 wa,  0.0 hi,  0.0 si,  0.0 st",
                         cpu_user_pct, cpu_sys_pct, cpu_idle_pct);
    let _ = output::waddstr(win, &cpu_str);

    row += 1;
    let _ = output::wmove(win, row, 0);

    // Line 4: Memory
    let mem_str = format!("MiB Mem : {:7} total, {:7} free, {:7} used, {:7} buff/cache",
                         stats.total_mem / 1024, stats.free_mem / 1024,
                         stats.used_mem / 1024, (stats.buffers + stats.cached) / 1024);
    let _ = output::waddstr(win, &mem_str);

    row += 1;
    let _ = output::wmove(win, row, 0);

    // Line 5: Swap
    let swap_used = stats.total_swap.saturating_sub(stats.free_swap);
    let swap_str = format!("MiB Swap: {:7} total, {:7} free, {:7} used.",
                          stats.total_swap / 1024, stats.free_swap / 1024,
                          swap_used / 1024);
    let _ = output::waddstr(win, &swap_str);

    row += 1;
    let _ = output::wmove(win, row, 0);

    // Blank line
    row += 1;
    let _ = output::wmove(win, row, 0);

    // — ByteRiot: Column headers padded to fill terminal width. Without
    // padding, reverse-video only covers the text, leaving ragged edges.
    let _ = attributes::wattron(win, attrs::A_REVERSE);
    let header_base = "  PID USER      PR  NI    VIRT    RES  %CPU %MEM     TIME+ COMMAND";
    let _ = output::waddstr(win, header_base);
    // Pad remainder of line with spaces for full reverse-video bar
    let header_len = header_base.len() as i32;
    for _ in header_len..max_x {
        let _ = output::waddstr(win, " ");
    }
    let _ = attributes::wattroff(win, attrs::A_REVERSE);

    row += 1;

    // Process list
    // Reserve one line at bottom for status
    let available_rows = (max_y - row - 1).max(0);
    let display_count = available_rows.min(processes.len() as i32);

    for i in 0..display_count as usize {
        if row >= max_y {
            break;
        }

        let proc = &processes[i];
        let _ = output::wmove(win, row, 0);

        // Apply colors based on CPU usage and state
        if config.color_mode && color::has_colors() {
            let color_pair_num = if proc.state == b'Z' {
                6  // Magenta for zombie
            } else if proc.state == b'R' {
                if proc.cpu_percent > 50.0 {
                    3  // Red for high CPU
                } else if proc.cpu_percent > 10.0 {
                    2  // Yellow for medium CPU
                } else {
                    1  // Green for low CPU
                }
            } else {
                4  // Cyan for sleeping/other
            };
            let _ = attributes::wattron(win, color::color_pair(color_pair_num));
        }

        // Highlight running processes with bold
        if config.highlight_running && proc.state == b'R' {
            let _ = attributes::wattron(win, attrs::A_BOLD);
        }

        let proc_line = format!(
            "{:5} {:8} {:3} {:3} {:7} {:6} {:4.1} {:4.1} {:5}:{:02} {}",
            proc.pid,
            "root",
            if proc.priority >= 0 { format!("{:2}", proc.priority) } else { "rt".to_string() },
            proc.nice,
            proc.vsize / 1024,
            (proc.rss * 4096) / 1024,
            proc.cpu_percent,
            proc.mem_percent,
            (proc.user_time + proc.system_time) / 6000, // minutes
            ((proc.user_time + proc.system_time) / 100) % 60, // seconds
            proc.name_str()
        );

        // — ByteRiot: Truncate to terminal width, then pad remainder so
        // color attributes fill the entire row cleanly.
        let display_len = max_x.min(proc_line.len() as i32);
        let _ = output::waddstr(win, &proc_line[..display_len as usize]);
        let line_len = display_len;
        for _ in line_len..max_x {
            let _ = output::waddstr(win, " ");
        }

        if config.highlight_running && proc.state == b'R' {
            let _ = attributes::wattroff(win, attrs::A_BOLD);
        }

        if config.color_mode && color::has_colors() {
            let _ = attributes::wattroff(win, attrs::A_COLOR);
        }

        row += 1;
    }

    // Add status line at bottom
    if max_y > row + 1 {
        let _ = output::wmove(win, max_y - 1, 0);
        let _ = attributes::wattron(win, attrs::A_REVERSE);
        
        let sort_name = match config.sort_field {
            SortField::Pid => "PID",
            SortField::CpuPercent => "CPU",
            SortField::MemPercent => "MEM",
            SortField::Time => "TIME",
            SortField::Command => "CMD",
            SortField::User => "USER",
        };
        
        let status = format!(
            " Sort: {} {} | q/ESC:Quit h:Help M/P/T/N:Sort R:Reverse i:Idle ",
            sort_name,
            if config.reverse_sort { "▼" } else { "▲" }
        );
        
        let display_len = max_x.min(status.len() as i32);
        let _ = output::waddstr(win, &status[..display_len as usize]);
        
        // Fill rest of line with spaces for full reverse video
        for _ in display_len..max_x {
            let _ = output::waddstr(win, " ");
        }
        
        let _ = attributes::wattroff(win, attrs::A_REVERSE);
    }

    let _ = screen::wrefresh(win);
}

/// Display help screen
/// — ByteRiot: Uses wtimeout(-1) to block for keypress, then restores
/// the original delay so the main loop resumes auto-refresh.
fn display_help_screen(win: WINDOW, restore_delay_ms: i32) {
    let _ = output::werase(win);
    let _ = output::wmove(win, 0, 0);

    let help_text = [
        "Help for Interactive Commands - top",
        "",
        "  Space    = Update display",
        "  h or ?   = Help (this screen)",
        "  q or ESC = Quit",
        "",
        "  M        = Sort by memory usage",
        "  P        = Sort by CPU usage",
        "  T        = Sort by time",
        "  N        = Sort by PID",
        "  R        = Reverse sort order",
        "",
        "  i        = Toggle idle processes",
        "",
        "  Arrows   = Scroll (future)",
        "  PgUp/Dn  = Page up/down (future)",
        "",
        "Press any key to continue...",
    ];

    for (i, line) in help_text.iter().enumerate() {
        let _ = output::wmove(win, i as i32, 0);
        let _ = output::waddstr(win, line);
    }

    let _ = screen::wrefresh(win);

    // Block until keypress
    input::wtimeout(win, -1);
    let _ = input::getch();
    // Restore auto-refresh timeout
    input::wtimeout(win, restore_delay_ms);
}

/// Print a number with zero padding
fn print_padded_num(n: u32, width: usize) {
    let mut buf = [b'0'; 10];
    let mut pos = buf.len();
    let mut num = n;

    if num == 0 {
        pos -= 1;
    } else {
        while num > 0 && pos > 0 {
            pos -= 1;
            buf[pos] = b'0' + (num % 10) as u8;
            num /= 10;
        }
    }

    let num_len = buf.len() - pos;
    if num_len < width {
        for _ in 0..(width - num_len) {
            prints(" ");
        }
    }

    if let Ok(s) = core::str::from_utf8(&buf[pos..]) {
        prints(s);
    }
}

/// Print signed integer
fn print_i32(n: i32) {
    if n < 0 {
        prints("-");
        print_u64((-n) as u64);
    } else {
        print_u64(n as u64);
    }
}

/// Print float with specified decimal places
fn print_float(f: f32, decimals: u32) {
    let int_part = f as u64;
    print_u64(int_part);
    prints(".");
    
    let mut frac = f - (int_part as f32);
    for _ in 0..decimals {
        frac *= 10.0;
        let digit = frac as u32 % 10;
        prints(match digit {
            0 => "0", 1 => "1", 2 => "2", 3 => "3", 4 => "4",
            5 => "5", 6 => "6", 7 => "7", 8 => "8", 9 => "9",
            _ => "0",
        });
    }
}

#[unsafe(no_mangle)]
fn main(argc: i32, argv: *const *const u8) -> i32 {
    let mut config = TopConfig::new();
    parse_args(argc, argv, &mut config);

    if config.batch_mode {
        run_batch_mode(&config);
    } else {
        run_interactive_mode(&mut config);
    }

    0
}
