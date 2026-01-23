//! POSIX unistd functions
//!
//! Standard UNIX functions like read, write, fork, exec, etc.

use crate::fcntl::*;
use crate::syscall;

/// Write bytes to file descriptor
pub fn write(fd: i32, buf: &[u8]) -> isize {
    syscall::sys_write(fd, buf)
}

/// Read bytes from file descriptor
pub fn read(fd: i32, buf: &mut [u8]) -> isize {
    syscall::sys_read(fd, buf)
}

/// Open file with mode
pub fn open(path: &str, flags: u32, mode: u32) -> i32 {
    syscall::sys_open(path, flags, mode)
}

/// Open file without mode (uses 0 as default)
pub fn open2(path: &str, flags: u32) -> i32 {
    syscall::sys_open(path, flags, 0)
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
    // Provide argv[0] to match expected user stack layout
    let argv: [*const u8; 2] = [path.as_ptr(), core::ptr::null()];
    syscall::sys_execve(path, argv.as_ptr(), core::ptr::null())
}

/// Execute program with arguments (NULL-terminated argv array)
/// argv[0] should be the program name, argv[argc] must be NULL
pub fn execv(path: &str, argv: *const *const u8) -> i32 {
    syscall::sys_execve(path, argv, core::ptr::null())
}

/// Execute program with arguments and environment
pub fn execve(path: &str, argv: *const *const u8, envp: *const *const u8) -> i32 {
    syscall::sys_execve(path, argv, envp)
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

/// Exit process (alias for _exit)
pub fn exit(status: i32) -> ! {
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

/// Create a pipe
///
/// Creates a pair of file descriptors: pipefd[0] for reading, pipefd[1] for writing.
pub fn pipe(pipefd: &mut [i32; 2]) -> i32 {
    syscall::sys_pipe(pipefd)
}

/// Change current working directory
pub fn chdir(path: &str) -> i32 {
    syscall::sys_chdir(path)
}

/// Get current working directory
///
/// Returns the length of the path on success, -1 on error.
pub fn getcwd(buf: &mut [u8]) -> i32 {
    syscall::sys_getcwd(buf)
}

/// Seek to position in file
pub fn lseek(fd: i32, offset: i64, whence: i32) -> i64 {
    syscall::sys_lseek(fd, offset, whence)
}

/// Create a new session
pub fn setsid() -> i32 {
    syscall::sys_setsid()
}

/// Set process group
pub fn setpgid(pid: i32, pgid: i32) -> i32 {
    syscall::sys_setpgid(pid, pgid)
}

/// Get process group
pub fn getpgid(pid: i32) -> i32 {
    syscall::sys_getpgid(pid)
}

/// Seek constants
pub const SEEK_SET: i32 = 0;
pub const SEEK_CUR: i32 = 1;
pub const SEEK_END: i32 = 2;
