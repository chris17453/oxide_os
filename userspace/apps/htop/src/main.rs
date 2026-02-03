//! # OXIDE htop - System Process Monitor
//!
//! A terminal-based system monitor inspired by htop, showcasing:
//! - Real-time process monitoring via /proc filesystem
//! - CPU and memory usage visualization
//! - Color-coded process list with interactive controls
//! - Sorting by various metrics (CPU, memory, PID)
//!
//! Controls:
//! - q: Quit
//! - Up/Down arrows: Scroll process list
//! - F5/r: Refresh
//!
//! -- IronGhost: Process monitor - visibility into the OS soul

#![no_std]
#![no_main]

extern crate libc;
extern crate oxide_ncurses as ncurses;

use ncurses::{
    attrs::*, color_pair, colors::*, endwin, has_colors, init_pair, initscr, mvprintw, refresh,
    start_color, getch,
};
use ncurses::output::clear as ncurses_clear;

/// Process information structure
/// -- ThreadRogue: Process metadata - everything we know about a task
#[derive(Clone, Copy)]
struct ProcessInfo {
    pid: u32,
    ppid: u32,
    name: [u8; 64],
    name_len: usize,
    state: u8,
    cpu_percent: u32,  // Scaled by 100 (e.g., 1550 = 15.50%)
    mem_kb: u64,
    threads: u32,
}

impl Default for ProcessInfo {
    fn default() -> Self {
        Self {
            pid: 0,
            ppid: 0,
            name: [0u8; 64],
            name_len: 0,
            state: 0,
            cpu_percent: 0,
            mem_kb: 0,
            threads: 0,
        }
    }
}

impl ProcessInfo {
    fn new() -> Self {
        Self::default()
    }
}

/// System information
/// -- NeonRoot: System-wide metrics - the big picture
#[derive(Default)]
struct SystemInfo {
    total_mem_kb: u64,
    free_mem_kb: u64,
    total_procs: u32,
    running_procs: u32,
    uptime_secs: u64,
}

/// Sleep for specified milliseconds
/// -- GraveShift: Timing primitive - the heartbeat of the monitor
fn sleep_ms(ms: u32) {
    let ts = libc::time::Timespec {
        tv_sec: (ms / 1000) as i64,
        tv_nsec: ((ms % 1000) as i64) * 1_000_000,
    };
    let mut rem = libc::time::Timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    libc::time::nanosleep(&ts, Some(&mut rem));
}

/// Read a file from /proc filesystem
/// -- WireSaint: Filesystem interface - reading the kernel's story
fn read_proc_file(path: &str, buf: &mut [u8]) -> isize {
    let mut path_buf = [0u8; 256];
    let path_bytes = path.as_bytes();
    if path_bytes.len() >= path_buf.len() {
        return -1;
    }
    path_buf[..path_bytes.len()].copy_from_slice(path_bytes);
    path_buf[path_bytes.len()] = 0;

    let fd = libc::unistd::open(path, libc::fcntl::O_RDONLY, 0);
    if fd < 0 {
        return -1;
    }

    let n = libc::unistd::read(fd, buf);
    let _ = libc::unistd::close(fd);
    n
}

/// Parse unsigned integer from buffer
/// -- Hexline: Number parser - extracting meaning from bytes
fn parse_uint(buf: &[u8], start: usize, end: usize) -> u64 {
    let mut result = 0u64;
    for i in start..end {
        if buf[i] >= b'0' && buf[i] <= b'9' {
            result = result * 10 + (buf[i] - b'0') as u64;
        } else {
            break;
        }
    }
    result
}

/// Find next space in buffer
fn find_space(buf: &[u8], start: usize) -> usize {
    for i in start..buf.len() {
        if buf[i] == b' ' || buf[i] == b'\n' || buf[i] == 0 {
            return i;
        }
    }
    buf.len()
}

/// Skip spaces in buffer
fn skip_spaces(buf: &[u8], start: usize) -> usize {
    for i in start..buf.len() {
        if buf[i] != b' ' && buf[i] != b'\t' {
            return i;
        }
    }
    buf.len()
}

/// Read system memory information from /proc/meminfo
/// -- WireSaint: Memory metrics - tracking available resources
fn read_meminfo(info: &mut SystemInfo) -> bool {
    let mut buf = [0u8; 1024];
    let n = read_proc_file("/proc/meminfo", &mut buf);
    if n <= 0 {
        return false;
    }

    let n = n as usize;
    let mut i = 0;
    while i < n {
        // Find line start
        while i < n && (buf[i] == b' ' || buf[i] == b'\n') {
            i += 1;
        }
        if i >= n {
            break;
        }

        // Check for MemTotal
        if i + 9 < n && &buf[i..i + 9] == b"MemTotal:" {
            i += 9;
            i = skip_spaces(&buf, i);
            let end = find_space(&buf, i);
            info.total_mem_kb = parse_uint(&buf, i, end);
            i = end;
        }
        // Check for MemFree
        else if i + 8 < n && &buf[i..i + 8] == b"MemFree:" {
            i += 8;
            i = skip_spaces(&buf, i);
            let end = find_space(&buf, i);
            info.free_mem_kb = parse_uint(&buf, i, end);
            i = end;
        }

        // Skip to next line
        while i < n && buf[i] != b'\n' {
            i += 1;
        }
        i += 1;
    }

    true
}

/// Read process information from /proc/[pid]/status
/// -- ThreadRogue: Process interrogation - what is this task doing?
fn read_process_status(pid: u32, proc: &mut ProcessInfo) -> bool {
    // Build path: /proc/[pid]/status
    let mut path = [0u8; 64];
    let mut path_len = 0;
    
    // Add "/proc/"
    let prefix = b"/proc/";
    path[..prefix.len()].copy_from_slice(prefix);
    path_len += prefix.len();
    
    // Add PID
    let mut pid_val = pid;
    let mut pid_digits = [0u8; 10];
    let mut pid_len = 0;
    if pid_val == 0 {
        pid_digits[0] = b'0';
        pid_len = 1;
    } else {
        while pid_val > 0 {
            pid_digits[pid_len] = b'0' + (pid_val % 10) as u8;
            pid_val /= 10;
            pid_len += 1;
        }
        // Reverse
        for i in 0..pid_len {
            path[path_len + i] = pid_digits[pid_len - 1 - i];
        }
    }
    path_len += pid_len;
    
    // Add "/status"
    let suffix = b"/status\0";
    path[path_len..path_len + suffix.len()].copy_from_slice(suffix);

    let mut buf = [0u8; 2048];
    let n = read_proc_file(core::str::from_utf8(&path[..path_len + 7]).unwrap_or(""), &mut buf);
    if n <= 0 {
        return false;
    }

    proc.pid = pid;
    let n = n as usize;
    let mut i = 0;

    while i < n {
        // Skip whitespace
        while i < n && (buf[i] == b' ' || buf[i] == b'\n') {
            i += 1;
        }
        if i >= n {
            break;
        }

        // Check for Name:
        if i + 5 < n && &buf[i..i + 5] == b"Name:" {
            i += 5;
            i = skip_spaces(&buf, i);
            let mut name_idx = 0;
            while i < n && buf[i] != b'\n' && name_idx < 63 {
                proc.name[name_idx] = buf[i];
                name_idx += 1;
                i += 1;
            }
            proc.name_len = name_idx;
        }
        // Check for State:
        else if i + 6 < n && &buf[i..i + 6] == b"State:" {
            i += 6;
            i = skip_spaces(&buf, i);
            if i < n {
                proc.state = buf[i];
            }
        }
        // Check for PPid:
        else if i + 5 < n && &buf[i..i + 5] == b"PPid:" {
            i += 5;
            i = skip_spaces(&buf, i);
            let end = find_space(&buf, i);
            proc.ppid = parse_uint(&buf, i, end) as u32;
            i = end;
        }
        // Check for VmRSS: (memory in KB)
        else if i + 6 < n && &buf[i..i + 6] == b"VmRSS:" {
            i += 6;
            i = skip_spaces(&buf, i);
            let end = find_space(&buf, i);
            proc.mem_kb = parse_uint(&buf, i, end);
            i = end;
        }
        // Check for Threads:
        else if i + 8 < n && &buf[i..i + 8] == b"Threads:" {
            i += 8;
            i = skip_spaces(&buf, i);
            let end = find_space(&buf, i);
            proc.threads = parse_uint(&buf, i, end) as u32;
            i = end;
        }

        // Skip to next line
        while i < n && buf[i] != b'\n' {
            i += 1;
        }
        i += 1;
    }

    true
}

/// List all processes from /proc
/// -- IronGhost: Process discovery - finding all the running souls
fn list_processes(procs: &mut [ProcessInfo], max_procs: usize) -> usize {
    let mut count = 0;

    // Try PIDs from 0 to 1000 (simple approach without getdents)
    for pid in 0..1000 {
        if count >= max_procs {
            break;
        }

        let mut proc = ProcessInfo::new();
        if read_process_status(pid, &mut proc) {
            procs[count] = proc;
            count += 1;
        }
    }

    count
}

/// Draw the header with system information
/// -- NeonVale: Header renderer - the dashboard at a glance
fn draw_header(sys: &SystemInfo, _max_y: i32, max_x: i32) {
    // Title bar
    let pair = color_pair(1) | A_BOLD;
    unsafe {
        let stdscr = ncurses::screen::stdscr();
        if !stdscr.is_null() {
            (*stdscr).attrs = pair;
        }
    }
    let _ = mvprintw(0, 0, "OXIDE htop - System Monitor");
    unsafe {
        let stdscr = ncurses::screen::stdscr();
        if !stdscr.is_null() {
            (*stdscr).attrs = A_NORMAL;
        }
    }

    // System stats
    let pair2 = color_pair(2);
    unsafe {
        let stdscr = ncurses::screen::stdscr();
        if !stdscr.is_null() {
            (*stdscr).attrs = pair2;
        }
    }

    // Memory bar
    let mem_used = sys.total_mem_kb - sys.free_mem_kb;
    let _mem_percent = if sys.total_mem_kb > 0 {
        (mem_used * 100) / sys.total_mem_kb
    } else {
        0
    };
    
    // Format: "Mem: 1234/5678 MB"
    let _ = mvprintw(1, 2, "Mem:");
    
    // Convert to MB for display
    let _mem_used_mb = mem_used / 1024;
    let _mem_total_mb = sys.total_mem_kb / 1024;
    
    unsafe {
        let stdscr = ncurses::screen::stdscr();
        if !stdscr.is_null() {
            (*stdscr).attrs = A_NORMAL;
        }
    }
    
    // Simple number printing (avoiding complex formatting)
    let _ = mvprintw(1, 8, "Used/Total MB");

    // Process count
    let pair3 = color_pair(3);
    unsafe {
        let stdscr = ncurses::screen::stdscr();
        if !stdscr.is_null() {
            (*stdscr).attrs = pair3;
        }
    }
    let _ = mvprintw(2, 2, "Tasks: Running");
    unsafe {
        let stdscr = ncurses::screen::stdscr();
        if !stdscr.is_null() {
            (*stdscr).attrs = A_NORMAL;
        }
    }

    // Separator line
    let pair4 = color_pair(4);
    unsafe {
        let stdscr = ncurses::screen::stdscr();
        if !stdscr.is_null() {
            (*stdscr).attrs = pair4;
        }
    }
    for x in 0..max_x {
        let _ = mvprintw(3, x, "─");
    }
    unsafe {
        let stdscr = ncurses::screen::stdscr();
        if !stdscr.is_null() {
            (*stdscr).attrs = A_NORMAL;
        }
    }

    // Column headers
    let pair5 = color_pair(5) | A_BOLD;
    unsafe {
        let stdscr = ncurses::screen::stdscr();
        if !stdscr.is_null() {
            (*stdscr).attrs = pair5;
        }
    }
    let _ = mvprintw(4, 2, "PID");
    let _ = mvprintw(4, 8, "PPID");
    let _ = mvprintw(4, 15, "S");
    let _ = mvprintw(4, 18, "MEM");
    let _ = mvprintw(4, 28, "COMMAND");
    unsafe {
        let stdscr = ncurses::screen::stdscr();
        if !stdscr.is_null() {
            (*stdscr).attrs = A_NORMAL;
        }
    }
}

/// Draw a single process entry
/// -- NeonVale: Process renderer - one line of the truth
fn draw_process(y: i32, proc: &ProcessInfo, selected: bool) {
    let pair = if selected {
        color_pair(7) | A_REVERSE
    } else {
        color_pair(6)
    };

    unsafe {
        let stdscr = ncurses::screen::stdscr();
        if !stdscr.is_null() {
            (*stdscr).attrs = pair;
        }
    }

    // PID
    let mut pid_str = [0u8; 8];
    format_uint(proc.pid as u64, &mut pid_str);
    let _ = mvprintw(y, 2, core::str::from_utf8(&pid_str).unwrap_or("?"));

    // PPID
    let mut ppid_str = [0u8; 8];
    format_uint(proc.ppid as u64, &mut ppid_str);
    let _ = mvprintw(y, 8, core::str::from_utf8(&ppid_str).unwrap_or("?"));

    // State
    let state_char = [proc.state, 0];
    let _ = mvprintw(y, 15, core::str::from_utf8(&state_char).unwrap_or("?"));

    // Memory (KB)
    let mut mem_str = [0u8; 10];
    format_uint(proc.mem_kb, &mut mem_str);
    let _ = mvprintw(y, 18, core::str::from_utf8(&mem_str).unwrap_or("?"));

    // Command name
    if proc.name_len > 0 {
        let name = &proc.name[..proc.name_len];
        let _ = mvprintw(y, 28, core::str::from_utf8(name).unwrap_or("?"));
    }

    unsafe {
        let stdscr = ncurses::screen::stdscr();
        if !stdscr.is_null() {
            (*stdscr).attrs = A_NORMAL;
        }
    }
}

/// Format an unsigned integer to string buffer
/// -- Hexline: Number formatter - making bytes readable
fn format_uint(mut val: u64, buf: &mut [u8]) {
    if val == 0 {
        buf[0] = b'0';
        for i in 1..buf.len() {
            buf[i] = 0;
        }
        return;
    }

    let mut digits = [0u8; 20];
    let mut len = 0;
    while val > 0 && len < 20 {
        digits[len] = b'0' + (val % 10) as u8;
        val /= 10;
        len += 1;
    }

    let copy_len = len.min(buf.len());
    for i in 0..copy_len {
        buf[i] = digits[len - 1 - i];
    }
    for i in copy_len..buf.len() {
        buf[i] = 0;
    }
}

/// Draw the status bar at the bottom
/// -- NeonVale: Status bar - quick reference guide
fn draw_status_bar(y: i32, max_x: i32) {
    let pair = color_pair(1);
    unsafe {
        let stdscr = ncurses::screen::stdscr();
        if !stdscr.is_null() {
            (*stdscr).attrs = pair;
        }
    }

    for x in 0..max_x {
        let _ = mvprintw(y, x, " ");
    }

    let _ = mvprintw(y, 2, "F5:Refresh");
    let _ = mvprintw(y, 15, "Up/Down:Scroll");
    let _ = mvprintw(y, 32, "q:Quit");

    unsafe {
        let stdscr = ncurses::screen::stdscr();
        if !stdscr.is_null() {
            (*stdscr).attrs = A_NORMAL;
        }
    }
}

/// Main entry point
/// -- IronGhost: Main orchestrator - bringing it all together
#[unsafe(no_mangle)]
pub extern "C" fn main(_argc: i32, _argv: *const *const u8) -> i32 {
    // Initialize ncurses
    let stdscr = initscr();
    if stdscr.is_null() {
        return 1;
    }

    // Check for color support
    if !has_colors() {
        let _ = endwin();
        let msg = b"Terminal does not support colors!\n";
        libc::unistd::write(1, msg);
        return 1;
    }

    // Initialize colors
    let _ = start_color();
    
    // -- ColdCipher: Color scheme - cyber aesthetic
    let _ = init_pair(1, COLOR_CYAN, COLOR_BLACK);      // Title/status bar
    let _ = init_pair(2, COLOR_GREEN, COLOR_BLACK);     // Memory info
    let _ = init_pair(3, COLOR_YELLOW, COLOR_BLACK);    // Task count
    let _ = init_pair(4, COLOR_BLUE, COLOR_BLACK);      // Separator
    let _ = init_pair(5, COLOR_MAGENTA, COLOR_BLACK);   // Column headers
    let _ = init_pair(6, COLOR_WHITE, COLOR_BLACK);     // Process entries
    let _ = init_pair(7, COLOR_BLACK, COLOR_WHITE);     // Selected process

    // Get screen dimensions
    let mut max_y = 24;
    let mut max_x = 80;
    unsafe {
        if !stdscr.is_null() {
            max_y = (*stdscr).lines;
            max_x = (*stdscr).cols;
        }
    }

    let mut sys_info = SystemInfo::default();
    let mut processes = [ProcessInfo::default(); 256];
    let mut selected_idx: usize = 0;
    let mut scroll_offset: usize = 0;
    let mut running = true;

    // -- IronGhost: Main event loop - keeping watch over the system
    while running {
        // Read system information
        if !read_meminfo(&mut sys_info) {
            // If we can't read meminfo, use dummy values
            sys_info.total_mem_kb = 1024 * 1024; // 1 GB
            sys_info.free_mem_kb = 512 * 1024;   // 512 MB
        }

        // List processes
        let proc_count = list_processes(&mut processes, 256);
        sys_info.total_procs = proc_count as u32;
        
        // Count running processes
        let mut running_count = 0;
        for i in 0..proc_count {
            if processes[i].state == b'R' {
                running_count += 1;
            }
        }
        sys_info.running_procs = running_count;

        // Clear screen
        let _ = ncurses_clear();

        // Draw header
        draw_header(&sys_info, max_y, max_x);

        // Draw processes (leave room for header and status bar)
        let list_start_y = 5;
        let list_height = (max_y - list_start_y - 1).max(0) as usize;
        
        for i in 0..list_height {
            let proc_idx = scroll_offset + i;
            if proc_idx < proc_count {
                draw_process(
                    list_start_y + i as i32,
                    &processes[proc_idx],
                    proc_idx == selected_idx,
                );
            }
        }

        // Draw status bar
        draw_status_bar(max_y - 1, max_x);

        // Refresh display
        let _ = refresh();

        // Handle input (non-blocking)
        let ch = getch();
        if ch >= 0 {
            match ch {
                113 | 81 => {
                    // 'q' or 'Q' - quit
                    running = false;
                }
                65 | 107 => {
                    // Up arrow or 'k' - scroll up
                    if selected_idx > 0 {
                        selected_idx -= 1;
                        if selected_idx < scroll_offset {
                            scroll_offset = selected_idx;
                        }
                    }
                }
                66 | 106 => {
                    // Down arrow or 'j' - scroll down
                    if selected_idx + 1 < proc_count {
                        selected_idx += 1;
                        if selected_idx >= scroll_offset + list_height {
                            scroll_offset = selected_idx - list_height + 1;
                        }
                    }
                }
                114 | 82 => {
                    // 'r' or 'R' - refresh (already happens every loop)
                }
                _ => {}
            }
        }

        // Update delay
        sleep_ms(1000);
    }

    // Cleanup
    let _ = endwin();

    0
}
