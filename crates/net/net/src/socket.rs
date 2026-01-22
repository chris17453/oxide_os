//! Socket Abstraction

use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use spin::Mutex;

use crate::{NetError, NetResult, SocketAddr};

/// Socket domain (address family)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum SocketDomain {
    /// Unix domain sockets
    Unix = 1,
    /// IPv4
    Inet = 2,
    /// IPv6
    Inet6 = 10,
}

impl TryFrom<u16> for SocketDomain {
    type Error = NetError;

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(SocketDomain::Unix),
            2 => Ok(SocketDomain::Inet),
            10 => Ok(SocketDomain::Inet6),
            _ => Err(NetError::InvalidArgument),
        }
    }
}

/// Socket type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum SocketType {
    /// Stream (TCP)
    Stream = 1,
    /// Datagram (UDP)
    Dgram = 2,
    /// Raw
    Raw = 3,
    /// Sequenced packet
    SeqPacket = 5,
}

impl TryFrom<u16> for SocketType {
    type Error = NetError;

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(SocketType::Stream),
            2 => Ok(SocketType::Dgram),
            3 => Ok(SocketType::Raw),
            5 => Ok(SocketType::SeqPacket),
            _ => Err(NetError::InvalidArgument),
        }
    }
}

/// Socket protocol
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum SocketProtocol {
    /// Default protocol for socket type
    Default = 0,
    /// ICMP
    Icmp = 1,
    /// TCP
    Tcp = 6,
    /// UDP
    Udp = 17,
    /// ICMPv6
    Icmpv6 = 58,
}

impl TryFrom<u16> for SocketProtocol {
    type Error = NetError;

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(SocketProtocol::Default),
            1 => Ok(SocketProtocol::Icmp),
            6 => Ok(SocketProtocol::Tcp),
            17 => Ok(SocketProtocol::Udp),
            58 => Ok(SocketProtocol::Icmpv6),
            _ => Err(NetError::ProtocolNotSupported),
        }
    }
}

/// Socket state (for TCP)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SocketState {
    /// Not connected
    Closed,
    /// Listening for connections
    Listen,
    /// SYN sent
    SynSent,
    /// SYN received
    SynReceived,
    /// Connection established
    Established,
    /// FIN wait 1
    FinWait1,
    /// FIN wait 2
    FinWait2,
    /// Close wait
    CloseWait,
    /// Closing
    Closing,
    /// Last ACK
    LastAck,
    /// Time wait
    TimeWait,
}

/// Socket options
#[derive(Debug, Clone, Copy)]
pub struct SocketOptions {
    /// Reuse address
    pub reuse_addr: bool,
    /// Reuse port
    pub reuse_port: bool,
    /// Keep alive
    pub keep_alive: bool,
    /// Broadcast
    pub broadcast: bool,
    /// Receive timeout (ms, 0 = infinite)
    pub recv_timeout: u32,
    /// Send timeout (ms, 0 = infinite)
    pub send_timeout: u32,
    /// Receive buffer size
    pub recv_buf_size: u32,
    /// Send buffer size
    pub send_buf_size: u32,
    /// TCP no delay
    pub tcp_nodelay: bool,
}

impl Default for SocketOptions {
    fn default() -> Self {
        SocketOptions {
            reuse_addr: false,
            reuse_port: false,
            keep_alive: false,
            broadcast: false,
            recv_timeout: 0,
            send_timeout: 0,
            recv_buf_size: 65536,
            send_buf_size: 65536,
            tcp_nodelay: false,
        }
    }
}

/// Socket shutdown modes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum Shutdown {
    /// Shutdown reads
    Read = 0,
    /// Shutdown writes
    Write = 1,
    /// Shutdown both
    Both = 2,
}

/// Socket implementation
pub struct Socket {
    /// Socket domain
    pub domain: SocketDomain,
    /// Socket type
    pub sock_type: SocketType,
    /// Protocol
    pub protocol: SocketProtocol,
    /// Local address
    pub local_addr: Mutex<Option<SocketAddr>>,
    /// Remote address (for connected sockets)
    pub remote_addr: Mutex<Option<SocketAddr>>,
    /// State
    pub state: Mutex<SocketState>,
    /// Options
    pub options: Mutex<SocketOptions>,
    /// Receive buffer
    pub recv_buf: Mutex<Vec<u8>>,
    /// Send buffer
    pub send_buf: Mutex<Vec<u8>>,
    /// Backlog (for listening sockets)
    pub backlog: AtomicU32,
    /// Pending connections (for listening sockets)
    pub pending: Mutex<Vec<Arc<Socket>>>,
    /// Non-blocking
    pub nonblocking: AtomicBool,
    /// Closed
    pub closed: AtomicBool,
}

impl Socket {
    /// Create a new socket
    pub fn new(
        domain: SocketDomain,
        sock_type: SocketType,
        protocol: SocketProtocol,
    ) -> NetResult<Arc<Self>> {
        // Validate combination
        match (sock_type, protocol) {
            (SocketType::Stream, SocketProtocol::Default)
            | (SocketType::Stream, SocketProtocol::Tcp)
            | (SocketType::Dgram, SocketProtocol::Default)
            | (SocketType::Dgram, SocketProtocol::Udp)
            | (SocketType::Raw, _) => {}
            _ => return Err(NetError::ProtocolNotSupported),
        }

        Ok(Arc::new(Socket {
            domain,
            sock_type,
            protocol,
            local_addr: Mutex::new(None),
            remote_addr: Mutex::new(None),
            state: Mutex::new(SocketState::Closed),
            options: Mutex::new(SocketOptions::default()),
            recv_buf: Mutex::new(Vec::new()),
            send_buf: Mutex::new(Vec::new()),
            backlog: AtomicU32::new(0),
            pending: Mutex::new(Vec::new()),
            nonblocking: AtomicBool::new(false),
            closed: AtomicBool::new(false),
        }))
    }

    /// Bind to an address
    pub fn bind(&self, addr: SocketAddr) -> NetResult<()> {
        let mut local = self.local_addr.lock();
        if local.is_some() {
            return Err(NetError::AddrInUse);
        }
        *local = Some(addr);
        Ok(())
    }

    /// Listen for connections (TCP only)
    pub fn listen(&self, backlog: u32) -> NetResult<()> {
        if self.sock_type != SocketType::Stream {
            return Err(NetError::SocketTypeNotSupported);
        }

        let local = self.local_addr.lock();
        if local.is_none() {
            return Err(NetError::InvalidArgument);
        }

        self.backlog.store(backlog, Ordering::SeqCst);
        *self.state.lock() = SocketState::Listen;
        Ok(())
    }

    /// Connect to a remote address
    pub fn connect(&self, addr: SocketAddr) -> NetResult<()> {
        let state = *self.state.lock();
        if state != SocketState::Closed {
            return Err(NetError::AlreadyConnected);
        }

        *self.remote_addr.lock() = Some(addr);
        // Would initiate TCP handshake for stream sockets
        *self.state.lock() = SocketState::Established;
        Ok(())
    }

    /// Accept a connection (TCP only)
    pub fn accept(&self) -> NetResult<Arc<Socket>> {
        if self.sock_type != SocketType::Stream {
            return Err(NetError::SocketTypeNotSupported);
        }

        let state = *self.state.lock();
        if state != SocketState::Listen {
            return Err(NetError::InvalidArgument);
        }

        let mut pending = self.pending.lock();
        if let Some(conn) = pending.pop() {
            Ok(conn)
        } else if self.nonblocking.load(Ordering::SeqCst) {
            Err(NetError::WouldBlock)
        } else {
            // Would block until connection available
            Err(NetError::WouldBlock)
        }
    }

    /// Send data
    pub fn send(&self, data: &[u8]) -> NetResult<usize> {
        let state = *self.state.lock();
        if state != SocketState::Established {
            return Err(NetError::NotConnected);
        }

        if self.closed.load(Ordering::SeqCst) {
            return Err(NetError::ConnectionReset);
        }

        let mut send_buf = self.send_buf.lock();
        let opts = self.options.lock();
        let max_size = opts.send_buf_size as usize;

        let available = max_size.saturating_sub(send_buf.len());
        if available == 0 {
            if self.nonblocking.load(Ordering::SeqCst) {
                return Err(NetError::WouldBlock);
            }
            // Would block until space available
            return Err(NetError::WouldBlock);
        }

        let to_send = data.len().min(available);
        send_buf.extend_from_slice(&data[..to_send]);
        Ok(to_send)
    }

    /// Receive data
    pub fn recv(&self, buf: &mut [u8]) -> NetResult<usize> {
        let state = *self.state.lock();
        match state {
            SocketState::Established | SocketState::CloseWait => {}
            SocketState::Closed => return Err(NetError::NotConnected),
            _ => return Err(NetError::InvalidArgument),
        }

        let mut recv_buf = self.recv_buf.lock();
        if recv_buf.is_empty() {
            if self.closed.load(Ordering::SeqCst) {
                return Ok(0); // EOF
            }
            if self.nonblocking.load(Ordering::SeqCst) {
                return Err(NetError::WouldBlock);
            }
            // Would block until data available
            return Err(NetError::WouldBlock);
        }

        let to_recv = buf.len().min(recv_buf.len());
        buf[..to_recv].copy_from_slice(&recv_buf[..to_recv]);
        recv_buf.drain(..to_recv);
        Ok(to_recv)
    }

    /// Send to a specific address (UDP)
    pub fn sendto(&self, data: &[u8], addr: SocketAddr) -> NetResult<usize> {
        if self.sock_type != SocketType::Dgram && self.sock_type != SocketType::Raw {
            return Err(NetError::SocketTypeNotSupported);
        }

        // For UDP, would send packet to the specified address
        Ok(data.len())
    }

    /// Receive from a specific address (UDP)
    pub fn recvfrom(&self, buf: &mut [u8]) -> NetResult<(usize, SocketAddr)> {
        if self.sock_type != SocketType::Dgram && self.sock_type != SocketType::Raw {
            return Err(NetError::SocketTypeNotSupported);
        }

        let mut recv_buf = self.recv_buf.lock();
        if recv_buf.is_empty() {
            if self.nonblocking.load(Ordering::SeqCst) {
                return Err(NetError::WouldBlock);
            }
            return Err(NetError::WouldBlock);
        }

        // Would extract address from packet
        let to_recv = buf.len().min(recv_buf.len());
        buf[..to_recv].copy_from_slice(&recv_buf[..to_recv]);
        recv_buf.drain(..to_recv);

        // Placeholder address
        use crate::addr::{IpAddr, Ipv4Addr};
        Ok((to_recv, SocketAddr::new(IpAddr::V4(Ipv4Addr::ANY), 0)))
    }

    /// Shutdown the socket
    pub fn shutdown(&self, how: Shutdown) -> NetResult<()> {
        match how {
            Shutdown::Read | Shutdown::Both => {
                self.recv_buf.lock().clear();
            }
            Shutdown::Write => {}
        }

        match how {
            Shutdown::Write | Shutdown::Both => {
                *self.state.lock() = SocketState::FinWait1;
            }
            Shutdown::Read => {}
        }

        Ok(())
    }

    /// Close the socket
    pub fn close(&self) -> NetResult<()> {
        self.closed.store(true, Ordering::SeqCst);
        *self.state.lock() = SocketState::Closed;
        self.recv_buf.lock().clear();
        self.send_buf.lock().clear();
        Ok(())
    }

    /// Get local address
    pub fn local_addr(&self) -> Option<SocketAddr> {
        *self.local_addr.lock()
    }

    /// Get peer address
    pub fn peer_addr(&self) -> Option<SocketAddr> {
        *self.remote_addr.lock()
    }

    /// Set non-blocking mode
    pub fn set_nonblocking(&self, nonblocking: bool) {
        self.nonblocking.store(nonblocking, Ordering::SeqCst);
    }

    /// Get non-blocking mode
    pub fn is_nonblocking(&self) -> bool {
        self.nonblocking.load(Ordering::SeqCst)
    }
}
