//! free - Display amount of free and used memory
//!
//! Reads from /proc/meminfo to get memory statistics.

#![no_std]
#![no_main]

use libc::*;

/// Parse a value from a meminfo line like "MemTotal:       12345 kB"
fn parse_meminfo_line(line: &[u8]) -> Option<u64> {
    // Find the colon
    let colon_pos = line.iter().position(|&c| c == b':')?;

    // Skip whitespace after colon
    let value_start = line[colon_pos + 1..]
        .iter()
        .position(|&c| c >= b'0' && c <= b'9')?;

    let value_bytes = &line[colon_pos + 1 + value_start..];

    // Parse the number
    let mut value: u64 = 0;
    for &c in value_bytes {
        if c >= b'0' && c <= b'9' {
            value = value * 10 + (c - b'0') as u64;
        } else {
            break;
        }
    }

    Some(value)
}

/// Check if line starts with prefix
fn starts_with(line: &[u8], prefix: &[u8]) -> bool {
    if line.len() < prefix.len() {
        return false;
    }
    &line[..prefix.len()] == prefix
}

#[unsafe(no_mangle)]
fn main() -> i32 {
    // Open /proc/meminfo
    let fd = open("/proc/meminfo", O_RDONLY, 0);
    if fd < 0 {
        eprintlns("free: cannot open /proc/meminfo");
        return 1;
    }

    // Read the file
    let mut buf = [0u8; 1024];
    let n = read(fd, &mut buf);
    close(fd);

    if n < 0 {
        eprintlns("free: cannot read /proc/meminfo");
        return 1;
    }

    let data = &buf[..n as usize];

    // Parse values (all in kB)
    let mut mem_total: u64 = 0;
    let mut mem_free: u64 = 0;
    let mut buffers: u64 = 0;
    let mut cached: u64 = 0;
    let mut swap_total: u64 = 0;
    let mut swap_free: u64 = 0;

    // Process line by line
    for line in data.split(|&c| c == b'\n') {
        if line.is_empty() {
            continue;
        }

        if starts_with(line, b"MemTotal:") {
            mem_total = parse_meminfo_line(line).unwrap_or(0);
        } else if starts_with(line, b"MemFree:") {
            mem_free = parse_meminfo_line(line).unwrap_or(0);
        } else if starts_with(line, b"Buffers:") {
            buffers = parse_meminfo_line(line).unwrap_or(0);
        } else if starts_with(line, b"Cached:") {
            cached = parse_meminfo_line(line).unwrap_or(0);
        } else if starts_with(line, b"SwapTotal:") {
            swap_total = parse_meminfo_line(line).unwrap_or(0);
        } else if starts_with(line, b"SwapFree:") {
            swap_free = parse_meminfo_line(line).unwrap_or(0);
        }
    }

    let mem_used = mem_total.saturating_sub(mem_free);
    let swap_used = swap_total.saturating_sub(swap_free);
    let buff_cache = buffers + cached;

    // Print header
    printlns("              total        used        free      shared  buff/cache   available");

    // Print Mem line
    prints("Mem:    ");
    print_mem_value(mem_total);
    print_mem_value(mem_used);
    print_mem_value(mem_free);
    print_mem_value(0); // shared (not tracked)
    print_mem_value(buff_cache);
    print_mem_value(mem_free + buff_cache); // available
    printlns("");

    // Print Swap line
    prints("Swap:   ");
    print_mem_value(swap_total);
    print_mem_value(swap_used);
    print_mem_value(swap_free);
    printlns("");

    0
}

/// Print a memory value right-aligned in 12 characters (in kB)
fn print_mem_value(kb: u64) {
    // Convert to string
    let mut buf = [b' '; 12];
    let mut val = kb;
    let mut pos = 11;

    if val == 0 {
        buf[pos] = b'0';
    } else {
        while val > 0 && pos > 0 {
            buf[pos] = b'0' + (val % 10) as u8;
            val /= 10;
            pos = pos.saturating_sub(1);
        }
    }

    // Print the buffer
    for &c in &buf {
        putchar(c);
    }
}
