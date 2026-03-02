//! ABI types — the repr(C) contracts between userspace and kernel.
//!
//! — WireSaint: Every field offset matters. Get one wrong and you'll
//! spend three hours staring at garbage data wondering if you've lost your mind.

/// File status structure (must match kernel's stat layout)
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stat {
    pub dev: u64,
    pub ino: u64,
    pub mode: u32,
    pub nlink: u64,
    pub uid: u32,
    pub gid: u32,
    pub rdev: u64,
    pub size: u64,
    pub blksize: u64,
    pub blocks: u64,
    pub atime: u64,
    pub mtime: u64,
    pub ctime: u64,
}

impl Stat {
    pub const fn zeroed() -> Self {
        Self {
            dev: 0, ino: 0, mode: 0, nlink: 0, uid: 0, gid: 0,
            rdev: 0, size: 0, blksize: 0, blocks: 0,
            atime: 0, mtime: 0, ctime: 0,
        }
    }
}

/// Time specification (seconds + nanoseconds)
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Timespec {
    pub tv_sec: i64,
    pub tv_nsec: i64,
}

impl Timespec {
    pub const fn zero() -> Self {
        Self { tv_sec: 0, tv_nsec: 0 }
    }
}

/// Time value (seconds + microseconds)
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Timeval {
    pub tv_sec: i64,
    pub tv_usec: i64,
}

/// Directory entry (must match kernel's dirent layout)
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Dirent {
    pub d_ino: u64,
    pub d_off: u64,
    pub d_reclen: u16,
    pub d_type: u8,
    pub d_name: [u8; 256],
}

impl Dirent {
    pub const fn zeroed() -> Self {
        Self {
            d_ino: 0,
            d_off: 0,
            d_reclen: 0,
            d_type: 0,
            d_name: [0; 256],
        }
    }

    /// Get the name as a byte slice (up to first null)
    pub fn name_bytes(&self) -> &[u8] {
        let len = self.d_name.iter().position(|&b| b == 0).unwrap_or(256);
        &self.d_name[..len]
    }
}

/// Directory entry types
pub mod dt {
    pub const DT_UNKNOWN: u8 = 0;
    pub const DT_FIFO: u8 = 1;
    pub const DT_CHR: u8 = 2;
    pub const DT_DIR: u8 = 4;
    pub const DT_BLK: u8 = 6;
    pub const DT_REG: u8 = 8;
    pub const DT_LNK: u8 = 10;
    pub const DT_SOCK: u8 = 12;
}

/// Socket address (IPv4)
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct SockAddrIn {
    pub sin_family: u16,
    pub sin_port: u16,
    pub sin_addr: InAddr,
    pub sin_zero: [u8; 8],
}

/// IPv4 address
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct InAddr {
    pub s_addr: u32,
}

/// Socket address (IPv6)
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct SockAddrIn6 {
    pub sin6_family: u16,
    pub sin6_port: u16,
    pub sin6_flowinfo: u32,
    pub sin6_addr: In6Addr,
    pub sin6_scope_id: u32,
}

/// IPv6 address
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct In6Addr {
    pub s6_addr: [u8; 16],
}

/// Generic socket address storage
#[repr(C)]
#[derive(Clone, Copy)]
pub struct SockAddrStorage {
    pub ss_family: u16,
    pub __ss_pad: [u8; 126],
}

/// UTS name structure for uname()
#[repr(C)]
pub struct UtsName {
    pub sysname: [u8; 65],
    pub nodename: [u8; 65],
    pub release: [u8; 65],
    pub version: [u8; 65],
    pub machine: [u8; 65],
}

/// Poll file descriptor
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct PollFd {
    pub fd: i32,
    pub events: i16,
    pub revents: i16,
}

/// I/O vector for readv/writev
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct IoVec {
    pub iov_base: *mut u8,
    pub iov_len: usize,
}

/// Signal action structure
#[repr(C)]
#[derive(Clone, Copy)]
pub struct SigAction {
    pub sa_handler: usize,
    pub sa_flags: u64,
    pub sa_restorer: usize,
    pub sa_mask: u64,
}

/// Signal set (bitmask)
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct SigSet {
    pub bits: u64,
}

impl SigSet {
    pub const fn empty() -> Self {
        Self { bits: 0 }
    }

    pub const fn full() -> Self {
        Self { bits: !0 }
    }

    pub fn add(&mut self, sig: i32) {
        if sig > 0 && sig < 64 {
            self.bits |= 1u64 << (sig - 1);
        }
    }

    pub fn remove(&mut self, sig: i32) {
        if sig > 0 && sig < 64 {
            self.bits &= !(1u64 << (sig - 1));
        }
    }

    pub fn contains(&self, sig: i32) -> bool {
        if sig > 0 && sig < 64 {
            (self.bits & (1u64 << (sig - 1))) != 0
        } else {
            false
        }
    }
}

/// File mode bits
pub mod mode {
    pub const S_IFMT: u32 = 0o170000;
    pub const S_IFSOCK: u32 = 0o140000;
    pub const S_IFLNK: u32 = 0o120000;
    pub const S_IFREG: u32 = 0o100000;
    pub const S_IFBLK: u32 = 0o060000;
    pub const S_IFDIR: u32 = 0o040000;
    pub const S_IFCHR: u32 = 0o020000;
    pub const S_IFIFO: u32 = 0o010000;
}

/// Open flags
pub mod oflags {
    pub const O_RDONLY: i32 = 0;
    pub const O_WRONLY: i32 = 1;
    pub const O_RDWR: i32 = 2;
    pub const O_CREAT: i32 = 0o100;
    pub const O_EXCL: i32 = 0o200;
    pub const O_TRUNC: i32 = 0o1000;
    pub const O_APPEND: i32 = 0o2000;
    pub const O_NONBLOCK: i32 = 0o4000;
    pub const O_CLOEXEC: i32 = 0o2000000;
    pub const O_DIRECTORY: i32 = 0o200000;
    pub const O_NOFOLLOW: i32 = 0o400000;
}

/// Socket address families
pub mod af {
    pub const AF_UNSPEC: i32 = 0;
    pub const AF_UNIX: i32 = 1;
    pub const AF_INET: i32 = 2;
    pub const AF_INET6: i32 = 10;
}

/// Socket types
pub mod sock {
    pub const SOCK_STREAM: i32 = 1;
    pub const SOCK_DGRAM: i32 = 2;
    pub const SOCK_RAW: i32 = 3;
    pub const SOCK_NONBLOCK: i32 = 0o4000;
    pub const SOCK_CLOEXEC: i32 = 0o2000000;
}

/// IP protocols
pub mod ipproto {
    pub const IPPROTO_IP: i32 = 0;
    pub const IPPROTO_TCP: i32 = 6;
    pub const IPPROTO_UDP: i32 = 17;
    pub const IPPROTO_IPV6: i32 = 41;
}

/// Socket option levels
pub mod sol {
    pub const SOL_SOCKET: i32 = 1;
    pub const SOL_TCP: i32 = 6;
    pub const SOL_IPV6: i32 = 41;
}

/// Socket options
pub mod so {
    pub const SO_REUSEADDR: i32 = 2;
    pub const SO_ERROR: i32 = 4;
    pub const SO_KEEPALIVE: i32 = 9;
    pub const SO_RCVTIMEO: i32 = 20;
    pub const SO_SNDTIMEO: i32 = 21;
    pub const SO_LINGER: i32 = 13;
    pub const SO_BROADCAST: i32 = 6;
}

/// TCP options
pub mod tcp {
    pub const TCP_NODELAY: i32 = 1;
}

/// Shutdown flags
pub mod shut {
    pub const SHUT_RD: i32 = 0;
    pub const SHUT_WR: i32 = 1;
    pub const SHUT_RDWR: i32 = 2;
}

/// Memory protection flags
pub mod prot {
    pub const PROT_NONE: i32 = 0;
    pub const PROT_READ: i32 = 1;
    pub const PROT_WRITE: i32 = 2;
    pub const PROT_EXEC: i32 = 4;
}

/// Memory map flags
pub mod map_flags {
    pub const MAP_SHARED: i32 = 0x01;
    pub const MAP_PRIVATE: i32 = 0x02;
    pub const MAP_ANONYMOUS: i32 = 0x20;
    pub const MAP_FIXED: i32 = 0x10;
}

/// Clock IDs
pub mod clock {
    pub const CLOCK_REALTIME: i32 = 0;
    pub const CLOCK_MONOTONIC: i32 = 1;
}

/// Seek positions
pub mod seek {
    pub const SEEK_SET: i32 = 0;
    pub const SEEK_CUR: i32 = 1;
    pub const SEEK_END: i32 = 2;
}

/// Futex operations
pub mod futex_op {
    pub const FUTEX_WAIT: i32 = 0;
    pub const FUTEX_WAKE: i32 = 1;
    pub const FUTEX_PRIVATE_FLAG: i32 = 128;
    pub const FUTEX_WAIT_PRIVATE: i32 = FUTEX_WAIT | FUTEX_PRIVATE_FLAG;
    pub const FUTEX_WAKE_PRIVATE: i32 = FUTEX_WAKE | FUTEX_PRIVATE_FLAG;
}

/// Wait options
pub mod wait_flags {
    pub const WNOHANG: i32 = 1;
    pub const WUNTRACED: i32 = 2;
    pub const WCONTINUED: i32 = 8;
}

/// Poll event flags
pub mod poll_events {
    pub const POLLIN: i16 = 0x001;
    pub const POLLOUT: i16 = 0x004;
    pub const POLLERR: i16 = 0x008;
    pub const POLLHUP: i16 = 0x010;
    pub const POLLNVAL: i16 = 0x020;
    pub const POLLRDNORM: i16 = 0x040;
    pub const POLLWRNORM: i16 = 0x100;
}

/// Ioctl requests
pub mod ioctl_nr {
    pub const TIOCGWINSZ: u64 = 0x5413;
    pub const TIOCSWINSZ: u64 = 0x5414;
    pub const TIOCGPGRP: u64 = 0x540F;
    pub const TIOCSPGRP: u64 = 0x5410;
}

/// Window size structure for TIOCGWINSZ/TIOCSWINSZ
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Winsize {
    pub ws_row: u16,
    pub ws_col: u16,
    pub ws_xpixel: u16,
    pub ws_ypixel: u16,
}

/// Clone flags for thread creation
pub mod clone_flags {
    pub const CLONE_VM: u64 = 0x00000100;
    pub const CLONE_FS: u64 = 0x00000200;
    pub const CLONE_FILES: u64 = 0x00000400;
    pub const CLONE_SIGHAND: u64 = 0x00000800;
    pub const CLONE_THREAD: u64 = 0x00010000;
    pub const CLONE_CHILD_SETTID: u64 = 0x01000000;
    pub const CLONE_CHILD_CLEARTID: u64 = 0x00200000;
    pub const CLONE_PARENT_SETTID: u64 = 0x00100000;
}

/// Linger structure for SO_LINGER
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Linger {
    pub l_onoff: i32,
    pub l_linger: i32,
}
