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
use crate::uaccess;
use net::socket::{Shutdown, SocketState};
use net::{
    IpAddr, Ipv4Addr, Ipv6Addr, NetError, Socket, SocketAddr, SocketDomain, SocketProtocol,
    SocketType,
};
use tcpip; // ShadePacket: Network stack polling for packet reception

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

/// Unified loopback socket registry
/// -- ShadePacket: One registry to rule them all, BOUND_SOCKETS is dead weight now
/// Maps (port, protocol_num) -> socket fd for ALL socket types
/// Protocol: 0=any, 1=ICMP, 6=TCP, 17=UDP, 58=ICMPv6
static LOOPBACK_REGISTRY: Mutex<BTreeMap<(u16, u8), i32>> = Mutex::new(BTreeMap::new());

/// Non-loopback TCP connections
/// —ShadePacket: Maps socket fd -> TCP connection ID in the tcpip stack
/// This allows send/recv to use the real TCP/IP stack for external connections
static TCP_CONNECTIONS: Mutex<BTreeMap<i32, u32>> = Mutex::new(BTreeMap::new());

/// Serial debug print helper (goes to serial port)
#[allow(dead_code)]
fn serial_print(msg: &str) {
    use core::ptr::addr_of;
    unsafe {
        let ctx = addr_of!(crate::SYSCALL_CONTEXT);
        if let Some(write_fn) = (*ctx).serial_write {
            write_fn(b"[SOCK] ");
            write_fn(msg.as_bytes());
            write_fn(b"\n");
        }
    }
}

/// Serial debug print with number
#[allow(dead_code)]
fn serial_print_num(msg: &str, num: i64) {
    use core::ptr::addr_of;
    unsafe {
        let ctx = addr_of!(crate::SYSCALL_CONTEXT);
        if let Some(write_fn) = (*ctx).serial_write {
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

/// Debug print helper (no-op in production)
#[allow(dead_code)]
fn debug_print(_msg: &str) {}

/// Debug print with number (no-op in production)
#[allow(dead_code)]
fn debug_print_num(_msg: &str, _num: i64) {}

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

// ============================================================================
// UNIFIED LOOPBACK SYSTEM
// ============================================================================

/// Convert SocketProtocol to protocol number
fn protocol_to_num(protocol: SocketProtocol) -> u8 {
    match protocol {
        SocketProtocol::Default => 0,
        SocketProtocol::Icmp => 1,
        SocketProtocol::Tcp => 6,
        SocketProtocol::Udp => 17,
        SocketProtocol::Icmpv6 => 58,
    }
}

/// Check if an address is loopback (127.0.0.0/8 or ::1)
fn is_loopback_addr(addr: &SocketAddr) -> bool {
    addr.ip.is_loopback()
}

/// Register a socket in the unified loopback registry
fn register_loopback_socket(port: u16, protocol: u8, fd: i32) {
    LOOPBACK_REGISTRY.lock().insert((port, protocol), fd);
}

/// Unregister a socket from the loopback registry
fn unregister_loopback_socket(port: u16, protocol: u8) {
    LOOPBACK_REGISTRY.lock().remove(&(port, protocol));
}

/// Unregister all entries for a given fd from loopback registry
fn unregister_loopback_socket_by_fd(fd: i32) {
    let mut registry = LOOPBACK_REGISTRY.lock();
    let keys_to_remove: Vec<_> = registry
        .iter()
        .filter(|&(_, &f)| f == fd)
        .map(|(k, _)| *k)
        .collect();
    for key in keys_to_remove {
        registry.remove(&key);
    }
}

/// Find a socket in the loopback registry by destination
fn find_loopback_socket(port: u16, protocol: u8) -> Option<Arc<Socket>> {
    let registry = LOOPBACK_REGISTRY.lock();

    // Try exact match first
    if let Some(&fd) = registry.get(&(port, protocol)) {
        return get_socket(fd);
    }

    // For RAW sockets, also try protocol-only match (port 0)
    if protocol != 0 {
        if let Some(&fd) = registry.get(&(0, protocol)) {
            return get_socket(fd);
        }
    }

    None
}

/// Deliver a packet to a socket's receive buffer
/// Returns the number of bytes delivered, or negative errno on error
fn deliver_loopback_packet(dest_socket: &Arc<Socket>, data: &[u8], _src_addr: SocketAddr) -> i64 {
    // Use the socket's existing recv_buf for loopback delivery
    let mut recv_buf = dest_socket.recv_buf.lock();
    recv_buf.extend_from_slice(data);
    data.len() as i64
}

/// Receive a packet from recv_buf (used for loopback)
/// Returns (data_len, source_addr) or None if buffer is empty
fn receive_loopback_packet(socket: &Arc<Socket>, buf: &mut [u8]) -> Option<(usize, SocketAddr)> {
    let mut recv_buf = socket.recv_buf.lock();

    if recv_buf.is_empty() {
        return None;
    }

    let copy_len = buf.len().min(recv_buf.len());
    buf[..copy_len].copy_from_slice(&recv_buf[..copy_len]);
    recv_buf.drain(..copy_len);

    // Return localhost as source address for loopback
    Some((
        copy_len,
        SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0),
    ))
}

/// ICMP echo request type
const ICMP_ECHO_REQUEST: u8 = 8;
/// ICMP echo reply type
const ICMP_ECHO_REPLY: u8 = 0;

/// Handle ICMP echo request -> generate echo reply
/// This is the ONLY place ICMP loopback is handled
fn handle_icmp_echo(socket: &Arc<Socket>, icmp_data: &[u8], _src_addr: SocketAddr) -> Option<i64> {
    debug_print_num("handle_icmp_echo: icmp_data.len=", icmp_data.len() as i64);

    // ICMP header: type(1) + code(1) + checksum(2) + identifier(2) + sequence(2) + data
    if icmp_data.len() < 8 {
        debug_print("handle_icmp_echo: data too short");
        return None;
    }

    let icmp_type = icmp_data[0];
    debug_print_num("handle_icmp_echo: icmp_type=", icmp_type as i64);
    if icmp_type != ICMP_ECHO_REQUEST {
        debug_print("handle_icmp_echo: not echo request");
        return None; // Only handle echo requests
    }

    // Build echo reply with IP header
    let mut reply = Vec::new();

    // Build minimal IP header (20 bytes)
    reply.push(0x45); // version=4, IHL=5 (20 bytes)
    reply.push(0x00); // TOS
    let total_len = (20 + icmp_data.len()) as u16;
    reply.extend_from_slice(&total_len.to_be_bytes());
    reply.extend_from_slice(&[0x00, 0x00]); // ID
    reply.extend_from_slice(&[0x00, 0x00]); // Flags + Fragment offset
    reply.push(64); // TTL
    reply.push(1); // Protocol = ICMP
    reply.extend_from_slice(&[0x00, 0x00]); // Checksum placeholder
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

    // Deliver reply via loopback queue (with proper source address)
    let reply_src = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0);
    debug_print_num("handle_icmp_echo: reply.len=", reply.len() as i64);
    deliver_loopback_packet(socket, &reply, reply_src);
    debug_print("handle_icmp_echo: delivered to queue");

    Some(icmp_data.len() as i64)
}

/// Calculate internet checksum (RFC 1071)
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

/// Unified loopback send handler
/// Handles ALL loopback sends (TCP, UDP, RAW, ICMP) in one place
fn loopback_send(socket: &Arc<Socket>, data: &[u8], dest: &SocketAddr) -> i64 {
    let protocol = protocol_to_num(socket.protocol);

    // Get source address for the packet
    let src_addr = socket
        .local_addr
        .lock()
        .clone()
        .unwrap_or_else(|| SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0));

    // Special case: RAW ICMP socket sending echo request
    if socket.sock_type == SocketType::Raw && socket.protocol == SocketProtocol::Icmp {
        if let Some(result) = handle_icmp_echo(socket, data, src_addr) {
            return result;
        }
        // Not an echo request, just pretend we sent it
        return data.len() as i64;
    }

    // For connected stream sockets (TCP), use SOCKET_PAIRS mechanism
    if socket.sock_type == SocketType::Stream {
        // This is handled separately in sys_send via SOCKET_PAIRS
        // This function is for sendto() which shouldn't be used on connected TCP
        return errno::EISCONN;
    }

    // For UDP/RAW, find destination socket and deliver
    let dest_socket = find_loopback_socket(dest.port, protocol);

    if let Some(dest_sock) = dest_socket {
        deliver_loopback_packet(&dest_sock, data, src_addr)
    } else {
        // No receiver - for UDP this is OK, just drop the packet
        data.len() as i64
    }
}

// ============================================================================
// SYSCALL IMPLEMENTATIONS
// ============================================================================

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

    let raw = uaccess::copy_from_user(addr, 16).ok()?;

    // sockaddr_in: family (2), port (2), addr (4), zero (8)
    let family = u16::from_ne_bytes([raw[0], raw[1]]);
    if family != 2 {
        // AF_INET
        return None;
    }

    let port = u16::from_be_bytes([raw[2], raw[3]]);

    // The IP address is stored as a u32 created by from_be_bytes([a,b,c,d]).
    // On little-endian x86, this u32 is stored in memory as [d,c,b,a].
    // So to get the original bytes [a,b,c,d], we read in reverse order.
    let ip_bytes = [raw[7], raw[6], raw[5], raw[4]];
    let ip = Ipv4Addr::new(ip_bytes[0], ip_bytes[1], ip_bytes[2], ip_bytes[3]);

    Some(SocketAddr::new(IpAddr::V4(ip), port))
}

/// Parse sockaddr_in6 structure from user memory
fn parse_sockaddr_in6(addr: u64, addrlen: u32) -> Option<SocketAddr> {
    // sockaddr_in6: family (2), port (2), flowinfo (4), addr (16), scope_id (4)
    if addrlen < 28 {
        return None;
    }

    let raw = uaccess::copy_from_user(addr, 28).ok()?;

    let family = u16::from_ne_bytes([raw[0], raw[1]]);
    if family != 10 {
        // AF_INET6
        return None;
    }

    let port = u16::from_be_bytes([raw[2], raw[3]]);
    let mut ip_bytes = [0u8; 16];
    ip_bytes.copy_from_slice(&raw[8..24]);
    let ip = Ipv6Addr(ip_bytes);

    Some(SocketAddr::new(IpAddr::V6(ip), port))
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

    let mut raw = [0u8; 16];

    // Write family
    let family: u16 = 2; // AF_INET
    raw[0] = family as u8;
    raw[1] = (family >> 8) as u8;

    // Write port (big-endian)
    let port_be = socket_addr.port.to_be_bytes();
    raw[2] = port_be[0];
    raw[3] = port_be[1];

    // Write IP address
    if let IpAddr::V4(ip) = socket_addr.ip {
        let bytes = ip.as_bytes();
        raw[4] = bytes[0];
        raw[5] = bytes[1];
        raw[6] = bytes[2];
        raw[7] = bytes[3];
    }

    if uaccess::copy_to_user(addr, &raw).is_err() {
        return errno::EFAULT;
    }

    if addrlen != 0 && uaccess::put_user(addrlen, 16u32).is_err() {
        return errno::EFAULT;
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

            let protocol = protocol_to_num(socket.protocol);

            // -- ShadePacket: Register in unified loopback registry (handles all socket types)
            register_loopback_socket(socket_addr.port, protocol, fd);

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

    // ShadePacket: Poll network stack to process incoming connection requests (SYN packets)
    let _ = tcpip::poll();

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
                let rc = write_sockaddr_in(addr, addrlen, &peer);
                if rc < 0 {
                    return rc;
                }
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

    // —ShadePacket: Non-loopback TCP - use real TCP/IP stack
    if socket.sock_type == SocketType::Stream {
        serial_print("connect: non-loopback TCP, using TCP/IP stack");

        if let Some(stack) = tcpip::stack() {
            // Initiate TCP connection
            match stack.tcp_connect(socket_addr) {
                Ok(conn) => {
                    let conn_id = conn.id;
                    serial_print_num("connect: TCP connection initiated, id=", conn_id as i64);

                    // Store mapping from socket fd to TCP connection id
                    TCP_CONNECTIONS.lock().insert(fd, conn_id);

                    // — TorqueJax: Wait for TCP 3-way handshake to complete.
                    // The old spin_loop() burned 100% CPU in ring 0 for up to
                    // 10M iterations. Replaced with HLT+kpo: allow preemption so
                    // the network stack's timer IRQ can run, poll after each wakeup.
                    // 500 HLT wakeups × ~10ms tick ≈ 5 second connect timeout.
                    const MAX_HLT_POLLS: u32 = 500;

                    for _ in 0..MAX_HLT_POLLS {
                        // Poll network stack before sleeping
                        let _ = tcpip::poll();

                        // Check connection state
                        if conn.is_established() {
                            serial_print("connect: TCP connection established");
                            // Update socket state
                            *socket.state.lock() = SocketState::Established;
                            *socket.remote_addr.lock() = Some(socket_addr);
                            return 0;
                        }

                        if conn.is_closed() || conn.is_reset() {
                            serial_print("connect: TCP connection failed");
                            TCP_CONNECTIONS.lock().remove(&fd);
                            return errno::ECONNREFUSED;
                        }

                        // — TorqueJax: HLT backoff — wake on next timer IRQ.
                        // allow_kernel_preempt lets the scheduler context-switch
                        // while we're blocked; clear it after wakeup so we don't
                        // get unexpectedly preempted mid-critical-section below.
                        os_core::allow_kernel_preempt();
                        os_core::wait_for_interrupt();
                        os_core::disallow_kernel_preempt();
                    }

                    // Timeout
                    serial_print("connect: TCP connection timeout");
                    TCP_CONNECTIONS.lock().remove(&fd);
                    return errno::ETIMEDOUT;
                }
                Err(e) => {
                    serial_print("connect: tcp_connect failed");
                    return net_error_to_errno(e);
                }
            }
        } else {
            serial_print("connect: no TCP/IP stack");
            return errno::ENETDOWN;
        }
    }

    // Non-loopback non-TCP: use standard connect (for RAW sockets, etc.)
    // Just store the remote address so send() knows where to send
    *socket.remote_addr.lock() = Some(socket_addr);
    *socket.state.lock() = SocketState::Established;
    0
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

    // ShadePacket: Poll network stack to process ACKs from previous sends
    let _ = tcpip::poll();

    // Copy user payload into kernel-owned memory immediately. Keeping a borrowed
    // user slice alive across deeper stack calls can fault if AC/SMAP state flips.
    let data_vec = match uaccess::copy_from_user(buf, len) {
        Ok(v) => v,
        Err(e) => {
            serial_print_num("send: copy_from_user failed, buf=", buf as i64);
            serial_print_num("send: copy_from_user failed, len=", len as i64);
            return e;
        }
    };
    let data = data_vec.as_slice();

    // Handle raw ICMP socket - loopback or real network
    if socket.sock_type == SocketType::Raw && socket.protocol == SocketProtocol::Icmp {
        serial_print("send: RAW ICMP socket detected");
        let remote = socket.remote_addr.lock();
        if let Some(ref addr) = *remote {
            serial_print_num(
                "send: remote addr is_loopback=",
                is_loopback_addr(addr) as i64,
            );
            if is_loopback_addr(addr) {
                serial_print_num("send: calling handle_icmp_echo, len=", len as i64);
                // Use unified ICMP echo handler
                let src_addr = socket
                    .local_addr
                    .lock()
                    .clone()
                    .unwrap_or_else(|| SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0));
                if let Some(result) = handle_icmp_echo(&socket, data, src_addr) {
                    serial_print_num("send: handle_icmp_echo returned", result);
                    return result;
                }
                serial_print("send: handle_icmp_echo returned None");
                // If not an echo request, just pretend we sent it
                return len as i64;
            } else {
                // —ShadePacket: Non-loopback ICMP - route through TCP/IP stack
                // This is the path for pinging external IPs like 8.8.8.8
                serial_print("send: non-loopback ICMP, using TCP/IP stack");

                let dst_ip = match addr.ip {
                    IpAddr::V4(ip) => ip,
                    IpAddr::V6(_) => {
                        return errno::EAFNOSUPPORT;
                    }
                };

                // Get the TCP/IP stack and send via it
                if let Some(stack) = tcpip::stack() {
                    // —ShadePacket: The data is already a complete ICMP packet from userspace
                    // We just need to wrap it in IP and send it out
                    serial_print_num("send: ICMP payload len=", data.len() as i64);
                    match stack.send_ipv4_packet(dst_ip, tcpip::IpProtocol::Icmp, data) {
                        Ok(()) => {
                            serial_print("send: ICMP packet sent successfully");
                            return len as i64;
                        }
                        Err(e) => {
                            serial_print("send: ICMP send failed");
                            return net_error_to_errno(e);
                        }
                    }
                } else {
                    serial_print("send: no TCP/IP stack available");
                    return errno::ENETDOWN;
                }
            }
        } else {
            serial_print("send: no remote addr set");
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

    // —ShadePacket: Check for non-loopback TCP connection
    if socket.sock_type == SocketType::Stream {
        if let Some(conn_id) = TCP_CONNECTIONS.lock().get(&fd).copied() {
            serial_print_num("send: using TCP connection id=", conn_id as i64);
            if let Some(stack) = tcpip::stack() {
                if let Some(conn) = stack.get_tcp_connection(conn_id) {
                    // Send data through TCP connection
                    match conn.send(data) {
                        Ok(n) => {
                            // Transmit any queued segments
                            let _ = stack.transmit_tcp_segments(&conn);
                            return n as i64;
                        }
                        Err(e) => {
                            return net_error_to_errno(e);
                        }
                    }
                }
            }
            // Connection not found in stack
            return errno::ENOTCONN;
        }
    }

    // Non-loopback non-TCP: use standard socket send (stub for now)
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

    // ShadePacket: Poll network stack to process incoming packets
    // This is CRITICAL - without this, packets sit in VirtIO-net RX queue!
    let _ = tcpip::poll();

    // —ShadePacket: Check for buffered ICMP replies from TCP/IP stack (non-loopback)
    // Raw ICMP sockets receive replies here when pinging external IPs
    if socket.sock_type == SocketType::Raw && socket.protocol == SocketProtocol::Icmp {
        if let Some(reply) = tcpip::get_icmp_reply() {
            serial_print("recv: got ICMP reply from buffer");
            // —ShadePacket: Build IP+ICMP packet like loopback path does
            // Ping expects raw socket to return IP header + ICMP data
            let mut packet = Vec::new();

            // Build minimal IP header (20 bytes)
            packet.push(0x45); // version=4, IHL=5 (20 bytes)
            packet.push(0x00); // TOS
            let total_len = (20 + reply.data.len()) as u16;
            packet.extend_from_slice(&total_len.to_be_bytes());
            packet.extend_from_slice(&[0x00, 0x00]); // ID
            packet.extend_from_slice(&[0x00, 0x00]); // Flags + Fragment offset
            packet.push(64); // TTL
            packet.push(1); // Protocol = ICMP
            packet.extend_from_slice(&[0x00, 0x00]); // Checksum placeholder

            // Source IP (from reply)
            let src_bytes = reply.src_ip.as_bytes();
            packet.extend_from_slice(src_bytes);

            // Dest IP (use our local IP or 0.0.0.0)
            if let Some(stack) = tcpip::stack() {
                if let Some(local_ip) = stack.interface().ipv4_addr() {
                    let local_bytes = local_ip.as_bytes();
                    packet.extend_from_slice(local_bytes);
                } else {
                    packet.extend_from_slice(&[0, 0, 0, 0]);
                }
            } else {
                packet.extend_from_slice(&[0, 0, 0, 0]);
            }

            // Calculate IP header checksum
            let ip_checksum = internet_checksum(&packet[..20]);
            packet[10] = (ip_checksum >> 8) as u8;
            packet[11] = ip_checksum as u8;

            // Append ICMP data
            packet.extend_from_slice(&reply.data);

            // Copy to user buffer
            let copy_len = len.min(packet.len());
            if uaccess::copy_to_user(buf, &packet[..copy_len]).is_err() {
                serial_print_num("recv: copy_to_user failed, buf=", buf as i64);
                serial_print_num("recv: copy_to_user failed, len=", copy_len as i64);
                return errno::EFAULT;
            }

            serial_print_num("recv: returning ICMP packet len=", copy_len as i64);
            return copy_len as i64;
        }
    }

    // —ShadePacket: Check for non-loopback TCP connection
    if socket.sock_type == SocketType::Stream {
        if let Some(conn_id) = TCP_CONNECTIONS.lock().get(&fd).copied() {
            if let Some(stack) = tcpip::stack() {
                if let Some(conn) = stack.get_tcp_connection(conn_id) {
                    let mut kbuf = Vec::new();
                    kbuf.resize(len, 0);
                    // Receive data from TCP connection
                    match conn.recv(&mut kbuf) {
                        Ok(n) if n > 0 => {
                            if uaccess::copy_to_user(buf, &kbuf[..n]).is_err() {
                                serial_print_num("recv: tcp copy_to_user failed, buf=", buf as i64);
                                serial_print_num("recv: tcp copy_to_user failed, len=", n as i64);
                                return errno::EFAULT;
                            }
                            serial_print_num("recv: TCP got bytes=", n as i64);
                            return n as i64;
                        }
                        Ok(_) => {
                            // No data available, check if connection closed
                            if conn.is_closed() {
                                return 0; // EOF
                            }
                            // Fall through to return EAGAIN
                        }
                        Err(_) => {
                            // Connection error
                            if conn.is_reset() {
                                return errno::ECONNRESET;
                            }
                            // Fall through to return EAGAIN
                        }
                    }
                }
            }
        }
    }

    // First check the unified loopback queue (for all loopback-delivered data)
    let mut kbuf = Vec::new();
    kbuf.resize(len, 0);
    if let Some((read_len, _src_addr)) = receive_loopback_packet(&socket, &mut kbuf) {
        if uaccess::copy_to_user(buf, &kbuf[..read_len]).is_err() {
            serial_print_num("recv: loopback copy_to_user failed, buf=", buf as i64);
            serial_print_num("recv: loopback copy_to_user failed, len=", read_len as i64);
            return errno::EFAULT;
        }
        return read_len as i64;
    }

    // Then check legacy recv_buf (for TCP stream data via SOCKET_PAIRS)
    {
        let mut recv_buf = socket.recv_buf.lock();
        if !recv_buf.is_empty() {
            let to_read = len.min(recv_buf.len());
            let chunk = recv_buf[..to_read].to_vec();
            recv_buf.drain(..to_read);
            if uaccess::copy_to_user(buf, &chunk).is_err() {
                serial_print_num("recv: legacy copy_to_user failed, buf=", buf as i64);
                serial_print_num("recv: legacy copy_to_user failed, len=", to_read as i64);
                return errno::EFAULT;
            }
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

/// sys_sendto - Send data to a specific address (UDP/RAW)
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

    // ShadePacket: Poll network stack to process any pending responses
    let _ = tcpip::poll();

    // Copy user payload up front so later socket/tcpip calls never dereference
    // userspace memory directly.
    let data_vec = match uaccess::copy_from_user(buf, len) {
        Ok(v) => v,
        Err(e) => {
            serial_print_num("sendto: copy_from_user failed, buf=", buf as i64);
            serial_print_num("sendto: copy_from_user failed, len=", len as i64);
            return e;
        }
    };
    let data = data_vec.as_slice();

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

    // Check if destination is loopback - use unified loopback system
    if is_loopback_addr(&dest) {
        return loopback_send(&socket, data, &dest);
    }

    // Non-loopback: use real network stack
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

    // ShadePacket: Poll network stack to process incoming packets
    let _ = tcpip::poll();

    let _ = flags; // Flags not fully supported yet
    let mut kbuf = Vec::new();
    kbuf.resize(len, 0);

    // First check the unified loopback queue (returns proper source address!)
    if let Some((read_len, sender_addr)) = receive_loopback_packet(&socket, &mut kbuf) {
        if uaccess::copy_to_user(buf, &kbuf[..read_len]).is_err() {
            serial_print_num("recvfrom: copy_to_user failed, buf=", buf as i64);
            serial_print_num("recvfrom: copy_to_user failed, len=", read_len as i64);
            return errno::EFAULT;
        }
        if src_addr != 0 {
            let rc = write_sockaddr_in(src_addr, addrlen, &sender_addr);
            if rc < 0 {
                return rc;
            }
        }
        return read_len as i64;
    }

    // Fall back to legacy recv_buf (no source address available)
    {
        let mut recv_buf = socket.recv_buf.lock();
        if !recv_buf.is_empty() {
            let to_read = len.min(recv_buf.len());
            let chunk = recv_buf[..to_read].to_vec();
            recv_buf.drain(..to_read);
            if uaccess::copy_to_user(buf, &chunk).is_err() {
                serial_print_num("recvfrom: legacy copy_to_user failed, buf=", buf as i64);
                serial_print_num("recvfrom: legacy copy_to_user failed, len=", to_read as i64);
                return errno::EFAULT;
            }
            // No source address for legacy path - use placeholder
            if src_addr != 0 {
                let placeholder = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 0);
                let rc = write_sockaddr_in(src_addr, addrlen, &placeholder);
                if rc < 0 {
                    return rc;
                }
            }
            return to_read as i64;
        }
    }

    // No data available
    errno::EAGAIN
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
            15 => {
                // — ShadePacket: SO_REUSEPORT — let multiple sockets bind the same port
                if optlen >= 4 {
                    let val = unsafe { *(optval as *const i32) };
                    opts.reuse_port = val != 0;
                }
            }
            _ => {
                // — ShadePacket: unknown SOL_SOCKET option — don't silently swallow it
                return errno::ENOPROTOOPT;
            }
        }
    }
    // IPPROTO_TCP level
    else if level == 6 {
        match optname {
            1 => {
                // TCP_NODELAY
                if optlen >= 4 {
                    let val = unsafe { *(optval as *const i32) };
                    opts.tcp_nodelay = val != 0;
                }
            }
            _ => {
                return errno::ENOPROTOOPT;
            }
        }
    } else {
        // — ShadePacket: unknown protocol level
        return errno::ENOPROTOOPT;
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
            15 => {
                // — ShadePacket: SO_REUSEPORT
                unsafe {
                    *(optval as *mut i32) = if opts.reuse_port { 1 } else { 0 };
                    if !len_ptr.is_null() {
                        *len_ptr = 4;
                    }
                }
            }
            _ => {
                return errno::ENOPROTOOPT;
            }
        }
    }
    // IPPROTO_TCP level
    else if level == 6 {
        match optname {
            1 => {
                // TCP_NODELAY
                unsafe {
                    *(optval as *mut i32) = if opts.tcp_nodelay { 1 } else { 0 };
                    if !len_ptr.is_null() {
                        *len_ptr = 4;
                    }
                }
            }
            _ => {
                return errno::ENOPROTOOPT;
            }
        }
    } else {
        return errno::ENOPROTOOPT;
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

    // -- ShadePacket: Unified loopback registry handles all socket type cleanup
    unregister_loopback_socket_by_fd(fd);

    // —ShadePacket: Clean up non-loopback TCP connection
    if let Some(conn_id) = TCP_CONNECTIONS.lock().remove(&fd) {
        // Close the TCP connection in the stack
        if let Some(stack) = tcpip::stack() {
            if let Some(conn) = stack.get_tcp_connection(conn_id) {
                let _ = conn.close();
                // Transmit FIN segment
                let _ = stack.transmit_tcp_segments(&conn);
            }
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

/// sys_accept4 - Accept connection with flags
///
/// Delegates to sys_accept, ignoring flags (SOCK_NONBLOCK, SOCK_CLOEXEC).
pub fn sys_accept4(fd: i32, addr: u64, addrlen: u64, _flags: i32) -> i64 {
    sys_accept(fd, addr, addrlen)
}

/// sys_socketpair - Create a pair of connected sockets
///
/// Simplified implementation: creates a bidirectional pipe pair.
pub fn sys_socketpair(_domain: i32, _socktype: i32, _protocol: i32, sv_ptr: u64) -> i64 {
    // Delegate to sys_pipe which creates a pipe pair
    // This is a simplification - a real socketpair is bidirectional
    // but for many use cases (e.g., parent-child communication) a pipe suffices
    crate::vfs::sys_pipe(sv_ptr)
}

// ============================================================================
// Network Control Syscall
// ============================================================================

/// Network control operation codes
pub mod net_op {
    /// Trigger DHCP lease acquisition
    pub const DHCP_REQUEST: u64 = 1;
    /// Release DHCP lease (future)
    pub const DHCP_RELEASE: u64 = 2;
    /// Renew DHCP lease (future)
    pub const DHCP_RENEW: u64 = 3;
}

/// sys_net_control - Network control operations from userspace
///
/// —ShadePacket: Kernel DHCP at boot has a ~2s timeout and limited retries.
/// This syscall allows userspace (networkd, dhclient) to trigger DHCP after
/// boot when the network becomes available or when a retry is needed.
///
/// # Arguments
/// * `operation` - Network operation code (NET_OP_DHCP_REQUEST, etc.)
/// * `iface_ptr` - Pointer to interface name string
/// * `iface_len` - Length of interface name
///
/// # Returns
/// 0 on success, negative errno on error
pub fn sys_net_control(operation: u64, iface_ptr: u64, iface_len: usize) -> i64 {
    use crate::errno;
    use crate::vfs::{FileFlags, GLOBAL_VFS, Mode};
    use alloc::format;
    use alloc::string::String;

    // —ShadePacket: Validate interface name from userspace
    let iface_name = match copy_iface_from_user(iface_ptr, iface_len) {
        Some(name) => name,
        None => return errno::EFAULT,
    };

    match operation {
        net_op::DHCP_REQUEST => {
            // —ShadePacket: Get the network interface by name
            let interface = match net::interface::get_interface(&iface_name) {
                Some(iface) => iface,
                None => {
                    // —ShadePacket: Interface not found in kernel - might be virtual
                    return errno::ENODEV;
                }
            };

            // —ShadePacket: Perform DHCP lease acquisition
            // This blocks until DHCP completes or times out
            match tcpip::acquire_lease(interface) {
                Ok(lease) => {
                    // —ShadePacket: Write lease file to /var/lib/dhcp/<iface>.lease
                    // so networkd can read it and apply the configuration
                    if let Err(e) = write_dhcp_lease(&iface_name, &lease) {
                        // —ShadePacket: DHCP succeeded but couldn't write lease file
                        // Return success anyway - the interface is configured
                        serial_print(&format!("DHCP OK but lease write failed: {:?}", e));
                    }
                    0
                }
                Err(e) => {
                    // —ShadePacket: DHCP failed - return appropriate error
                    match e {
                        net::NetError::TimedOut | net::NetError::Timeout => errno::ETIMEDOUT,
                        net::NetError::NetworkUnreachable => errno::ENETUNREACH,
                        net::NetError::DeviceDown => errno::ENETDOWN,
                        _ => errno::EIO,
                    }
                }
            }
        }
        net_op::DHCP_RELEASE | net_op::DHCP_RENEW => {
            // —ShadePacket: Future operations - not yet implemented
            errno::ENOSYS
        }
        _ => errno::EINVAL,
    }
}

/// Copy interface name from userspace
fn copy_iface_from_user(ptr: u64, len: usize) -> Option<alloc::string::String> {
    // —ShadePacket: Sanity check on length
    if len == 0 || len > 64 {
        return None;
    }

    // Validate pointer is in userspace
    if ptr < 0x1000 || ptr > 0x0000_7FFF_FFFF_FFFF {
        return None;
    }

    // Enable SMAP bypass for userspace access
    unsafe {
        os_core::user_access_begin();
    }

    let slice = unsafe { core::slice::from_raw_parts(ptr as *const u8, len) };
    let result = core::str::from_utf8(slice)
        .ok()
        .map(alloc::string::String::from);

    unsafe {
        os_core::user_access_end();
    }

    result
}

/// Write DHCP lease to filesystem
///
/// Creates /var/lib/dhcp/<iface>.lease with lease information
fn write_dhcp_lease(iface: &str, lease: &dhcp::DhcpLease) -> Result<(), &'static str> {
    use crate::vfs::{FileFlags, GLOBAL_VFS, Mode};
    use alloc::format;

    // —ShadePacket: Ensure directories exist
    ensure_dir_exists("/var")?;
    ensure_dir_exists("/var/lib")?;
    ensure_dir_exists("/var/lib/dhcp")?;

    // —ShadePacket: Format lease content
    let content = tcpip::format_lease_file(lease);

    // —ShadePacket: Create lease file path
    let path = format!("/var/lib/dhcp/{}.lease", iface);

    // —ShadePacket: Create or truncate the lease file
    let vnode = match GLOBAL_VFS.lookup(&path) {
        Ok(v) => {
            // File exists, truncate it
            let _ = v.truncate(0);
            v
        }
        Err(_) => {
            // File doesn't exist, create it
            match GLOBAL_VFS.lookup_parent(&path) {
                Ok((parent, name)) => parent
                    .create(&name, Mode::new(0o644))
                    .map_err(|_| "Failed to create lease file")?,
                Err(_) => return Err("Parent directory not found"),
            }
        }
    };

    // —ShadePacket: Write lease content
    vnode
        .write(0, content.as_bytes())
        .map_err(|_| "Failed to write lease file")?;

    Ok(())
}

/// Ensure a directory exists, creating it if needed
fn ensure_dir_exists(path: &str) -> Result<(), &'static str> {
    use crate::vfs::{GLOBAL_VFS, Mode, VnodeType};

    match GLOBAL_VFS.lookup(path) {
        Ok(vnode) => {
            if vnode.vtype() != VnodeType::Directory {
                return Err("Path exists but is not a directory");
            }
            Ok(())
        }
        Err(_) => {
            // Directory doesn't exist, create it
            match GLOBAL_VFS.lookup_parent(path) {
                Ok((parent, name)) => {
                    parent
                        .mkdir(&name, Mode::new(0o755))
                        .map_err(|_| "Failed to create directory")?;
                    Ok(())
                }
                Err(_) => Err("Parent directory not found"),
            }
        }
    }
}
