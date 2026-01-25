//! Socket System Calls
//!
//! Implements BSD socket API with loopback networking support.
//!
//! For connections to localhost (127.0.0.0/8), this module implements
//! in-kernel loopback that directly routes data between sockets.

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use alloc::vec::Vec;
use spin::Mutex;

use crate::errno;
use net::socket::{Shutdown, SocketState};
use net::{
    IpAddr, Ipv4Addr, Ipv6Addr, NetError, Socket, SocketAddr, SocketDomain, SocketProtocol,
    SocketType,
};

/// Socket file descriptor offset (to distinguish from file FDs)
const SOCKET_FD_BASE: i32 = 1000;

/// Maximum number of sockets
const MAX_SOCKETS: usize = 256;

/// Global socket table
static SOCKET_TABLE: Mutex<BTreeMap<i32, Arc<Socket>>> = Mutex::new(BTreeMap::new());

/// Next socket FD
static NEXT_SOCKET_FD: Mutex<i32> = Mutex::new(SOCKET_FD_BASE);

/// Listening sockets by port (for loopback connections)
/// Maps port -> (socket fd, socket arc)
static LISTENING_SOCKETS: Mutex<BTreeMap<u16, (i32, Arc<Socket>)>> = Mutex::new(BTreeMap::new());

/// Connected socket pairs (for loopback data routing)
/// Maps socket fd -> peer socket fd
static SOCKET_PAIRS: Mutex<BTreeMap<i32, i32>> = Mutex::new(BTreeMap::new());

/// Next ephemeral port for client sockets
static NEXT_EPHEMERAL_PORT: Mutex<u16> = Mutex::new(49152);

/// Debug print helper (kernel debug output)
#[allow(dead_code)]
fn debug_print(msg: &str) {
    use core::ptr::addr_of;
    unsafe {
        let ctx = addr_of!(crate::SYSCALL_CONTEXT);
        if let Some(write_fn) = (*ctx).console_write {
            write_fn(b"[SOCK] ");
            write_fn(msg.as_bytes());
            write_fn(b"\n");
        }
    }
}

/// Debug print with number
#[allow(dead_code)]
fn debug_print_num(msg: &str, num: i64) {
    use core::ptr::addr_of;
    unsafe {
        let ctx = addr_of!(crate::SYSCALL_CONTEXT);
        if let Some(write_fn) = (*ctx).console_write {
            write_fn(b"[SOCK] ");
            write_fn(msg.as_bytes());
            // Simple number to string
            let mut buf = [0u8; 20];
            let mut n = if num < 0 {
                write_fn(b"-");
                (-num) as u64
            } else {
                num as u64
            };
            let mut i = 0;
            if n == 0 {
                buf[0] = b'0';
                i = 1;
            } else {
                while n > 0 {
                    buf[i] = b'0' + (n % 10) as u8;
                    n /= 10;
                    i += 1;
                }
            }
            // Reverse
            for j in 0..i / 2 {
                buf.swap(j, i - 1 - j);
            }
            write_fn(&buf[..i]);
            write_fn(b"\n");
        }
    }
}

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
        NetError::NoBuffers => errno::ENOBUFS,
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

/// Parse sockaddr_in6 structure from user memory
fn parse_sockaddr_in6(addr: u64, addrlen: u32) -> Option<SocketAddr> {
    // sockaddr_in6: family (2), port (2), flowinfo (4), addr (16), scope_id (4)
    if addrlen < 28 {
        return None;
    }

    unsafe {
        let ptr = addr as *const u8;

        let family = u16::from_ne_bytes([*ptr, *ptr.add(1)]);
        if family != 10 {
            // AF_INET6
            return None;
        }

        let port = u16::from_be_bytes([*ptr.add(2), *ptr.add(3)]);
        let mut ip_bytes = [0u8; 16];
        for i in 0..16 {
            ip_bytes[i] = *ptr.add(8 + i);
        }
        let ip = Ipv6Addr(ip_bytes);

        Some(SocketAddr::new(IpAddr::V6(ip), port))
    }
}

/// Parse sockaddr (IPv4 or IPv6) from user memory
fn parse_sockaddr(addr: u64, addrlen: u32) -> Option<SocketAddr> {
    // Try IPv4 first, then IPv6
    parse_sockaddr_in(addr, addrlen).or_else(|| parse_sockaddr_in6(addr, addrlen))
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
    debug_print_num("bind: fd=", fd as i64);

    // Validate address pointer
    if addr >= 0x0000_8000_0000_0000 {
        return errno::EFAULT;
    }

    let socket = match get_socket(fd) {
        Some(s) => s,
        None => return errno::EBADF,
    };

    let socket_addr = match parse_sockaddr_in(addr, addrlen) {
        Some(a) => {
            debug_print_num("bind: port=", a.port as i64);
            a
        }
        None => return errno::EINVAL,
    };

    match socket.bind(socket_addr) {
        Ok(()) => {
            debug_print("bind: success");
            0
        }
        Err(e) => net_error_to_errno(e),
    }
}

/// sys_listen - Listen for connections on a socket
pub fn sys_listen(fd: i32, backlog: i32) -> i64 {
    debug_print_num("listen: fd=", fd as i64);

    let socket = match get_socket(fd) {
        Some(s) => s,
        None => {
            debug_print("listen: socket not found");
            return errno::EBADF;
        }
    };

    // Get bound port
    let port = match socket.local_addr() {
        Some(addr) => addr.port,
        None => {
            debug_print("listen: socket not bound");
            return errno::EINVAL; // Must bind before listen
        }
    };

    debug_print_num("listen: port=", port as i64);

    match socket.listen(backlog as u32) {
        Ok(()) => {
            // Register as listening socket for loopback connections
            LISTENING_SOCKETS.lock().insert(port, (fd, socket));
            debug_print_num("listen: registered on port ", port as i64);
            0
        }
        Err(e) => {
            debug_print("listen: failed");
            net_error_to_errno(e)
        }
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

    // Check socket state
    let state = *socket.state.lock();
    if state != SocketState::Listen {
        return errno::EINVAL;
    }

    // Try to get a pending connection
    let mut pending = socket.pending.lock();
    if let Some(new_socket) = pending.pop() {
        // Write peer address if requested
        if addr != 0 {
            if let Some(peer) = new_socket.peer_addr() {
                write_sockaddr_in(addr, addrlen, &peer);
            }
        }

        // Allocate FD for new socket
        let new_fd = alloc_socket_fd(new_socket.clone());
        if new_fd < 0 {
            return new_fd;
        }

        // The peer fd was stored in the socket's backlog count temporarily
        // We need to set up the socket pair properly
        // The pending socket already has its peer connection set up

        new_fd
    } else {
        // No pending connections - would block
        // For blocking sockets, we should wait; for now return EAGAIN
        errno::EAGAIN
    }
}

/// Allocate an ephemeral port
fn alloc_ephemeral_port() -> u16 {
    let mut port = NEXT_EPHEMERAL_PORT.lock();
    let p = *port;
    *port = if p >= 65535 { 49152 } else { p + 1 };
    p
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

    let socket_addr = match parse_sockaddr(addr, addrlen) {
        Some(a) => a,
        None => return errno::EINVAL,
    };

    // Check if this is a loopback connection (IPv4 or IPv6)
    let is_loopback = socket_addr.ip.is_loopback();

    if is_loopback && socket.sock_type == SocketType::Stream {
        // Handle loopback TCP connection
        return loopback_connect(fd, socket, socket_addr);
    }

    // Non-loopback: use standard connect (stub for now)
    match socket.connect(socket_addr) {
        Ok(()) => 0,
        Err(e) => net_error_to_errno(e),
    }
}

/// Handle loopback TCP connection
fn loopback_connect(client_fd: i32, client_socket: Arc<Socket>, addr: SocketAddr) -> i64 {
    let port = addr.port;
    debug_print_num("loopback_connect: port=", port as i64);

    // Find listening socket for this port
    let listener = {
        let listeners = LISTENING_SOCKETS.lock();
        debug_print_num("loopback_connect: num_listeners=", listeners.len() as i64);
        for (p, _) in listeners.iter() {
            debug_print_num("loopback_connect: listening_port=", *p as i64);
        }
        listeners.get(&port).cloned()
    };

    let (listener_fd, listener_socket) = match listener {
        Some(l) => {
            debug_print_num("loopback_connect: found listener fd=", l.0 as i64);
            l
        }
        None => {
            debug_print("loopback_connect: no listener - ECONNREFUSED");
            return errno::ECONNREFUSED; // No one listening
        }
    };

    // Allocate ephemeral port for client
    let client_port = alloc_ephemeral_port();
    let client_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), client_port);

    // Set client's local and remote addresses
    *client_socket.local_addr.lock() = Some(client_addr);
    *client_socket.remote_addr.lock() = Some(addr);
    *client_socket.state.lock() = SocketState::Established;

    // Create server-side socket for this connection
    let server_socket = match Socket::new(
        client_socket.domain,
        client_socket.sock_type,
        client_socket.protocol,
    ) {
        Ok(s) => s,
        Err(_) => return errno::ENOMEM,
    };

    // Set server socket's addresses (reverse of client)
    *server_socket.local_addr.lock() = Some(addr);
    *server_socket.remote_addr.lock() = Some(client_addr);
    *server_socket.state.lock() = SocketState::Established;

    // Allocate FD for server socket
    let server_fd = alloc_socket_fd(server_socket.clone());
    if server_fd < 0 {
        return server_fd;
    }

    // Create socket pair for data routing
    {
        let mut pairs = SOCKET_PAIRS.lock();
        pairs.insert(client_fd, server_fd as i32);
        pairs.insert(server_fd as i32, client_fd);
    }

    // Add server socket to listener's pending queue
    listener_socket.pending.lock().push(server_socket);

    0
}

/// ICMP echo request type
const ICMP_ECHO_REQUEST: u8 = 8;
/// ICMP echo reply type
const ICMP_ECHO_REPLY: u8 = 0;

/// Check if an address is loopback (127.0.0.0/8 or ::1)
fn is_loopback_addr(addr: &SocketAddr) -> bool {
    addr.ip.is_loopback()
}

/// Handle raw ICMP loopback - generate echo reply for echo request
fn handle_icmp_loopback(socket: &Arc<Socket>, icmp_data: &[u8]) -> Option<i64> {
    // ICMP header: type(1) + code(1) + checksum(2) + identifier(2) + sequence(2) + data
    if icmp_data.len() < 8 {
        return None;
    }

    let icmp_type = icmp_data[0];
    if icmp_type != ICMP_ECHO_REQUEST {
        return None; // Only handle echo requests
    }

    // Build echo reply
    let mut reply = Vec::new();

    // Build minimal IP header (20 bytes)
    reply.push(0x45); // version=4, IHL=5 (20 bytes)
    reply.push(0x00); // TOS
    let total_len = (20 + icmp_data.len()) as u16;
    reply.extend_from_slice(&total_len.to_be_bytes()); // Total length
    reply.extend_from_slice(&[0x00, 0x00]); // ID
    reply.extend_from_slice(&[0x00, 0x00]); // Flags + Fragment offset
    reply.push(64); // TTL
    reply.push(1);  // Protocol = ICMP
    reply.extend_from_slice(&[0x00, 0x00]); // Checksum (placeholder)
    reply.extend_from_slice(&[127, 0, 0, 1]); // Source IP
    reply.extend_from_slice(&[127, 0, 0, 1]); // Dest IP

    // Calculate IP header checksum
    let ip_checksum = internet_checksum(&reply[..20]);
    reply[10] = (ip_checksum >> 8) as u8;
    reply[11] = ip_checksum as u8;

    // Build ICMP echo reply
    reply.push(ICMP_ECHO_REPLY); // Type = echo reply
    reply.push(0); // Code = 0
    reply.extend_from_slice(&[0x00, 0x00]); // Checksum placeholder
    reply.extend_from_slice(&icmp_data[4..]); // Copy identifier, sequence, and data

    // Calculate ICMP checksum
    let icmp_start = 20;
    let icmp_checksum = internet_checksum(&reply[icmp_start..]);
    reply[icmp_start + 2] = (icmp_checksum >> 8) as u8;
    reply[icmp_start + 3] = icmp_checksum as u8;

    // Put reply in socket's recv buffer
    socket.recv_buf.lock().extend_from_slice(&reply);

    Some(icmp_data.len() as i64)
}

/// Calculate internet checksum
fn internet_checksum(data: &[u8]) -> u16 {
    let mut sum: u32 = 0;
    let mut i = 0;

    while i + 1 < data.len() {
        sum += u16::from_be_bytes([data[i], data[i + 1]]) as u32;
        i += 2;
    }

    if i < data.len() {
        sum += (data[i] as u32) << 8;
    }

    while sum >> 16 != 0 {
        sum = (sum & 0xFFFF) + (sum >> 16);
    }

    !sum as u16
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

    // Check socket state
    let state = *socket.state.lock();
    if state != SocketState::Established && state != SocketState::CloseWait {
        return errno::ENOTCONN;
    }

    let data = unsafe { core::slice::from_raw_parts(buf as *const u8, len) };

    // Handle raw ICMP socket loopback
    if socket.sock_type == SocketType::Raw && socket.protocol == SocketProtocol::Icmp {
        let remote = socket.remote_addr.lock();
        if let Some(ref addr) = *remote {
            if is_loopback_addr(addr) {
                // Handle ICMP loopback - generate echo reply
                if let Some(result) = handle_icmp_loopback(&socket, data) {
                    return result;
                }
                // If not an echo request, just pretend we sent it
                return len as i64;
            }
        }
    }

    // Check if this is a loopback socket pair
    let peer_fd = SOCKET_PAIRS.lock().get(&fd).copied();

    if let Some(peer_fd) = peer_fd {
        // Loopback: send data directly to peer's recv buffer
        let peer_socket = match get_socket(peer_fd) {
            Some(s) => s,
            None => {
                // Peer closed - remove from pairs and signal error
                SOCKET_PAIRS.lock().remove(&fd);
                return errno::EPIPE;
            }
        };

        // Check peer state
        let peer_state = *peer_socket.state.lock();
        if peer_socket
            .closed
            .load(core::sync::atomic::Ordering::SeqCst)
            || peer_state == SocketState::Closed
        {
            return errno::EPIPE;
        }

        // Add data to peer's receive buffer
        peer_socket.recv_buf.lock().extend_from_slice(data);
        return len as i64;
    }

    // Non-loopback: use standard socket send (stub for now)
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

    // Check socket state
    let state = *socket.state.lock();
    match state {
        SocketState::Established | SocketState::CloseWait => {}
        SocketState::Closed => return errno::ENOTCONN,
        _ => return errno::EINVAL,
    }

    let data = unsafe { core::slice::from_raw_parts_mut(buf as *mut u8, len) };

    // For loopback sockets, data arrives directly in recv_buf
    // Check if there's data available
    {
        let mut recv_buf = socket.recv_buf.lock();
        if !recv_buf.is_empty() {
            let to_read = len.min(recv_buf.len());
            data[..to_read].copy_from_slice(&recv_buf[..to_read]);
            recv_buf.drain(..to_read);
            return to_read as i64;
        }
    }

    // Check if peer is closed (for loopback sockets)
    let peer_fd = SOCKET_PAIRS.lock().get(&fd).copied();
    if let Some(peer_fd) = peer_fd {
        let peer_socket = get_socket(peer_fd);
        if peer_socket.is_none() {
            return 0; // EOF - peer closed
        }
        let peer = peer_socket.unwrap();
        if peer.closed.load(core::sync::atomic::Ordering::SeqCst) {
            return 0; // EOF
        }
    }

    // No data available - return EAGAIN
    // Userspace should retry with sched_yield() to let sender run
    errno::EAGAIN
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
    // Remove from socket pairs
    let peer_fd = SOCKET_PAIRS.lock().remove(&fd);
    if let Some(peer) = peer_fd {
        SOCKET_PAIRS.lock().remove(&peer);
    }

    // Remove from listening sockets if this was a listener
    {
        let mut listeners = LISTENING_SOCKETS.lock();
        // Find and remove by fd
        let port_to_remove = listeners
            .iter()
            .find(|(_, (f, _))| *f == fd)
            .map(|(p, _)| *p);
        if let Some(port) = port_to_remove {
            listeners.remove(&port);
        }
    }

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

/// Get socket state information for poll operations
///
/// Returns: (is_connected, has_data, can_send, is_listening, has_pending_connection)
pub fn get_socket_info(fd: i32) -> Option<(bool, bool, bool, bool, bool)> {
    let socket = get_socket(fd)?;

    let state = *socket.state.lock();

    let is_connected = matches!(state, SocketState::Established | SocketState::CloseWait);
    let is_listening = state == SocketState::Listen;

    // Check if receive buffer has data
    let has_data = !socket.recv_buf.lock().is_empty();

    // Check if socket can send (connected and not shut down for writing)
    // For loopback sockets, check if peer is still alive
    let can_send = if is_connected {
        // Check if peer socket still exists for loopback
        let peer_fd = SOCKET_PAIRS.lock().get(&fd).copied();
        if let Some(peer) = peer_fd {
            // Loopback socket - check peer state
            if let Some(peer_socket) = get_socket(peer) {
                !peer_socket
                    .closed
                    .load(core::sync::atomic::Ordering::SeqCst)
            } else {
                false // Peer gone
            }
        } else {
            // Non-loopback socket - assume writable if connected
            true
        }
    } else {
        false
    };

    // Check for pending connections (listening sockets)
    let has_pending_connection = if is_listening {
        !socket.pending.lock().is_empty()
    } else {
        false
    };

    Some((
        is_connected,
        has_data,
        can_send,
        is_listening,
        has_pending_connection,
    ))
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
