//! Socket System Calls
//!
//! Implements BSD socket API by connecting to the net network stack.

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use spin::Mutex;

use crate::errno;
use net::socket::Shutdown;
use net::{
    IpAddr, Ipv4Addr, NetError, Socket, SocketAddr, SocketDomain, SocketProtocol, SocketType,
};

/// Socket file descriptor offset (to distinguish from file FDs)
const SOCKET_FD_BASE: i32 = 1000;

/// Maximum number of sockets
const MAX_SOCKETS: usize = 256;

/// Global socket table
static SOCKET_TABLE: Mutex<BTreeMap<i32, Arc<Socket>>> = Mutex::new(BTreeMap::new());

/// Next socket FD
static NEXT_SOCKET_FD: Mutex<i32> = Mutex::new(SOCKET_FD_BASE);

/// Allocate a new socket file descriptor
fn alloc_socket_fd(socket: Arc<Socket>) -> i64 {
    let mut next_fd = NEXT_SOCKET_FD.lock();
    let mut table = SOCKET_TABLE.lock();

    if table.len() >= MAX_SOCKETS {
        return errno::EMFILE;
    }

    let fd = *next_fd;
    *next_fd += 1;

    table.insert(fd, socket);
    fd as i64
}

/// Get socket by file descriptor
fn get_socket(fd: i32) -> Option<Arc<Socket>> {
    SOCKET_TABLE.lock().get(&fd).cloned()
}

/// Remove socket from table
fn remove_socket(fd: i32) -> Option<Arc<Socket>> {
    SOCKET_TABLE.lock().remove(&fd)
}

/// Convert network error to errno
fn net_error_to_errno(e: NetError) -> i64 {
    match e {
        NetError::NotFound => errno::ENOENT,
        NetError::DeviceDown => errno::ENETUNREACH,
        NetError::NoRoute => errno::ENETUNREACH,
        NetError::HostUnreachable => errno::EHOSTUNREACH,
        NetError::ConnectionRefused => errno::ECONNREFUSED,
        NetError::ConnectionReset => errno::ECONNRESET,
        NetError::TimedOut | NetError::Timeout => errno::ETIMEDOUT,
        NetError::AddrInUse => errno::EADDRINUSE,
        NetError::AddrNotAvailable => errno::EADDRNOTAVAIL,
        NetError::AddressFamilyNotSupported => errno::EINVAL,
        NetError::NetworkUnreachable => errno::ENETUNREACH,
        NetError::WouldBlock => errno::EAGAIN,
        NetError::InvalidArgument => errno::EINVAL,
        NetError::NotConnected => errno::ENOTCONN,
        NetError::AlreadyConnected => errno::EISCONN,
        NetError::BufferTooSmall => errno::EINVAL,
        NetError::ProtocolNotSupported => errno::EINVAL,
        NetError::SocketTypeNotSupported => errno::EINVAL,
        NetError::IoError => EIO,
        NetError::PermissionDenied => errno::EPERM,
    }
}

/// I/O error errno
const EIO: i64 = -5;

/// sys_socket - Create a socket
///
/// # Arguments
/// * `domain` - Protocol family (AF_INET, AF_INET6, AF_UNIX)
/// * `sock_type` - Socket type (SOCK_STREAM, SOCK_DGRAM, SOCK_RAW)
/// * `protocol` - Protocol (usually 0 for default)
///
/// # Returns
/// Socket file descriptor or negative errno
pub fn sys_socket(domain: i32, sock_type: i32, protocol: i32) -> i64 {
    // Parse domain
    let socket_domain = match domain {
        1 => SocketDomain::Unix,
        2 => SocketDomain::Inet,
        10 => SocketDomain::Inet6,
        _ => return errno::EINVAL,
    };

    // Parse socket type (strip flags)
    let socket_type = match sock_type & 0x0F {
        1 => SocketType::Stream,
        2 => SocketType::Dgram,
        3 => SocketType::Raw,
        5 => SocketType::SeqPacket,
        _ => return errno::EINVAL,
    };

    // Parse protocol
    let socket_protocol = match protocol {
        0 => SocketProtocol::Default,
        1 => SocketProtocol::Icmp,
        6 => SocketProtocol::Tcp,
        17 => SocketProtocol::Udp,
        58 => SocketProtocol::Icmpv6,
        _ => return errno::EINVAL,
    };

    // Create socket
    match Socket::new(socket_domain, socket_type, socket_protocol) {
        Ok(socket) => {
            // Handle NONBLOCK and CLOEXEC flags
            if sock_type & 0x800 != 0 {
                socket.set_nonblocking(true);
            }

            alloc_socket_fd(socket)
        }
        Err(e) => net_error_to_errno(e),
    }
}

/// Parse sockaddr_in structure from user memory
fn parse_sockaddr_in(addr: u64, addrlen: u32) -> Option<SocketAddr> {
    if addrlen < 16 {
        return None;
    }

    unsafe {
        let ptr = addr as *const u8;

        // sockaddr_in: family (2), port (2), addr (4), zero (8)
        let family = u16::from_ne_bytes([*ptr, *ptr.add(1)]);
        if family != 2 {
            // AF_INET
            return None;
        }

        let port = u16::from_be_bytes([*ptr.add(2), *ptr.add(3)]);
        let ip_bytes = [*ptr.add(4), *ptr.add(5), *ptr.add(6), *ptr.add(7)];
        let ip = Ipv4Addr::new(ip_bytes[0], ip_bytes[1], ip_bytes[2], ip_bytes[3]);

        Some(SocketAddr::new(IpAddr::V4(ip), port))
    }
}

/// Write sockaddr_in structure to user memory
fn write_sockaddr_in(addr: u64, addrlen: u64, socket_addr: &SocketAddr) -> i64 {
    if addr == 0 {
        return 0;
    }

    unsafe {
        let ptr = addr as *mut u8;
        let len_ptr = addrlen as *mut u32;

        // Write family
        let family: u16 = 2; // AF_INET
        *ptr = family as u8;
        *ptr.add(1) = (family >> 8) as u8;

        // Write port (big-endian)
        let port_be = socket_addr.port.to_be_bytes();
        *ptr.add(2) = port_be[0];
        *ptr.add(3) = port_be[1];

        // Write IP address
        if let IpAddr::V4(ip) = socket_addr.ip {
            let bytes = ip.as_bytes();
            *ptr.add(4) = bytes[0];
            *ptr.add(5) = bytes[1];
            *ptr.add(6) = bytes[2];
            *ptr.add(7) = bytes[3];
        }

        // Zero padding
        for i in 8..16 {
            *ptr.add(i) = 0;
        }

        // Write length
        if !len_ptr.is_null() {
            *len_ptr = 16;
        }
    }

    0
}

/// sys_bind - Bind a socket to an address
pub fn sys_bind(fd: i32, addr: u64, addrlen: u32) -> i64 {
    // Validate address pointer
    if addr >= 0x0000_8000_0000_0000 {
        return errno::EFAULT;
    }

    let socket = match get_socket(fd) {
        Some(s) => s,
        None => return errno::EBADF,
    };

    let socket_addr = match parse_sockaddr_in(addr, addrlen) {
        Some(a) => a,
        None => return errno::EINVAL,
    };

    match socket.bind(socket_addr) {
        Ok(()) => 0,
        Err(e) => net_error_to_errno(e),
    }
}

/// sys_listen - Listen for connections on a socket
pub fn sys_listen(fd: i32, backlog: i32) -> i64 {
    let socket = match get_socket(fd) {
        Some(s) => s,
        None => return errno::EBADF,
    };

    match socket.listen(backlog as u32) {
        Ok(()) => 0,
        Err(e) => net_error_to_errno(e),
    }
}

/// sys_accept - Accept a connection on a socket
pub fn sys_accept(fd: i32, addr: u64, addrlen: u64) -> i64 {
    // Validate pointers if provided
    if addr != 0 && addr >= 0x0000_8000_0000_0000 {
        return errno::EFAULT;
    }
    if addrlen != 0 && addrlen >= 0x0000_8000_0000_0000 {
        return errno::EFAULT;
    }

    let socket = match get_socket(fd) {
        Some(s) => s,
        None => return errno::EBADF,
    };

    match socket.accept() {
        Ok(new_socket) => {
            // Write peer address if requested
            if addr != 0 {
                if let Some(peer) = new_socket.peer_addr() {
                    write_sockaddr_in(addr, addrlen, &peer);
                }
            }

            alloc_socket_fd(new_socket)
        }
        Err(e) => net_error_to_errno(e),
    }
}

/// sys_connect - Connect a socket to a remote address
pub fn sys_connect(fd: i32, addr: u64, addrlen: u32) -> i64 {
    if addr >= 0x0000_8000_0000_0000 {
        return errno::EFAULT;
    }

    let socket = match get_socket(fd) {
        Some(s) => s,
        None => return errno::EBADF,
    };

    let socket_addr = match parse_sockaddr_in(addr, addrlen) {
        Some(a) => a,
        None => return errno::EINVAL,
    };

    match socket.connect(socket_addr) {
        Ok(()) => 0,
        Err(e) => net_error_to_errno(e),
    }
}

/// sys_send - Send data on a connected socket
pub fn sys_send(fd: i32, buf: u64, len: usize, flags: i32) -> i64 {
    if buf >= 0x0000_8000_0000_0000 {
        return errno::EFAULT;
    }
    if buf.saturating_add(len as u64) >= 0x0000_8000_0000_0000 {
        return errno::EFAULT;
    }

    let socket = match get_socket(fd) {
        Some(s) => s,
        None => return errno::EBADF,
    };

    let data = unsafe { core::slice::from_raw_parts(buf as *const u8, len) };

    // Handle MSG_DONTWAIT
    let was_nonblocking = socket.is_nonblocking();
    if flags & 0x40 != 0 && !was_nonblocking {
        socket.set_nonblocking(true);
    }

    let result = match socket.send(data) {
        Ok(n) => n as i64,
        Err(e) => net_error_to_errno(e),
    };

    // Restore nonblocking state
    if flags & 0x40 != 0 && !was_nonblocking {
        socket.set_nonblocking(false);
    }

    result
}

/// sys_recv - Receive data from a connected socket
pub fn sys_recv(fd: i32, buf: u64, len: usize, flags: i32) -> i64 {
    if buf >= 0x0000_8000_0000_0000 {
        return errno::EFAULT;
    }
    if buf.saturating_add(len as u64) >= 0x0000_8000_0000_0000 {
        return errno::EFAULT;
    }

    let socket = match get_socket(fd) {
        Some(s) => s,
        None => return errno::EBADF,
    };

    let data = unsafe { core::slice::from_raw_parts_mut(buf as *mut u8, len) };

    // Handle MSG_DONTWAIT
    let was_nonblocking = socket.is_nonblocking();
    if flags & 0x40 != 0 && !was_nonblocking {
        socket.set_nonblocking(true);
    }

    let result = match socket.recv(data) {
        Ok(n) => n as i64,
        Err(e) => net_error_to_errno(e),
    };

    // Restore nonblocking state
    if flags & 0x40 != 0 && !was_nonblocking {
        socket.set_nonblocking(false);
    }

    result
}

/// sys_sendto - Send data to a specific address (UDP)
pub fn sys_sendto(fd: i32, buf: u64, len: usize, flags: i32, dest_addr: u64, addrlen: u32) -> i64 {
    if buf >= 0x0000_8000_0000_0000 {
        return errno::EFAULT;
    }
    if buf.saturating_add(len as u64) >= 0x0000_8000_0000_0000 {
        return errno::EFAULT;
    }
    if dest_addr != 0 && dest_addr >= 0x0000_8000_0000_0000 {
        return errno::EFAULT;
    }

    let socket = match get_socket(fd) {
        Some(s) => s,
        None => return errno::EBADF,
    };

    let data = unsafe { core::slice::from_raw_parts(buf as *const u8, len) };

    let dest = if dest_addr != 0 {
        match parse_sockaddr_in(dest_addr, addrlen) {
            Some(a) => a,
            None => return errno::EINVAL,
        }
    } else {
        // Use connected address
        match socket.peer_addr() {
            Some(a) => a,
            None => return errno::ENOTCONN,
        }
    };

    let _ = flags; // Flags not fully supported yet

    match socket.sendto(data, dest) {
        Ok(n) => n as i64,
        Err(e) => net_error_to_errno(e),
    }
}

/// sys_recvfrom - Receive data with source address (UDP)
pub fn sys_recvfrom(fd: i32, buf: u64, len: usize, flags: i32, src_addr: u64, addrlen: u64) -> i64 {
    if buf >= 0x0000_8000_0000_0000 {
        return errno::EFAULT;
    }
    if buf.saturating_add(len as u64) >= 0x0000_8000_0000_0000 {
        return errno::EFAULT;
    }
    if src_addr != 0 && src_addr >= 0x0000_8000_0000_0000 {
        return errno::EFAULT;
    }
    if addrlen != 0 && addrlen >= 0x0000_8000_0000_0000 {
        return errno::EFAULT;
    }

    let socket = match get_socket(fd) {
        Some(s) => s,
        None => return errno::EBADF,
    };

    let data = unsafe { core::slice::from_raw_parts_mut(buf as *mut u8, len) };

    let _ = flags; // Flags not fully supported yet

    match socket.recvfrom(data) {
        Ok((n, addr)) => {
            if src_addr != 0 {
                write_sockaddr_in(src_addr, addrlen, &addr);
            }
            n as i64
        }
        Err(e) => net_error_to_errno(e),
    }
}

/// sys_shutdown - Shut down part of a full-duplex connection
pub fn sys_shutdown(fd: i32, how: i32) -> i64 {
    let shutdown_mode = match how {
        0 => Shutdown::Read,
        1 => Shutdown::Write,
        2 => Shutdown::Both,
        _ => return errno::EINVAL,
    };

    let socket = match get_socket(fd) {
        Some(s) => s,
        None => return errno::EBADF,
    };

    match socket.shutdown(shutdown_mode) {
        Ok(()) => 0,
        Err(e) => net_error_to_errno(e),
    }
}

/// sys_getsockname - Get socket local address
pub fn sys_getsockname(fd: i32, addr: u64, addrlen: u64) -> i64 {
    if addr >= 0x0000_8000_0000_0000 {
        return errno::EFAULT;
    }
    if addrlen >= 0x0000_8000_0000_0000 {
        return errno::EFAULT;
    }

    let socket = match get_socket(fd) {
        Some(s) => s,
        None => return errno::EBADF,
    };

    match socket.local_addr() {
        Some(local) => write_sockaddr_in(addr, addrlen, &local),
        None => errno::EINVAL,
    }
}

/// sys_getpeername - Get socket peer address
pub fn sys_getpeername(fd: i32, addr: u64, addrlen: u64) -> i64 {
    if addr >= 0x0000_8000_0000_0000 {
        return errno::EFAULT;
    }
    if addrlen >= 0x0000_8000_0000_0000 {
        return errno::EFAULT;
    }

    let socket = match get_socket(fd) {
        Some(s) => s,
        None => return errno::EBADF,
    };

    match socket.peer_addr() {
        Some(peer) => write_sockaddr_in(addr, addrlen, &peer),
        None => errno::ENOTCONN,
    }
}

/// sys_setsockopt - Set socket option
pub fn sys_setsockopt(fd: i32, level: i32, optname: i32, optval: u64, optlen: u32) -> i64 {
    if optval >= 0x0000_8000_0000_0000 {
        return errno::EFAULT;
    }

    let socket = match get_socket(fd) {
        Some(s) => s,
        None => return errno::EBADF,
    };

    let mut opts = socket.options.lock();

    // SOL_SOCKET level
    if level == 1 {
        match optname {
            2 => {
                // SO_REUSEADDR
                if optlen >= 4 {
                    let val = unsafe { *(optval as *const i32) };
                    opts.reuse_addr = val != 0;
                }
            }
            6 => {
                // SO_BROADCAST
                if optlen >= 4 {
                    let val = unsafe { *(optval as *const i32) };
                    opts.broadcast = val != 0;
                }
            }
            9 => {
                // SO_KEEPALIVE
                if optlen >= 4 {
                    let val = unsafe { *(optval as *const i32) };
                    opts.keep_alive = val != 0;
                }
            }
            7 => {
                // SO_SNDBUF
                if optlen >= 4 {
                    let val = unsafe { *(optval as *const i32) };
                    opts.send_buf_size = val as u32;
                }
            }
            8 => {
                // SO_RCVBUF
                if optlen >= 4 {
                    let val = unsafe { *(optval as *const i32) };
                    opts.recv_buf_size = val as u32;
                }
            }
            _ => {}
        }
    }
    // IPPROTO_TCP level
    else if level == 6 {
        if optname == 1 {
            // TCP_NODELAY
            if optlen >= 4 {
                let val = unsafe { *(optval as *const i32) };
                opts.tcp_nodelay = val != 0;
            }
        }
    }

    0
}

/// sys_getsockopt - Get socket option
pub fn sys_getsockopt(fd: i32, level: i32, optname: i32, optval: u64, optlen: u64) -> i64 {
    if optval >= 0x0000_8000_0000_0000 {
        return errno::EFAULT;
    }
    if optlen >= 0x0000_8000_0000_0000 {
        return errno::EFAULT;
    }

    let socket = match get_socket(fd) {
        Some(s) => s,
        None => return errno::EBADF,
    };

    let opts = socket.options.lock();
    let len_ptr = optlen as *mut u32;

    // SOL_SOCKET level
    if level == 1 {
        match optname {
            2 => {
                // SO_REUSEADDR
                unsafe {
                    *(optval as *mut i32) = if opts.reuse_addr { 1 } else { 0 };
                    if !len_ptr.is_null() {
                        *len_ptr = 4;
                    }
                }
            }
            6 => {
                // SO_BROADCAST
                unsafe {
                    *(optval as *mut i32) = if opts.broadcast { 1 } else { 0 };
                    if !len_ptr.is_null() {
                        *len_ptr = 4;
                    }
                }
            }
            9 => {
                // SO_KEEPALIVE
                unsafe {
                    *(optval as *mut i32) = if opts.keep_alive { 1 } else { 0 };
                    if !len_ptr.is_null() {
                        *len_ptr = 4;
                    }
                }
            }
            4 => {
                // SO_ERROR
                unsafe {
                    *(optval as *mut i32) = 0; // No pending error
                    if !len_ptr.is_null() {
                        *len_ptr = 4;
                    }
                }
            }
            _ => {}
        }
    }
    // IPPROTO_TCP level
    else if level == 6 {
        if optname == 1 {
            // TCP_NODELAY
            unsafe {
                *(optval as *mut i32) = if opts.tcp_nodelay { 1 } else { 0 };
                if !len_ptr.is_null() {
                    *len_ptr = 4;
                }
            }
        }
    }

    0
}

/// Close a socket (called from close syscall)
pub fn close_socket(fd: i32) -> i64 {
    match remove_socket(fd) {
        Some(socket) => {
            let _ = socket.close();
            0
        }
        None => errno::EBADF,
    }
}

/// Check if fd is a socket
pub fn is_socket_fd(fd: i32) -> bool {
    fd >= SOCKET_FD_BASE && SOCKET_TABLE.lock().contains_key(&fd)
}

// Keep the constants for compatibility
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
    pub const RCVTIMEO: i32 = 20;
    pub const SNDTIMEO: i32 = 21;
    pub const ACCEPTCONN: i32 = 30;
}

/// Address families
pub mod af {
    pub const UNSPEC: i32 = 0;
    pub const UNIX: i32 = 1;
    pub const INET: i32 = 2;
    pub const INET6: i32 = 10;
}

/// Socket types
pub mod sock {
    pub const STREAM: i32 = 1;
    pub const DGRAM: i32 = 2;
    pub const RAW: i32 = 3;
    pub const SEQPACKET: i32 = 5;
    // Flags (OR'd with type)
    pub const NONBLOCK: i32 = 0x800;
    pub const CLOEXEC: i32 = 0x80000;
}

/// Shutdown modes
pub mod shut {
    pub const RD: i32 = 0;
    pub const WR: i32 = 1;
    pub const RDWR: i32 = 2;
}

/// Message flags
pub mod msg {
    pub const OOB: i32 = 1;
    pub const PEEK: i32 = 2;
    pub const DONTROUTE: i32 = 4;
    pub const WAITALL: i32 = 0x100;
    pub const DONTWAIT: i32 = 0x40;
    pub const NOSIGNAL: i32 = 0x4000;
}
