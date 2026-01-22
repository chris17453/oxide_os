//! Socket system call wrappers
//!
//! Provides BSD socket API for network programming.

use crate::syscall::{nr, syscall3, syscall4, syscall5, syscall6};

/// Address families
pub mod af {
    pub const UNSPEC: i32 = 0;
    pub const UNIX: i32 = 1;
    pub const LOCAL: i32 = 1; // Alias for UNIX
    pub const INET: i32 = 2;
    pub const INET6: i32 = 10;
    pub const NETLINK: i32 = 16;
    pub const PACKET: i32 = 17;
}

/// Socket types
pub mod sock {
    pub const STREAM: i32 = 1; // TCP
    pub const DGRAM: i32 = 2; // UDP
    pub const RAW: i32 = 3; // Raw socket
    pub const SEQPACKET: i32 = 5; // Sequential packet
    pub const NONBLOCK: i32 = 0x800;
    pub const CLOEXEC: i32 = 0x80000;
}

/// Protocol numbers
pub mod ipproto {
    pub const IP: i32 = 0;
    pub const ICMP: i32 = 1;
    pub const TCP: i32 = 6;
    pub const UDP: i32 = 17;
    pub const IPV6: i32 = 41;
    pub const ICMPV6: i32 = 58;
    pub const RAW: i32 = 255;
}

/// Socket option levels
pub mod sol {
    pub const SOCKET: i32 = 1;
    pub const IP: i32 = 0;
    pub const IPV6: i32 = 41;
    pub const TCP: i32 = 6;
    pub const UDP: i32 = 17;
}

/// Socket options (SOL_SOCKET level)
pub mod so {
    pub const DEBUG: i32 = 1;
    pub const REUSEADDR: i32 = 2;
    pub const TYPE: i32 = 3;
    pub const ERROR: i32 = 4;
    pub const DONTROUTE: i32 = 5;
    pub const BROADCAST: i32 = 6;
    pub const SNDBUF: i32 = 7;
    pub const RCVBUF: i32 = 8;
    pub const KEEPALIVE: i32 = 9;
    pub const OOBINLINE: i32 = 10;
    pub const LINGER: i32 = 13;
    pub const REUSEPORT: i32 = 15;
    pub const RCVTIMEO: i32 = 20;
    pub const SNDTIMEO: i32 = 21;
    pub const ACCEPTCONN: i32 = 30;
}

/// TCP options
pub mod tcp {
    pub const NODELAY: i32 = 1;
    pub const MAXSEG: i32 = 2;
    pub const CORK: i32 = 3;
    pub const KEEPIDLE: i32 = 4;
    pub const KEEPINTVL: i32 = 5;
    pub const KEEPCNT: i32 = 6;
}

/// Shutdown modes
pub mod shut {
    pub const RD: i32 = 0;
    pub const WR: i32 = 1;
    pub const RDWR: i32 = 2;
}

/// Message flags for send/recv
pub mod msg {
    pub const OOB: i32 = 1;
    pub const PEEK: i32 = 2;
    pub const DONTROUTE: i32 = 4;
    pub const WAITALL: i32 = 0x100;
    pub const DONTWAIT: i32 = 0x40;
    pub const NOSIGNAL: i32 = 0x4000;
}

/// IPv4 socket address
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct SockAddrIn {
    pub sin_family: u16,   // AF_INET
    pub sin_port: u16,     // Port (network byte order)
    pub sin_addr: InAddr,  // IPv4 address
    pub sin_zero: [u8; 8], // Padding
}

/// IPv4 address
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct InAddr {
    pub s_addr: u32, // IPv4 address (network byte order)
}

/// IPv6 socket address
#[repr(C)]
#[derive(Clone, Copy)]
pub struct SockAddrIn6 {
    pub sin6_family: u16, // AF_INET6
    pub sin6_port: u16,   // Port (network byte order)
    pub sin6_flowinfo: u32,
    pub sin6_addr: In6Addr, // IPv6 address
    pub sin6_scope_id: u32,
}

/// IPv6 address
#[repr(C)]
#[derive(Clone, Copy)]
pub struct In6Addr {
    pub s6_addr: [u8; 16],
}

impl Default for SockAddrIn6 {
    fn default() -> Self {
        Self {
            sin6_family: 0,
            sin6_port: 0,
            sin6_flowinfo: 0,
            sin6_addr: In6Addr { s6_addr: [0; 16] },
            sin6_scope_id: 0,
        }
    }
}

impl Default for In6Addr {
    fn default() -> Self {
        Self { s6_addr: [0; 16] }
    }
}

/// Unix domain socket address
#[repr(C)]
#[derive(Clone, Copy)]
pub struct SockAddrUn {
    pub sun_family: u16,     // AF_UNIX
    pub sun_path: [u8; 108], // Path name
}

impl Default for SockAddrUn {
    fn default() -> Self {
        Self {
            sun_family: 0,
            sun_path: [0; 108],
        }
    }
}

/// Generic socket address (for type-agnostic syscalls)
#[repr(C)]
#[derive(Clone, Copy)]
pub struct SockAddr {
    pub sa_family: u16,
    pub sa_data: [u8; 14],
}

impl Default for SockAddr {
    fn default() -> Self {
        Self {
            sa_family: 0,
            sa_data: [0; 14],
        }
    }
}

/// Socket address storage (large enough for any address type)
#[repr(C)]
pub union SockAddrStorage {
    pub ss_family: u16,
    pub in4: SockAddrIn,
    pub in6: SockAddrIn6,
    pub un: SockAddrUn,
    pub _storage: [u8; 128],
}

impl Default for SockAddrStorage {
    fn default() -> Self {
        Self { _storage: [0; 128] }
    }
}

// ============================================================================
// Helper functions for byte order conversion
// ============================================================================

/// Convert u16 from host to network byte order
#[inline]
pub fn htons(x: u16) -> u16 {
    x.to_be()
}

/// Convert u16 from network to host byte order
#[inline]
pub fn ntohs(x: u16) -> u16 {
    u16::from_be(x)
}

/// Convert u32 from host to network byte order
#[inline]
pub fn htonl(x: u32) -> u32 {
    x.to_be()
}

/// Convert u32 from network to host byte order
#[inline]
pub fn ntohl(x: u32) -> u32 {
    u32::from_be(x)
}

// ============================================================================
// IP address manipulation
// ============================================================================

/// Create an IPv4 address from four octets
pub fn inet_addr(a: u8, b: u8, c: u8, d: u8) -> u32 {
    u32::from_be_bytes([a, b, c, d])
}

/// Special addresses
pub const INADDR_ANY: u32 = 0;
pub const INADDR_LOOPBACK: u32 = 0x7f000001; // 127.0.0.1 in network byte order
pub const INADDR_BROADCAST: u32 = 0xffffffff;

// ============================================================================
// Socket system call wrappers
// ============================================================================

/// Create a socket
///
/// # Arguments
/// * `domain` - Address family (AF_INET, AF_INET6, AF_UNIX)
/// * `type_` - Socket type (SOCK_STREAM, SOCK_DGRAM, SOCK_RAW)
/// * `protocol` - Protocol (usually 0 for default)
///
/// # Returns
/// Socket file descriptor or negative errno
pub fn socket(domain: i32, type_: i32, protocol: i32) -> i32 {
    syscall3(
        nr::SOCKET,
        domain as usize,
        type_ as usize,
        protocol as usize,
    ) as i32
}

/// Bind a socket to an address
///
/// # Arguments
/// * `sockfd` - Socket file descriptor
/// * `addr` - Address to bind to
/// * `addrlen` - Length of address structure
///
/// # Returns
/// 0 on success, negative errno on error
pub fn bind(sockfd: i32, addr: &SockAddrIn, addrlen: u32) -> i32 {
    syscall3(
        nr::BIND,
        sockfd as usize,
        addr as *const SockAddrIn as usize,
        addrlen as usize,
    ) as i32
}

/// Bind a socket to an IPv6 address
pub fn bind6(sockfd: i32, addr: &SockAddrIn6, addrlen: u32) -> i32 {
    syscall3(
        nr::BIND,
        sockfd as usize,
        addr as *const SockAddrIn6 as usize,
        addrlen as usize,
    ) as i32
}

/// Bind a socket to a Unix domain address
pub fn bind_unix(sockfd: i32, addr: &SockAddrUn, addrlen: u32) -> i32 {
    syscall3(
        nr::BIND,
        sockfd as usize,
        addr as *const SockAddrUn as usize,
        addrlen as usize,
    ) as i32
}

/// Listen for connections on a socket
///
/// # Arguments
/// * `sockfd` - Socket file descriptor
/// * `backlog` - Maximum pending connection queue length
///
/// # Returns
/// 0 on success, negative errno on error
pub fn listen(sockfd: i32, backlog: i32) -> i32 {
    syscall3(nr::LISTEN, sockfd as usize, backlog as usize, 0) as i32
}

/// Accept a connection on a listening socket
///
/// # Arguments
/// * `sockfd` - Listening socket file descriptor
/// * `addr` - Buffer to store peer address (may be null)
/// * `addrlen` - Pointer to address length (in/out)
///
/// # Returns
/// New socket file descriptor or negative errno
pub fn accept(sockfd: i32, addr: Option<&mut SockAddrIn>, addrlen: Option<&mut u32>) -> i32 {
    let addr_ptr = addr.map_or(0, |a| a as *mut SockAddrIn as usize);
    let len_ptr = addrlen.map_or(0, |l| l as *mut u32 as usize);
    syscall3(nr::ACCEPT, sockfd as usize, addr_ptr, len_ptr) as i32
}

/// Connect a socket to a remote address
///
/// # Arguments
/// * `sockfd` - Socket file descriptor
/// * `addr` - Destination address
/// * `addrlen` - Length of address structure
///
/// # Returns
/// 0 on success, negative errno on error
pub fn connect(sockfd: i32, addr: &SockAddrIn, addrlen: u32) -> i32 {
    syscall3(
        nr::CONNECT,
        sockfd as usize,
        addr as *const SockAddrIn as usize,
        addrlen as usize,
    ) as i32
}

/// Connect a socket to an IPv6 remote address
pub fn connect6(sockfd: i32, addr: &SockAddrIn6, addrlen: u32) -> i32 {
    syscall3(
        nr::CONNECT,
        sockfd as usize,
        addr as *const SockAddrIn6 as usize,
        addrlen as usize,
    ) as i32
}

/// Send data on a connected socket
///
/// # Arguments
/// * `sockfd` - Socket file descriptor
/// * `buf` - Data buffer
/// * `flags` - Send flags (MSG_*)
///
/// # Returns
/// Number of bytes sent or negative errno
pub fn send(sockfd: i32, buf: &[u8], flags: i32) -> isize {
    syscall4(
        nr::SEND,
        sockfd as usize,
        buf.as_ptr() as usize,
        buf.len(),
        flags as usize,
    ) as isize
}

/// Receive data from a connected socket
///
/// # Arguments
/// * `sockfd` - Socket file descriptor
/// * `buf` - Receive buffer
/// * `flags` - Receive flags (MSG_*)
///
/// # Returns
/// Number of bytes received or negative errno
pub fn recv(sockfd: i32, buf: &mut [u8], flags: i32) -> isize {
    syscall4(
        nr::RECV,
        sockfd as usize,
        buf.as_mut_ptr() as usize,
        buf.len(),
        flags as usize,
    ) as isize
}

/// Send data to a specific address (UDP)
///
/// # Arguments
/// * `sockfd` - Socket file descriptor
/// * `buf` - Data buffer
/// * `flags` - Send flags
/// * `dest_addr` - Destination address
/// * `addrlen` - Length of address structure
///
/// # Returns
/// Number of bytes sent or negative errno
pub fn sendto(sockfd: i32, buf: &[u8], flags: i32, dest_addr: &SockAddrIn, addrlen: u32) -> isize {
    syscall6(
        nr::SENDTO,
        sockfd as usize,
        buf.as_ptr() as usize,
        buf.len(),
        flags as usize,
        dest_addr as *const SockAddrIn as usize,
        addrlen as usize,
    ) as isize
}

/// Receive data with source address (UDP)
///
/// # Arguments
/// * `sockfd` - Socket file descriptor
/// * `buf` - Receive buffer
/// * `flags` - Receive flags
/// * `src_addr` - Buffer to store source address
/// * `addrlen` - Pointer to address length (in/out)
///
/// # Returns
/// Number of bytes received or negative errno
pub fn recvfrom(
    sockfd: i32,
    buf: &mut [u8],
    flags: i32,
    src_addr: Option<&mut SockAddrIn>,
    addrlen: Option<&mut u32>,
) -> isize {
    let addr_ptr = src_addr.map_or(0, |a| a as *mut SockAddrIn as usize);
    let len_ptr = addrlen.map_or(0, |l| l as *mut u32 as usize);
    syscall6(
        nr::RECVFROM,
        sockfd as usize,
        buf.as_mut_ptr() as usize,
        buf.len(),
        flags as usize,
        addr_ptr,
        len_ptr,
    ) as isize
}

/// Shut down part of a full-duplex connection
///
/// # Arguments
/// * `sockfd` - Socket file descriptor
/// * `how` - SHUT_RD, SHUT_WR, or SHUT_RDWR
///
/// # Returns
/// 0 on success, negative errno on error
pub fn shutdown(sockfd: i32, how: i32) -> i32 {
    syscall3(nr::SHUTDOWN, sockfd as usize, how as usize, 0) as i32
}

/// Get socket local address
pub fn getsockname(sockfd: i32, addr: &mut SockAddrIn, addrlen: &mut u32) -> i32 {
    syscall3(
        nr::GETSOCKNAME,
        sockfd as usize,
        addr as *mut SockAddrIn as usize,
        addrlen as *mut u32 as usize,
    ) as i32
}

/// Get socket peer address
pub fn getpeername(sockfd: i32, addr: &mut SockAddrIn, addrlen: &mut u32) -> i32 {
    syscall3(
        nr::GETPEERNAME,
        sockfd as usize,
        addr as *mut SockAddrIn as usize,
        addrlen as *mut u32 as usize,
    ) as i32
}

/// Set socket option
///
/// # Arguments
/// * `sockfd` - Socket file descriptor
/// * `level` - Option level (SOL_SOCKET, IPPROTO_TCP, etc.)
/// * `optname` - Option name
/// * `optval` - Option value
///
/// # Returns
/// 0 on success, negative errno on error
pub fn setsockopt<T>(sockfd: i32, level: i32, optname: i32, optval: &T) -> i32 {
    syscall5(
        nr::SETSOCKOPT,
        sockfd as usize,
        level as usize,
        optname as usize,
        optval as *const T as usize,
        core::mem::size_of::<T>(),
    ) as i32
}

/// Set socket option with raw bytes
pub fn setsockopt_raw(sockfd: i32, level: i32, optname: i32, optval: &[u8]) -> i32 {
    syscall5(
        nr::SETSOCKOPT,
        sockfd as usize,
        level as usize,
        optname as usize,
        optval.as_ptr() as usize,
        optval.len(),
    ) as i32
}

/// Get socket option
pub fn getsockopt<T>(
    sockfd: i32,
    level: i32,
    optname: i32,
    optval: &mut T,
    optlen: &mut u32,
) -> i32 {
    syscall5(
        nr::GETSOCKOPT,
        sockfd as usize,
        level as usize,
        optname as usize,
        optval as *mut T as usize,
        optlen as *mut u32 as usize,
    ) as i32
}

// ============================================================================
// Convenience functions
// ============================================================================

/// Create a TCP socket (IPv4)
pub fn tcp_socket() -> i32 {
    socket(af::INET, sock::STREAM, ipproto::TCP)
}

/// Create a UDP socket (IPv4)
pub fn udp_socket() -> i32 {
    socket(af::INET, sock::DGRAM, ipproto::UDP)
}

/// Create a TCP socket (IPv6)
pub fn tcp6_socket() -> i32 {
    socket(af::INET6, sock::STREAM, ipproto::TCP)
}

/// Create a UDP socket (IPv6)
pub fn udp6_socket() -> i32 {
    socket(af::INET6, sock::DGRAM, ipproto::UDP)
}

/// Create an IPv4 socket address
pub fn sockaddr_in(port: u16, addr: u32) -> SockAddrIn {
    SockAddrIn {
        sin_family: af::INET as u16,
        sin_port: htons(port),
        sin_addr: InAddr { s_addr: addr },
        sin_zero: [0; 8],
    }
}

/// Create an IPv4 socket address from four octets
pub fn sockaddr_in_octets(port: u16, a: u8, b: u8, c: u8, d: u8) -> SockAddrIn {
    sockaddr_in(port, inet_addr(a, b, c, d))
}

/// Size of IPv4 socket address structure
pub const SOCKADDR_IN_SIZE: u32 = core::mem::size_of::<SockAddrIn>() as u32;

/// Size of IPv6 socket address structure
pub const SOCKADDR_IN6_SIZE: u32 = core::mem::size_of::<SockAddrIn6>() as u32;

/// Size of Unix socket address structure
pub const SOCKADDR_UN_SIZE: u32 = core::mem::size_of::<SockAddrUn>() as u32;
