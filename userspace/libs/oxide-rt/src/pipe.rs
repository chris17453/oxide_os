//! Pipe syscall wrapper — unidirectional data channels.
//!
//! — SableWire: Two fds, one direction. The original IPC mechanism.
//! Shell pipelines, process communication, and tears.

use crate::syscall::*;
use crate::nr;

/// pipe — create a unidirectional pipe
/// On success, pipefd[0] is read end, pipefd[1] is write end
pub fn pipe(pipefd: &mut [i32; 2]) -> i32 {
    syscall1(nr::PIPE, pipefd.as_mut_ptr() as usize) as i32
}

/// pipe2 — create pipe with flags (O_CLOEXEC, O_NONBLOCK)
pub fn pipe2(pipefd: &mut [i32; 2], flags: i32) -> i32 {
    syscall2(nr::PIPE2, pipefd.as_mut_ptr() as usize, flags as usize) as i32
}
