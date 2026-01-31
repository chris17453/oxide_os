//! Syslog API for OXIDE OS
//!
//! Provides structured logging to /dev/kmsg from userspace.
//!
//! Usage:
//!   openlog("myapp");
//!   syslog(LOG_INFO, "Service started");
//!   closelog();

use crate::fcntl::O_WRONLY;
use crate::unistd::{close, open2, write};

/// Syslog priority levels
pub const LOG_EMERG: u8 = 0;
pub const LOG_ALERT: u8 = 1;
pub const LOG_CRIT: u8 = 2;
pub const LOG_ERR: u8 = 3;
pub const LOG_WARNING: u8 = 4;
pub const LOG_NOTICE: u8 = 5;
pub const LOG_INFO: u8 = 6;
pub const LOG_DEBUG: u8 = 7;

/// Maximum tag length
const MAX_TAG: usize = 32;

/// Maximum formatted message size
const MAX_MSG: usize = 512;

/// Cached file descriptor for /dev/kmsg
static mut KMSG_FD: i32 = -1;

/// Cached tag
static mut TAG: [u8; MAX_TAG] = [0; MAX_TAG];
static mut TAG_LEN: usize = 0;

/// Open the syslog connection with a tag
///
/// The tag identifies the source of log messages (e.g. "sshd", "networkd").
/// Opens /dev/kmsg for writing and caches the file descriptor.
pub fn openlog(tag: &str) {
    unsafe {
        // Close any existing connection
        if KMSG_FD >= 0 {
            close(KMSG_FD);
        }

        // Cache the tag
        let len = tag.len().min(MAX_TAG);
        TAG[..len].copy_from_slice(&tag.as_bytes()[..len]);
        TAG_LEN = len;

        // Open /dev/kmsg for writing
        KMSG_FD = open2("/dev/kmsg", O_WRONLY);
    }
}

/// Log a message with the given priority
///
/// Formats and writes a structured log entry to /dev/kmsg.
/// Format sent: `<priority>,<tag>;<message>\n`
///
/// If openlog() hasn't been called, this will open /dev/kmsg automatically
/// with an empty tag.
pub fn syslog(priority: u8, msg: &str) {
    unsafe {
        // Check if this priority is enabled in the mask
        if LOG_MASK & (1 << (priority & 7)) == 0 {
            return;
        }

        // Auto-open if needed
        if KMSG_FD < 0 {
            KMSG_FD = open2("/dev/kmsg", O_WRONLY);
            if KMSG_FD < 0 {
                return; // Can't log
            }
        }

        let mut buf = [0u8; MAX_MSG];
        let mut pos = 0;

        // Priority digit
        let p = if priority > 7 { 7 } else { priority };
        buf[pos] = p + b'0';
        pos += 1;

        // Comma
        buf[pos] = b',';
        pos += 1;

        // Tag
        let tag_len = TAG_LEN.min(buf.len() - pos - 2);
        buf[pos..pos + tag_len].copy_from_slice(&TAG[..tag_len]);
        pos += tag_len;

        // Semicolon
        buf[pos] = b';';
        pos += 1;

        // Message
        let msg_bytes = msg.as_bytes();
        let msg_len = msg_bytes.len().min(buf.len() - pos - 1);
        buf[pos..pos + msg_len].copy_from_slice(&msg_bytes[..msg_len]);
        pos += msg_len;

        // Newline
        buf[pos] = b'\n';
        pos += 1;

        write(KMSG_FD, &buf[..pos]);
    }
}

/// Close the syslog connection
///
/// Closes the cached /dev/kmsg file descriptor and clears the tag.
pub fn closelog() {
    unsafe {
        if KMSG_FD >= 0 {
            close(KMSG_FD);
            KMSG_FD = -1;
        }
        TAG_LEN = 0;
    }
}

/// Log mask — each bit corresponds to a priority level.
/// Default 0xFF means all priorities enabled.
static mut LOG_MASK: i32 = 0xFF;

/// Set the log priority mask.
///
/// If `mask` is non-zero, sets the new mask and returns the previous one.
/// If `mask` is zero, returns the current mask without changing it.
/// Bit N corresponds to priority level N (e.g., bit 6 = LOG_INFO).
pub fn setlogmask(mask: i32) -> i32 {
    unsafe {
        let old = LOG_MASK;
        if mask != 0 {
            LOG_MASK = mask;
        }
        old
    }
}

/// Check if a priority level is enabled in the current mask
pub fn priority_enabled(priority: u8) -> bool {
    unsafe { LOG_MASK & (1 << (priority & 7)) != 0 }
}
