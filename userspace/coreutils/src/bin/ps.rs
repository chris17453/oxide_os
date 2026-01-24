//! ps - report a snapshot of current processes
//!
//! Shows processes with services/daemons in brackets like Linux:
//! - Kernel threads (no cmdline) shown in brackets: [kworker]
//! - Services registered in /etc/services.d shown in brackets: [sshd]

#![no_std]
#![no_main]

use libc::dirent::{closedir, opendir, readdir};
use libc::*;

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
    name: [u8; 64],
    name_len: usize,
    is_daemon: bool,    // Kernel thread or daemon service
    is_service: bool,   // Registered service from /etc/services.d
}

impl ProcInfo {
    fn new() -> Self {
        ProcInfo {
            pid: 0,
            ppid: 0,
            state: b'?',
            name: [0; 64],
            name_len: 0,
            is_daemon: false,
            is_service: false,
        }
    }

    /// Get name as str
    fn name_str(&self) -> &str {
        core::str::from_utf8(&self.name[..self.name_len]).unwrap_or("???")
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

/// Check if process has a cmdline (kernel threads don't)
fn has_cmdline(pid: u32) -> bool {
    let mut path = [0u8; 32];
    let prefix = b"/proc/";
    path[..prefix.len()].copy_from_slice(prefix);
    let mut pos = prefix.len();
    pos += pid_to_path(pid, &mut path, pos);

    let suffix = b"/cmdline";
    path[pos..pos + suffix.len()].copy_from_slice(suffix);
    pos += suffix.len();

    let path_str = match core::str::from_utf8(&path[..pos]) {
        Ok(s) => s,
        Err(_) => return true,
    };

    let mut buf = [0u8; 16];
    let n = read_file(path_str, &mut buf);

    // Kernel threads have empty cmdline
    n > 0
}

/// Check if process is registered as a service in /etc/services.d
fn is_registered_service(name: &str) -> bool {
    // Try to open /etc/services.d/<name>
    let mut path = [0u8; 128];
    let prefix = b"/etc/services.d/";
    let name_bytes = name.as_bytes();

    if prefix.len() + name_bytes.len() >= 128 {
        return false;
    }

    path[..prefix.len()].copy_from_slice(prefix);
    path[prefix.len()..prefix.len() + name_bytes.len()].copy_from_slice(name_bytes);
    let path_len = prefix.len() + name_bytes.len();

    let path_str = match core::str::from_utf8(&path[..path_len]) {
        Ok(s) => s,
        Err(_) => return false,
    };

    // Just check if file exists
    let fd = open(path_str, O_RDONLY as u32, 0);
    if fd >= 0 {
        close(fd);
        return true;
    }
    false
}

/// Read process info from /proc/[pid]/status
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
    path[pos] = 0;

    // Read status file
    let mut buf = [0u8; 512];
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
                // Parse PPID
                if let Some(ppid) = parse_num(val) {
                    info.ppid = ppid;
                }
            }

            line_start = i + 1;
        }
    }

    // Check 1: Kernel thread (no cmdline)
    if !has_cmdline(pid) {
        info.is_daemon = true;
    }

    // Check 2: Registered as a service in /etc/services.d
    // Need to copy name to avoid borrow conflict
    let name_str = core::str::from_utf8(&info.name[..info.name_len]).unwrap_or("");
    if is_registered_service(name_str) {
        info.is_service = true;
    }

    Some(info)
}

/// Print a number with padding
fn print_padded_num(n: u32, width: usize) {
    let mut buf = [b' '; 10];
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

#[unsafe(no_mangle)]
fn main() -> i32 {
    // Header like Linux ps
    printlns("  PID TTY          TIME CMD");

    // Open /proc directory
    let dir = match opendir("/proc") {
        Some(d) => d,
        None => {
            eprintlns("ps: cannot open /proc");
            return 1;
        }
    };

    // Collect PIDs first (to sort them)
    let mut pids = [0u32; 64];
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

    // Print each process
    for i in 0..pid_count {
        let pid = pids[i];
        if let Some(info) = read_proc_info(pid) {
            // PID (5 chars right-aligned)
            print_padded_num(info.pid, 5);
            prints(" ");

            // TTY (always ? for now)
            prints("?        ");

            // TIME (CPU time, 00:00:00 for now)
            prints("00:00:00 ");

            // CMD (process name)
            // Show in brackets if it's a kernel thread or registered service
            let name = info.name_str();
            if info.is_daemon || info.is_service {
                prints("[");
                prints(name);
                prints("]");
            } else if !name.is_empty() {
                prints(name);
            } else {
                prints("???");
            }
            prints("\n");
        }
    }

    0
}
