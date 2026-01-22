//! uptime - Tell how long the system has been running
//!
//! Shows current time, uptime, number of users, and load averages.

#![no_std]
#![no_main]

use libc::*;

#[unsafe(no_mangle)]
fn main() -> i32 {
    // Get current time
    let now = time::time(None);
    let hours = (now % 86400) / 3600;
    let mins = (now % 3600) / 60;

    // Print current time
    prints(" ");
    if hours < 10 {
        prints("0");
    }
    print_u64(hours as u64);
    prints(":");
    if mins < 10 {
        prints("0");
    }
    print_u64(mins as u64);

    // Get uptime from /proc/uptime (seconds since boot)
    let fd = open("/proc/uptime", O_RDONLY, 0);
    if fd >= 0 {
        let mut buf = [0u8; 64];
        let n = read(fd, &mut buf);
        close(fd);

        if n > 0 {
            // Parse the first number (uptime in seconds)
            let mut uptime_secs: u64 = 0;
            for &c in &buf[..n as usize] {
                if c >= b'0' && c <= b'9' {
                    uptime_secs = uptime_secs * 10 + (c - b'0') as u64;
                } else if c == b'.' || c == b' ' {
                    break;
                }
            }

            prints(" up ");
            print_uptime(uptime_secs);
        }
    } else {
        // No /proc/uptime - use time since epoch as approximation
        prints(" up ");
        print_uptime(now as u64);
    }

    // Count users (simplified - just show 1 for now)
    prints(",  1 user");

    // Load averages (simplified - we don't track this yet)
    prints(",  load average: 0.00, 0.00, 0.00");

    printlns("");

    0
}

/// Print uptime in human-readable format
fn print_uptime(secs: u64) {
    let days = secs / 86400;
    let hours = (secs % 86400) / 3600;
    let mins = (secs % 3600) / 60;

    if days > 0 {
        print_u64(days);
        if days == 1 {
            prints(" day, ");
        } else {
            prints(" days, ");
        }
    }

    if hours < 10 {
        prints(" ");
    }
    print_u64(hours);
    prints(":");
    if mins < 10 {
        prints("0");
    }
    print_u64(mins);
}
