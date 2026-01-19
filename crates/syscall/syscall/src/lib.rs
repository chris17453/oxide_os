//! System call handlers for EFFLUX
//!
//! Provides the syscall dispatch table and handlers.

#![no_std]

extern crate alloc;

pub mod vfs;
pub mod dir;
pub mod signal;
pub mod socket;

use os_core::VirtAddr;
use proc::process_table;
use proc_traits::Pid;

/// Syscall numbers
pub mod nr {
    // Process syscalls
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
    pub const EXECVE: u64 = 13;  // exec with argv/envp
    pub const GETUID: u64 = 14;
    pub const GETGID: u64 = 15;
    pub const GETEUID: u64 = 16;
    pub const GETEGID: u64 = 17;
    pub const SETUID: u64 = 18;
    pub const SETGID: u64 = 19;

    // VFS syscalls
    pub const OPEN: u64 = 20;
    pub const CLOSE: u64 = 21;
    pub const LSEEK: u64 = 22;
    pub const FSTAT: u64 = 23;
    pub const STAT: u64 = 24;
    pub const DUP: u64 = 25;
    pub const DUP2: u64 = 26;
    pub const FTRUNCATE: u64 = 27;

    // Directory syscalls
    pub const MKDIR: u64 = 30;
    pub const RMDIR: u64 = 31;
    pub const UNLINK: u64 = 32;
    pub const RENAME: u64 = 33;
    pub const GETDENTS: u64 = 34;
    pub const CHDIR: u64 = 35;
    pub const GETCWD: u64 = 36;
    pub const PIPE: u64 = 37;

    // TTY/device syscalls
    pub const IOCTL: u64 = 40;

    // Module syscalls
    pub const INIT_MODULE: u64 = 60;
    pub const DELETE_MODULE: u64 = 61;
    pub const QUERY_MODULE: u64 = 62;

    // Signal syscalls
    pub const KILL: u64 = 50;
    pub const SIGACTION: u64 = 51;
    pub const SIGPROCMASK: u64 = 52;
    pub const SIGPENDING: u64 = 53;
    pub const SIGSUSPEND: u64 = 54;
    pub const PAUSE: u64 = 55;
    pub const SIGRETURN: u64 = 56;

    // Socket syscalls
    pub const SOCKET: u64 = 70;
    pub const BIND: u64 = 71;
    pub const LISTEN: u64 = 72;
    pub const ACCEPT: u64 = 73;
    pub const CONNECT: u64 = 74;
    pub const SEND: u64 = 75;
    pub const RECV: u64 = 76;
    pub const SENDTO: u64 = 77;
    pub const RECVFROM: u64 = 78;
    pub const SHUTDOWN: u64 = 79;
    pub const GETSOCKNAME: u64 = 80;
    pub const GETPEERNAME: u64 = 81;
    pub const SETSOCKOPT: u64 = 82;
    pub const GETSOCKOPT: u64 = 83;
}

/// Error codes (negative return values)
pub mod errno {
    pub const ENOSYS: i64 = -38;    // Function not implemented
    pub const EBADF: i64 = -9;      // Bad file descriptor
    pub const EFAULT: i64 = -14;    // Bad address
    pub const EINVAL: i64 = -22;    // Invalid argument
    pub const ENOMEM: i64 = -12;    // Out of memory
    pub const ESRCH: i64 = -3;      // No such process
    pub const ECHILD: i64 = -10;    // No child processes
    pub const EAGAIN: i64 = -11;    // Resource temporarily unavailable
    pub const EPERM: i64 = -1;      // Operation not permitted
    pub const ENOENT: i64 = -2;     // No such file or directory
    pub const EEXIST: i64 = -17;    // File exists
    pub const ENOTDIR: i64 = -20;   // Not a directory
    pub const EISDIR: i64 = -21;    // Is a directory
    pub const ENOTEMPTY: i64 = -39; // Directory not empty
    pub const ENOSPC: i64 = -28;    // No space left on device
    pub const EROFS: i64 = -30;     // Read-only file system
    pub const ENOTTY: i64 = -25;    // Not a typewriter (inappropriate ioctl)
    pub const EINTR: i64 = -4;      // Interrupted system call
    pub const ERANGE: i64 = -34;    // Result too large
    pub const EMFILE: i64 = -24;    // Too many open files

    // Socket errors
    pub const ENOTSOCK: i64 = -88;      // Socket operation on non-socket
    pub const EADDRINUSE: i64 = -98;    // Address already in use
    pub const EADDRNOTAVAIL: i64 = -99; // Cannot assign requested address
    pub const ENETUNREACH: i64 = -101;  // Network is unreachable
    pub const ECONNABORTED: i64 = -103; // Connection aborted
    pub const ECONNRESET: i64 = -104;   // Connection reset by peer
    pub const ENOBUFS: i64 = -105;      // No buffer space available
    pub const EISCONN: i64 = -106;      // Transport endpoint is already connected
    pub const ENOTCONN: i64 = -107;     // Transport endpoint is not connected
    pub const ETIMEDOUT: i64 = -110;    // Connection timed out
    pub const ECONNREFUSED: i64 = -111; // Connection refused
    pub const EHOSTUNREACH: i64 = -113; // No route to host
    pub const EALREADY: i64 = -114;     // Operation already in progress
    pub const EINPROGRESS: i64 = -115;  // Operation now in progress
}

/// Console output callback type
pub type ConsoleWriteFn = fn(&[u8]);

/// Console input callback type (returns bytes read, or 0 if no data)
pub type ConsoleReadFn = fn(&mut [u8]) -> usize;

/// Exit callback type
pub type ExitFn = fn(i32) -> !;

/// Fork callback type - returns child PID to parent, 0 to child, or negative error
pub type ForkFn = fn() -> i64;

/// Exec callback type - path, argv, envp; returns error code (doesn't return on success)
/// Arguments:
/// - path: pointer to path string
/// - path_len: length of path
/// - argv: pointer to null-terminated array of string pointers
/// - envp: pointer to null-terminated array of string pointers
pub type ExecFn = fn(*const u8, usize, *const *const u8, *const *const u8) -> i64;

/// Wait callback type - returns (child_pid, status) packed as (pid << 32) | (status & 0xFFFFFFFF)
pub type WaitFn = fn(i32, i32) -> i64;

/// Syscall context containing callbacks for I/O operations
pub struct SyscallContext {
    /// Function to write to console (fd 1 and 2)
    pub console_write: Option<ConsoleWriteFn>,
    /// Function to read from console (fd 0)
    pub console_read: Option<ConsoleReadFn>,
    /// Function to exit the current process
    pub exit: Option<ExitFn>,
    /// Function to fork the current process
    pub fork: Option<ForkFn>,
    /// Function to exec a new program
    pub exec: Option<ExecFn>,
    /// Function to wait for child processes
    pub wait: Option<WaitFn>,
}

impl SyscallContext {
    /// Create an empty syscall context
    pub const fn new() -> Self {
        Self {
            console_write: None,
            console_read: None,
            exit: None,
            fork: None,
            exec: None,
            wait: None,
        }
    }
}

/// Global syscall context
static mut SYSCALL_CONTEXT: SyscallContext = SyscallContext::new();

/// Initialize syscall handlers
///
/// # Safety
/// Must be called once during kernel initialization.
pub unsafe fn init(ctx: SyscallContext) {
    use core::ptr::addr_of_mut;
    unsafe {
        *addr_of_mut!(SYSCALL_CONTEXT) = ctx;
    }
}

/// Dispatch a syscall
///
/// This is called from the architecture-specific syscall entry point.
///
/// # Arguments
/// * `number` - Syscall number
/// * `arg1` through `arg6` - Syscall arguments
///
/// # Returns
/// Syscall result (positive) or negated errno (negative)
pub fn dispatch(
    number: u64,
    arg1: u64,
    arg2: u64,
    arg3: u64,
    arg4: u64,
    arg5: u64,
    arg6: u64,
) -> i64 {
    match number {
        // Process syscalls
        nr::EXIT => sys_exit(arg1 as i32),
        nr::WRITE => sys_write(arg1 as i32, arg2, arg3 as usize),
        nr::READ => sys_read(arg1 as i32, arg2, arg3 as usize),
        nr::FORK => sys_fork(),
        nr::EXEC => sys_exec(arg1, arg2 as usize, core::ptr::null(), core::ptr::null()),
        nr::EXECVE => sys_exec(arg1, arg2 as usize, arg3 as *const *const u8, arg4 as *const *const u8),
        nr::WAIT => sys_wait(arg1),
        nr::WAITPID => sys_waitpid(arg1 as i32, arg2, arg3 as i32),
        nr::GETPID => sys_getpid(),
        nr::GETPPID => sys_getppid(),
        nr::SETPGID => sys_setpgid(arg1 as Pid, arg2 as Pid),
        nr::GETPGID => sys_getpgid(arg1 as Pid),
        nr::SETSID => sys_setsid(),
        nr::GETSID => sys_getsid(arg1 as Pid),
        nr::GETUID => sys_getuid(),
        nr::GETGID => sys_getgid(),
        nr::GETEUID => sys_geteuid(),
        nr::GETEGID => sys_getegid(),
        nr::SETUID => sys_setuid(arg1 as u32),
        nr::SETGID => sys_setgid(arg1 as u32),

        // VFS syscalls
        nr::OPEN => vfs::sys_open(arg1, arg2 as usize, arg3 as u32, arg4 as u32),
        nr::CLOSE => vfs::sys_close(arg1 as i32),
        nr::LSEEK => vfs::sys_lseek(arg1 as i32, arg2 as i64, arg3 as i32),
        nr::FSTAT => vfs::sys_fstat(arg1 as i32, arg2),
        nr::STAT => vfs::sys_stat(arg1, arg2 as usize, arg3),
        nr::DUP => vfs::sys_dup(arg1 as i32),
        nr::DUP2 => vfs::sys_dup2(arg1 as i32, arg2 as i32),
        nr::FTRUNCATE => vfs::sys_ftruncate(arg1 as i32, arg2),

        // Directory syscalls
        nr::MKDIR => dir::sys_mkdir(arg1, arg2 as usize, arg3 as u32),
        nr::RMDIR => dir::sys_rmdir(arg1, arg2 as usize),
        nr::UNLINK => dir::sys_unlink(arg1, arg2 as usize),
        nr::RENAME => dir::sys_rename(arg1, arg2 as usize, arg3, arg4 as usize),
        nr::GETDENTS => dir::sys_getdents(arg1 as i32, arg2, arg3 as usize),
        nr::CHDIR => dir::sys_chdir(arg1, arg2 as usize),
        nr::GETCWD => dir::sys_getcwd(arg1, arg2 as usize),
        nr::PIPE => vfs::sys_pipe(arg1),

        // TTY/device syscalls
        nr::IOCTL => vfs::sys_ioctl(arg1 as i32, arg2, arg3),

        // Module syscalls
        nr::INIT_MODULE => sys_init_module(arg1, arg2 as usize, arg3),
        nr::DELETE_MODULE => sys_delete_module(arg1, arg2 as u32),
        nr::QUERY_MODULE => errno::ENOSYS, // Deprecated, not implemented

        // Signal syscalls
        nr::KILL => signal::sys_kill(arg1 as i32, arg2 as i32),
        nr::SIGACTION => signal::sys_sigaction(arg1 as i32, arg2, arg3),
        nr::SIGPROCMASK => signal::sys_sigprocmask(arg1 as i32, arg2, arg3),
        nr::SIGPENDING => signal::sys_sigpending(arg1),
        nr::SIGSUSPEND => signal::sys_sigsuspend(arg1),
        nr::PAUSE => signal::sys_pause(),
        nr::SIGRETURN => signal::sys_sigreturn(),

        // Socket syscalls
        nr::SOCKET => socket::sys_socket(arg1 as i32, arg2 as i32, arg3 as i32),
        nr::BIND => socket::sys_bind(arg1 as i32, arg2, arg3 as u32),
        nr::LISTEN => socket::sys_listen(arg1 as i32, arg2 as i32),
        nr::ACCEPT => socket::sys_accept(arg1 as i32, arg2, arg3),
        nr::CONNECT => socket::sys_connect(arg1 as i32, arg2, arg3 as u32),
        nr::SEND => socket::sys_send(arg1 as i32, arg2, arg3 as usize, arg4 as i32),
        nr::RECV => socket::sys_recv(arg1 as i32, arg2, arg3 as usize, arg4 as i32),
        nr::SENDTO => socket::sys_sendto(arg1 as i32, arg2, arg3 as usize, arg4 as i32, arg5, arg6 as u32),
        nr::RECVFROM => socket::sys_recvfrom(arg1 as i32, arg2, arg3 as usize, arg4 as i32, arg5, arg6),
        nr::SHUTDOWN => socket::sys_shutdown(arg1 as i32, arg2 as i32),
        nr::GETSOCKNAME => socket::sys_getsockname(arg1 as i32, arg2, arg3),
        nr::GETPEERNAME => socket::sys_getpeername(arg1 as i32, arg2, arg3),
        nr::SETSOCKOPT => socket::sys_setsockopt(arg1 as i32, arg2 as i32, arg3 as i32, arg4, arg5 as u32),
        nr::GETSOCKOPT => socket::sys_getsockopt(arg1 as i32, arg2 as i32, arg3 as i32, arg4, arg5),

        _ => errno::ENOSYS,
    }
}

/// sys_exit - Terminate the current process
///
/// # Arguments
/// * `status` - Exit status code
fn sys_exit(status: i32) -> i64 {
    use core::ptr::addr_of;

    unsafe {
        let ctx = addr_of!(SYSCALL_CONTEXT);
        if let Some(exit_fn) = (*ctx).exit {
            exit_fn(status);
        }
    }

    // If no exit handler, just loop forever
    // (This shouldn't happen in a properly configured system)
    loop {
        core::hint::spin_loop();
    }
}

/// sys_write - Write to a file descriptor
///
/// # Arguments
/// * `fd` - File descriptor (1 = stdout, 2 = stderr)
/// * `buf` - User buffer address
/// * `count` - Number of bytes to write
///
/// # Returns
/// Number of bytes written, or negative errno
fn sys_write(fd: i32, buf: u64, count: usize) -> i64 {
    use core::ptr::addr_of;

    // Only support stdout (1) and stderr (2) for now
    if fd != 1 && fd != 2 {
        return errno::EBADF;
    }

    // Validate count
    if count == 0 {
        return 0;
    }

    // Validate buffer is in user space
    let buf_addr = VirtAddr::new(buf);
    if buf_addr.as_u64() >= 0x0000_8000_0000_0000 {
        return errno::EFAULT;
    }

    // Check end of buffer doesn't overflow into kernel space
    if buf.saturating_add(count as u64) >= 0x0000_8000_0000_0000 {
        return errno::EFAULT;
    }

    // Get the buffer slice
    // NOTE: In a real implementation, we'd need to verify the pages are mapped
    // and copy to a kernel buffer. For simplicity, we directly access it here.
    let buffer = unsafe {
        core::slice::from_raw_parts(buf as *const u8, count)
    };

    unsafe {
        let ctx = addr_of!(SYSCALL_CONTEXT);
        if let Some(write_fn) = (*ctx).console_write {
            write_fn(buffer);
            return count as i64;
        }
    }

    // No console configured
    errno::ENOSYS
}

/// sys_read - Read from a file descriptor
///
/// # Arguments
/// * `fd` - File descriptor (0 = stdin)
/// * `buf` - User buffer address
/// * `count` - Maximum number of bytes to read
///
/// # Returns
/// Number of bytes read, or negative errno
fn sys_read(fd: i32, buf: u64, count: usize) -> i64 {
    use core::ptr::addr_of;

    // Only support stdin (0) for now
    if fd != 0 {
        return errno::EBADF;
    }

    // Validate count
    if count == 0 {
        return 0;
    }

    // Validate buffer is in user space
    let buf_addr = VirtAddr::new(buf);
    if buf_addr.as_u64() >= 0x0000_8000_0000_0000 {
        return errno::EFAULT;
    }

    // Check end of buffer doesn't overflow into kernel space
    if buf.saturating_add(count as u64) >= 0x0000_8000_0000_0000 {
        return errno::EFAULT;
    }

    // Get the buffer slice
    let buffer = unsafe {
        core::slice::from_raw_parts_mut(buf as *mut u8, count)
    };

    unsafe {
        let ctx = addr_of!(SYSCALL_CONTEXT);
        if let Some(read_fn) = (*ctx).console_read {
            let bytes_read = read_fn(buffer);
            return bytes_read as i64;
        }
    }

    // No console configured, return 0 (EOF)
    0
}

/// sys_fork - Create a child process
fn sys_fork() -> i64 {
    use core::ptr::addr_of;

    unsafe {
        let ctx = addr_of!(SYSCALL_CONTEXT);
        if let Some(fork_fn) = (*ctx).fork {
            return fork_fn();
        }
    }

    errno::ENOSYS
}

/// sys_exec - Replace process image with new executable
///
/// # Arguments
/// * `path` - Pointer to path string
/// * `path_len` - Length of path string
/// * `argv` - Pointer to null-terminated array of string pointers
/// * `envp` - Pointer to null-terminated array of string pointers
fn sys_exec(path: u64, path_len: usize, argv: *const *const u8, envp: *const *const u8) -> i64 {
    use core::ptr::addr_of;

    // Validate path is in user space
    if path >= 0x0000_8000_0000_0000 {
        return errno::EFAULT;
    }

    if path.saturating_add(path_len as u64) >= 0x0000_8000_0000_0000 {
        return errno::EFAULT;
    }

    // Validate argv and envp pointers (if non-null)
    if !argv.is_null() && (argv as u64) >= 0x0000_8000_0000_0000 {
        return errno::EFAULT;
    }
    if !envp.is_null() && (envp as u64) >= 0x0000_8000_0000_0000 {
        return errno::EFAULT;
    }

    unsafe {
        let ctx = addr_of!(SYSCALL_CONTEXT);
        if let Some(exec_fn) = (*ctx).exec {
            return exec_fn(path as *const u8, path_len, argv, envp);
        }
    }

    errno::ENOSYS
}

/// sys_wait - Wait for any child process
///
/// # Arguments
/// * `status_ptr` - Pointer to store exit status
fn sys_wait(status_ptr: u64) -> i64 {
    use core::ptr::addr_of;

    unsafe {
        let ctx = addr_of!(SYSCALL_CONTEXT);
        if let Some(wait_fn) = (*ctx).wait {
            let result = wait_fn(-1, 0); // Wait for any child, no options

            if result >= 0 {
                // Result is (pid << 32) | status
                let pid = (result >> 32) as i32;
                let status = result as i32;

                // Write status if pointer provided
                if status_ptr != 0 && status_ptr < 0x0000_8000_0000_0000 {
                    let status_out = status_ptr as *mut i32;
                    *status_out = status;
                }

                return pid as i64;
            }

            return result;
        }
    }

    errno::ENOSYS
}

/// sys_waitpid - Wait for specific child process
///
/// # Arguments
/// * `pid` - PID to wait for (-1 = any, 0 = process group, > 0 = specific)
/// * `status_ptr` - Pointer to store exit status
/// * `options` - Wait options
fn sys_waitpid(pid: i32, status_ptr: u64, options: i32) -> i64 {
    use core::ptr::addr_of;

    unsafe {
        let ctx = addr_of!(SYSCALL_CONTEXT);
        if let Some(wait_fn) = (*ctx).wait {
            let result = wait_fn(pid, options);

            if result >= 0 {
                // Result is (pid << 32) | status
                let child_pid = (result >> 32) as i32;
                let status = result as i32;

                // Write status if pointer provided
                if status_ptr != 0 && status_ptr < 0x0000_8000_0000_0000 {
                    let status_out = status_ptr as *mut i32;
                    *status_out = status;
                }

                return child_pid as i64;
            }

            return result;
        }
    }

    errno::ENOSYS
}

/// sys_getpid - Get current process ID
fn sys_getpid() -> i64 {
    let table = process_table();
    table.current_pid() as i64
}

/// sys_getppid - Get parent process ID
fn sys_getppid() -> i64 {
    let table = process_table();
    if let Some(proc) = table.current() {
        proc.lock().ppid() as i64
    } else {
        0 // No current process, return 0 (kernel)
    }
}

/// sys_setpgid - Set process group ID
///
/// # Arguments
/// * `pid` - Process to modify (0 = current)
/// * `pgid` - New process group (0 = use pid)
fn sys_setpgid(pid: Pid, pgid: Pid) -> i64 {
    let table = process_table();

    // Get target PID
    let target_pid = if pid == 0 {
        table.current_pid()
    } else {
        pid
    };

    // Get target PGID
    let target_pgid = if pgid == 0 {
        target_pid
    } else {
        pgid
    };

    // Get the process
    if let Some(proc) = table.get(target_pid) {
        proc.lock().set_pgid(target_pgid);
        0
    } else {
        errno::ESRCH
    }
}

/// sys_getpgid - Get process group ID
///
/// # Arguments
/// * `pid` - Process to query (0 = current)
fn sys_getpgid(pid: Pid) -> i64 {
    let table = process_table();

    let target_pid = if pid == 0 {
        table.current_pid()
    } else {
        pid
    };

    if let Some(proc) = table.get(target_pid) {
        proc.lock().pgid() as i64
    } else {
        errno::ESRCH
    }
}

/// sys_setsid - Create new session
fn sys_setsid() -> i64 {
    let table = process_table();
    let pid = table.current_pid();

    if let Some(proc) = table.get(pid) {
        let mut p = proc.lock();

        // Check if already a session leader
        if p.sid() == pid {
            return errno::EPERM;
        }

        // Check if already a process group leader
        if p.pgid() == pid {
            return errno::EPERM;
        }

        // Create new session
        p.set_sid(pid);
        p.set_pgid(pid);

        pid as i64
    } else {
        errno::ESRCH
    }
}

/// sys_getsid - Get session ID
///
/// # Arguments
/// * `pid` - Process to query (0 = current)
fn sys_getsid(pid: Pid) -> i64 {
    let table = process_table();

    let target_pid = if pid == 0 {
        table.current_pid()
    } else {
        pid
    };

    if let Some(proc) = table.get(target_pid) {
        proc.lock().sid() as i64
    } else {
        errno::ESRCH
    }
}

/// sys_getuid - Get real user ID
fn sys_getuid() -> i64 {
    let table = process_table();
    if let Some(proc) = table.current() {
        proc.lock().credentials().uid as i64
    } else {
        0 // Kernel context
    }
}

/// sys_getgid - Get real group ID
fn sys_getgid() -> i64 {
    let table = process_table();
    if let Some(proc) = table.current() {
        proc.lock().credentials().gid as i64
    } else {
        0 // Kernel context
    }
}

/// sys_geteuid - Get effective user ID
fn sys_geteuid() -> i64 {
    let table = process_table();
    if let Some(proc) = table.current() {
        proc.lock().credentials().euid as i64
    } else {
        0 // Kernel context
    }
}

/// sys_getegid - Get effective group ID
fn sys_getegid() -> i64 {
    let table = process_table();
    if let Some(proc) = table.current() {
        proc.lock().credentials().egid as i64
    } else {
        0 // Kernel context
    }
}

/// sys_setuid - Set user ID
///
/// # Arguments
/// * `uid` - New user ID
fn sys_setuid(uid: u32) -> i64 {
    let table = process_table();
    if let Some(proc) = table.current() {
        let mut p = proc.lock();
        let creds = p.credentials();

        // If effective UID is 0 (root), set all UIDs
        if creds.euid == 0 {
            let new_creds = proc::Credentials {
                uid,
                gid: creds.gid,
                euid: uid,
                egid: creds.egid,
            };
            p.set_credentials(new_creds);
            0
        } else if uid == creds.uid || uid == creds.euid {
            // Non-root can only set euid to real or saved uid
            let new_creds = proc::Credentials {
                uid: creds.uid,
                gid: creds.gid,
                euid: uid,
                egid: creds.egid,
            };
            p.set_credentials(new_creds);
            0
        } else {
            errno::EPERM
        }
    } else {
        errno::ESRCH
    }
}

/// sys_setgid - Set group ID
///
/// # Arguments
/// * `gid` - New group ID
fn sys_setgid(gid: u32) -> i64 {
    let table = process_table();
    if let Some(proc) = table.current() {
        let mut p = proc.lock();
        let creds = p.credentials();

        // If effective UID is 0 (root), set all GIDs
        if creds.euid == 0 {
            let new_creds = proc::Credentials {
                uid: creds.uid,
                gid,
                euid: creds.euid,
                egid: gid,
            };
            p.set_credentials(new_creds);
            0
        } else if gid == creds.gid || gid == creds.egid {
            // Non-root can only set egid to real or saved gid
            let new_creds = proc::Credentials {
                uid: creds.uid,
                gid: creds.gid,
                euid: creds.euid,
                egid: gid,
            };
            p.set_credentials(new_creds);
            0
        } else {
            errno::EPERM
        }
    } else {
        errno::ESRCH
    }
}

/// sys_init_module - Load a kernel module
///
/// # Arguments
/// * `image` - Pointer to module image (ELF data)
/// * `len` - Length of module image
/// * `params` - Pointer to module parameters string
fn sys_init_module(image: u64, len: usize, params: u64) -> i64 {
    // Validate image pointer
    if image >= 0x0000_8000_0000_0000 {
        return errno::EFAULT;
    }
    if image.saturating_add(len as u64) >= 0x0000_8000_0000_0000 {
        return errno::EFAULT;
    }

    // Get the module data
    let data = unsafe {
        core::slice::from_raw_parts(image as *const u8, len)
    };

    // Get params string (if provided)
    let _params_str = if params != 0 && params < 0x0000_8000_0000_0000 {
        // Read params string
        let params_ptr = params as *const u8;
        let mut params_len = 0;
        unsafe {
            while *params_ptr.add(params_len) != 0 && params_len < 1024 {
                params_len += 1;
            }
            core::str::from_utf8_unchecked(core::slice::from_raw_parts(params_ptr, params_len))
        }
    } else {
        ""
    };

    // NOTE: In full implementation, this would:
    // 1. Parse the ELF module
    // 2. Allocate kernel memory
    // 3. Load and relocate sections
    // 4. Call init_module()
    //
    // For now, return ENOSYS until module is integrated
    let _ = data;
    errno::ENOSYS
}

/// sys_delete_module - Unload a kernel module
///
/// # Arguments
/// * `name_ptr` - Pointer to module name
/// * `flags` - Removal flags
fn sys_delete_module(name_ptr: u64, flags: u32) -> i64 {
    // Validate name pointer
    if name_ptr >= 0x0000_8000_0000_0000 {
        return errno::EFAULT;
    }

    // Read module name
    let name_ptr = name_ptr as *const u8;
    let mut name_len = 0;
    unsafe {
        while *name_ptr.add(name_len) != 0 && name_len < 256 {
            name_len += 1;
        }
    }

    let _name = unsafe {
        core::str::from_utf8_unchecked(core::slice::from_raw_parts(name_ptr, name_len))
    };
    let _flags = flags;

    // NOTE: In full implementation, this would:
    // 1. Find the module by name
    // 2. Check if it's in use
    // 3. Call cleanup_module()
    // 4. Free kernel memory
    //
    // For now, return ENOSYS until module is integrated
    errno::ENOSYS
}
