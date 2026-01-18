//! POSIX unistd functions
//!
//! Standard UNIX functions like read, write, fork, exec, etc.

use crate::syscall;
use crate::fcntl::*;

/// Write bytes to file descriptor
pub fn write(fd: i32, buf: &[u8]) -> isize {
    syscall::sys_write(fd, buf)
}

/// Read bytes from file descriptor
pub fn read(fd: i32, buf: &mut [u8]) -> isize {
    syscall::sys_read(fd, buf)
}

/// Open file
pub fn open(path: &str, flags: u32, mode: u32) -> i32 {
    syscall::sys_open(path, flags, mode)
}

/// Close file descriptor
pub fn close(fd: i32) -> i32 {
    syscall::sys_close(fd)
}

/// Create child process
pub fn fork() -> i32 {
    syscall::sys_fork()
}

/// Execute program
pub fn exec(path: &str) -> i32 {
    syscall::sys_exec(path)
}

/// Wait for any child
pub fn wait(status: &mut i32) -> i32 {
    syscall::sys_wait(status)
}

/// Wait for specific child
pub fn waitpid(pid: i32, status: &mut i32, options: i32) -> i32 {
    syscall::sys_waitpid(pid, status, options)
}

/// Get process ID
pub fn getpid() -> i32 {
    syscall::sys_getpid()
}

/// Get parent process ID
pub fn getppid() -> i32 {
    syscall::sys_getppid()
}

/// Duplicate file descriptor
pub fn dup(fd: i32) -> i32 {
    syscall::sys_dup(fd)
}

/// Duplicate file descriptor to specific number
pub fn dup2(oldfd: i32, newfd: i32) -> i32 {
    syscall::sys_dup2(oldfd, newfd)
}

/// Exit process
pub fn _exit(status: i32) -> ! {
    syscall::sys_exit(status)
}

/// Print string to stdout
pub fn puts(s: &str) {
    write(STDOUT_FILENO, s.as_bytes());
}

/// Print to stderr
pub fn eputs(s: &str) {
    write(STDERR_FILENO, s.as_bytes());
}

/// Wait options
pub const WNOHANG: i32 = 1;
pub const WUNTRACED: i32 = 2;
pub const WCONTINUED: i32 = 8;

/// Check if child exited normally
pub fn wifexited(status: i32) -> bool {
    (status & 0x7F) == 0
}

/// Get exit status
pub fn wexitstatus(status: i32) -> i32 {
    (status >> 8) & 0xFF
}

/// Check if child was signaled
pub fn wifsignaled(status: i32) -> bool {
    ((status & 0x7F) + 1) >> 1 > 0
}

/// Get signal that killed child
pub fn wtermsig(status: i32) -> i32 {
    status & 0x7F
}

/// Check if child stopped
pub fn wifstopped(status: i32) -> bool {
    (status & 0xFF) == 0x7F
}

/// Get signal that stopped child
pub fn wstopsig(status: i32) -> i32 {
    (status >> 8) & 0xFF
}
