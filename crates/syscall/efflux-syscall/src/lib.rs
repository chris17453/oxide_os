//! System call handlers for EFFLUX
//!
//! Provides the syscall dispatch table and handlers.

#![no_std]

extern crate alloc;

use efflux_core::VirtAddr;
use efflux_proc::process_table;
use efflux_proc_traits::Pid;

/// Syscall numbers
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
}

/// Console output callback type
pub type ConsoleWriteFn = fn(&[u8]);

/// Console input callback type (returns bytes read, or 0 if no data)
pub type ConsoleReadFn = fn(&mut [u8]) -> usize;

/// Exit callback type
pub type ExitFn = fn(i32) -> !;

/// Fork callback type - returns child PID to parent, 0 to child, or negative error
pub type ForkFn = fn() -> i64;

/// Exec callback type - path, returns error code (doesn't return on success)
pub type ExecFn = fn(*const u8, usize) -> i64;

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
    _arg4: u64,
    _arg5: u64,
    _arg6: u64,
) -> i64 {
    match number {
        nr::EXIT => sys_exit(arg1 as i32),
        nr::WRITE => sys_write(arg1 as i32, arg2, arg3 as usize),
        nr::READ => sys_read(arg1 as i32, arg2, arg3 as usize),
        nr::FORK => sys_fork(),
        nr::EXEC => sys_exec(arg1, arg2 as usize),
        nr::WAIT => sys_wait(arg1),
        nr::WAITPID => sys_waitpid(arg1 as i32, arg2, arg3 as i32),
        nr::GETPID => sys_getpid(),
        nr::GETPPID => sys_getppid(),
        nr::SETPGID => sys_setpgid(arg1 as Pid, arg2 as Pid),
        nr::GETPGID => sys_getpgid(arg1 as Pid),
        nr::SETSID => sys_setsid(),
        nr::GETSID => sys_getsid(arg1 as Pid),
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
fn sys_exec(path: u64, path_len: usize) -> i64 {
    use core::ptr::addr_of;

    // Validate path is in user space
    if path >= 0x0000_8000_0000_0000 {
        return errno::EFAULT;
    }

    if path.saturating_add(path_len as u64) >= 0x0000_8000_0000_0000 {
        return errno::EFAULT;
    }

    unsafe {
        let ctx = addr_of!(SYSCALL_CONTEXT);
        if let Some(exec_fn) = (*ctx).exec {
            return exec_fn(path as *const u8, path_len);
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
