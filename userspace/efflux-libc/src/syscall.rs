//! System call interface
//!
//! Provides syscall wrappers for EFFLUX userspace.
//! Architecture-specific raw syscall implementations are in arch/.

// Re-export arch-specific raw syscall functions
pub use crate::arch::syscall::{syscall0, syscall1, syscall2, syscall3, syscall4, syscall5, syscall6};

/// Syscall numbers (must match kernel)
pub mod nr {
    pub const EXIT: u64 = 0;
    pub const WRITE: u64 = 1;
    pub const READ: u64 = 2;
    pub const FORK: u64 = 3;
    pub const EXEC: u64 = 4;
    pub const WAIT: u64 = 5;
    pub const WAITPID: u64 = 6;
    pub const GETPID: u64 = 7;
    pub const GETPPID: u64 = 8;
    pub const SETPGID: u64 = 9;
    pub const GETPGID: u64 = 10;
    pub const SETSID: u64 = 11;
    pub const GETSID: u64 = 12;
    pub const OPEN: u64 = 20;
    pub const CLOSE: u64 = 21;
    pub const LSEEK: u64 = 22;
    pub const FSTAT: u64 = 23;
    pub const STAT: u64 = 24;
    pub const DUP: u64 = 25;
    pub const DUP2: u64 = 26;
    pub const FTRUNCATE: u64 = 27;
    pub const MKDIR: u64 = 30;
    pub const RMDIR: u64 = 31;
    pub const UNLINK: u64 = 32;
    pub const RENAME: u64 = 33;
    pub const GETDENTS: u64 = 34;
    pub const CHDIR: u64 = 35;
    pub const GETCWD: u64 = 36;
    pub const PIPE: u64 = 37;
    pub const IOCTL: u64 = 40;
    pub const KILL: u64 = 50;
    pub const SIGACTION: u64 = 51;
    pub const SIGPROCMASK: u64 = 52;
    pub const SIGPENDING: u64 = 53;
    pub const SIGSUSPEND: u64 = 54;
    pub const PAUSE: u64 = 55;
    pub const SIGRETURN: u64 = 56;
    // Time syscalls
    pub const GETTIMEOFDAY: u64 = 60;
    pub const CLOCK_GETTIME: u64 = 61;
    pub const CLOCK_GETRES: u64 = 62;
    pub const NANOSLEEP: u64 = 63;
    // Poll/select syscalls
    pub const POLL: u64 = 70;
    pub const PPOLL: u64 = 71;
    pub const SELECT: u64 = 72;
    pub const PSELECT6: u64 = 73;
    // Directory syscalls
    pub const GETDENTS64: u64 = 80;
    // User/group syscalls
    pub const GETUID: u64 = 100;
    pub const GETEUID: u64 = 101;
    pub const GETGID: u64 = 102;
    pub const GETEGID: u64 = 103;
    pub const SETUID: u64 = 104;
    pub const SETGID: u64 = 105;
    pub const SETEUID: u64 = 106;
    pub const SETEGID: u64 = 107;
    // Memory syscalls
    pub const MMAP: u64 = 110;
    pub const MUNMAP: u64 = 111;
    pub const MPROTECT: u64 = 112;
    pub const BRK: u64 = 113;
}

// Re-export syscall numbers at module level for convenience
pub use nr::OPEN as SYS_OPEN;
pub use nr::CLOSE as SYS_CLOSE;
pub use nr::READ as SYS_READ;
pub use nr::WRITE as SYS_WRITE;
pub use nr::LSEEK as SYS_LSEEK;
pub use nr::IOCTL as SYS_IOCTL;
pub use nr::GETDENTS as SYS_GETDENTS;
pub use nr::GETTIMEOFDAY as SYS_GETTIMEOFDAY;
pub use nr::CLOCK_GETTIME as SYS_CLOCK_GETTIME;
pub use nr::CLOCK_GETRES as SYS_CLOCK_GETRES;
pub use nr::NANOSLEEP as SYS_NANOSLEEP;
pub use nr::POLL as SYS_POLL;
pub use nr::PPOLL as SYS_PPOLL;
pub use nr::SELECT as SYS_SELECT;
pub use nr::PSELECT6 as SYS_PSELECT6;
pub use nr::GETDENTS64 as SYS_GETDENTS64;
pub use nr::GETUID as SYS_GETUID;
pub use nr::GETEUID as SYS_GETEUID;
pub use nr::GETGID as SYS_GETGID;
pub use nr::GETEGID as SYS_GETEGID;
pub use nr::SETUID as SYS_SETUID;
pub use nr::SETGID as SYS_SETGID;
pub use nr::SETEUID as SYS_SETEUID;
pub use nr::SETEGID as SYS_SETEGID;
pub use nr::MMAP as SYS_MMAP;
pub use nr::MUNMAP as SYS_MUNMAP;
pub use nr::MPROTECT as SYS_MPROTECT;
pub use nr::BRK as SYS_BRK;

// ============================================================================
// High-level syscall wrappers (architecture-independent)
// ============================================================================

/// sys_exit - Terminate process
pub fn sys_exit(status: i32) -> ! {
    syscall1(nr::EXIT, status as usize);
    loop {}
}

/// sys_write - Write to file descriptor
pub fn sys_write(fd: i32, buf: &[u8]) -> isize {
    syscall3(nr::WRITE, fd as usize, buf.as_ptr() as usize, buf.len()) as isize
}

/// sys_read - Read from file descriptor
pub fn sys_read(fd: i32, buf: &mut [u8]) -> isize {
    syscall3(nr::READ, fd as usize, buf.as_mut_ptr() as usize, buf.len()) as isize
}

/// sys_open - Open file
pub fn sys_open(path: &str, flags: u32, mode: u32) -> i32 {
    syscall4(nr::OPEN, path.as_ptr() as usize, path.len(), flags as usize, mode as usize) as i32
}

/// sys_close - Close file descriptor
pub fn sys_close(fd: i32) -> i32 {
    syscall1(nr::CLOSE, fd as usize) as i32
}

/// sys_fork - Create child process
pub fn sys_fork() -> i32 {
    syscall0(nr::FORK) as i32
}

/// sys_exec - Execute new program
pub fn sys_exec(path: &str) -> i32 {
    syscall2(nr::EXEC, path.as_ptr() as usize, path.len()) as i32
}

/// sys_wait - Wait for any child
pub fn sys_wait(status: &mut i32) -> i32 {
    syscall1(nr::WAIT, status as *mut i32 as usize) as i32
}

/// sys_waitpid - Wait for specific child
pub fn sys_waitpid(pid: i32, status: &mut i32, options: i32) -> i32 {
    syscall3(nr::WAITPID, pid as usize, status as *mut i32 as usize, options as usize) as i32
}

/// sys_getpid - Get process ID
pub fn sys_getpid() -> i32 {
    syscall0(nr::GETPID) as i32
}

/// sys_getppid - Get parent process ID
pub fn sys_getppid() -> i32 {
    syscall0(nr::GETPPID) as i32
}

/// sys_kill - Send signal to process
pub fn sys_kill(pid: i32, sig: i32) -> i32 {
    syscall2(nr::KILL, pid as usize, sig as usize) as i32
}

/// sys_dup - Duplicate file descriptor
pub fn sys_dup(fd: i32) -> i32 {
    syscall1(nr::DUP, fd as usize) as i32
}

/// sys_dup2 - Duplicate file descriptor to specific fd
pub fn sys_dup2(oldfd: i32, newfd: i32) -> i32 {
    syscall2(nr::DUP2, oldfd as usize, newfd as usize) as i32
}

/// sys_mkdir - Create directory
pub fn sys_mkdir(path: &str, mode: u32) -> i32 {
    syscall3(nr::MKDIR, path.as_ptr() as usize, path.len(), mode as usize) as i32
}

/// sys_rmdir - Remove directory
pub fn sys_rmdir(path: &str) -> i32 {
    syscall2(nr::RMDIR, path.as_ptr() as usize, path.len()) as i32
}

/// sys_unlink - Remove file
pub fn sys_unlink(path: &str) -> i32 {
    syscall2(nr::UNLINK, path.as_ptr() as usize, path.len()) as i32
}

/// sys_getdents - Read directory entries
pub fn sys_getdents(fd: i32, buf: &mut [u8]) -> i32 {
    syscall3(nr::GETDENTS, fd as usize, buf.as_mut_ptr() as usize, buf.len()) as i32
}

/// sys_ioctl - Device control
pub fn sys_ioctl(fd: i32, request: u64, arg: u64) -> i32 {
    syscall3(nr::IOCTL, fd as usize, request as usize, arg as usize) as i32
}

/// sys_chdir - Change working directory
pub fn sys_chdir(path: &str) -> i32 {
    syscall2(nr::CHDIR, path.as_ptr() as usize, path.len()) as i32
}

/// sys_getcwd - Get current working directory
pub fn sys_getcwd(buf: &mut [u8]) -> i32 {
    syscall2(nr::GETCWD, buf.as_mut_ptr() as usize, buf.len()) as i32
}

/// sys_pipe - Create pipe
pub fn sys_pipe(pipefd: &mut [i32; 2]) -> i32 {
    syscall1(nr::PIPE, pipefd.as_mut_ptr() as usize) as i32
}

/// sys_lseek - Seek in file
pub fn sys_lseek(fd: i32, offset: i64, whence: i32) -> i64 {
    syscall3(nr::LSEEK, fd as usize, offset as usize, whence as usize)
}

/// sys_setsid - Create new session
pub fn sys_setsid() -> i32 {
    syscall0(nr::SETSID) as i32
}

/// sys_setpgid - Set process group
pub fn sys_setpgid(pid: i32, pgid: i32) -> i32 {
    syscall2(nr::SETPGID, pid as usize, pgid as usize) as i32
}

/// sys_getpgid - Get process group
pub fn sys_getpgid(pid: i32) -> i32 {
    syscall1(nr::GETPGID, pid as usize) as i32
}
