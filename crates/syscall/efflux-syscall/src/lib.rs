//! System call handlers for EFFLUX
//!
//! Provides the syscall dispatch table and handlers.

#![no_std]

use efflux_core::VirtAddr;

/// Syscall numbers
pub mod nr {
    pub const EXIT: u64 = 0;
    pub const WRITE: u64 = 1;
    pub const READ: u64 = 2;
}

/// Error codes (negative return values)
pub mod errno {
    pub const ENOSYS: i64 = -38;    // Function not implemented
    pub const EBADF: i64 = -9;      // Bad file descriptor
    pub const EFAULT: i64 = -14;    // Bad address
    pub const EINVAL: i64 = -22;    // Invalid argument
}

/// Console output callback type
pub type ConsoleWriteFn = fn(&[u8]);

/// Console input callback type (returns bytes read, or 0 if no data)
pub type ConsoleReadFn = fn(&mut [u8]) -> usize;

/// Exit callback type
pub type ExitFn = fn(i32) -> !;

/// Syscall context containing callbacks for I/O operations
pub struct SyscallContext {
    /// Function to write to console (fd 1 and 2)
    pub console_write: Option<ConsoleWriteFn>,
    /// Function to read from console (fd 0)
    pub console_read: Option<ConsoleReadFn>,
    /// Function to exit the current process
    pub exit: Option<ExitFn>,
}

impl SyscallContext {
    /// Create an empty syscall context
    pub const fn new() -> Self {
        Self {
            console_write: None,
            console_read: None,
            exit: None,
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
