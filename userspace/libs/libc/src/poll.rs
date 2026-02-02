//! Poll and select functions

use crate::syscall;
use crate::time::Timespec;

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

impl PollFd {
    /// Create new poll fd
    pub fn new(fd: i32, events: i16) -> Self {
        PollFd {
            fd,
            events,
            revents: 0,
        }
    }
}

/// Poll events
pub mod events {
    /// Data available to read
    pub const POLLIN: i16 = 0x0001;
    /// Priority data available
    pub const POLLPRI: i16 = 0x0002;
    /// Writing possible
    pub const POLLOUT: i16 = 0x0004;
    /// Error occurred
    pub const POLLERR: i16 = 0x0008;
    /// Hang up
    pub const POLLHUP: i16 = 0x0010;
    /// Invalid fd
    pub const POLLNVAL: i16 = 0x0020;
    /// Normal data readable
    pub const POLLRDNORM: i16 = 0x0040;
    /// Priority data readable
    pub const POLLRDBAND: i16 = 0x0080;
    /// Writing of normal data possible
    pub const POLLWRNORM: i16 = 0x0100;
    /// Writing of priority data possible
    pub const POLLWRBAND: i16 = 0x0200;
    /// Peer closed connection
    pub const POLLRDHUP: i16 = 0x2000;
}

/// Poll multiple file descriptors
pub fn poll(fds: &mut [PollFd], timeout: i32) -> i32 {
    syscall::syscall3(
        syscall::SYS_POLL,
        fds.as_mut_ptr() as usize,
        fds.len(),
        timeout as usize,
    ) as i32
}

/// Poll with signal mask
pub fn ppoll(fds: &mut [PollFd], timeout: Option<&Timespec>, sigmask: Option<&[u64]>) -> i32 {
    let timeout_ptr = timeout
        .map(|t| t as *const Timespec)
        .unwrap_or(core::ptr::null());
    let sigmask_ptr = sigmask.map(|s| s.as_ptr()).unwrap_or(core::ptr::null());

    syscall::syscall5(
        syscall::SYS_PPOLL,
        fds.as_mut_ptr() as usize,
        fds.len(),
        timeout_ptr as usize,
        sigmask_ptr as usize,
        8, // sigsetsize
    ) as i32
}

/// File descriptor set for select
#[repr(C)]
#[derive(Clone)]
pub struct FdSet {
    /// Bit array of file descriptors
    bits: [u64; 16], // Supports up to 1024 fds
}

impl FdSet {
    /// Maximum fd supported
    pub const FD_SETSIZE: usize = 1024;

    /// Create empty set
    pub fn new() -> Self {
        FdSet { bits: [0; 16] }
    }

    /// Clear all bits
    pub fn zero(&mut self) {
        self.bits = [0; 16];
    }

    /// Set a bit
    pub fn set(&mut self, fd: i32) {
        if fd >= 0 && (fd as usize) < Self::FD_SETSIZE {
            let idx = fd as usize / 64;
            let bit = fd as usize % 64;
            self.bits[idx] |= 1 << bit;
        }
    }

    /// Clear a bit
    pub fn clr(&mut self, fd: i32) {
        if fd >= 0 && (fd as usize) < Self::FD_SETSIZE {
            let idx = fd as usize / 64;
            let bit = fd as usize % 64;
            self.bits[idx] &= !(1 << bit);
        }
    }

    /// Test a bit
    pub fn isset(&self, fd: i32) -> bool {
        if fd >= 0 && (fd as usize) < Self::FD_SETSIZE {
            let idx = fd as usize / 64;
            let bit = fd as usize % 64;
            self.bits[idx] & (1 << bit) != 0
        } else {
            false
        }
    }
}

impl Default for FdSet {
    fn default() -> Self {
        Self::new()
    }
}

/// Timeval for select
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct Timeval {
    pub tv_sec: i64,
    pub tv_usec: i64,
}

/// Select on multiple file descriptors
pub fn select(
    nfds: i32,
    readfds: Option<&mut FdSet>,
    writefds: Option<&mut FdSet>,
    exceptfds: Option<&mut FdSet>,
    timeout: Option<&mut Timeval>,
) -> i32 {
    let readfds_ptr = readfds
        .map(|f| f as *mut FdSet)
        .unwrap_or(core::ptr::null_mut());
    let writefds_ptr = writefds
        .map(|f| f as *mut FdSet)
        .unwrap_or(core::ptr::null_mut());
    let exceptfds_ptr = exceptfds
        .map(|f| f as *mut FdSet)
        .unwrap_or(core::ptr::null_mut());
    let timeout_ptr = timeout
        .map(|t| t as *mut Timeval)
        .unwrap_or(core::ptr::null_mut());

    syscall::syscall5(
        syscall::SYS_SELECT,
        nfds as usize,
        readfds_ptr as usize,
        writefds_ptr as usize,
        exceptfds_ptr as usize,
        timeout_ptr as usize,
    ) as i32
}

/// Select with signal mask
pub fn pselect(
    nfds: i32,
    readfds: Option<&mut FdSet>,
    writefds: Option<&mut FdSet>,
    exceptfds: Option<&mut FdSet>,
    timeout: Option<&Timespec>,
    sigmask: Option<&[u64]>,
) -> i32 {
    let readfds_ptr = readfds
        .map(|f| f as *mut FdSet)
        .unwrap_or(core::ptr::null_mut());
    let writefds_ptr = writefds
        .map(|f| f as *mut FdSet)
        .unwrap_or(core::ptr::null_mut());
    let exceptfds_ptr = exceptfds
        .map(|f| f as *mut FdSet)
        .unwrap_or(core::ptr::null_mut());
    let timeout_ptr = timeout
        .map(|t| t as *const Timespec)
        .unwrap_or(core::ptr::null());
    let sigmask_ptr = sigmask.map(|s| s.as_ptr()).unwrap_or(core::ptr::null());

    syscall::syscall6(
        syscall::SYS_PSELECT6,
        nfds as usize,
        readfds_ptr as usize,
        writefds_ptr as usize,
        exceptfds_ptr as usize,
        timeout_ptr as usize,
        sigmask_ptr as usize,
    ) as i32
}
