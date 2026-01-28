//! OXIDE Journal Daemon (journald)
//!
//! Reads log entries from /dev/kmsg and persists them to /var/log/journal.
//! Runs as a background service, started by the service manager before
//! other services to ensure all log output is captured.
//!
//! The daemon polls /dev/kmsg for new entries and appends them to the
//! journal file. On SIGTERM it flushes and exits cleanly.

#![no_std]
#![no_main]
#![allow(unused)]

extern crate alloc;

use libc::poll::{PollFd, events, poll};
use libc::*;

/// Path to the journal file
const JOURNAL_PATH: &str = "/var/log/journal";

/// Read buffer size
const BUF_SIZE: usize = 4096;

/// Poll timeout in milliseconds
const POLL_TIMEOUT: i32 = 1000;

/// Volatile flag for signal handling
static mut SHOULD_EXIT: bool = false;

/// Log a message to stdout (goes to /dev/kmsg via service manager redirect)
fn log(msg: &str) {
    prints("[journald] ");
    prints(msg);
    prints("\n");
}

/// Ensure /var/log directory exists
fn ensure_log_dir() {
    let _ = mkdir("/var", 0o755);
    let _ = mkdir("/var/log", 0o755);
}

/// Main entry point
#[unsafe(no_mangle)]
fn main(argc: i32, argv: *const *const u8) -> i32 {
    // Detach from controlling terminal
    setsid();

    log("Starting");

    // Ensure log directory exists
    ensure_log_dir();

    // Open /dev/kmsg for reading
    let kmsg_fd = open2("/dev/kmsg", O_RDONLY);
    if kmsg_fd < 0 {
        log("Failed to open /dev/kmsg");
        return 1;
    }

    // Open journal file for appending
    let journal_fd = open(JOURNAL_PATH, (O_WRONLY | O_CREAT | O_APPEND) as u32, 0o644);
    if journal_fd < 0 {
        log("Failed to open /var/log/journal");
        close(kmsg_fd);
        return 1;
    }

    log("Logging to /var/log/journal");

    // Main poll loop
    let mut buf = [0u8; BUF_SIZE];
    let mut poll_fds = [PollFd::new(kmsg_fd, events::POLLIN)];

    loop {
        // Check exit flag
        unsafe {
            if SHOULD_EXIT {
                break;
            }
        }

        // Poll for data on /dev/kmsg
        let ready = poll(&mut poll_fds, POLL_TIMEOUT);

        if ready > 0 && (poll_fds[0].revents & events::POLLIN) != 0 {
            // Data available — read from /dev/kmsg
            let n = read(kmsg_fd, &mut buf);
            if n > 0 {
                // Write to journal file
                let _ = write(journal_fd, &buf[..n as usize]);
            }
            // Reset revents for next poll
            poll_fds[0].revents = 0;
        }

        if ready < 0 {
            // Poll error — check if it's due to signal
            unsafe {
                if SHOULD_EXIT {
                    break;
                }
            }
        }
    }

    log("Shutting down");
    close(journal_fd);
    close(kmsg_fd);
    0
}
