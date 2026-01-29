//! System call handlers for OXIDE
//!
//! Provides the syscall dispatch table and handlers.

#![no_std]
#![allow(unused)]

extern crate alloc;

pub mod dir;
pub mod firewall;
pub mod memory;
pub mod poll;
pub mod signal;
pub mod socket;
pub mod time;
pub mod vfs;

use alloc::sync::Arc;
use os_core::VirtAddr;
use proc_traits::Pid;
use spin::Mutex;

// ============================================================================
// Unified process access helpers
// ============================================================================
// These functions provide access to process state through the unified model
// where Task holds ProcessMeta. For backward compatibility during migration,
// we still fall back to ProcessTable for things like fd_table.

/// Get the current process PID (from scheduler)
#[inline]
pub fn current_pid() -> Pid {
    sched::current_pid().unwrap_or(0)
}

/// Get process metadata from the scheduler
///
/// Returns the ProcessMeta Arc for the given PID if available.
#[inline]
pub fn get_meta(pid: Pid) -> Option<Arc<Mutex<sched::ProcessMeta>>> {
    sched::get_task_meta(pid)
}

/// Get current process metadata from the scheduler
#[inline]
pub fn get_current_meta() -> Option<Arc<Mutex<sched::ProcessMeta>>> {
    sched::get_current_meta()
}

/// Execute a closure with read access to current task's ProcessMeta
///
/// This is the preferred way to access process metadata in syscalls.
#[inline]
pub fn with_current_meta<F, R>(f: F) -> Option<R>
where
    F: FnOnce(&sched::ProcessMeta) -> R,
{
    sched::with_current_meta(f)
}

/// Execute a closure with write access to current task's ProcessMeta
#[inline]
pub fn with_current_meta_mut<F, R>(f: F) -> Option<R>
where
    F: FnOnce(&mut sched::ProcessMeta) -> R,
{
    sched::with_current_meta_mut(f)
}

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
    pub const EXECVE: u64 = 13; // exec with argv/envp
    pub const GETUID: u64 = 14;
    pub const GETGID: u64 = 15;
    pub const GETEUID: u64 = 16;
    pub const GETEGID: u64 = 17;
    pub const SETUID: u64 = 18;
    pub const SETGID: u64 = 19;
    pub const SETEUID: u64 = 20;
    pub const SETEGID: u64 = 21;

    // Thread syscalls
    pub const CLONE: u64 = 56; // Create thread/process
    pub const GETTID: u64 = 186; // Get thread ID
    pub const FUTEX: u64 = 202; // Fast userspace locking
    pub const SET_TID_ADDRESS: u64 = 218; // Set clear_child_tid address
    pub const EXIT_GROUP: u64 = 231; // Exit all threads in group

    // VFS syscalls
    pub const OPEN: u64 = 20;
    pub const CLOSE: u64 = 21;
    pub const LSEEK: u64 = 22;
    pub const FSTAT: u64 = 23;
    pub const STAT: u64 = 24;
    pub const LSTAT: u64 = 28; // stat without following symlinks
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
    pub const LINK: u64 = 38;
    pub const SYMLINK: u64 = 39;
    pub const READLINK: u64 = 41;
    pub const GETDENTS64: u64 = 84;

    // TTY/device syscalls
    pub const IOCTL: u64 = 40;

    // Keyboard layout syscalls
    pub const SETKEYMAP: u64 = 120; // Set keyboard layout
    pub const GETKEYMAP: u64 = 121; // Get current keyboard layout name

    // Process priority syscalls
    pub const NICE: u64 = 122;
    pub const GETPRIORITY: u64 = 123;
    pub const SETPRIORITY: u64 = 124;

    // Timer/alarm syscalls
    pub const ALARM: u64 = 125;
    pub const SETITIMER: u64 = 126;
    pub const GETITIMER: u64 = 127;

    // Scheduler syscalls
    pub const SCHED_YIELD: u64 = 130;
    pub const SCHED_SETSCHEDULER: u64 = 131;
    pub const SCHED_GETSCHEDULER: u64 = 132;
    pub const SCHED_SETPARAM: u64 = 133;
    pub const SCHED_GETPARAM: u64 = 134;
    pub const SCHED_SETAFFINITY: u64 = 135;
    pub const SCHED_GETAFFINITY: u64 = 136;
    pub const SCHED_RR_GET_INTERVAL: u64 = 137;

    // Time syscalls
    pub const GETTIMEOFDAY: u64 = 60;
    pub const CLOCK_GETTIME: u64 = 61;
    pub const CLOCK_GETRES: u64 = 62;
    pub const NANOSLEEP: u64 = 63;

    // System info syscalls
    pub const UNAME: u64 = 64;
    pub const STATFS: u64 = 65;
    pub const FSTATFS: u64 = 66;

    // Poll/Select syscalls
    pub const POLL: u64 = 95;
    pub const PPOLL: u64 = 96;
    pub const SELECT: u64 = 97;
    pub const PSELECT6: u64 = 98;

    // Module syscalls (not implemented, moved to higher numbers)
    pub const INIT_MODULE: u64 = 160;
    pub const DELETE_MODULE: u64 = 161;
    pub const QUERY_MODULE: u64 = 162;

    // Signal syscalls
    pub const KILL: u64 = 50;
    pub const SIGACTION: u64 = 51;
    pub const SIGPROCMASK: u64 = 52;
    pub const SIGPENDING: u64 = 53;
    pub const SIGSUSPEND: u64 = 54;
    pub const PAUSE: u64 = 55;
    pub const SIGRETURN: u64 = 56;

    // Memory mapping syscalls
    pub const MMAP: u64 = 90;
    pub const MUNMAP: u64 = 91;
    pub const MPROTECT: u64 = 92;
    pub const MREMAP: u64 = 93;
    pub const BRK: u64 = 94;

    // File permission syscalls
    pub const CHMOD: u64 = 150;
    pub const FCHMOD: u64 = 151;
    pub const CHOWN: u64 = 152;
    pub const FCHOWN: u64 = 153;
    pub const UTIMES: u64 = 154;
    pub const FUTIMES: u64 = 155;

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

    // Firewall syscalls
    pub const FW_ADD_RULE: u64 = 200;
    pub const FW_DEL_RULE: u64 = 201;
    pub const FW_LIST_RULES: u64 = 202;
    pub const FW_SET_POLICY: u64 = 203;
    pub const FW_FLUSH: u64 = 204;
    pub const FW_GET_CONNTRACK: u64 = 205;

    // Random number generation
    pub const GETRANDOM: u64 = 318;

    // Filesystem mount syscalls
    pub const MOUNT: u64 = 165;
    pub const UMOUNT: u64 = 166;
}

/// Error codes (negative return values)
pub mod errno {
    pub const ENOSYS: i64 = -38; // Function not implemented
    pub const EBADF: i64 = -9; // Bad file descriptor
    pub const EFAULT: i64 = -14; // Bad address
    pub const EINVAL: i64 = -22; // Invalid argument
    pub const ENOMEM: i64 = -12; // Out of memory
    pub const ESRCH: i64 = -3; // No such process
    pub const ECHILD: i64 = -10; // No child processes
    pub const EAGAIN: i64 = -11; // Resource temporarily unavailable
    pub const EPERM: i64 = -1; // Operation not permitted
    pub const ENOENT: i64 = -2; // No such file or directory
    pub const EEXIST: i64 = -17; // File exists
    pub const ENOTDIR: i64 = -20; // Not a directory
    pub const EISDIR: i64 = -21; // Is a directory
    pub const ENOTEMPTY: i64 = -39; // Directory not empty
    pub const ENOSPC: i64 = -28; // No space left on device
    pub const EROFS: i64 = -30; // Read-only file system
    pub const ENOTTY: i64 = -25; // Not a typewriter (inappropriate ioctl)
    pub const EINTR: i64 = -4; // Interrupted system call
    pub const ERANGE: i64 = -34; // Result too large
    pub const EMFILE: i64 = -24; // Too many open files
    pub const EIO: i64 = -5; // I/O error

    // Socket errors
    pub const ENOTSOCK: i64 = -88; // Socket operation on non-socket
    pub const EADDRINUSE: i64 = -98; // Address already in use
    pub const EADDRNOTAVAIL: i64 = -99; // Cannot assign requested address
    pub const ENETUNREACH: i64 = -101; // Network is unreachable
    pub const ECONNABORTED: i64 = -103; // Connection aborted
    pub const ECONNRESET: i64 = -104; // Connection reset by peer
    pub const ENOBUFS: i64 = -105; // No buffer space available
    pub const EISCONN: i64 = -106; // Transport endpoint is already connected
    pub const ENOTCONN: i64 = -107; // Transport endpoint is not connected
    pub const ETIMEDOUT: i64 = -110; // Connection timed out
    pub const ECONNREFUSED: i64 = -111; // Connection refused
    pub const EHOSTUNREACH: i64 = -113; // No route to host
    pub const EALREADY: i64 = -114; // Operation already in progress
    pub const EINPROGRESS: i64 = -115; // Operation now in progress
    pub const EPIPE: i64 = -32; // Broken pipe
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

/// Mount callback type - device, mount_point, fstype, flags, data -> result
/// Arguments: device, mount_point, fstype, flags
/// Returns: 0 on success, negative errno on error
pub type MountFn = fn(&str, &str, &str, u32) -> i64;

/// Umount callback type - mount_point, flags -> result
pub type UmountFn = fn(&str, u32) -> i64;

/// Serial debug write function type
pub type SerialWriteFn = fn(&[u8]);

/// Get current task's FS base callback type
pub type GetFsBaseFn = fn() -> u64;

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
    /// Function to mount a filesystem
    pub mount: Option<MountFn>,
    /// Function to unmount a filesystem
    pub umount: Option<UmountFn>,
    /// Function to write to serial for debug output
    pub serial_write: Option<SerialWriteFn>,
    /// Function to get current task's FS base register (for TLS)
    pub get_current_fs_base: Option<GetFsBaseFn>,
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
            mount: None,
            umount: None,
            serial_write: None,
            get_current_fs_base: None,
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
        nr::EXECVE => sys_exec(
            arg1,
            arg2 as usize,
            arg3 as *const *const u8,
            arg4 as *const *const u8,
        ),
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
        nr::SETEUID => sys_seteuid(arg1 as u32),
        nr::SETEGID => sys_setegid(arg1 as u32),

        // Thread syscalls
        nr::CLONE => sys_clone(arg1 as u32, arg2, arg3, arg4, arg5),
        nr::GETTID => sys_gettid(),
        nr::FUTEX => sys_futex(arg1, arg2 as i32, arg3 as u32, arg4, arg5, arg6 as u32),
        nr::SET_TID_ADDRESS => sys_set_tid_address(arg1),
        nr::EXIT_GROUP => sys_exit_group(arg1 as i32),

        // VFS syscalls
        nr::OPEN => vfs::sys_open(arg1, arg2 as usize, arg3 as u32, arg4 as u32),
        nr::CLOSE => vfs::sys_close(arg1 as i32),
        nr::LSEEK => vfs::sys_lseek(arg1 as i32, arg2 as i64, arg3 as i32),
        nr::FSTAT => vfs::sys_fstat(arg1 as i32, arg2),
        nr::STAT => vfs::sys_stat(arg1, arg2 as usize, arg3),
        nr::LSTAT => vfs::sys_lstat(arg1, arg2 as usize, arg3),
        nr::DUP => vfs::sys_dup(arg1 as i32),
        nr::DUP2 => vfs::sys_dup2(arg1 as i32, arg2 as i32),
        nr::FTRUNCATE => vfs::sys_ftruncate(arg1 as i32, arg2),

        // Directory syscalls
        nr::MKDIR => dir::sys_mkdir(arg1, arg2 as usize, arg3 as u32),
        nr::RMDIR => dir::sys_rmdir(arg1, arg2 as usize),
        nr::UNLINK => dir::sys_unlink(arg1, arg2 as usize),
        nr::RENAME => dir::sys_rename(arg1, arg2 as usize, arg3, arg4 as usize),
        nr::GETDENTS => dir::sys_getdents(arg1 as i32, arg2, arg3 as usize),
        nr::GETDENTS64 => dir::sys_getdents(arg1 as i32, arg2, arg3 as usize),
        nr::CHDIR => dir::sys_chdir(arg1, arg2 as usize),
        nr::GETCWD => dir::sys_getcwd(arg1, arg2 as usize),
        nr::LINK => dir::sys_link(arg1, arg2 as usize, arg3, arg4 as usize),
        nr::SYMLINK => dir::sys_symlink(arg1, arg2 as usize, arg3, arg4 as usize),
        nr::READLINK => dir::sys_readlink(arg1, arg2 as usize, arg3, arg4 as usize),
        nr::PIPE => vfs::sys_pipe(arg1),

        // TTY/device syscalls
        nr::IOCTL => vfs::sys_ioctl(arg1 as i32, arg2, arg3),
        nr::SETKEYMAP => sys_setkeymap(arg1, arg2 as usize),
        nr::GETKEYMAP => sys_getkeymap(arg1, arg2 as usize),

        // Process priority syscalls
        nr::NICE => sys_nice(arg1 as i32),
        nr::GETPRIORITY => sys_getpriority(arg1 as i32, arg2 as i32),
        nr::SETPRIORITY => sys_setpriority(arg1 as i32, arg2 as i32, arg3 as i32),

        // Timer/alarm syscalls
        nr::ALARM => sys_alarm(arg1 as u32),
        nr::SETITIMER => sys_setitimer(arg1 as i32, arg2, arg3),
        nr::GETITIMER => sys_getitimer(arg1 as i32, arg2),

        // Scheduler syscalls
        nr::SCHED_YIELD => sys_sched_yield(),
        nr::SCHED_SETSCHEDULER => sys_sched_setscheduler(arg1 as i32, arg2 as i32, arg3),
        nr::SCHED_GETSCHEDULER => sys_sched_getscheduler(arg1 as i32),
        nr::SCHED_SETPARAM => sys_sched_setparam(arg1 as i32, arg2),
        nr::SCHED_GETPARAM => sys_sched_getparam(arg1 as i32, arg2),
        nr::SCHED_SETAFFINITY => sys_sched_setaffinity(arg1 as i32, arg2 as usize, arg3),
        nr::SCHED_GETAFFINITY => sys_sched_getaffinity(arg1 as i32, arg2 as usize, arg3),
        nr::SCHED_RR_GET_INTERVAL => sys_sched_rr_get_interval(arg1 as i32, arg2),

        // Time syscalls
        nr::GETTIMEOFDAY => time::sys_gettimeofday(arg1 as usize, arg2 as usize),
        nr::CLOCK_GETTIME => time::sys_clock_gettime(arg1 as i32, arg2 as usize),
        nr::CLOCK_GETRES => time::sys_clock_getres(arg1 as i32, arg2 as usize),
        nr::NANOSLEEP => time::sys_nanosleep(arg1 as usize, arg2 as usize),

        // System info syscalls
        nr::UNAME => sys_uname(arg1 as usize),
        nr::STATFS => vfs::sys_statfs(arg1, arg2 as usize, arg3 as usize),
        nr::FSTATFS => vfs::sys_fstatfs(arg1 as i32, arg2 as usize),

        // Poll/Select syscalls
        nr::POLL => poll::sys_poll(arg1 as usize, arg2 as usize, arg3 as i32),
        nr::PPOLL => poll::sys_ppoll(arg1 as usize, arg2 as usize, arg3 as usize, arg4 as usize),
        nr::SELECT => poll::sys_select(
            arg1 as i32,
            arg2 as usize,
            arg3 as usize,
            arg4 as usize,
            arg5 as usize,
        ),
        nr::PSELECT6 => poll::sys_pselect6(
            arg1 as i32,
            arg2 as usize,
            arg3 as usize,
            arg4 as usize,
            arg5 as usize,
            arg6 as usize,
        ),

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

        // Memory mapping syscalls
        nr::MMAP => memory::sys_mmap(
            arg1,
            arg2,
            arg3 as i32,
            arg4 as i32,
            arg5 as i32,
            arg6 as i64,
        ),
        nr::MUNMAP => memory::sys_munmap(arg1, arg2),
        nr::MPROTECT => memory::sys_mprotect(arg1, arg2, arg3 as i32),
        nr::MREMAP => memory::sys_mremap(arg1, arg2, arg3, arg4 as i32, arg5),
        nr::BRK => memory::sys_brk(arg1),

        // File permission syscalls
        nr::CHMOD => vfs::sys_chmod(arg1, arg2 as usize, arg3 as u32),
        nr::FCHMOD => vfs::sys_fchmod(arg1 as i32, arg2 as u32),
        nr::CHOWN => vfs::sys_chown(arg1, arg2 as usize, arg3 as i32, arg4 as i32),
        nr::FCHOWN => vfs::sys_fchown(arg1 as i32, arg2 as i32, arg3 as i32),
        nr::UTIMES => dir::sys_utimes(arg1, arg2 as usize, arg3, arg4),

        // Socket syscalls
        nr::SOCKET => socket::sys_socket(arg1 as i32, arg2 as i32, arg3 as i32),
        nr::BIND => socket::sys_bind(arg1 as i32, arg2, arg3 as u32),
        nr::LISTEN => socket::sys_listen(arg1 as i32, arg2 as i32),
        nr::ACCEPT => socket::sys_accept(arg1 as i32, arg2, arg3),
        nr::CONNECT => socket::sys_connect(arg1 as i32, arg2, arg3 as u32),
        nr::SEND => socket::sys_send(arg1 as i32, arg2, arg3 as usize, arg4 as i32),
        nr::RECV => socket::sys_recv(arg1 as i32, arg2, arg3 as usize, arg4 as i32),
        nr::SENDTO => socket::sys_sendto(
            arg1 as i32,
            arg2,
            arg3 as usize,
            arg4 as i32,
            arg5,
            arg6 as u32,
        ),
        nr::RECVFROM => {
            socket::sys_recvfrom(arg1 as i32, arg2, arg3 as usize, arg4 as i32, arg5, arg6)
        }
        nr::SHUTDOWN => socket::sys_shutdown(arg1 as i32, arg2 as i32),
        nr::GETSOCKNAME => socket::sys_getsockname(arg1 as i32, arg2, arg3),
        nr::GETPEERNAME => socket::sys_getpeername(arg1 as i32, arg2, arg3),
        nr::SETSOCKOPT => {
            socket::sys_setsockopt(arg1 as i32, arg2 as i32, arg3 as i32, arg4, arg5 as u32)
        }
        nr::GETSOCKOPT => socket::sys_getsockopt(arg1 as i32, arg2 as i32, arg3 as i32, arg4, arg5),

        // Firewall syscalls
        nr::FW_ADD_RULE => firewall::sys_fw_add_rule(VirtAddr::new(arg1)),
        nr::FW_DEL_RULE => firewall::sys_fw_del_rule(arg1 as usize),
        nr::FW_LIST_RULES => firewall::sys_fw_list_rules(VirtAddr::new(arg1), arg2 as usize),
        nr::FW_SET_POLICY => firewall::sys_fw_set_policy(arg1 as u8, arg2 as u8),
        nr::FW_FLUSH => firewall::sys_fw_flush(arg1 as u8),
        nr::FW_GET_CONNTRACK => firewall::sys_fw_get_conntrack(VirtAddr::new(arg1)),

        // Random number generation
        nr::GETRANDOM => sys_getrandom(arg1, arg2 as usize, arg3 as u32),

        // Filesystem mount syscalls
        nr::MOUNT => vfs::sys_mount(
            arg1,
            arg2 as usize,
            arg3,
            arg4 as usize,
            arg5,
            arg6 as usize,
        ),
        nr::UMOUNT => vfs::sys_umount(arg1, arg2 as usize, arg3 as u32),

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
/// * `fd` - File descriptor
/// * `buf` - User buffer address
/// * `count` - Number of bytes to write
///
/// # Returns
/// Number of bytes written, or negative errno
fn sys_write(fd: i32, buf: u64, count: usize) -> i64 {
    use core::ptr::addr_of;

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

    // Try VFS fd table first for ALL fds (including stdout/stderr which may
    // have been redirected via dup2)
    let vfs_result = vfs::sys_write_vfs(fd, buf, count);

    // If VFS write succeeded or returned a real error (not "no fd"), use it
    if vfs_result != errno::ESRCH && vfs_result != errno::EBADF {
        return vfs_result;
    }

    // Fallback for stdout/stderr: use console callback when no fd table entry
    // exists (early boot, kernel threads, or before fd table is initialized)
    if fd == 1 || fd == 2 {
        // Enable access to user pages for SMAP
        unsafe {
            core::arch::asm!("stac", options(nomem, nostack));
        }

        let buffer = unsafe { core::slice::from_raw_parts(buf as *const u8, count) };

        let result = unsafe {
            let ctx = addr_of!(SYSCALL_CONTEXT);
            if let Some(write_fn) = (*ctx).console_write {
                write_fn(buffer);
                count as i64
            } else {
                errno::ENOSYS
            }
        };

        // Disable access to user pages
        unsafe {
            core::arch::asm!("clac", options(nomem, nostack));
        }

        return result;
    }

    vfs_result
}

/// sys_read - Read from a file descriptor
///
/// # Arguments
/// * `fd` - File descriptor
/// * `buf` - User buffer address
/// * `count` - Maximum number of bytes to read
///
/// # Returns
/// Number of bytes read, or negative errno
fn sys_read(fd: i32, buf: u64, count: usize) -> i64 {
    use core::ptr::addr_of;

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

    // All file descriptors (including stdin) go through VFS
    vfs::sys_read_vfs(fd, buf, count)
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

    // TEMP DEBUG
    unsafe {
        if let Some(write_fn) = (*addr_of!(SYSCALL_CONTEXT)).serial_write {
            write_fn(b"[EXEC] sys_exec entry\n");
        }
    }

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

    // Check AC flag before STAC
    let rflags_before: u64;
    unsafe {
        core::arch::asm!("pushfq; pop {}", out(reg) rflags_before, options(nomem));
    }

    unsafe { core::arch::asm!("stac", options(nomem, nostack)); }

    // Check AC flag after STAC
    let rflags_after: u64;
    unsafe {
        core::arch::asm!("pushfq; pop {}", out(reg) rflags_after, options(nomem));
    }

    unsafe {
        use core::ptr::addr_of;
        if let Some(write_fn) = (*addr_of!(SYSCALL_CONTEXT)).serial_write {
            write_fn(b"[EXEC_TRACE] In sys_exec, about to print AC flags\n");
            write_fn(b"[EXEC] AC flag before STAC: ");
            if rflags_before & (1 << 18) != 0 {
                write_fn(b"SET\n");
            } else {
                write_fn(b"CLEAR\n");
            }
            write_fn(b"[EXEC] AC flag after STAC: ");
            if rflags_after & (1 << 18) != 0 {
                write_fn(b"SET\n");
            } else {
                write_fn(b"CLEAR\n");
            }
            write_fn(b"[EXEC_TRACE] About to call exec_fn now...\n");
        }
    }

    // TEMP DEBUG
    unsafe {
        if let Some(write_fn) = (*addr_of!(SYSCALL_CONTEXT)).serial_write {
            write_fn(b"[EXEC] About to call exec_fn\n");
        }
    }

    let result = unsafe {
        let ctx = addr_of!(SYSCALL_CONTEXT);
        if let Some(exec_fn) = (*ctx).exec {
            let res = exec_fn(path as *const u8, path_len, argv, envp);
            // TEMP DEBUG
            if let Some(write_fn) = (*ctx).serial_write {
                write_fn(b"[EXEC] exec_fn returned\n");
            }
            res
        } else {
            errno::ENOSYS
        }
    };

    // After exec, restore FS_BASE for TLS
    // The exec callback updated the task's context with new fs_base, but sysretq
    // doesn't restore FS_BASE MSR, so we must do it explicitly here.
    if result >= 0 {
        // Exec succeeded, get the updated fs_base from task context and write to MSR
        unsafe {
            let ctx = addr_of!(SYSCALL_CONTEXT);
            if let Some(get_fs_fn) = (*ctx).get_current_fs_base {
                let fs_base = get_fs_fn();
                // DEBUG
                if let Some(write_fn) = (*ctx).serial_write {
                    write_fn(b"[EXEC] FS_BASE value: 0x");
                    let mut buf = [0u8; 16];
                    for i in 0..16 {
                        let nibble = ((fs_base >> (60 - i * 4)) & 0xF) as u8;
                        buf[i] = if nibble < 10 { b'0' + nibble } else { b'a' + nibble - 10 };
                    }
                    write_fn(&buf);
                    write_fn(b"\n");
                }
                if fs_base != 0 {
                    core::arch::asm!(
                        "mov rcx, 0xC0000100",  // IA32_FS_BASE MSR
                        "mov rax, {0}",          // Low 32 bits
                        "mov rdx, {0}",          // Copy for shift
                        "shr rdx, 32",           // High 32 bits
                        "wrmsr",
                        in(reg) fs_base,
                        out("rax") _,
                        out("rcx") _,
                        out("rdx") _,
                        options(nostack, preserves_flags)
                    );
                    if let Some(write_fn) = (*ctx).serial_write {
                        write_fn(b"[EXEC] FS_BASE MSR written\n");
                    }
                }
            }
        }
    }

    unsafe { core::arch::asm!("clac", options(nomem, nostack)); }
    result
}

/// Pre-fault userspace pages by writing a byte to trigger COW
/// This must be done BEFORE copy_to_user to avoid deadlocks in the page fault handler
unsafe fn prefault_pages(user_ptr: u64, len: usize) {
    if len == 0 {
        return;
    }

    let page_size = 4096u64;
    let start_page = user_ptr / page_size;
    let end_page = (user_ptr + len as u64 - 1) / page_size;

    #[cfg(target_arch = "x86_64")]
    unsafe {
        core::arch::asm!("stac", options(nomem, nostack));
    }

    // Touch each page to trigger COW faults NOW (while we don't hold locks)
    // These faults will be from kernel mode but the COW handler can't deadlock
    // because we're not in a page fault context yet
    for page in start_page..=end_page {
        let addr = page * page_size;
        let ptr = addr as *mut u8;

        #[cfg(target_arch = "x86_64")]
        {
            // Read-modify-write to trigger COW without changing data
            // Use volatile to prevent optimization
            unsafe {
                let val = ptr.read_volatile();
                ptr.write_volatile(val);
            }
        }

        #[cfg(not(target_arch = "x86_64"))]
        {
            unsafe {
                let val = ptr.read_volatile();
                ptr.write_volatile(val);
            }
        }
    }

    #[cfg(target_arch = "x86_64")]
    unsafe {
        core::arch::asm!("clac", options(nomem, nostack));
    }
}

/// Copy data to userspace memory safely
///
/// Validates and copies to a userspace buffer from kernel context.
/// Uses STAC/CLAC on x86_64 to temporarily allow supervisor access to user pages.
pub(crate) unsafe fn copy_to_user(user_ptr: u64, kernel_data: &[u8]) -> bool {
    // Validate address is in userspace (canonical form check)
    if user_ptr == 0 || user_ptr >= 0x0000_8000_0000_0000 {
        return false;
    }

    let len = kernel_data.len();
    if len == 0 {
        return true;
    }

    // Check for overflow
    if user_ptr.checked_add(len as u64).is_none() {
        return false;
    }

    // Pre-fault all pages to trigger COW BEFORE we start the actual copy
    // This prevents deadlocks in the page fault handler
    unsafe { prefault_pages(user_ptr, len) };

    #[cfg(target_arch = "x86_64")]
    {
        // On x86_64, use STAC/CLAC to temporarily allow supervisor access to user pages
        // STAC = Set AC flag in EFLAGS (bit 18) to allow access
        // CLAC = Clear AC flag to restore protection
        // These are only available if SMAP is supported, but are safe NOPs otherwise
        unsafe {
            core::arch::asm!(
                "stac",                                      // Enable user page access
                "mov rcx, {len}",                           // Length in RCX
                "mov rsi, {src}",                           // Source (kernel) in RSI
                "mov rdi, {dst}",                           // Destination (user) in RDI
                "rep movsb",                                 // Copy bytes
                "clac",                                      // Disable user page access
                src = in(reg) kernel_data.as_ptr(),
                dst = in(reg) user_ptr,
                len = in(reg) len,
                out("rcx") _,
                out("rsi") _,
                out("rdi") _,
                options(nostack)
            );
        }
    }

    #[cfg(not(target_arch = "x86_64"))]
    {
        unsafe { core::ptr::copy_nonoverlapping(kernel_data.as_ptr(), user_ptr as *mut u8, len) };
    }

    true
}

/// Write a value to userspace memory safely
///
/// Validates and writes to a userspace pointer from kernel context.
unsafe fn write_user_i32(user_ptr: u64, value: i32) -> bool {
    let bytes = value.to_ne_bytes();
    unsafe { copy_to_user(user_ptr, &bytes) }
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

                if status_ptr != 0 {
                    let _ = write_user_i32(status_ptr, status);
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

                // Write status to userspace
                if status_ptr != 0 {
                    let _ = write_user_i32(status_ptr, status);
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
    // Use the unified model - get PID from scheduler
    current_pid() as i64
}

/// sys_getppid - Get parent process ID
fn sys_getppid() -> i64 {
    // Use the unified model - get PPID from scheduler Task
    sched::get_task_ppid(current_pid()).map(|p| p as i64).unwrap_or(0)
}

/// sys_setpgid - Set process group ID
///
/// # Arguments
/// * `pid` - Process to modify (0 = current)
/// * `pgid` - New process group (0 = use pid)
fn sys_setpgid(pid: Pid, pgid: Pid) -> i64 {
    // Get target PID
    let target_pid = if pid == 0 { current_pid() } else { pid };

    // Get target PGID
    let target_pgid = if pgid == 0 { target_pid } else { pgid };

    // Get the process
    if let Some(meta) = get_meta(target_pid) {
        meta.lock().pgid = target_pgid;
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
    let target_pid = if pid == 0 { current_pid() } else { pid };

    if let Some(meta) = get_meta(target_pid) {
        meta.lock().pgid as i64
    } else {
        errno::ESRCH
    }
}

/// sys_setsid - Create new session
fn sys_setsid() -> i64 {
    let pid = current_pid();

    if let Some(meta) = get_meta(pid) {
        let mut m = meta.lock();

        // Check if already a session leader
        if m.sid == pid {
            return errno::EPERM;
        }

        // Check if already a process group leader
        if m.pgid == pid {
            return errno::EPERM;
        }

        // Create new session
        m.sid = pid;
        m.pgid = pid;

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
    let target_pid = if pid == 0 { current_pid() } else { pid };

    if let Some(meta) = get_meta(target_pid) {
        meta.lock().sid as i64
    } else {
        errno::ESRCH
    }
}

/// sys_getuid - Get real user ID
fn sys_getuid() -> i64 {
    // Use the unified model - get credentials from ProcessMeta
    with_current_meta(|meta| meta.credentials.uid as i64).unwrap_or(0)
}

/// sys_getgid - Get real group ID
fn sys_getgid() -> i64 {
    // Use the unified model - get credentials from ProcessMeta
    with_current_meta(|meta| meta.credentials.gid as i64).unwrap_or(0)
}

/// sys_geteuid - Get effective user ID
fn sys_geteuid() -> i64 {
    // Use the unified model - get credentials from ProcessMeta
    with_current_meta(|meta| meta.credentials.euid as i64).unwrap_or(0)
}

/// sys_getegid - Get effective group ID
fn sys_getegid() -> i64 {
    // Use the unified model - get credentials from ProcessMeta
    with_current_meta(|meta| meta.credentials.egid as i64).unwrap_or(0)
}

/// sys_setuid - Set user ID
///
/// # Arguments
/// * `uid` - New user ID
fn sys_setuid(uid: u32) -> i64 {
    if let Some(meta) = get_current_meta() {
        let mut m = meta.lock();
        let creds = m.credentials;

        // If effective UID is 0 (root), set all UIDs
        if creds.euid == 0 {
            m.credentials = proc::Credentials {
                uid,
                gid: creds.gid,
                euid: uid,
                egid: creds.egid,
            };
            0
        } else if uid == creds.uid || uid == creds.euid {
            // Non-root can only set euid to real or saved uid
            m.credentials = proc::Credentials {
                uid: creds.uid,
                gid: creds.gid,
                euid: uid,
                egid: creds.egid,
            };
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
    if let Some(meta) = get_current_meta() {
        let mut m = meta.lock();
        let creds = m.credentials;

        // If effective UID is 0 (root), set all GIDs
        if creds.euid == 0 {
            m.credentials = proc::Credentials {
                uid: creds.uid,
                gid,
                euid: creds.euid,
                egid: gid,
            };
            0
        } else if gid == creds.gid || gid == creds.egid {
            // Non-root can only set egid to real or saved gid
            m.credentials = proc::Credentials {
                uid: creds.uid,
                gid: creds.gid,
                euid: creds.euid,
                egid: gid,
            };
            0
        } else {
            errno::EPERM
        }
    } else {
        errno::ESRCH
    }
}

/// sys_seteuid - Set effective user ID only
///
/// # Arguments
/// * `euid` - New effective user ID
fn sys_seteuid(euid: u32) -> i64 {
    if let Some(meta) = get_current_meta() {
        let mut m = meta.lock();
        let creds = m.credentials;

        // Root can set euid to any value
        // Non-root can set euid to real uid or current euid
        if creds.euid == 0 || euid == creds.uid || euid == creds.euid {
            m.credentials = proc::Credentials {
                uid: creds.uid,
                gid: creds.gid,
                euid,
                egid: creds.egid,
            };
            0
        } else {
            errno::EPERM
        }
    } else {
        errno::ESRCH
    }
}

/// sys_setegid - Set effective group ID only
///
/// # Arguments
/// * `egid` - New effective group ID
fn sys_setegid(egid: u32) -> i64 {
    if let Some(meta) = get_current_meta() {
        let mut m = meta.lock();
        let creds = m.credentials;

        // Root can set egid to any value
        // Non-root can set egid to real gid or current egid
        if creds.euid == 0 || egid == creds.gid || egid == creds.egid {
            m.credentials = proc::Credentials {
                uid: creds.uid,
                gid: creds.gid,
                euid: creds.euid,
                egid,
            };
            0
        } else {
            errno::EPERM
        }
    } else {
        errno::ESRCH
    }
}

/// Priority constants
mod priority {
    pub const PRIO_PROCESS: i32 = 0;
    pub const PRIO_PGRP: i32 = 1;
    pub const PRIO_USER: i32 = 2;
}

/// sys_nice - Change process priority by increment
///
/// # Arguments
/// * `inc` - Priority increment (positive = lower priority, negative = higher)
fn sys_nice(inc: i32) -> i64 {
    let pid = current_pid();
    let current_nice = match sched::get_task_nice(pid) {
        Some(n) => n as i32,
        None => return errno::ESRCH,
    };

    // Calculate new nice value (-20 to +19)
    let new_nice = (current_nice + inc).max(-20).min(19);

    // Check permissions for increasing priority (lowering nice value)
    if new_nice < current_nice {
        let euid = with_current_meta(|m| m.credentials.euid).unwrap_or(u32::MAX);
        if euid != 0 {
            return errno::EPERM;
        }
    }

    sched::set_task_nice(pid, new_nice as i8);
    new_nice as i64
}

/// sys_getpriority - Get scheduling priority
///
/// # Arguments
/// * `which` - PRIO_PROCESS, PRIO_PGRP, or PRIO_USER
/// * `who` - Process ID, process group ID, or user ID (0 = current)
fn sys_getpriority(which: i32, who: i32) -> i64 {
    match which {
        priority::PRIO_PROCESS => {
            let target_pid = if who == 0 { current_pid() } else { who as u32 };

            if let Some(nice) = sched::get_task_nice(target_pid) {
                // Return 20 - nice to match POSIX (0 to 40 range)
                (20 - nice as i32) as i64
            } else {
                errno::ESRCH
            }
        }
        priority::PRIO_PGRP => {
            // For now, just return current process priority if who == 0
            if who == 0 {
                if let Some(nice) = sched::get_task_nice(current_pid()) {
                    (20 - nice as i32) as i64
                } else {
                    errno::ESRCH
                }
            } else {
                errno::ENOSYS // Process group priority not fully implemented
            }
        }
        priority::PRIO_USER => {
            errno::ENOSYS // User priority not fully implemented
        }
        _ => errno::EINVAL,
    }
}

/// sys_setpriority - Set scheduling priority
///
/// # Arguments
/// * `which` - PRIO_PROCESS, PRIO_PGRP, or PRIO_USER
/// * `who` - Process ID, process group ID, or user ID (0 = current)
/// * `prio` - New priority (0-40, where 0 is highest priority)
fn sys_setpriority(which: i32, who: i32, prio: i32) -> i64 {
    // Convert POSIX priority (0-40) to nice value (-20 to +19)
    let nice_value = 20 - prio.max(0).min(40);

    match which {
        priority::PRIO_PROCESS => {
            let target_pid = if who == 0 { current_pid() } else { who as u32 };

            let current_nice = match sched::get_task_nice(target_pid) {
                Some(n) => n as i32,
                None => return errno::ESRCH,
            };

            // Check permissions for increasing priority
            if nice_value < current_nice {
                let euid = with_current_meta(|m| m.credentials.euid).unwrap_or(u32::MAX);
                if euid != 0 {
                    return errno::EPERM;
                }
            }

            sched::set_task_nice(target_pid, nice_value as i8);
            0
        }
        priority::PRIO_PGRP => {
            errno::ENOSYS // Process group priority not fully implemented
        }
        priority::PRIO_USER => {
            errno::ENOSYS // User priority not fully implemented
        }
        _ => errno::EINVAL,
    }
}

/// Timer constants
mod timer {
    pub const ITIMER_REAL: i32 = 0; // Real time (SIGALRM)
    pub const ITIMER_VIRTUAL: i32 = 1; // User time (SIGVTALRM)
    pub const ITIMER_PROF: i32 = 2; // User + system time (SIGPROF)
}

/// sys_alarm - Set an alarm clock for delivery of a signal
///
/// # Arguments
/// * `seconds` - Seconds until SIGALRM (0 = cancel alarm)
///
/// # Returns
/// Seconds remaining from previous alarm, or 0
fn sys_alarm(seconds: u32) -> i64 {
    if let Some(meta) = get_current_meta() {
        let mut m = meta.lock();

        // Get remaining time from previous alarm
        let remaining = m.alarm_remaining;

        if seconds == 0 {
            // Cancel alarm
            m.alarm_remaining = 0;
        } else {
            // Set new alarm
            m.alarm_remaining = seconds;
        }

        remaining as i64
    } else {
        0
    }
}

/// Interval timer structure (matches userspace struct itimerval)
#[repr(C)]
#[derive(Copy, Clone)]
struct ITimerVal {
    it_interval_sec: i64,  // Timer interval (seconds)
    it_interval_usec: i64, // Timer interval (microseconds)
    it_value_sec: i64,     // Current value (seconds)
    it_value_usec: i64,    // Current value (microseconds)
}

/// sys_setitimer - Set value of an interval timer
///
/// # Arguments
/// * `which` - ITIMER_REAL, ITIMER_VIRTUAL, or ITIMER_PROF
/// * `new_value` - Pointer to new timer value
/// * `old_value` - Pointer to receive old timer value (may be null)
fn sys_setitimer(which: i32, new_value: u64, old_value: u64) -> i64 {
    // Validate pointers
    if new_value == 0 || new_value >= 0x0000_8000_0000_0000 {
        return errno::EFAULT;
    }

    if old_value != 0 && old_value >= 0x0000_8000_0000_0000 {
        return errno::EFAULT;
    }

    let meta = match get_current_meta() {
        Some(m) => m,
        None => return errno::ESRCH,
    };

    match which {
        timer::ITIMER_REAL => {
            let mut m = meta.lock();

            // Read old value if requested
            if old_value != 0 {
                let (int_sec, int_usec, val_sec, val_usec) = m.get_itimer();
                let old_timer = ITimerVal {
                    it_interval_sec: int_sec,
                    it_interval_usec: int_usec,
                    it_value_sec: val_sec,
                    it_value_usec: val_usec,
                };

                unsafe {
                    let dest = old_value as *mut ITimerVal;
                    *dest = old_timer;
                }
            }

            // Read new value
            let new_timer = unsafe { *(new_value as *const ITimerVal) };

            // Set new timer
            m.set_itimer(
                new_timer.it_interval_sec,
                new_timer.it_interval_usec,
                new_timer.it_value_sec,
                new_timer.it_value_usec,
            );

            0
        }
        timer::ITIMER_VIRTUAL | timer::ITIMER_PROF => {
            errno::ENOSYS // Virtual and prof timers not yet implemented
        }
        _ => errno::EINVAL,
    }
}

/// sys_getitimer - Get value of an interval timer
///
/// # Arguments
/// * `which` - ITIMER_REAL, ITIMER_VIRTUAL, or ITIMER_PROF
/// * `curr_value` - Pointer to receive current timer value
fn sys_getitimer(which: i32, curr_value: u64) -> i64 {
    // Validate pointer
    if curr_value == 0 || curr_value >= 0x0000_8000_0000_0000 {
        return errno::EFAULT;
    }

    let meta = match get_current_meta() {
        Some(m) => m,
        None => return errno::ESRCH,
    };

    match which {
        timer::ITIMER_REAL => {
            let m = meta.lock();
            let (int_sec, int_usec, val_sec, val_usec) = m.get_itimer();

            let timer = ITimerVal {
                it_interval_sec: int_sec,
                it_interval_usec: int_usec,
                it_value_sec: val_sec,
                it_value_usec: val_usec,
            };

            unsafe {
                let dest = curr_value as *mut ITimerVal;
                *dest = timer;
            }

            0
        }
        timer::ITIMER_VIRTUAL | timer::ITIMER_PROF => {
            errno::ENOSYS // Virtual and prof timers not yet implemented
        }
        _ => errno::EINVAL,
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

    unsafe { core::arch::asm!("stac", options(nomem, nostack)); }

    // Get the module data
    let data = unsafe { core::slice::from_raw_parts(image as *const u8, len) };

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

    unsafe { core::arch::asm!("clac", options(nomem, nostack)); }

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

    let _name =
        unsafe { core::str::from_utf8_unchecked(core::slice::from_raw_parts(name_ptr, name_len)) };
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

/// sys_setkeymap - Set keyboard layout
///
/// # Arguments
/// * `name_ptr` - Pointer to layout name string
/// * `name_len` - Length of layout name
fn sys_setkeymap(name_ptr: u64, name_len: usize) -> i64 {
    // Validate pointer
    if name_ptr >= 0x0000_8000_0000_0000 {
        return errno::EFAULT;
    }
    if name_ptr.saturating_add(name_len as u64) >= 0x0000_8000_0000_0000 {
        return errno::EFAULT;
    }
    if name_len > 32 {
        return errno::EINVAL;
    }

    // Enable access to user pages for SMAP
    unsafe {
        core::arch::asm!("stac", options(nomem, nostack));
    }

    // Read layout name
    let name = unsafe {
        core::str::from_utf8_unchecked(core::slice::from_raw_parts(name_ptr as *const u8, name_len))
    };

    // Try to set the layout
    let result = if input::keymap::set_layout(name) {
        0
    } else {
        errno::EINVAL // Layout not found
    };

    // Disable access to user pages
    unsafe {
        core::arch::asm!("clac", options(nomem, nostack));
    }

    result
}

/// sys_getkeymap - Get current keyboard layout name
///
/// # Arguments
/// * `buf_ptr` - Pointer to buffer for layout name
/// * `buf_len` - Size of buffer
///
/// # Returns
/// Length of layout name written, or negative error
fn sys_getkeymap(buf_ptr: u64, buf_len: usize) -> i64 {
    // Validate pointer
    if buf_ptr >= 0x0000_8000_0000_0000 {
        return errno::EFAULT;
    }
    if buf_ptr.saturating_add(buf_len as u64) >= 0x0000_8000_0000_0000 {
        return errno::EFAULT;
    }

    let layout = input::keymap::current_layout();
    let name = layout.name;
    let name_bytes = name.as_bytes();

    if name_bytes.len() >= buf_len {
        return errno::ENOSPC; // Buffer too small
    }

    // Enable access to user pages for SMAP
    unsafe {
        core::arch::asm!("stac", options(nomem, nostack));
    }

    // Copy layout name to user buffer
    let buf = unsafe { core::slice::from_raw_parts_mut(buf_ptr as *mut u8, buf_len) };
    buf[..name_bytes.len()].copy_from_slice(name_bytes);
    buf[name_bytes.len()] = 0; // Null terminate

    // Disable access to user pages
    unsafe {
        core::arch::asm!("clac", options(nomem, nostack));
    }

    name_bytes.len() as i64
}

// ============================================================================
// Thread syscalls
// ============================================================================

/// Clone callback type - returns child TID to parent, 0 to child
pub type CloneFn = fn(u32, u64, u64, u64, u64) -> i64;

/// sys_clone - Create a new process or thread
///
/// # Arguments
/// * `flags` - Clone flags (CLONE_VM, CLONE_THREAD, etc.)
/// * `stack` - New stack pointer (0 to inherit parent's)
/// * `parent_tid` - Location to store parent TID
/// * `child_tid` - Location to store child TID
/// * `tls` - Thread-local storage pointer
fn sys_clone(flags: u32, stack: u64, parent_tid: u64, child_tid: u64, tls: u64) -> i64 {
    // Validate pointers are in user space
    if parent_tid != 0 && parent_tid >= 0x0000_8000_0000_0000 {
        return errno::EFAULT;
    }
    if child_tid != 0 && child_tid >= 0x0000_8000_0000_0000 {
        return errno::EFAULT;
    }
    if stack != 0 && stack >= 0x0000_8000_0000_0000 {
        return errno::EFAULT;
    }

    // For now, we need a callback from the kernel to handle clone
    // The actual clone implementation requires access to the frame allocator
    // and the current process context, which are in the kernel
    use core::ptr::addr_of;

    // If no CLONE_VM flag, this is like fork
    if flags & proc::clone_flags::CLONE_VM == 0 {
        return sys_fork();
    }

    // Thread creation requires kernel support
    // For now, return ENOSYS until we wire up the clone callback
    // In a full implementation, we'd call into the kernel's clone handler
    errno::ENOSYS
}

/// sys_gettid - Get thread ID
fn sys_gettid() -> i64 {
    // For now, tid == pid (no thread support yet)
    current_pid() as i64
}

/// Futex operations
mod futex_op {
    pub const FUTEX_WAIT: i32 = 0;
    pub const FUTEX_WAKE: i32 = 1;
    pub const FUTEX_PRIVATE_FLAG: i32 = 128;
}

/// sys_futex - Fast userspace mutex operations
///
/// # Arguments
/// * `addr` - Address of the futex word
/// * `op` - Operation (FUTEX_WAIT, FUTEX_WAKE, etc.)
/// * `val` - Value (expected value for WAIT, count for WAKE)
/// * `timeout` - Timeout for WAIT operations (nanoseconds, 0 = infinite)
/// * `addr2` - Second address (for some operations)
/// * `val3` - Third value (for some operations)
fn sys_futex(addr: u64, op: i32, val: u32, timeout: u64, _addr2: u64, _val3: u32) -> i64 {
    // Validate address
    if addr == 0 || addr >= 0x0000_8000_0000_0000 {
        return errno::EFAULT;
    }

    // Strip private flag for operation dispatch
    let op_masked = op & !futex_op::FUTEX_PRIVATE_FLAG;

    match op_masked {
        futex_op::FUTEX_WAIT => {
            // Prepare for futex wait - adds us to wait queue if value matches
            let current = current_pid();
            match proc::futex_wait_prepare(current, addr, val) {
                Ok(proc::FutexWaitResult::ValueMismatch) => errno::EAGAIN,
                Ok(proc::FutexWaitResult::ShouldBlock) => {
                    // Block the current task via scheduler
                    // The scheduler will handle putting us to sleep
                    sched::block_current(sched::TaskState::TASK_INTERRUPTIBLE);
                    0
                }
                Err(proc::FutexError::WouldBlock) => errno::EAGAIN,
                Err(proc::FutexError::InvalidAddress) => errno::EFAULT,
                Err(proc::FutexError::TimedOut) => errno::ETIMEDOUT,
                Err(proc::FutexError::Interrupted) => errno::EINTR,
                Err(_) => errno::EINVAL,
            }
        }
        futex_op::FUTEX_WAKE => {
            // Get list of PIDs to wake
            match proc::futex_wake(addr, val as i32) {
                Ok(pids) => {
                    let count = pids.len();
                    // Wake each process via scheduler
                    for pid in pids {
                        sched::wake_up(pid);
                    }
                    count as i64
                }
                Err(proc::FutexError::InvalidAddress) => errno::EFAULT,
                Err(_) => errno::EINVAL,
            }
        }
        _ => errno::ENOSYS,
    }
}

/// sys_set_tid_address - Set pointer to thread ID
///
/// Sets the clear_child_tid address and returns the current thread ID.
/// When the thread exits, the kernel will write 0 to this address
/// and wake any futex waiters.
fn sys_set_tid_address(tidptr: u64) -> i64 {
    // Validate pointer
    if tidptr != 0 && tidptr >= 0x0000_8000_0000_0000 {
        return errno::EFAULT;
    }

    if let Some(meta) = get_current_meta() {
        meta.lock().clear_child_tid = tidptr;
        current_pid() as i64
    } else {
        errno::ESRCH
    }
}

/// sys_exit_group - Exit all threads in the current thread group
///
/// Terminates all threads in the thread group and exits with the given status.
fn sys_exit_group(status: i32) -> i64 {
    // For now, just exit the current thread
    // In a full implementation, we'd send SIGKILL to all threads in the group
    sys_exit(status)
}

/// sys_sched_yield - Yield the processor
///
/// This syscall voluntarily yields the CPU. The actual yielding happens
/// when we return from the syscall to usermode, where the timer interrupt
/// can preempt us and schedule other processes.
fn sys_sched_yield() -> i64 {
    // Just return success - the timer interrupt will handle scheduling
    // The key is that returning from this syscall puts us back in user mode,
    // where the scheduler can preempt us.
    0
}

/// sched_param structure for scheduler syscalls
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct SchedParam {
    /// RT priority (1-99 for FIFO/RR, 0 for normal)
    sched_priority: i32,
}

/// Scheduling policies (matches Linux)
mod sched_policy {
    pub const SCHED_NORMAL: i32 = 0;
    pub const SCHED_FIFO: i32 = 1;
    pub const SCHED_RR: i32 = 2;
    pub const SCHED_BATCH: i32 = 3;
    pub const SCHED_IDLE: i32 = 5;
}

/// Convert syscall policy to SchedPolicy
fn policy_from_syscall(policy: i32) -> Option<sched::SchedPolicy> {
    match policy {
        sched_policy::SCHED_NORMAL => Some(sched::SchedPolicy::Normal),
        sched_policy::SCHED_FIFO => Some(sched::SchedPolicy::Fifo),
        sched_policy::SCHED_RR => Some(sched::SchedPolicy::RoundRobin),
        sched_policy::SCHED_BATCH => Some(sched::SchedPolicy::Batch),
        sched_policy::SCHED_IDLE => Some(sched::SchedPolicy::Idle),
        _ => None,
    }
}

/// Convert SchedPolicy to syscall policy
fn policy_to_syscall(policy: sched::SchedPolicy) -> i32 {
    match policy {
        sched::SchedPolicy::Normal => sched_policy::SCHED_NORMAL,
        sched::SchedPolicy::Fifo => sched_policy::SCHED_FIFO,
        sched::SchedPolicy::RoundRobin => sched_policy::SCHED_RR,
        sched::SchedPolicy::Batch => sched_policy::SCHED_BATCH,
        sched::SchedPolicy::Idle => sched_policy::SCHED_IDLE,
    }
}

/// sys_sched_setscheduler - Set scheduling policy and parameters
///
/// # Arguments
/// * `pid` - Process ID (0 = current)
/// * `policy` - Scheduling policy (SCHED_NORMAL, SCHED_FIFO, etc.)
/// * `param_ptr` - Pointer to sched_param structure
fn sys_sched_setscheduler(pid: i32, policy: i32, param_ptr: u64) -> i64 {
    // Validate param pointer
    if param_ptr == 0 || param_ptr >= 0x0000_8000_0000_0000 {
        return errno::EFAULT;
    }

    // Get target PID
    let target_pid = if pid == 0 { current_pid() } else { pid as u32 };

    // Validate process exists via scheduler (check if task has metadata)
    if sched::get_task_meta(target_pid).is_none() {
        return errno::ESRCH;
    }

    // Parse policy
    let sched_policy = match policy_from_syscall(policy) {
        Some(p) => p,
        None => return errno::EINVAL,
    };

    // Read param from userspace
    let param: SchedParam = unsafe {
        core::arch::asm!("stac", options(nomem, nostack));
        let p = core::ptr::read_volatile(param_ptr as *const SchedParam);
        core::arch::asm!("clac", options(nomem, nostack));
        p
    };

    // Validate priority for RT policies
    if sched_policy.is_realtime() {
        if param.sched_priority < 1 || param.sched_priority > 99 {
            return errno::EINVAL;
        }
    }

    // Check permissions - only root can set RT policies
    if sched_policy.is_realtime() {
        let euid = with_current_meta(|m| m.credentials.euid).unwrap_or(u32::MAX);
        if euid != 0 {
            return errno::EPERM;
        }
    }

    // Set the scheduler
    sched::set_scheduler(target_pid, sched_policy, param.sched_priority as u8);

    0
}

/// sys_sched_getscheduler - Get scheduling policy
///
/// # Arguments
/// * `pid` - Process ID (0 = current)
fn sys_sched_getscheduler(pid: i32) -> i64 {
    let target_pid = if pid == 0 { current_pid() } else { pid as u32 };

    // Validate process exists
    if sched::get_task_meta(target_pid).is_none() {
        return errno::ESRCH;
    }

    match sched::get_scheduler(target_pid) {
        Some((policy, _)) => policy_to_syscall(policy) as i64,
        None => errno::ESRCH,
    }
}

/// sys_sched_setparam - Set scheduling parameters
///
/// # Arguments
/// * `pid` - Process ID (0 = current)
/// * `param_ptr` - Pointer to sched_param structure
fn sys_sched_setparam(pid: i32, param_ptr: u64) -> i64 {
    // Validate param pointer
    if param_ptr == 0 || param_ptr >= 0x0000_8000_0000_0000 {
        return errno::EFAULT;
    }

    let target_pid = if pid == 0 { current_pid() } else { pid as u32 };

    // Validate process exists
    if sched::get_task_meta(target_pid).is_none() {
        return errno::ESRCH;
    }

    // Read param from userspace
    let param: SchedParam = unsafe {
        core::arch::asm!("stac", options(nomem, nostack));
        let p = core::ptr::read_volatile(param_ptr as *const SchedParam);
        core::arch::asm!("clac", options(nomem, nostack));
        p
    };

    // Get current policy
    let (current_policy, _) = match sched::get_scheduler(target_pid) {
        Some(p) => p,
        None => return errno::ESRCH,
    };

    // Validate priority for RT policies
    if current_policy.is_realtime() {
        if param.sched_priority < 1 || param.sched_priority > 99 {
            return errno::EINVAL;
        }
    }

    // Set the scheduler with same policy but new param
    sched::set_scheduler(target_pid, current_policy, param.sched_priority as u8);

    0
}

/// sys_sched_getparam - Get scheduling parameters
///
/// # Arguments
/// * `pid` - Process ID (0 = current)
/// * `param_ptr` - Pointer to sched_param structure
fn sys_sched_getparam(pid: i32, param_ptr: u64) -> i64 {
    // Validate param pointer
    if param_ptr == 0 || param_ptr >= 0x0000_8000_0000_0000 {
        return errno::EFAULT;
    }

    let target_pid = if pid == 0 { current_pid() } else { pid as u32 };

    // Validate process exists
    if sched::get_task_meta(target_pid).is_none() {
        return errno::ESRCH;
    }

    let (_, priority) = match sched::get_scheduler(target_pid) {
        Some(p) => p,
        None => return errno::ESRCH,
    };

    let param = SchedParam {
        sched_priority: priority as i32,
    };

    // Write param to userspace
    unsafe {
        core::arch::asm!("stac", options(nomem, nostack));
        core::ptr::write_volatile(param_ptr as *mut SchedParam, param);
        core::arch::asm!("clac", options(nomem, nostack));
    }

    0
}

/// sys_sched_setaffinity - Set CPU affinity mask
///
/// # Arguments
/// * `pid` - Process ID (0 = current)
/// * `cpusetsize` - Size of CPU mask in bytes
/// * `mask_ptr` - Pointer to CPU mask
fn sys_sched_setaffinity(pid: i32, cpusetsize: usize, mask_ptr: u64) -> i64 {
    // Validate mask pointer
    if mask_ptr == 0 || mask_ptr >= 0x0000_8000_0000_0000 {
        return errno::EFAULT;
    }
    if cpusetsize == 0 || cpusetsize > 32 {
        return errno::EINVAL;
    }

    let target_pid = if pid == 0 { current_pid() } else { pid as u32 };

    // Validate process exists
    if sched::get_task_meta(target_pid).is_none() {
        return errno::ESRCH;
    }

    // Read mask from userspace
    let mut mask_bytes = [0u8; 32];
    unsafe {
        core::arch::asm!("stac", options(nomem, nostack));
        let src = mask_ptr as *const u8;
        for i in 0..cpusetsize {
            mask_bytes[i] = core::ptr::read_volatile(src.add(i));
        }
        core::arch::asm!("clac", options(nomem, nostack));
    }

    // Convert to CpuSet
    let mut bits = [0u64; 4];
    for i in 0..4.min((cpusetsize + 7) / 8) {
        for j in 0..8 {
            let byte_idx = i * 8 + j;
            if byte_idx < cpusetsize {
                bits[i] |= (mask_bytes[byte_idx] as u64) << (j * 8);
            }
        }
    }

    let cpuset = sched::CpuSet::from_bits(bits);

    // Verify at least one CPU is set
    if cpuset.is_empty() {
        return errno::EINVAL;
    }

    sched::set_affinity(target_pid, cpuset);

    0
}

/// sys_sched_getaffinity - Get CPU affinity mask
///
/// # Arguments
/// * `pid` - Process ID (0 = current)
/// * `cpusetsize` - Size of CPU mask in bytes
/// * `mask_ptr` - Pointer to CPU mask buffer
fn sys_sched_getaffinity(pid: i32, cpusetsize: usize, mask_ptr: u64) -> i64 {
    // Validate mask pointer
    if mask_ptr == 0 || mask_ptr >= 0x0000_8000_0000_0000 {
        return errno::EFAULT;
    }
    if cpusetsize == 0 {
        return errno::EINVAL;
    }

    let target_pid = if pid == 0 { current_pid() } else { pid as u32 };

    // Validate process exists
    if sched::get_task_meta(target_pid).is_none() {
        return errno::ESRCH;
    }

    let cpuset = match sched::get_affinity(target_pid) {
        Some(c) => c,
        None => sched::CpuSet::all(), // Default to all CPUs
    };

    // Convert to bytes
    let bits = cpuset.as_bits();
    let mut mask_bytes = [0u8; 32];
    for i in 0..4 {
        for j in 0..8 {
            let byte_idx = i * 8 + j;
            if byte_idx < 32 {
                mask_bytes[byte_idx] = ((bits[i] >> (j * 8)) & 0xFF) as u8;
            }
        }
    }

    // Write to userspace (only up to cpusetsize bytes)
    let write_size = cpusetsize.min(32);
    unsafe {
        core::arch::asm!("stac", options(nomem, nostack));
        let dest = mask_ptr as *mut u8;
        for i in 0..write_size {
            core::ptr::write_volatile(dest.add(i), mask_bytes[i]);
        }
        core::arch::asm!("clac", options(nomem, nostack));
    }

    write_size as i64
}

/// sys_sched_rr_get_interval - Get round-robin time quantum
///
/// # Arguments
/// * `pid` - Process ID (0 = current)
/// * `tp_ptr` - Pointer to timespec structure
fn sys_sched_rr_get_interval(pid: i32, tp_ptr: u64) -> i64 {
    // Validate pointer
    if tp_ptr == 0 || tp_ptr >= 0x0000_8000_0000_0000 {
        return errno::EFAULT;
    }

    let target_pid = if pid == 0 { current_pid() } else { pid as u32 };

    // Validate process exists
    if sched::get_task_meta(target_pid).is_none() {
        return errno::ESRCH;
    }

    // Return the RR time slice (100ms = 0.1s)
    // timespec: tv_sec (i64), tv_nsec (i64)
    unsafe {
        core::arch::asm!("stac", options(nomem, nostack));
        let tp = tp_ptr as *mut i64;
        core::ptr::write_volatile(tp, 0); // tv_sec
        core::ptr::write_volatile(tp.add(1), 100_000_000); // tv_nsec = 100ms
        core::arch::asm!("clac", options(nomem, nostack));
    }

    0
}

// ============================================================================
// System information syscalls
// ============================================================================

/// UtsName structure for uname syscall
#[repr(C)]
pub struct UtsName {
    /// Operating system name
    pub sysname: [u8; 65],
    /// Network node hostname
    pub nodename: [u8; 65],
    /// Operating system release
    pub release: [u8; 65],
    /// Operating system version
    pub version: [u8; 65],
    /// Hardware identifier (machine)
    pub machine: [u8; 65],
    /// Domain name (Linux extension)
    pub domainname: [u8; 65],
}

impl UtsName {
    /// Create a zeroed UtsName
    pub const fn new() -> Self {
        UtsName {
            sysname: [0; 65],
            nodename: [0; 65],
            release: [0; 65],
            version: [0; 65],
            machine: [0; 65],
            domainname: [0; 65],
        }
    }

    /// Copy a string into a field
    fn set_field(field: &mut [u8; 65], s: &str) {
        let bytes = s.as_bytes();
        let len = bytes.len().min(64);
        field[..len].copy_from_slice(&bytes[..len]);
        field[len] = 0; // Null terminator
    }
}

/// sys_uname - Get system identification
///
/// # Arguments
/// * `buf_ptr` - Pointer to UtsName structure in user space
///
/// # Returns
/// 0 on success, negative errno on error
fn sys_uname(buf_ptr: usize) -> i64 {
    if buf_ptr == 0 || buf_ptr >= 0x0000_8000_0000_0000 {
        return errno::EFAULT;
    }

    let mut utsname = UtsName::new();
    UtsName::set_field(&mut utsname.sysname, "OXIDE");
    UtsName::set_field(&mut utsname.nodename, "localhost");
    UtsName::set_field(&mut utsname.release, "0.1.0");
    UtsName::set_field(&mut utsname.version, "#1 2026-01-24");
    UtsName::set_field(&mut utsname.machine, "x86_64");
    UtsName::set_field(&mut utsname.domainname, "(none)");

    // Copy to userspace
    unsafe {
        core::arch::asm!("stac", options(nomem, nostack));
        let dest = buf_ptr as *mut UtsName;
        core::ptr::write_volatile(dest, utsname);
        core::arch::asm!("clac", options(nomem, nostack));
    }

    0
}

// ============================================================================
// Random number generation
// ============================================================================

/// getrandom flags
mod grnd_flags {
    /// Block until enough entropy available (ignored, we always have enough)
    pub const GRND_RANDOM: u32 = 0x0002;
    /// Non-blocking mode (return EAGAIN if not enough entropy)
    pub const GRND_NONBLOCK: u32 = 0x0001;
    /// Use /dev/random pool instead of /dev/urandom
    pub const GRND_INSECURE: u32 = 0x0004;
}

/// sys_getrandom - Get random bytes
///
/// # Arguments
/// * `buf` - User buffer to fill with random bytes
/// * `buflen` - Size of buffer
/// * `flags` - GRND_RANDOM, GRND_NONBLOCK, etc.
///
/// # Returns
/// Number of bytes written, or negative errno
fn sys_getrandom(buf: u64, buflen: usize, flags: u32) -> i64 {
    // Validate buffer pointer
    if buf == 0 || buf >= 0x0000_8000_0000_0000 {
        return errno::EFAULT;
    }

    // Check for buffer overflow into kernel space
    if buf.saturating_add(buflen as u64) >= 0x0000_8000_0000_0000 {
        return errno::EFAULT;
    }

    // Limit to reasonable size
    let len = buflen.min(256 * 1024); // Max 256KB per call

    // Pre-fault pages for write access
    unsafe {
        prefault_pages(buf, len);
    }

    // Generate random bytes directly into user buffer
    // Using STAC/CLAC for safe user-space access
    #[cfg(target_arch = "x86_64")]
    {
        // Generate random data into a stack buffer first, then copy
        // (crypto::random doesn't take arbitrary pointers)
        let mut temp = [0u8; 4096];
        let mut written = 0;

        while written < len {
            let chunk_size = (len - written).min(4096);
            crypto::random::fill_bytes(&mut temp[..chunk_size]);

            // Copy to userspace
            unsafe {
                let dest = (buf + written as u64) as *mut u8;
                core::arch::asm!(
                    "stac",
                    "mov rcx, {len}",
                    "mov rsi, {src}",
                    "mov rdi, {dst}",
                    "rep movsb",
                    "clac",
                    src = in(reg) temp.as_ptr(),
                    dst = in(reg) dest,
                    len = in(reg) chunk_size,
                    out("rcx") _,
                    out("rsi") _,
                    out("rdi") _,
                    options(nostack)
                );
            }

            written += chunk_size;
        }
    }

    #[cfg(not(target_arch = "x86_64"))]
    {
        let user_buf = unsafe { core::slice::from_raw_parts_mut(buf as *mut u8, len) };
        crypto::random::fill_bytes(user_buf);
    }

    len as i64
}
