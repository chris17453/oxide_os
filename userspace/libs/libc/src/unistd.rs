//! POSIX unistd functions
//!
//! Standard UNIX functions like read, write, fork, exec, etc.

use crate::fcntl::*;
use crate::syscall;

// TTY ioctls
const TIOCGPGRP: u64 = 0x540F;
const TIOCSPGRP: u64 = 0x5410;

/// Write bytes to file descriptor
pub fn write(fd: i32, buf: &[u8]) -> isize {
    syscall::sys_write(fd, buf)
}

/// Read bytes from file descriptor
/// — GraveShift: auto-retry on EINTR. Signal delivery during a blocking read
/// makes the kernel return -EINTR. Without retry, every read loop in every
/// userspace program silently breaks when signals arrive. Linux libc retries
/// by default (SA_RESTART), but we don't have that luxury yet.
pub fn read(fd: i32, buf: &mut [u8]) -> isize {
    loop {
        let r = syscall::sys_read(fd, buf);
        if r != -4 { return r; } // -4 = EINTR, retry
    }
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
    // Provide argv[0] so exec receives a valid argument vector
    let mut argv0_buf = [0u8; 256];
    let path_bytes = path.as_bytes();
    let copy_len = path_bytes.len().min(argv0_buf.len() - 1);
    argv0_buf[..copy_len].copy_from_slice(&path_bytes[..copy_len]);
    argv0_buf[copy_len] = 0;

    let argv: [*const u8; 2] = [argv0_buf.as_ptr(), core::ptr::null()];
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
    waitpid(-1, status, 0)
}

/// Wait for specific child
///
/// If options does not include WNOHANG and kernel returns EAGAIN,
/// we yield and retry. This allows the scheduler to run child processes.
pub fn waitpid(pid: i32, status: &mut i32, options: i32) -> i32 {
    const WNOHANG: i32 = 1;
    const EAGAIN: i32 = -11;
    const EINTR: i32 = -4;

    loop {
        let result = syscall::sys_waitpid(pid, status, options);

        // — GraveShift: retry on both EAGAIN (child not yet exited) and EINTR
        // (interrupted by signal). Without the EINTR retry, any signal delivery
        // — like SIGCHLD from a grandchild — causes waitpid to bail early,
        // making login think the shell exited when it's still running.
        if (options & WNOHANG) != 0 || (result != EAGAIN && result != EINTR) {
            return result;
        }

        // Yield to scheduler then retry
        // This brief return to usermode allows timer interrupt to
        // preempt us and schedule child processes
        syscall::sys_sched_yield();
    }
}

/// Yield the processor to other processes
pub fn sched_yield() {
    syscall::sys_sched_yield();
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

/// Exit process (flushes stdio then exits)
/// 🔥 GraveShift: Proper exit - flush buffers before terminating 🔥
pub fn exit(status: i32) -> ! {
    // Flush all stdio buffers before exiting
    crate::stdio::fflush_all();
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

/// Get foreground process group of TTY
pub fn tcgetpgrp(fd: i32) -> i32 {
    let mut pgid: i32 = 0;
    if syscall::sys_ioctl(fd, TIOCGPGRP, &mut pgid as *mut i32 as u64) < 0 {
        return -1;
    }
    pgid
}

/// Set foreground process group of TTY
pub fn tcsetpgrp(fd: i32, pgid: i32) -> i32 {
    syscall::sys_ioctl(fd, TIOCSPGRP, &pgid as *const i32 as u64)
}

/// Set process group (POSIX setpgrp = setpgid(0, 0))
pub fn setpgrp() -> i32 {
    setpgid(0, 0)
}

/// Truncate file to specified length
pub fn ftruncate(fd: i32, length: i64) -> i32 {
    syscall::sys_ftruncate(fd, length)
}

/// Truncate a file by path to specified length
pub fn truncate(path: &str, length: i64) -> i32 {
    let fd = open(path, crate::fcntl::O_WRONLY, 0);
    if fd < 0 {
        return fd;
    }
    let result = ftruncate(fd, length);
    close(fd);
    result
}

/// Get hostname
///
/// Reads the hostname from uname().nodename.
pub fn gethostname(buf: &mut [u8]) -> i32 {
    let mut uts = syscall::UtsName::new();
    let r = syscall::uname(&mut uts);
    if r != 0 {
        return -1;
    }
    // Copy nodename to buf
    let name = &uts.nodename;
    let len = name.iter().position(|&c| c == 0).unwrap_or(name.len());
    let copy_len = len.min(buf.len().saturating_sub(1));
    buf[..copy_len].copy_from_slice(&name[..copy_len]);
    if copy_len < buf.len() {
        buf[copy_len] = 0;
    }
    0
}

/// Check if a file descriptor refers to a terminal
pub fn isatty(fd: i32) -> i32 {
    // Use TIOCGPGRP as a simple TTY test - it returns -ENOTTY on non-TTYs
    let mut pgid: i32 = 0;
    if syscall::sys_ioctl(fd, TIOCGPGRP, &mut pgid as *mut i32 as u64) >= 0 {
        1
    } else {
        0
    }
}

/// Get name of terminal associated with file descriptor
///
/// Returns a pointer to a static buffer containing the terminal name.
/// Returns null pointer if fd is not a terminal.
pub fn ttyname(fd: i32) -> *const u8 {
    if isatty(fd) == 0 {
        return core::ptr::null();
    }
    // Our kernel uses /dev/console as the primary TTY
    b"/dev/console\0".as_ptr()
}

/// Check file accessibility
pub fn access(path: &str, mode: i32) -> i32 {
    // Use stat to check if file exists and permissions
    let mut st = crate::stat::Stat::zeroed();
    let r = crate::stat::stat(path, &mut st);
    if r < 0 {
        return r;
    }

    // F_OK (0) = existence check, already passed
    if mode == 0 {
        return 0;
    }

    // For now, grant all access if file exists (we don't have full permission checking)
    0
}

/// F_OK - test for existence
pub const F_OK: i32 = 0;
/// R_OK - test for read permission
pub const R_OK: i32 = 4;
/// W_OK - test for write permission
pub const W_OK: i32 = 2;
/// X_OK - test for execute permission
pub const X_OK: i32 = 1;

/// Get login name of current user
///
/// Returns "root" since OXIDE currently always runs as root.
pub fn getlogin() -> *const u8 {
    b"root\0".as_ptr()
}

/// Get login name into buffer
pub fn getlogin_r(buf: &mut [u8]) -> i32 {
    let name = b"root";
    if buf.len() < name.len() + 1 {
        return -34; // ERANGE
    }
    buf[..name.len()].copy_from_slice(name);
    buf[name.len()] = 0;
    0
}

/// Execute a shell command
///
/// Forks a child process and execs the shell with "-c" and the command.
pub fn system(command: &str) -> i32 {
    if command.is_empty() {
        // Check if shell is available
        let fd = open("/bin/esh", 0, 0);
        if fd >= 0 {
            close(fd);
            return 1; // Shell available
        }
        return 0; // No shell
    }

    let child = fork();
    if child == 0 {
        // Child - exec shell
        let mut cmd_buf = [0u8; 4096];
        let cmd_bytes = command.as_bytes();
        let copy_len = cmd_bytes.len().min(cmd_buf.len() - 1);
        cmd_buf[..copy_len].copy_from_slice(&cmd_bytes[..copy_len]);
        cmd_buf[copy_len] = 0;

        let argv: [*const u8; 4] = [
            b"/bin/esh\0".as_ptr(),
            b"-c\0".as_ptr(),
            cmd_buf.as_ptr(),
            core::ptr::null(),
        ];
        syscall::sys_execve("/bin/esh", argv.as_ptr(), core::ptr::null());
        _exit(127);
    } else if child > 0 {
        let mut status: i32 = 0;
        waitpid(child, &mut status, 0);
        return status;
    }
    -1 // fork failed
}

/// Get configurable pathname variable
///
/// Returns a value for the given name, or -1 if not supported.
pub fn pathconf(_path: &str, name: i32) -> i64 {
    fpathconf(-1, name)
}

/// Get configurable file descriptor variable
pub fn fpathconf(_fd: i32, name: i32) -> i64 {
    match name {
        0 => 255,  // _PC_LINK_MAX
        1 => 14,   // _PC_MAX_CANON
        2 => 255,  // _PC_MAX_INPUT
        3 => 255,  // _PC_NAME_MAX
        4 => 4096, // _PC_PATH_MAX
        5 => 512,  // _PC_PIPE_BUF
        6 => 1,    // _PC_CHOWN_RESTRICTED
        7 => 1,    // _PC_NO_TRUNC
        8 => 1,    // _PC_VDISABLE
        _ => -1,
    }
}

/// Get system configuration value
pub fn sysconf(name: i32) -> i64 {
    match name {
        0 => 4096,  // _SC_ARG_MAX
        1 => -1,    // _SC_CHILD_MAX (no limit)
        2 => 100,   // _SC_CLK_TCK (timer frequency)
        3 => 20,    // _SC_NGROUPS_MAX
        4 => -1,    // _SC_OPEN_MAX
        6 => 4096,  // _SC_PAGESIZE / _SC_PAGE_SIZE
        8 => 1,     // _SC_VERSION (POSIX.1)
        29 => 4096, // _SC_PAGESIZE (alternate)
        30 => 4096, // _SC_PAGE_SIZE
        58 => 1,    // _SC_NPROCESSORS_ONLN
        84 => 1,    // _SC_NPROCESSORS_CONF
        _ => -1,
    }
}

/// Resolve a pathname to an absolute path with no `.`, `..`, or symlinks
///
/// If `resolved` is null, allocates a buffer. Otherwise writes to `resolved`
/// which must be at least 4096 bytes.
pub fn realpath(path: &str, resolved: &mut [u8]) -> i32 {
    if path.is_empty() {
        return -22; // EINVAL
    }

    // Start with absolute path
    let mut buf = [0u8; 4096];
    let mut pos = 0;

    if !path.starts_with('/') {
        // Relative path - prepend cwd
        let cwd_len = getcwd(&mut buf);
        if cwd_len < 0 {
            return cwd_len;
        }
        pos = buf.iter().position(|&c| c == 0).unwrap_or(cwd_len as usize);
        if pos > 0 && buf[pos - 1] != b'/' {
            buf[pos] = b'/';
            pos += 1;
        }
    }

    // Append the path
    let path_bytes = path.as_bytes();
    let copy_len = path_bytes.len().min(buf.len() - pos - 1);
    buf[pos..pos + copy_len].copy_from_slice(&path_bytes[..copy_len]);
    pos += copy_len;
    buf[pos] = 0;

    // Normalize: resolve . and ..
    let mut result = [0u8; 4096];
    let mut rpos = 0;

    let mut i = 0;
    while i < pos {
        if buf[i] == b'/' {
            // Skip duplicate slashes
            if rpos > 0 && result[rpos - 1] == b'/' {
                i += 1;
                continue;
            }
            result[rpos] = b'/';
            rpos += 1;
            i += 1;

            // Check for . or ..
            if i < pos && buf[i] == b'.' {
                if i + 1 >= pos || buf[i + 1] == b'/' || buf[i + 1] == 0 {
                    // "." - skip it (remove the trailing slash too)
                    i += 1;
                    if rpos > 1 {
                        rpos -= 1; // Remove the slash we just added (unless it's root)
                    }
                    continue;
                }
                if buf[i + 1] == b'.' && (i + 2 >= pos || buf[i + 2] == b'/' || buf[i + 2] == 0) {
                    // ".." - go up one directory
                    i += 2;
                    // Remove trailing slash
                    if rpos > 1 {
                        rpos -= 1;
                    }
                    // Go back to previous /
                    while rpos > 1 && result[rpos - 1] != b'/' {
                        rpos -= 1;
                    }
                    continue;
                }
            }
        } else {
            result[rpos] = buf[i];
            rpos += 1;
            i += 1;
        }
    }

    // Ensure we have at least "/"
    if rpos == 0 {
        result[0] = b'/';
        rpos = 1;
    }
    // Remove trailing slash (unless it's root)
    if rpos > 1 && result[rpos - 1] == b'/' {
        rpos -= 1;
    }
    result[rpos] = 0;

    // Verify the path exists
    let mut st = crate::stat::Stat::zeroed();
    let path_str = unsafe { core::str::from_utf8_unchecked(&result[..rpos]) };
    let r = crate::stat::stat(path_str, &mut st);
    if r < 0 {
        return r; // Path doesn't exist
    }

    // Copy to output
    let copy_len = (rpos + 1).min(resolved.len());
    resolved[..copy_len].copy_from_slice(&result[..copy_len]);
    0
}
