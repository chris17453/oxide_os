//! Poll syscall wrapper — multiplexed I/O notification.
//!
//! — SableWire: poll() lets you wait on multiple file descriptors at once.
//! std's net implementation uses this for timeouts and non-blocking I/O.

use crate::syscall::*;
use crate::nr;
use crate::types::PollFd;

/// poll — wait for events on file descriptors
/// Returns number of fds with events, 0 on timeout, negative on error.
pub fn poll(fds: &mut [PollFd], timeout_ms: i32) -> i32 {
    syscall3(
        nr::POLL,
        fds.as_mut_ptr() as usize,
        fds.len(),
        timeout_ms as usize,
    ) as i32
}
