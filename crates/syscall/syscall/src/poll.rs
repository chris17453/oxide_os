//! Poll/Select system calls
//!
//! Provides multiplexed I/O operations for waiting on multiple file descriptors.

use crate::errno;
use crate::socket;
use crate::time::{self, Timespec};
use crate::with_current_meta;
use alloc::vec::Vec;

/// Poll event flags (POSIX)
pub mod events {
    /// Data available for reading
    pub const POLLIN: i16 = 0x0001;
    /// Urgent data available
    pub const POLLPRI: i16 = 0x0002;
    /// Writing possible
    pub const POLLOUT: i16 = 0x0004;
    /// Error condition
    pub const POLLERR: i16 = 0x0008;
    /// Hang up
    pub const POLLHUP: i16 = 0x0010;
    /// Invalid request
    pub const POLLNVAL: i16 = 0x0020;
    /// Normal data readable
    pub const POLLRDNORM: i16 = 0x0040;
    /// Priority data readable
    pub const POLLRDBAND: i16 = 0x0080;
    /// Writing normal data possible
    pub const POLLWRNORM: i16 = 0x0100;
    /// Writing priority data possible
    pub const POLLWRBAND: i16 = 0x0200;
}

/// Poll file descriptor structure
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct PollFd {
    /// File descriptor
    pub fd: i32,
    /// Requested events
    pub events: i16,
    /// Returned events
    pub revents: i16,
}

/// Timer frequency in Hz (must match time.rs)
const TIMER_HZ: u64 = 100;
const NS_PER_TICK: u64 = 1_000_000_000 / TIMER_HZ;

fn get_ticks() -> u64 {
    arch_x86_64::timer_ticks()
}

/// Check if a file descriptor is ready for the requested operations
fn check_fd_ready(fd: i32, events: i16) -> i16 {
    // First check if this is a socket fd
    if socket::is_socket_fd(fd) {
        return check_socket_ready(fd, events);
    }

    // Regular file descriptor through VFS
    let file = match with_current_meta(|meta| {
        meta.fd_table.get(fd).map(|fd_entry| fd_entry.file.clone())
    }) {
        Some(Ok(f)) => f,
        Some(Err(_)) => return events::POLLNVAL,
        None => return events::POLLNVAL,
    };

    let mut revents: i16 = 0;

    // Check for readability
    if events & (events::POLLIN | events::POLLRDNORM) != 0 {
        // Regular files are always readable if opened for reading
        if file.can_read() {
            revents |= events::POLLIN | events::POLLRDNORM;
        }
    }

    // Check for writability
    if events & (events::POLLOUT | events::POLLWRNORM) != 0 {
        // Regular files are always writable if opened for writing
        if file.can_write() {
            revents |= events::POLLOUT | events::POLLWRNORM;
        }
    }

    revents
}

/// Check if a socket fd is ready for the requested operations
fn check_socket_ready(fd: i32, events: i16) -> i16 {
    let mut revents: i16 = 0;

    // Get the socket state
    let socket_info = socket::get_socket_info(fd);
    if socket_info.is_none() {
        return events::POLLNVAL;
    }

    let (is_connected, has_data, can_send, is_listening, has_pending_connection) =
        socket_info.unwrap();

    // Check for readability
    if events & (events::POLLIN | events::POLLRDNORM) != 0 {
        // Socket is readable if:
        // - It has data in the receive buffer
        // - It's a listening socket with pending connections
        // - Connection was closed (returns 0 on read)
        if has_data || (is_listening && has_pending_connection) {
            revents |= events::POLLIN | events::POLLRDNORM;
        }
    }

    // Check for writability
    if events & (events::POLLOUT | events::POLLWRNORM) != 0 {
        // Socket is writable if:
        // - It's connected and send buffer has space
        if is_connected && can_send {
            revents |= events::POLLOUT | events::POLLWRNORM;
        }
    }

    // Check for errors/hangup
    if !is_connected && !is_listening {
        // Not connected and not listening - hung up
        revents |= events::POLLHUP;
    }

    revents
}

/// sys_poll - Wait for events on file descriptors
///
/// # Arguments
/// * `fds_ptr` - Pointer to array of pollfd structures
/// * `nfds` - Number of file descriptors
/// * `timeout_ms` - Timeout in milliseconds (-1 = infinite, 0 = return immediately)
pub fn sys_poll(fds_ptr: usize, nfds: usize, timeout_ms: i32) -> i64 {
    if fds_ptr == 0 && nfds > 0 {
        return errno::EFAULT;
    }

    if nfds > 1024 {
        // Limit to reasonable number
        return errno::EINVAL;
    }

    // Read pollfd array from userspace
    let mut fds: Vec<PollFd> = Vec::with_capacity(nfds);

    unsafe {
        core::arch::asm!("stac", options(nomem, nostack));
        let ptr = fds_ptr as *const PollFd;
        for i in 0..nfds {
            fds.push(core::ptr::read_volatile(ptr.add(i)));
        }
        core::arch::asm!("clac", options(nomem, nostack));
    }

    // Calculate deadline
    let start_ticks = get_ticks();
    let deadline_ticks = if timeout_ms < 0 {
        u64::MAX // Infinite
    } else if timeout_ms == 0 {
        start_ticks // Return immediately
    } else {
        let timeout_ns = (timeout_ms as u64) * 1_000_000;
        let timeout_ticks = (timeout_ns + NS_PER_TICK - 1) / NS_PER_TICK;
        start_ticks + timeout_ticks
    };

    // Poll loop
    loop {
        let mut ready_count = 0i64;

        // Check each fd
        for pollfd in fds.iter_mut() {
            pollfd.revents = 0;

            if pollfd.fd < 0 {
                // Negative fd means ignore this entry
                continue;
            }

            let revents = check_fd_ready(pollfd.fd, pollfd.events);
            pollfd.revents = revents;

            if revents != 0 {
                ready_count += 1;
            }
        }

        // If any fd is ready, return
        if ready_count > 0 {
            // Write results back to userspace
            unsafe {
                core::arch::asm!("stac", options(nomem, nostack));
                let ptr = fds_ptr as *mut PollFd;
                for (i, pollfd) in fds.iter().enumerate() {
                    core::ptr::write_volatile(ptr.add(i), *pollfd);
                }
                core::arch::asm!("clac", options(nomem, nostack));
            }
            return ready_count;
        }

        // Check timeout
        let current_ticks = get_ticks();
        if current_ticks >= deadline_ticks {
            // Timeout - write back results (all zero revents) and return 0
            unsafe {
                core::arch::asm!("stac", options(nomem, nostack));
                let ptr = fds_ptr as *mut PollFd;
                for (i, pollfd) in fds.iter().enumerate() {
                    core::ptr::write_volatile(ptr.add(i), *pollfd);
                }
                core::arch::asm!("clac", options(nomem, nostack));
            }
            return 0;
        }

        // Check for signals
        if with_current_meta(|meta| meta.has_pending_signals()).unwrap_or(false) {
            return errno::EINTR;
        }

        // Allow scheduler to preempt us while we wait
        arch_x86_64::allow_kernel_preempt();

        // HLT yields CPU until next interrupt
        // With KERNEL_PREEMPT_OK set, scheduler will switch to other processes
        unsafe {
            core::arch::asm!("sti"); // Ensure interrupts enabled
            core::arch::asm!("hlt", options(nomem, nostack));
        }

        // Clear preempt flag if we're still running (no switch occurred)
        arch_x86_64::disallow_kernel_preempt();
    }
}

/// sys_ppoll - Poll with nanosecond timeout and signal mask
///
/// # Arguments
/// * `fds_ptr` - Pointer to array of pollfd structures
/// * `nfds` - Number of file descriptors
/// * `timeout_ptr` - Pointer to timespec (NULL = infinite)
/// * `sigmask_ptr` - Signal mask to apply during poll (currently ignored)
pub fn sys_ppoll(fds_ptr: usize, nfds: usize, timeout_ptr: usize, sigmask_ptr: usize) -> i64 {
    // Convert timespec to milliseconds for sys_poll
    let timeout_ms = if timeout_ptr == 0 {
        -1 // Infinite
    } else {
        let ts: Timespec = unsafe {
            core::arch::asm!("stac", options(nomem, nostack));
            let tp = timeout_ptr as *const Timespec;
            let val = core::ptr::read_volatile(tp);
            core::arch::asm!("clac", options(nomem, nostack));
            val
        };

        if ts.tv_sec < 0 || ts.tv_nsec < 0 {
            return errno::EINVAL;
        }

        // Convert to milliseconds (saturating)
        let ms = (ts.tv_sec as i64)
            .saturating_mul(1000)
            .saturating_add(ts.tv_nsec / 1_000_000);

        if ms > i32::MAX as i64 {
            i32::MAX
        } else {
            ms as i32
        }
    };

    // Apply signal mask if provided (temporarily set during poll)
    let mut old_mask: Option<signal::SigSet> = None;
    if sigmask_ptr != 0 {
        if let Some(sigset) = crate::signal::read_sigset(sigmask_ptr) {
            old_mask = Some(crate::signal::swap_signal_mask(sigset));
        }
    }

    let ret = sys_poll(fds_ptr, nfds, timeout_ms);

    // Restore previous mask
    if let Some(mask) = old_mask {
        crate::signal::set_signal_mask(mask);
    }

    ret
}

/// FD set for select() - bitmap of file descriptors
#[repr(C)]
#[derive(Clone, Copy)]
pub struct FdSet {
    /// Bitmap of file descriptors (supports up to 1024 fds)
    pub fds_bits: [u64; 16],
}

impl FdSet {
    pub const fn new() -> Self {
        FdSet { fds_bits: [0; 16] }
    }

    pub fn is_set(&self, fd: i32) -> bool {
        if fd < 0 || fd >= 1024 {
            return false;
        }
        let idx = (fd / 64) as usize;
        let bit = (fd % 64) as u64;
        (self.fds_bits[idx] & (1 << bit)) != 0
    }

    pub fn set(&mut self, fd: i32) {
        if fd >= 0 && fd < 1024 {
            let idx = (fd / 64) as usize;
            let bit = (fd % 64) as u64;
            self.fds_bits[idx] |= 1 << bit;
        }
    }

    pub fn clear(&mut self, fd: i32) {
        if fd >= 0 && fd < 1024 {
            let idx = (fd / 64) as usize;
            let bit = (fd % 64) as u64;
            self.fds_bits[idx] &= !(1 << bit);
        }
    }

    pub fn zero(&mut self) {
        self.fds_bits = [0; 16];
    }
}

impl Default for FdSet {
    fn default() -> Self {
        Self::new()
    }
}

/// sys_select - Synchronous I/O multiplexing (legacy interface)
///
/// # Arguments
/// * `nfds` - Highest fd + 1
/// * `readfds_ptr` - FDs to check for reading
/// * `writefds_ptr` - FDs to check for writing
/// * `exceptfds_ptr` - FDs to check for exceptions
/// * `timeout_ptr` - Timeout (timeval structure)
pub fn sys_select(
    nfds: i32,
    readfds_ptr: usize,
    writefds_ptr: usize,
    exceptfds_ptr: usize,
    timeout_ptr: usize,
) -> i64 {
    if nfds < 0 || nfds > 1024 {
        return errno::EINVAL;
    }

    // Read fd sets from userspace
    let mut readfds = FdSet::new();
    let mut writefds = FdSet::new();
    let mut exceptfds = FdSet::new();

    unsafe {
        core::arch::asm!("stac", options(nomem, nostack));

        if readfds_ptr != 0 {
            readfds = core::ptr::read_volatile(readfds_ptr as *const FdSet);
        }
        if writefds_ptr != 0 {
            writefds = core::ptr::read_volatile(writefds_ptr as *const FdSet);
        }
        if exceptfds_ptr != 0 {
            exceptfds = core::ptr::read_volatile(exceptfds_ptr as *const FdSet);
        }

        core::arch::asm!("clac", options(nomem, nostack));
    }

    // Read timeout
    let timeout_ms = if timeout_ptr == 0 {
        -1i32 // Infinite
    } else {
        unsafe {
            core::arch::asm!("stac", options(nomem, nostack));
            let tv = core::ptr::read_volatile(timeout_ptr as *const time::Timeval);
            core::arch::asm!("clac", options(nomem, nostack));

            if tv.tv_sec < 0 || tv.tv_usec < 0 {
                return errno::EINVAL;
            }

            let ms = (tv.tv_sec as i64)
                .saturating_mul(1000)
                .saturating_add(tv.tv_usec / 1000);

            if ms > i32::MAX as i64 {
                i32::MAX
            } else {
                ms as i32
            }
        }
    };

    // Build pollfd array from fd sets
    let mut pollfds: Vec<PollFd> = Vec::new();
    let mut fd_map: Vec<(i32, bool, bool, bool)> = Vec::new(); // (fd, read, write, except)

    for fd in 0..nfds {
        let in_read = readfds.is_set(fd);
        let in_write = writefds.is_set(fd);
        let in_except = exceptfds.is_set(fd);

        if in_read || in_write || in_except {
            let mut events: i16 = 0;
            if in_read {
                events |= events::POLLIN;
            }
            if in_write {
                events |= events::POLLOUT;
            }
            if in_except {
                events |= events::POLLPRI;
            }

            pollfds.push(PollFd {
                fd,
                events,
                revents: 0,
            });
            fd_map.push((fd, in_read, in_write, in_except));
        }
    }

    // Calculate deadline
    let start_ticks = get_ticks();
    let deadline_ticks = if timeout_ms < 0 {
        u64::MAX
    } else if timeout_ms == 0 {
        start_ticks
    } else {
        let timeout_ns = (timeout_ms as u64) * 1_000_000;
        let timeout_ticks = (timeout_ns + NS_PER_TICK - 1) / NS_PER_TICK;
        start_ticks + timeout_ticks
    };

    // Poll loop
    loop {
        let mut ready_count = 0i64;

        // Zero result sets
        let mut result_read = FdSet::new();
        let mut result_write = FdSet::new();
        let mut result_except = FdSet::new();

        // Check each fd
        for (i, pollfd) in pollfds.iter_mut().enumerate() {
            let (fd, in_read, in_write, in_except) = fd_map[i];
            let revents = check_fd_ready(fd, pollfd.events);
            pollfd.revents = revents;

            if revents & events::POLLNVAL != 0 {
                // Invalid fd - set in exceptfds if it was requested there
                if in_except {
                    result_except.set(fd);
                    ready_count += 1;
                }
            } else {
                if in_read && (revents & (events::POLLIN | events::POLLHUP | events::POLLERR) != 0)
                {
                    result_read.set(fd);
                    ready_count += 1;
                }
                if in_write && (revents & (events::POLLOUT | events::POLLERR) != 0) {
                    result_write.set(fd);
                    ready_count += 1;
                }
                if in_except && (revents & events::POLLPRI != 0) {
                    result_except.set(fd);
                    ready_count += 1;
                }
            }
        }

        // If any fd is ready or timeout
        if ready_count > 0 || get_ticks() >= deadline_ticks {
            // Write results back to userspace
            unsafe {
                core::arch::asm!("stac", options(nomem, nostack));

                if readfds_ptr != 0 {
                    core::ptr::write_volatile(readfds_ptr as *mut FdSet, result_read);
                }
                if writefds_ptr != 0 {
                    core::ptr::write_volatile(writefds_ptr as *mut FdSet, result_write);
                }
                if exceptfds_ptr != 0 {
                    core::ptr::write_volatile(exceptfds_ptr as *mut FdSet, result_except);
                }

                core::arch::asm!("clac", options(nomem, nostack));
            }

            return ready_count;
        }

        // Check for signals
        if with_current_meta(|meta| meta.has_pending_signals()).unwrap_or(false) {
            return errno::EINTR;
        }

        core::hint::spin_loop();
    }
}

/// sys_pselect6 - Select with nanosecond timeout and signal mask
pub fn sys_pselect6(
    nfds: i32,
    readfds_ptr: usize,
    writefds_ptr: usize,
    exceptfds_ptr: usize,
    timeout_ptr: usize,
    sigmask_ptr: usize,
) -> i64 {
    // Apply signal mask if provided
    let mut old_mask: Option<signal::SigSet> = None;
    if sigmask_ptr != 0 {
        if let Some(sigset) = crate::signal::read_sigset(sigmask_ptr) {
            old_mask = Some(crate::signal::swap_signal_mask(sigset));
        }
    }

    // Convert timespec to timeval for sys_select
    // For now, just call select with the timespec converted
    if timeout_ptr == 0 {
        let ret = sys_select(nfds, readfds_ptr, writefds_ptr, exceptfds_ptr, 0);
        if let Some(mask) = old_mask {
            crate::signal::set_signal_mask(mask);
        }
        return ret;
    }

    let ts: Timespec = unsafe {
        core::arch::asm!("stac", options(nomem, nostack));
        let tp = timeout_ptr as *const Timespec;
        let val = core::ptr::read_volatile(tp);
        core::arch::asm!("clac", options(nomem, nostack));
        val
    };

    // We need to pass this to select somehow - for now just convert timeout
    let timeout_ms = if ts.tv_sec < 0 {
        -1i32
    } else {
        let ms = (ts.tv_sec as i64)
            .saturating_mul(1000)
            .saturating_add(ts.tv_nsec / 1_000_000);
        if ms > i32::MAX as i64 {
            i32::MAX
        } else {
            ms as i32
        }
    };

    // For pselect6, we implement inline rather than calling sys_select
    // to avoid the timeval conversion issue

    if nfds < 0 || nfds > 1024 {
        return errno::EINVAL;
    }

    let mut readfds = FdSet::new();
    let mut writefds = FdSet::new();
    let mut exceptfds = FdSet::new();

    unsafe {
        core::arch::asm!("stac", options(nomem, nostack));
        if readfds_ptr != 0 {
            readfds = core::ptr::read_volatile(readfds_ptr as *const FdSet);
        }
        if writefds_ptr != 0 {
            writefds = core::ptr::read_volatile(writefds_ptr as *const FdSet);
        }
        if exceptfds_ptr != 0 {
            exceptfds = core::ptr::read_volatile(exceptfds_ptr as *const FdSet);
        }
        core::arch::asm!("clac", options(nomem, nostack));
    }

    let start_ticks = get_ticks();
    let deadline_ticks = if timeout_ms < 0 {
        u64::MAX
    } else if timeout_ms == 0 {
        start_ticks
    } else {
        let timeout_ns = (timeout_ms as u64) * 1_000_000;
        let timeout_ticks = (timeout_ns + NS_PER_TICK - 1) / NS_PER_TICK;
        start_ticks + timeout_ticks
    };

    loop {
        let mut ready_count = 0i64;
        let mut result_read = FdSet::new();
        let mut result_write = FdSet::new();
        let mut result_except = FdSet::new();

        for fd in 0..nfds {
            let in_read = readfds.is_set(fd);
            let in_write = writefds.is_set(fd);
            let in_except = exceptfds.is_set(fd);

            if !in_read && !in_write && !in_except {
                continue;
            }

            let mut poll_events: i16 = 0;
            if in_read {
                poll_events |= events::POLLIN;
            }
            if in_write {
                poll_events |= events::POLLOUT;
            }
            if in_except {
                poll_events |= events::POLLPRI;
            }

            let revents = check_fd_ready(fd, poll_events);

            if revents & events::POLLNVAL != 0 {
                if in_except {
                    result_except.set(fd);
                    ready_count += 1;
                }
            } else {
                if in_read && (revents & (events::POLLIN | events::POLLHUP | events::POLLERR) != 0)
                {
                    result_read.set(fd);
                    ready_count += 1;
                }
                if in_write && (revents & (events::POLLOUT | events::POLLERR) != 0) {
                    result_write.set(fd);
                    ready_count += 1;
                }
                if in_except && (revents & events::POLLPRI != 0) {
                    result_except.set(fd);
                    ready_count += 1;
                }
            }
        }

        if ready_count > 0 || get_ticks() >= deadline_ticks {
            unsafe {
                core::arch::asm!("stac", options(nomem, nostack));
                if readfds_ptr != 0 {
                    core::ptr::write_volatile(readfds_ptr as *mut FdSet, result_read);
                }
                if writefds_ptr != 0 {
                    core::ptr::write_volatile(writefds_ptr as *mut FdSet, result_write);
                }
                if exceptfds_ptr != 0 {
                    core::ptr::write_volatile(exceptfds_ptr as *mut FdSet, result_except);
                }
                core::arch::asm!("clac", options(nomem, nostack));
            }
            let res = ready_count;
            if let Some(mask) = old_mask {
                crate::signal::set_signal_mask(mask);
            }
            return res;
        }

        if with_current_meta(|meta| meta.has_pending_signals()).unwrap_or(false) {
            if let Some(mask) = old_mask {
                crate::signal::set_signal_mask(mask);
            }
            return errno::EINTR;
        }

        core::hint::spin_loop();
    }
}
