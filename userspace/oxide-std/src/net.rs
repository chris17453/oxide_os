//! Networking primitives compatible with std::net
//!
//! Provides TCP/UDP socket abstractions using OXIDE's libc layer.

use alloc::vec::Vec;
use core::fmt;

use crate::io::{Error, ErrorKind, Read, Result, Write};
use libc::socket::{
    self, InAddr, SockAddrIn, SOCKADDR_IN_SIZE, af, connect, htons, recv, send, shutdown,
    shut, sock, tcp,
};

// ============================================================================
// IP Address Types
// ============================================================================

/// An IPv4 address
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Ipv4Addr {
    octets: [u8; 4],
}

impl Ipv4Addr {
    /// Creates a new IPv4 address from four octets
    pub const fn new(a: u8, b: u8, c: u8, d: u8) -> Self {
        Ipv4Addr {
            octets: [a, b, c, d],
        }
    }

    /// Returns the four octets of the address
    pub const fn octets(&self) -> [u8; 4] {
        self.octets
    }

    /// Localhost address (127.0.0.1)
    pub const LOCALHOST: Ipv4Addr = Ipv4Addr::new(127, 0, 0, 1);

    /// Unspecified address (0.0.0.0)
    pub const UNSPECIFIED: Ipv4Addr = Ipv4Addr::new(0, 0, 0, 0);

    /// Convert to network byte order u32
    pub fn to_bits(&self) -> u32 {
        u32::from_be_bytes(self.octets)
    }
}

impl fmt::Debug for Ipv4Addr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}.{}.{}.{}",
            self.octets[0], self.octets[1], self.octets[2], self.octets[3]
        )
    }
}

impl fmt::Display for Ipv4Addr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}.{}.{}.{}",
            self.octets[0], self.octets[1], self.octets[2], self.octets[3]
        )
    }
}

// ============================================================================
// Socket Address Types
// ============================================================================

/// An IPv4 socket address (IP + port)
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct SocketAddrV4 {
    ip: Ipv4Addr,
    port: u16,
}

impl SocketAddrV4 {
    /// Creates a new socket address
    pub const fn new(ip: Ipv4Addr, port: u16) -> Self {
        SocketAddrV4 { ip, port }
    }

    /// Returns the IP address
    pub const fn ip(&self) -> &Ipv4Addr {
        &self.ip
    }

    /// Returns the port
    pub const fn port(&self) -> u16 {
        self.port
    }
}

impl fmt::Debug for SocketAddrV4 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.ip, self.port)
    }
}

impl fmt::Display for SocketAddrV4 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.ip, self.port)
    }
}

/// A socket address (either V4 or V6)
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum SocketAddr {
    V4(SocketAddrV4),
}

impl SocketAddr {
    /// Returns the port
    pub fn port(&self) -> u16 {
        match self {
            SocketAddr::V4(addr) => addr.port(),
        }
    }
}

impl fmt::Debug for SocketAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SocketAddr::V4(addr) => fmt::Debug::fmt(addr, f),
        }
    }
}

impl fmt::Display for SocketAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SocketAddr::V4(addr) => fmt::Display::fmt(addr, f),
        }
    }
}

// ============================================================================
// TCP Stream
// ============================================================================

/// A TCP stream between a local and remote socket
pub struct TcpStream {
    fd: i32,
}

impl TcpStream {
    /// Opens a TCP connection to a remote host
    pub fn connect(addr: SocketAddrV4) -> Result<TcpStream> {
        // Create socket
        let fd = socket::socket(af::INET, sock::STREAM, 0);
        if fd < 0 {
            return Err(Error::new(ErrorKind::Other, "failed to create socket"));
        }

        // Build sockaddr_in
        let sockaddr = SockAddrIn {
            sin_family: af::INET as u16,
            sin_port: htons(addr.port()),
            sin_addr: InAddr {
                s_addr: addr.ip().to_bits(),
            },
            sin_zero: [0; 8],
        };

        // Connect
        let result = connect(fd, &sockaddr, SOCKADDR_IN_SIZE);
        if result < 0 {
            libc::close(fd);
            return Err(Error::new(ErrorKind::ConnectionRefused, "connection refused"));
        }

        Ok(TcpStream { fd })
    }

    /// Connect using hostname and port
    pub fn connect_host(hostname: &str, port: u16) -> Result<TcpStream> {
        // Try to parse as IP address first
        if let Some(ip) = parse_ipv4(hostname) {
            return Self::connect(SocketAddrV4::new(ip, port));
        }

        // Try DNS resolution
        if let Some((a, b, c, d)) = libc::dns::resolve(hostname, None) {
            return Self::connect(SocketAddrV4::new(Ipv4Addr::new(a, b, c, d), port));
        }

        Err(Error::new(ErrorKind::NotFound, "could not resolve hostname"))
    }

    /// Get the underlying file descriptor
    pub fn as_raw_fd(&self) -> i32 {
        self.fd
    }

    /// Set TCP_NODELAY option
    pub fn set_nodelay(&self, nodelay: bool) -> Result<()> {
        let val: i32 = if nodelay { 1 } else { 0 };
        let result = socket::setsockopt(self.fd, socket::sol::TCP, tcp::NODELAY, &val);
        if result < 0 {
            Err(Error::from_raw_os_error(result))
        } else {
            Ok(())
        }
    }

    /// Shut down the read, write, or both halves of this connection
    pub fn shutdown(&self, how: Shutdown) -> Result<()> {
        let how = match how {
            Shutdown::Read => shut::RD,
            Shutdown::Write => shut::WR,
            Shutdown::Both => shut::RDWR,
        };
        let result = shutdown(self.fd, how);
        if result < 0 {
            Err(Error::from_raw_os_error(result))
        } else {
            Ok(())
        }
    }

    /// Try to clone the TcpStream (creates a new fd via dup)
    pub fn try_clone(&self) -> Result<TcpStream> {
        let new_fd = libc::dup(self.fd);
        if new_fd < 0 {
            Err(Error::from_raw_os_error(new_fd))
        } else {
            Ok(TcpStream { fd: new_fd })
        }
    }
}

impl Read for TcpStream {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        let n = recv(self.fd, buf, 0);
        if n < 0 {
            Err(Error::from_raw_os_error(n as i32))
        } else {
            Ok(n as usize)
        }
    }
}

impl Write for TcpStream {
    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        let n = send(self.fd, buf, 0);
        if n < 0 {
            Err(Error::from_raw_os_error(n as i32))
        } else {
            Ok(n as usize)
        }
    }

    fn flush(&mut self) -> Result<()> {
        Ok(()) // TCP doesn't buffer at application level
    }
}

impl Drop for TcpStream {
    fn drop(&mut self) {
        libc::close(self.fd);
    }
}

/// Shutdown modes for TCP connections
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shutdown {
    /// Shut down reading
    Read,
    /// Shut down writing
    Write,
    /// Shut down both reading and writing
    Both,
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Parse an IPv4 address from a string
pub fn parse_ipv4(s: &str) -> Option<Ipv4Addr> {
    let parts: Vec<&str> = s.split('.').collect();
    if parts.len() != 4 {
        return None;
    }

    let mut octets = [0u8; 4];
    for (i, part) in parts.iter().enumerate() {
        octets[i] = parse_u8(part)?;
    }
    Some(Ipv4Addr { octets })
}

fn parse_u8(s: &str) -> Option<u8> {
    let mut val = 0u16;
    for c in s.chars() {
        if !c.is_ascii_digit() {
            return None;
        }
        val = val.checked_mul(10)?;
        val = val.checked_add((c as u16) - ('0' as u16))?;
    }
    if val > 255 {
        None
    } else {
        Some(val as u8)
    }
}

/// Resolve a hostname to an IP address
pub fn resolve(hostname: &str) -> Option<Ipv4Addr> {
    // Try as IP first
    if let Some(ip) = parse_ipv4(hostname) {
        return Some(ip);
    }

    // DNS resolution
    if let Some((a, b, c, d)) = libc::dns::resolve(hostname, None) {
        return Some(Ipv4Addr::new(a, b, c, d));
    }

    None
}
