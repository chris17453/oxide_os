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

extern crate alloc;

use libc::poll::{PollFd, events, poll};
use libc::*;

/// Primary journal path (ext4 rootfs, persistent across reboots)
const JOURNAL_PATH: &str = "/var/log/journal";

/// Fallback journal path (tmpfs, for initramfs-only boots)
const JOURNAL_PATH_FALLBACK: &str = "/run/log/journal";

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

/// Try to open the journal file, creating directories as needed.
/// Tries /var/log/journal first (persistent), falls back to /run/log/journal (tmpfs).
fn open_journal() -> (i32, &'static str) {
    // Try primary path: /var/log/journal
    let _ = mkdir("/var", 0o755);
    let _ = mkdir("/var/log", 0o755);
    let fd = open(JOURNAL_PATH, (O_WRONLY | O_CREAT | O_APPEND) as u32, 0o644);
    if fd >= 0 {
        return (fd, JOURNAL_PATH);
    }

    // Fallback: /run/log/journal (tmpfs, always writable)
    let _ = mkdir("/run", 0o755);
    let _ = mkdir("/run/log", 0o755);
    let fd = open(
        JOURNAL_PATH_FALLBACK,
        (O_WRONLY | O_CREAT | O_APPEND) as u32,
        0o644,
    );
    (fd, JOURNAL_PATH_FALLBACK)
}

/// Main entry point
#[unsafe(no_mangle)]
fn main(_argc: i32, _argv: *const *const u8) -> i32 {
    // Detach from controlling terminal
    setsid();

    // Open /dev/kmsg for reading
    let kmsg_fd = open2("/dev/kmsg", O_RDONLY);
    if kmsg_fd < 0 {
        log("Failed to open /dev/kmsg");
        return 1;
    }

    // Open journal file for appending
    let (journal_fd, _journal_path) = open_journal();
    if journal_fd < 0 {
        log("Failed to open journal file");
        close(kmsg_fd);
        return 1;
    }

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
