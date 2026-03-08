//! Getty - terminal manager for OXIDE OS
//!
//! Opens a terminal device, configures it, and spawns login.
//! Typically started by init for each configured terminal.

#![no_std]
#![no_main]
#![allow(unused)]

use libc::*;

/// Terminal configuration
struct TermConfig {
    /// Terminal device path
    device: &'static str,
    /// Baud rate (for serial)
    baud: u32,
    /// Terminal type (for TERM env var)
    term_type: &'static str,
}

impl Default for TermConfig {
    fn default() -> Self {
        TermConfig {
            device: "/dev/console",
            baud: 115200,
            term_type: "vt100",
        }
    }
}

/// Clear screen escape sequence
fn clear_screen() {
    prints("\x1b[2J\x1b[H");
}

/// Print system identification banner
fn print_banner() {
    prints("\n");
    prints("OXIDE OS v0.1.0\n");
    prints("================\n");
    prints("\n");
}

/// Open and configure terminal device
fn setup_terminal(config: &TermConfig) -> i32 {
    // Open terminal device FIRST before closing stdio
    let fd = open2(config.device, O_RDWR);

    if fd < 0 {
        return -1;
    }

    // Close any existing stdio
    close(0);
    close(1);
    close(2);

    // Duplicate for stdout and stderr
    dup2(fd, 0);
    dup2(fd, 1);
    dup2(fd, 2);

    // If opened fd was > 2, close it
    if fd > 2 {
        close(fd);
    }

    0
}

/// Parse command line arguments
fn parse_args() -> TermConfig {
    // In a real implementation, we'd parse argv for:
    // - Terminal device path
    // - Baud rate
    // - Options like -L (local line), -h (hardware flow control)
    //
    // For now, return defaults
    TermConfig::default()
}

/// Check if a file descriptor is valid (opened by init before exec)
/// — SableWire: fstat returns 0 on valid fds, -EBADF on closed ones
fn is_fd_valid(fd: i32) -> bool {
    let mut st = libc::stat::Stat::zeroed();
    libc::stat::fstat(fd, &mut st) == 0
}

/// Main entry point
#[unsafe(no_mangle)]
pub fn main() -> i32 {
    // — SableWire: init already set up stdin/stdout/stderr on the correct
    // /dev/ttyN before exec'ing us. Only call setup_terminal() if our fds
    // are broken (e.g., direct exec without init's fd setup). The old code
    // unconditionally opened /dev/console, clobbering the per-VT device
    // and making ALL gettys fight over VT0's input ring. Six processes,
    // one input stream, zero working logins.
    let needs_setup = !is_fd_valid(0);

    loop {
        if needs_setup {
            let config = parse_args();
            if setup_terminal(&config) < 0 {
                exit(1);
            }
        }

        // Clear screen and print banner
        clear_screen();
        print_banner();

        prints("\n");

        // Fork and exec login
        let pid = fork();
        if pid < 0 {
            prints("getty: fork failed\n");
            exit(1);
        }

        if pid == 0 {
            exec("/bin/login");
            prints("getty: failed to exec login\n");
            exit(1);
        }

        // Parent - wait for login to exit
        let mut status = 0;
        waitpid(pid, &mut status, 0);

        // Login exited - respawn after brief delay to avoid rapid flicker
        // Sleep for 1 second before respawning
        let mut ts = TimeSpec {
            tv_sec: 1,
            tv_nsec: 0,
        };
        let mut rem = TimeSpec {
            tv_sec: 0,
            tv_nsec: 0,
        };
        unsafe {
            nanosleep(&ts as *const TimeSpec, &mut rem as *mut TimeSpec);
        }
    }
}

#[repr(C)]
struct TimeSpec {
    tv_sec: i64,
    tv_nsec: i64,
}

unsafe extern "C" {
    fn nanosleep(req: *const TimeSpec, rem: *mut TimeSpec) -> i32;
}
