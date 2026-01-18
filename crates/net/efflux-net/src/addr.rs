//! Network Address Types

use core::fmt;

/// MAC address (6 bytes)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(C)]
pub struct MacAddress(pub [u8; 6]);

impl MacAddress {
    /// Broadcast MAC address
    pub const BROADCAST: MacAddress = MacAddress([0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]);

    /// Create a new MAC address
    pub fn new(a: u8, b: u8, c: u8, d: u8, e: u8, f: u8) -> Self {
        MacAddress([a, b, c, d, e, f])
    }

    /// Get the bytes
    pub fn as_bytes(&self) -> &[u8; 6] {
        &self.0
    }

    /// Check if this is a broadcast address
    pub fn is_broadcast(&self) -> bool {
        *self == Self::BROADCAST
    }

    /// Check if this is a multicast address
    pub fn is_multicast(&self) -> bool {
        self.0[0] & 0x01 != 0
    }

    /// Check if this is a unicast address
    pub fn is_unicast(&self) -> bool {
        !self.is_multicast()
    }
}

impl fmt::Display for MacAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
            self.0[0], self.0[1], self.0[2], self.0[3], self.0[4], self.0[5]
        )
    }
}

/// IPv4 address (4 bytes)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(C)]
pub struct Ipv4Addr(pub [u8; 4]);

impl Ipv4Addr {
    /// Any address (0.0.0.0)
    pub const ANY: Ipv4Addr = Ipv4Addr([0, 0, 0, 0]);
    /// Localhost (127.0.0.1)
    pub const LOCALHOST: Ipv4Addr = Ipv4Addr([127, 0, 0, 1]);
    /// Broadcast (255.255.255.255)
    pub const BROADCAST: Ipv4Addr = Ipv4Addr([255, 255, 255, 255]);

    /// Create a new IPv4 address
    pub fn new(a: u8, b: u8, c: u8, d: u8) -> Self {
        Ipv4Addr([a, b, c, d])
    }

    /// Create from u32 (network byte order)
    pub fn from_u32(addr: u32) -> Self {
        Ipv4Addr(addr.to_be_bytes())
    }

    /// Convert to u32 (network byte order)
    pub fn to_u32(&self) -> u32 {
        u32::from_be_bytes(self.0)
    }

    /// Get the bytes
    pub fn as_bytes(&self) -> &[u8; 4] {
        &self.0
    }

    /// Check if this is a loopback address
    pub fn is_loopback(&self) -> bool {
        self.0[0] == 127
    }

    /// Check if this is a private address
    pub fn is_private(&self) -> bool {
        // 10.0.0.0/8
        self.0[0] == 10
            // 172.16.0.0/12
            || (self.0[0] == 172 && (self.0[1] & 0xf0) == 16)
            // 192.168.0.0/16
            || (self.0[0] == 192 && self.0[1] == 168)
    }

    /// Check if this is a multicast address
    pub fn is_multicast(&self) -> bool {
        (self.0[0] & 0xf0) == 224
    }

    /// Check if this is a broadcast address
    pub fn is_broadcast(&self) -> bool {
        *self == Self::BROADCAST
    }

    /// Check if this is the any address
    pub fn is_any(&self) -> bool {
        *self == Self::ANY
    }
}

impl fmt::Display for Ipv4Addr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}.{}", self.0[0], self.0[1], self.0[2], self.0[3])
    }
}

/// IPv6 address (16 bytes)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(C)]
pub struct Ipv6Addr(pub [u8; 16]);

impl Ipv6Addr {
    /// Any address (::)
    pub const ANY: Ipv6Addr = Ipv6Addr([0; 16]);
    /// Localhost (::1)
    pub const LOCALHOST: Ipv6Addr = Ipv6Addr([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1]);

    /// Create a new IPv6 address from segments
    pub fn new(a: u16, b: u16, c: u16, d: u16, e: u16, f: u16, g: u16, h: u16) -> Self {
        let mut addr = [0u8; 16];
        addr[0..2].copy_from_slice(&a.to_be_bytes());
        addr[2..4].copy_from_slice(&b.to_be_bytes());
        addr[4..6].copy_from_slice(&c.to_be_bytes());
        addr[6..8].copy_from_slice(&d.to_be_bytes());
        addr[8..10].copy_from_slice(&e.to_be_bytes());
        addr[10..12].copy_from_slice(&f.to_be_bytes());
        addr[12..14].copy_from_slice(&g.to_be_bytes());
        addr[14..16].copy_from_slice(&h.to_be_bytes());
        Ipv6Addr(addr)
    }

    /// Get the bytes
    pub fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }

    /// Get segments
    pub fn segments(&self) -> [u16; 8] {
        let mut segs = [0u16; 8];
        for i in 0..8 {
            segs[i] = u16::from_be_bytes([self.0[i * 2], self.0[i * 2 + 1]]);
        }
        segs
    }

    /// Check if this is a loopback address
    pub fn is_loopback(&self) -> bool {
        *self == Self::LOCALHOST
    }

    /// Check if this is a multicast address
    pub fn is_multicast(&self) -> bool {
        self.0[0] == 0xff
    }

    /// Check if this is the any address
    pub fn is_any(&self) -> bool {
        *self == Self::ANY
    }
}

impl fmt::Display for Ipv6Addr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let segs = self.segments();
        write!(
            f,
            "{:x}:{:x}:{:x}:{:x}:{:x}:{:x}:{:x}:{:x}",
            segs[0], segs[1], segs[2], segs[3], segs[4], segs[5], segs[6], segs[7]
        )
    }
}

/// IP address (v4 or v6)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IpAddr {
    /// IPv4 address
    V4(Ipv4Addr),
    /// IPv6 address
    V6(Ipv6Addr),
}

impl IpAddr {
    /// Check if this is a loopback address
    pub fn is_loopback(&self) -> bool {
        match self {
            IpAddr::V4(a) => a.is_loopback(),
            IpAddr::V6(a) => a.is_loopback(),
        }
    }

    /// Check if this is a multicast address
    pub fn is_multicast(&self) -> bool {
        match self {
            IpAddr::V4(a) => a.is_multicast(),
            IpAddr::V6(a) => a.is_multicast(),
        }
    }
}

impl fmt::Display for IpAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IpAddr::V4(a) => write!(f, "{}", a),
            IpAddr::V6(a) => write!(f, "{}", a),
        }
    }
}

impl From<Ipv4Addr> for IpAddr {
    fn from(addr: Ipv4Addr) -> Self {
        IpAddr::V4(addr)
    }
}

impl From<Ipv6Addr> for IpAddr {
    fn from(addr: Ipv6Addr) -> Self {
        IpAddr::V6(addr)
    }
}

/// Socket address (IP + port)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SocketAddr {
    /// IP address
    pub ip: IpAddr,
    /// Port number
    pub port: u16,
}

impl SocketAddr {
    /// Create a new socket address
    pub fn new(ip: IpAddr, port: u16) -> Self {
        SocketAddr { ip, port }
    }

    /// Create a new IPv4 socket address
    pub fn new_v4(ip: Ipv4Addr, port: u16) -> Self {
        SocketAddr {
            ip: IpAddr::V4(ip),
            port,
        }
    }

    /// Create a new IPv6 socket address
    pub fn new_v6(ip: Ipv6Addr, port: u16) -> Self {
        SocketAddr {
            ip: IpAddr::V6(ip),
            port,
        }
    }
}

impl fmt::Display for SocketAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.ip {
            IpAddr::V4(a) => write!(f, "{}:{}", a, self.port),
            IpAddr::V6(a) => write!(f, "[{}]:{}", a, self.port),
        }
    }
}

/// Socket address for syscalls (IPv4)
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct SockAddrIn {
    /// Address family (AF_INET = 2)
    pub sin_family: u16,
    /// Port (network byte order)
    pub sin_port: u16,
    /// IPv4 address (network byte order)
    pub sin_addr: u32,
    /// Padding
    pub sin_zero: [u8; 8],
}

impl SockAddrIn {
    /// AF_INET constant
    pub const AF_INET: u16 = 2;

    /// Create from socket address
    pub fn from_socket_addr(addr: SocketAddr) -> Option<Self> {
        match addr.ip {
            IpAddr::V4(ipv4) => Some(SockAddrIn {
                sin_family: Self::AF_INET,
                sin_port: addr.port.to_be(),
                sin_addr: ipv4.to_u32(),
                sin_zero: [0; 8],
            }),
            IpAddr::V6(_) => None,
        }
    }

    /// Convert to socket address
    pub fn to_socket_addr(&self) -> SocketAddr {
        SocketAddr {
            ip: IpAddr::V4(Ipv4Addr::from_u32(self.sin_addr)),
            port: u16::from_be(self.sin_port),
        }
    }
}

/// Socket address for syscalls (IPv6)
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct SockAddrIn6 {
    /// Address family (AF_INET6 = 10)
    pub sin6_family: u16,
    /// Port (network byte order)
    pub sin6_port: u16,
    /// Flow info
    pub sin6_flowinfo: u32,
    /// IPv6 address
    pub sin6_addr: [u8; 16],
    /// Scope ID
    pub sin6_scope_id: u32,
}

impl SockAddrIn6 {
    /// AF_INET6 constant
    pub const AF_INET6: u16 = 10;

    /// Create from socket address
    pub fn from_socket_addr(addr: SocketAddr) -> Option<Self> {
        match addr.ip {
            IpAddr::V6(ipv6) => Some(SockAddrIn6 {
                sin6_family: Self::AF_INET6,
                sin6_port: addr.port.to_be(),
                sin6_flowinfo: 0,
                sin6_addr: ipv6.0,
                sin6_scope_id: 0,
            }),
            IpAddr::V4(_) => None,
        }
    }

    /// Convert to socket address
    pub fn to_socket_addr(&self) -> SocketAddr {
        SocketAddr {
            ip: IpAddr::V6(Ipv6Addr(self.sin6_addr)),
            port: u16::from_be(self.sin6_port),
        }
    }
}
