# EFFLUX Network Stack Specification

**Version:** 1.0
**Status:** Draft
**License:** MIT

---

## 0) Overview

EFFLUX implements a full TCP/IP network stack with BSD socket compatibility.

**Goals:**
- POSIX socket API compatibility
- IPv4 and IPv6 support
- High performance with zero-copy where possible
- Network namespace support for containers
- Firewall and routing capabilities

**Implementation Strategy:**
- Phase 1: Integrate smoltcp for initial bring-up
- Phase 2: Native stack for performance-critical paths
- Phase 3: Full native stack with advanced features

---

## 1) Architecture

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                              Applications                                    │
│                                    │                                         │
│                                    ▼                                         │
│   ┌─────────────────────────────────────────────────────────────────────┐   │
│   │                         Socket API                                   │   │
│   │   socket(), bind(), listen(), accept(), connect()                   │   │
│   │   send(), recv(), sendto(), recvfrom(), sendmsg(), recvmsg()       │   │
│   │   setsockopt(), getsockopt(), shutdown(), close()                   │   │
│   └───────────────────────────────┬─────────────────────────────────────┘   │
│                                   │                                          │
│   ┌───────────────────────────────┼───────────────────────────────────────┐ │
│   │                       Socket Layer                                    │ │
│   │  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐             │ │
│   │  │   TCP    │  │   UDP    │  │   RAW    │  │  UNIX    │             │ │
│   │  │ Sockets  │  │ Sockets  │  │ Sockets  │  │ Sockets  │             │ │
│   │  └────┬─────┘  └────┬─────┘  └────┬─────┘  └──────────┘             │ │
│   └───────┼─────────────┼─────────────┼─────────────────────────────────┘ │
│           │             │             │                                     │
│   ┌───────┴─────────────┴─────────────┴─────────────────────────────────┐ │
│   │                      Transport Layer                                 │ │
│   │  ┌────────────────────────┐  ┌────────────────────────┐            │ │
│   │  │          TCP           │  │          UDP           │            │ │
│   │  │  • Connection state    │  │  • Connectionless      │            │ │
│   │  │  • Retransmission      │  │  • Checksum optional   │            │ │
│   │  │  • Flow control        │  │                        │            │ │
│   │  │  • Congestion control  │  │                        │            │ │
│   │  └───────────┬────────────┘  └───────────┬────────────┘            │ │
│   └──────────────┼───────────────────────────┼──────────────────────────┘ │
│                  │                           │                             │
│   ┌──────────────┴───────────────────────────┴──────────────────────────┐ │
│   │                       Network Layer                                  │ │
│   │  ┌──────────────────┐  ┌──────────────────┐  ┌──────────────────┐  │ │
│   │  │       IPv4       │  │       IPv6       │  │       ICMP       │  │ │
│   │  │  • Routing       │  │  • Routing       │  │  • Ping          │  │ │
│   │  │  • Fragmentation │  │  • Extension hdrs│  │  • Errors        │  │ │
│   │  └────────┬─────────┘  └────────┬─────────┘  └────────┬─────────┘  │ │
│   └───────────┼─────────────────────┼─────────────────────┼─────────────┘ │
│               │                     │                     │               │
│   ┌───────────┴─────────────────────┴─────────────────────┴─────────────┐ │
│   │                       Link Layer                                     │ │
│   │  ┌──────────────────┐  ┌──────────────────┐                        │ │
│   │  │       ARP        │  │      NDP         │                        │ │
│   │  │  (IPv4 → MAC)    │  │  (IPv6 → MAC)    │                        │ │
│   │  └────────┬─────────┘  └────────┬─────────┘                        │ │
│   │           │                     │                                   │ │
│   │  ┌────────┴─────────────────────┴─────────┐                        │ │
│   │  │              Ethernet                   │                        │ │
│   │  │  • Frame construction                  │                        │ │
│   │  │  • MAC addressing                      │                        │ │
│   │  │  • VLAN tagging (802.1Q)              │                        │ │
│   │  └──────────────────┬─────────────────────┘                        │ │
│   └─────────────────────┼───────────────────────────────────────────────┘ │
│                         │                                                  │
│   ┌─────────────────────┴───────────────────────────────────────────────┐ │
│   │                    Network Device Layer                              │ │
│   │  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐            │ │
│   │  │ virtio-  │  │  e1000   │  │  Intel   │  │ Loopback │            │ │
│   │  │   net    │  │          │  │  i225    │  │          │            │ │
│   │  └────┬─────┘  └────┬─────┘  └────┬─────┘  └────┬─────┘            │ │
│   └───────┼─────────────┼─────────────┼─────────────┼────────────────────┘ │
│           │             │             │             │                      │
│           ▼             ▼             ▼             ▼                      │
│       Hardware       Hardware       Hardware    (internal)                 │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## 2) Network Device Interface

### 2.1 Driver Trait

```rust
pub trait NetworkDriver: Send + Sync {
    /// Driver identification
    fn name(&self) -> &'static str;
    fn driver_type(&self) -> NetworkDriverType;

    /// Device info
    fn mac_address(&self) -> MacAddress;
    fn mtu(&self) -> usize;
    fn link_speed(&self) -> LinkSpeed;
    fn link_state(&self) -> LinkState;

    /// Capabilities
    fn capabilities(&self) -> NetworkCapabilities;

    /// Transmit packet
    fn transmit(&self, packet: &[u8]) -> Result<()>;

    /// Transmit with scatter-gather (zero-copy)
    fn transmit_sg(&self, segments: &[IoVec]) -> Result<()>;

    /// Receive packet (polling mode)
    fn receive(&self, buf: &mut [u8]) -> Result<usize>;

    /// Enable/disable interrupts
    fn set_interrupt_enabled(&mut self, enabled: bool);

    /// Handle interrupt (called from IRQ handler)
    fn handle_interrupt(&mut self) -> InterruptResult;

    /// Promiscuous mode
    fn set_promiscuous(&mut self, enabled: bool);

    /// Multicast filtering
    fn add_multicast(&mut self, addr: MacAddress);
    fn remove_multicast(&mut self, addr: MacAddress);

    /// Hardware offload
    fn set_checksum_offload(&mut self, rx: bool, tx: bool);
    fn set_tso(&mut self, enabled: bool, mss: u16);
    fn set_lro(&mut self, enabled: bool);
}

pub enum NetworkDriverType {
    VirtioNet,
    E1000,
    E1000E,
    I225,           // Intel 2.5GbE
    IxgbeVf,        // Intel 10GbE virtual function
    Loopback,
    TunTap,
}

#[derive(Clone, Copy)]
pub struct MacAddress(pub [u8; 6]);

pub enum LinkSpeed {
    Speed10M,
    Speed100M,
    Speed1G,
    Speed2_5G,
    Speed10G,
    Speed25G,
    Speed100G,
    Unknown,
}

pub enum LinkState {
    Up,
    Down,
    Unknown,
}

bitflags! {
    pub struct NetworkCapabilities: u32 {
        const CHECKSUM_IPV4_RX   = 0x0001;  // IPv4 checksum offload (RX)
        const CHECKSUM_IPV4_TX   = 0x0002;  // IPv4 checksum offload (TX)
        const CHECKSUM_TCP_RX    = 0x0004;  // TCP checksum offload (RX)
        const CHECKSUM_TCP_TX    = 0x0008;  // TCP checksum offload (TX)
        const CHECKSUM_UDP_RX    = 0x0010;  // UDP checksum offload (RX)
        const CHECKSUM_UDP_TX    = 0x0020;  // UDP checksum offload (TX)
        const TSO               = 0x0040;  // TCP Segmentation Offload
        const LRO               = 0x0080;  // Large Receive Offload
        const SCATTER_GATHER    = 0x0100;  // Scatter-gather DMA
        const VLAN_STRIP        = 0x0200;  // VLAN tag stripping
        const VLAN_INSERT       = 0x0400;  // VLAN tag insertion
        const MULTI_QUEUE       = 0x0800;  // Multiple TX/RX queues
        const RSS               = 0x1000;  // Receive Side Scaling
    }
}
```

### 2.2 Network Interface

Higher-level abstraction over drivers:

```rust
pub struct NetworkInterface {
    pub name: String,               // "eth0", "wlan0", etc.
    pub index: u32,                 // Interface index
    pub driver: Arc<dyn NetworkDriver>,
    pub mac: MacAddress,
    pub mtu: usize,
    pub flags: InterfaceFlags,
    pub ipv4_addrs: Vec<Ipv4Config>,
    pub ipv6_addrs: Vec<Ipv6Config>,
    pub stats: InterfaceStats,
}

pub struct Ipv4Config {
    pub address: Ipv4Addr,
    pub netmask: Ipv4Addr,
    pub broadcast: Ipv4Addr,
}

pub struct Ipv6Config {
    pub address: Ipv6Addr,
    pub prefix_len: u8,
    pub scope: Ipv6Scope,
}

bitflags! {
    pub struct InterfaceFlags: u32 {
        const UP            = 0x0001;
        const BROADCAST     = 0x0002;
        const LOOPBACK      = 0x0008;
        const POINTTOPOINT  = 0x0010;
        const RUNNING       = 0x0040;
        const PROMISC       = 0x0100;
        const MULTICAST     = 0x1000;
    }
}

pub struct InterfaceStats {
    pub rx_packets: u64,
    pub tx_packets: u64,
    pub rx_bytes: u64,
    pub tx_bytes: u64,
    pub rx_errors: u64,
    pub tx_errors: u64,
    pub rx_dropped: u64,
    pub tx_dropped: u64,
}
```

---

## 3) Protocol Implementations

### 3.1 Ethernet

```rust
#[repr(C, packed)]
pub struct EthernetHeader {
    pub dst_mac: [u8; 6],
    pub src_mac: [u8; 6],
    pub ethertype: u16,         // Big-endian
}

pub const ETHERTYPE_IPV4: u16 = 0x0800;
pub const ETHERTYPE_ARP:  u16 = 0x0806;
pub const ETHERTYPE_IPV6: u16 = 0x86DD;
pub const ETHERTYPE_VLAN: u16 = 0x8100;

pub const ETHERNET_MTU: usize = 1500;
pub const ETHERNET_HEADER_LEN: usize = 14;
pub const ETHERNET_MIN_FRAME: usize = 64;
pub const ETHERNET_MAX_FRAME: usize = 1518;
```

### 3.2 ARP (Address Resolution Protocol)

```rust
#[repr(C, packed)]
pub struct ArpPacket {
    pub htype: u16,             // Hardware type (1 = Ethernet)
    pub ptype: u16,             // Protocol type (0x0800 = IPv4)
    pub hlen: u8,               // Hardware address length (6)
    pub plen: u8,               // Protocol address length (4)
    pub operation: u16,         // 1 = Request, 2 = Reply
    pub sender_mac: [u8; 6],
    pub sender_ip: [u8; 4],
    pub target_mac: [u8; 6],
    pub target_ip: [u8; 4],
}

pub struct ArpCache {
    entries: HashMap<Ipv4Addr, ArpEntry>,
    pending: HashMap<Ipv4Addr, Vec<PendingPacket>>,
}

pub struct ArpEntry {
    pub mac: MacAddress,
    pub state: ArpState,
    pub expires: Instant,
}

pub enum ArpState {
    Incomplete,     // ARP request sent, waiting for reply
    Reachable,      // Valid entry
    Stale,          // Entry may be outdated
    Delay,          // Waiting before probe
    Probe,          // Sending unicast probes
}
```

### 3.3 IPv4

```rust
#[repr(C, packed)]
pub struct Ipv4Header {
    pub version_ihl: u8,        // Version (4 bits) + IHL (4 bits)
    pub dscp_ecn: u8,           // DSCP (6 bits) + ECN (2 bits)
    pub total_length: u16,
    pub identification: u16,
    pub flags_fragment: u16,    // Flags (3 bits) + Fragment offset (13 bits)
    pub ttl: u8,
    pub protocol: u8,
    pub checksum: u16,
    pub src_addr: [u8; 4],
    pub dst_addr: [u8; 4],
    // Options follow (if IHL > 5)
}

pub const IPPROTO_ICMP: u8 = 1;
pub const IPPROTO_TCP:  u8 = 6;
pub const IPPROTO_UDP:  u8 = 17;

impl Ipv4Header {
    pub fn checksum(&self) -> u16 {
        // Internet checksum (RFC 1071)
        let mut sum: u32 = 0;
        let words = unsafe {
            core::slice::from_raw_parts(
                self as *const _ as *const u16,
                self.header_len() / 2
            )
        };
        for &word in words {
            sum += u16::from_be(word) as u32;
        }
        while sum >> 16 != 0 {
            sum = (sum & 0xFFFF) + (sum >> 16);
        }
        !(sum as u16)
    }
}
```

### 3.4 IPv6

```rust
#[repr(C, packed)]
pub struct Ipv6Header {
    pub version_tc_flow: u32,   // Version (4) + TC (8) + Flow Label (20)
    pub payload_length: u16,
    pub next_header: u8,
    pub hop_limit: u8,
    pub src_addr: [u8; 16],
    pub dst_addr: [u8; 16],
}

pub const IPV6_HEADER_LEN: usize = 40;
```

### 3.5 ICMP

```rust
#[repr(C, packed)]
pub struct IcmpHeader {
    pub icmp_type: u8,
    pub code: u8,
    pub checksum: u16,
    pub rest: u32,              // Depends on type/code
}

// ICMP Types
pub const ICMP_ECHO_REPLY:   u8 = 0;
pub const ICMP_DEST_UNREACH: u8 = 3;
pub const ICMP_ECHO_REQUEST: u8 = 8;
pub const ICMP_TIME_EXCEEDED: u8 = 11;

pub struct IcmpHandler {
    pending_pings: HashMap<(Ipv4Addr, u16, u16), PingRequest>,
}

impl IcmpHandler {
    pub fn send_ping(&mut self, dst: Ipv4Addr, seq: u16) -> Result<()>;
    pub fn handle_echo_request(&mut self, src: Ipv4Addr, data: &[u8]) -> Result<()>;
    pub fn handle_echo_reply(&mut self, src: Ipv4Addr, id: u16, seq: u16, rtt: Duration);
}
```

### 3.6 UDP

```rust
#[repr(C, packed)]
pub struct UdpHeader {
    pub src_port: u16,
    pub dst_port: u16,
    pub length: u16,
    pub checksum: u16,
}

pub const UDP_HEADER_LEN: usize = 8;

pub struct UdpSocket {
    local_addr: Option<SocketAddr>,
    peer_addr: Option<SocketAddr>,
    recv_buffer: RingBuffer<UdpDatagram>,
    send_buffer: RingBuffer<UdpDatagram>,
    options: UdpOptions,
}

pub struct UdpDatagram {
    pub src: SocketAddr,
    pub dst: SocketAddr,
    pub data: Vec<u8>,
}
```

### 3.7 TCP

```rust
#[repr(C, packed)]
pub struct TcpHeader {
    pub src_port: u16,
    pub dst_port: u16,
    pub seq_num: u32,
    pub ack_num: u32,
    pub data_offset_flags: u16, // Data offset (4) + Reserved (3) + Flags (9)
    pub window: u16,
    pub checksum: u16,
    pub urgent_ptr: u16,
    // Options follow
}

bitflags! {
    pub struct TcpFlags: u16 {
        const FIN = 0x001;
        const SYN = 0x002;
        const RST = 0x004;
        const PSH = 0x008;
        const ACK = 0x010;
        const URG = 0x020;
        const ECE = 0x040;
        const CWR = 0x080;
        const NS  = 0x100;
    }
}

pub struct TcpConnection {
    local: SocketAddr,
    remote: SocketAddr,
    state: TcpState,

    // Sequence numbers
    snd_una: u32,       // Oldest unacknowledged
    snd_nxt: u32,       // Next to send
    snd_wnd: u16,       // Send window
    rcv_nxt: u32,       // Next expected
    rcv_wnd: u16,       // Receive window

    // Buffers
    send_buffer: TcpBuffer,
    recv_buffer: TcpBuffer,

    // Retransmission
    rto: Duration,      // Retransmission timeout
    srtt: Duration,     // Smoothed RTT
    rttvar: Duration,   // RTT variance
    retransmit_queue: VecDeque<TcpSegment>,

    // Congestion control
    cwnd: u32,          // Congestion window
    ssthresh: u32,      // Slow start threshold
    congestion_state: CongestionState,

    // Timers
    retransmit_timer: Option<Timer>,
    time_wait_timer: Option<Timer>,
    keepalive_timer: Option<Timer>,
}

pub enum TcpState {
    Closed,
    Listen,
    SynSent,
    SynReceived,
    Established,
    FinWait1,
    FinWait2,
    CloseWait,
    Closing,
    LastAck,
    TimeWait,
}

pub enum CongestionState {
    SlowStart,
    CongestionAvoidance,
    FastRecovery,
}
```

---

## 4) Socket API

### 4.1 Socket Structures

```rust
pub struct Socket {
    pub domain: AddressFamily,
    pub socket_type: SocketType,
    pub protocol: Protocol,
    pub state: SocketState,
    pub options: SocketOptions,
    pub inner: SocketInner,
}

pub enum AddressFamily {
    Unix = 1,       // AF_UNIX / AF_LOCAL
    Inet = 2,       // AF_INET (IPv4)
    Inet6 = 10,     // AF_INET6
    Netlink = 16,   // AF_NETLINK
    Packet = 17,    // AF_PACKET (raw)
}

pub enum SocketType {
    Stream = 1,     // SOCK_STREAM (TCP)
    Dgram = 2,      // SOCK_DGRAM (UDP)
    Raw = 3,        // SOCK_RAW
    SeqPacket = 5,  // SOCK_SEQPACKET
}

pub enum SocketInner {
    Tcp(TcpSocket),
    Udp(UdpSocket),
    Unix(UnixSocket),
    Raw(RawSocket),
    Netlink(NetlinkSocket),
    Packet(PacketSocket),
}

pub struct SocketOptions {
    pub reuseaddr: bool,
    pub reuseport: bool,
    pub keepalive: bool,
    pub broadcast: bool,
    pub nodelay: bool,          // TCP_NODELAY
    pub recv_timeout: Option<Duration>,
    pub send_timeout: Option<Duration>,
    pub recv_bufsize: usize,
    pub send_bufsize: usize,
    pub linger: Option<Duration>,
}
```

### 4.2 Socket Syscalls

```rust
// Socket creation
pub fn sys_socket(domain: i32, type_: i32, protocol: i32) -> Result<i32>;
pub fn sys_socketpair(domain: i32, type_: i32, protocol: i32, sv: *mut [i32; 2]) -> Result<()>;

// Socket addressing
pub fn sys_bind(sockfd: i32, addr: *const SockAddr, addrlen: u32) -> Result<()>;
pub fn sys_connect(sockfd: i32, addr: *const SockAddr, addrlen: u32) -> Result<()>;
pub fn sys_listen(sockfd: i32, backlog: i32) -> Result<()>;
pub fn sys_accept(sockfd: i32, addr: *mut SockAddr, addrlen: *mut u32) -> Result<i32>;
pub fn sys_accept4(sockfd: i32, addr: *mut SockAddr, addrlen: *mut u32, flags: i32) -> Result<i32>;

// Socket I/O
pub fn sys_send(sockfd: i32, buf: *const u8, len: usize, flags: i32) -> Result<isize>;
pub fn sys_recv(sockfd: i32, buf: *mut u8, len: usize, flags: i32) -> Result<isize>;
pub fn sys_sendto(sockfd: i32, buf: *const u8, len: usize, flags: i32,
                  dest: *const SockAddr, addrlen: u32) -> Result<isize>;
pub fn sys_recvfrom(sockfd: i32, buf: *mut u8, len: usize, flags: i32,
                    src: *mut SockAddr, addrlen: *mut u32) -> Result<isize>;
pub fn sys_sendmsg(sockfd: i32, msg: *const MsgHdr, flags: i32) -> Result<isize>;
pub fn sys_recvmsg(sockfd: i32, msg: *mut MsgHdr, flags: i32) -> Result<isize>;

// Socket options
pub fn sys_setsockopt(sockfd: i32, level: i32, optname: i32,
                      optval: *const u8, optlen: u32) -> Result<()>;
pub fn sys_getsockopt(sockfd: i32, level: i32, optname: i32,
                      optval: *mut u8, optlen: *mut u32) -> Result<()>;

// Socket info
pub fn sys_getsockname(sockfd: i32, addr: *mut SockAddr, addrlen: *mut u32) -> Result<()>;
pub fn sys_getpeername(sockfd: i32, addr: *mut SockAddr, addrlen: *mut u32) -> Result<()>;

// Socket control
pub fn sys_shutdown(sockfd: i32, how: i32) -> Result<()>;

// I/O flags
pub const MSG_PEEK:      i32 = 0x02;
pub const MSG_OOB:       i32 = 0x01;
pub const MSG_WAITALL:   i32 = 0x100;
pub const MSG_DONTWAIT:  i32 = 0x40;
pub const MSG_NOSIGNAL:  i32 = 0x4000;
```

### 4.3 Address Structures

```rust
#[repr(C)]
pub struct SockAddrIn {
    pub family: u16,        // AF_INET
    pub port: u16,          // Port (network byte order)
    pub addr: [u8; 4],      // IPv4 address
    pub zero: [u8; 8],      // Padding
}

#[repr(C)]
pub struct SockAddrIn6 {
    pub family: u16,        // AF_INET6
    pub port: u16,          // Port (network byte order)
    pub flowinfo: u32,
    pub addr: [u8; 16],     // IPv6 address
    pub scope_id: u32,
}

#[repr(C)]
pub struct SockAddrUn {
    pub family: u16,        // AF_UNIX
    pub path: [u8; 108],    // Path name
}
```

---

## 5) Routing

### 5.1 Routing Table

```rust
pub struct RoutingTable {
    routes: Vec<Route>,
}

pub struct Route {
    pub destination: IpNetwork,
    pub gateway: Option<IpAddr>,
    pub interface: InterfaceIndex,
    pub metric: u32,
    pub flags: RouteFlags,
    pub mtu: Option<u32>,
}

pub enum IpNetwork {
    V4 { addr: Ipv4Addr, prefix: u8 },
    V6 { addr: Ipv6Addr, prefix: u8 },
}

bitflags! {
    pub struct RouteFlags: u32 {
        const UP        = 0x0001;   // Route is usable
        const GATEWAY   = 0x0002;   // Destination is gateway
        const HOST      = 0x0004;   // Host route
        const REJECT    = 0x0008;   // Reject route
        const DYNAMIC   = 0x0010;   // Created dynamically
        const MODIFIED  = 0x0020;   // Modified dynamically
        const DEFAULT   = 0x0040;   // Default route
    }
}

impl RoutingTable {
    /// Longest prefix match
    pub fn lookup(&self, dst: IpAddr) -> Option<&Route> {
        let mut best: Option<&Route> = None;
        let mut best_prefix = 0;

        for route in &self.routes {
            if route.matches(dst) && route.prefix_len() > best_prefix {
                best = Some(route);
                best_prefix = route.prefix_len();
            }
        }
        best
    }

    pub fn add(&mut self, route: Route) -> Result<()>;
    pub fn remove(&mut self, destination: IpNetwork) -> Result<()>;
}
```

### 5.2 Routing Syscalls

```rust
// Netlink-based routing (modern)
pub fn sys_socket(AF_NETLINK, SOCK_DGRAM, NETLINK_ROUTE) -> Result<i32>;

// Legacy ioctl-based
pub fn sys_ioctl(sockfd: i32, SIOCADDRT, route: *const RtEntry) -> Result<()>;
pub fn sys_ioctl(sockfd: i32, SIOCDELRT, route: *const RtEntry) -> Result<()>;
```

---

## 6) DNS Resolution

### 6.1 Resolver

```rust
pub struct DnsResolver {
    nameservers: Vec<SocketAddr>,
    search_domains: Vec<String>,
    cache: DnsCache,
    options: ResolverOptions,
}

pub struct ResolverOptions {
    pub timeout: Duration,
    pub attempts: u32,
    pub rotate: bool,           // Rotate through nameservers
    pub use_tcp: bool,          // Use TCP for queries
}

impl DnsResolver {
    pub fn resolve(&self, hostname: &str) -> Result<Vec<IpAddr>>;
    pub fn resolve_ptr(&self, addr: IpAddr) -> Result<String>;
    pub fn resolve_mx(&self, domain: &str) -> Result<Vec<(u16, String)>>;
    pub fn resolve_txt(&self, domain: &str) -> Result<Vec<String>>;
}

pub struct DnsCache {
    entries: HashMap<(String, RecordType), CacheEntry>,
}

pub struct CacheEntry {
    records: Vec<DnsRecord>,
    expires: Instant,
}
```

### 6.2 /etc/resolv.conf

```rust
pub fn parse_resolv_conf(path: &str) -> Result<ResolverConfig> {
    // Parse:
    // nameserver 8.8.8.8
    // nameserver 8.8.4.4
    // search example.com
    // options timeout:2 attempts:3
}
```

### 6.3 /etc/hosts

```rust
pub struct HostsFile {
    entries: HashMap<String, Vec<IpAddr>>,
}

impl HostsFile {
    pub fn lookup(&self, hostname: &str) -> Option<&[IpAddr]>;
}
```

---

## 7) DHCP Client

```rust
pub struct DhcpClient {
    interface: InterfaceIndex,
    state: DhcpState,
    xid: u32,
    lease: Option<DhcpLease>,
}

pub enum DhcpState {
    Init,
    Selecting,
    Requesting,
    Bound,
    Renewing,
    Rebinding,
}

pub struct DhcpLease {
    pub ip_address: Ipv4Addr,
    pub subnet_mask: Ipv4Addr,
    pub gateway: Option<Ipv4Addr>,
    pub dns_servers: Vec<Ipv4Addr>,
    pub domain_name: Option<String>,
    pub lease_time: Duration,
    pub renewal_time: Duration,
    pub rebind_time: Duration,
    pub server: Ipv4Addr,
    pub obtained: Instant,
}

impl DhcpClient {
    pub fn start(&mut self) -> Result<()>;
    pub fn release(&mut self) -> Result<()>;
    pub fn renew(&mut self) -> Result<()>;
}
```

---

## 8) Firewall (Netfilter-like)

### 8.1 Packet Filter

```rust
pub struct PacketFilter {
    chains: HashMap<ChainType, Chain>,
    tables: HashMap<TableType, Table>,
}

pub enum TableType {
    Filter,     // Packet filtering
    Nat,        // Network address translation
    Mangle,     // Packet modification
    Raw,        // Before connection tracking
}

pub enum ChainType {
    Input,      // Incoming packets
    Output,     // Outgoing packets
    Forward,    // Forwarded packets
    PreRouting, // Before routing decision
    PostRouting,// After routing decision
}

pub struct Chain {
    rules: Vec<Rule>,
    policy: ChainPolicy,
}

pub struct Rule {
    pub matches: Vec<Match>,
    pub target: Target,
    pub counters: RuleCounters,
}

pub enum Match {
    SrcAddr(IpNetwork),
    DstAddr(IpNetwork),
    SrcPort(PortRange),
    DstPort(PortRange),
    Protocol(u8),
    Interface { name: String, inbound: bool },
    State(ConnectionState),
    // ... more
}

pub enum Target {
    Accept,
    Drop,
    Reject { code: IcmpCode },
    Log { prefix: String, level: LogLevel },
    Snat { to: SocketAddr },
    Dnat { to: SocketAddr },
    Masquerade,
    Jump { chain: String },
    Return,
}
```

### 8.2 Connection Tracking

```rust
pub struct ConnTrack {
    connections: HashMap<ConnTuple, Connection>,
}

pub struct ConnTuple {
    src: SocketAddr,
    dst: SocketAddr,
    protocol: u8,
}

pub struct Connection {
    state: ConnectionState,
    timeout: Instant,
    mark: u32,
    nat: Option<NatInfo>,
}

pub enum ConnectionState {
    New,
    Established,
    Related,
    Invalid,
}
```

---

## 9) Network Namespaces

For container isolation:

```rust
pub struct NetworkNamespace {
    id: NamespaceId,
    interfaces: Vec<NetworkInterface>,
    routing_table: RoutingTable,
    arp_cache: ArpCache,
    sockets: SocketTable,
    firewall: PacketFilter,
}

impl NetworkNamespace {
    pub fn new() -> Result<Self>;
    pub fn add_interface(&mut self, iface: NetworkInterface) -> Result<()>;
    pub fn remove_interface(&mut self, name: &str) -> Result<()>;
}

// Virtual ethernet pairs for connecting namespaces
pub struct VethPair {
    pub end_a: NetworkInterface,
    pub end_b: NetworkInterface,
}

impl VethPair {
    pub fn create(name_a: &str, name_b: &str) -> Result<Self>;
}
```

---

## 10) Network Drivers

### 10.1 virtio-net (QEMU/KVM)

```rust
pub struct VirtioNet {
    device: VirtioDevice,
    rx_queue: VirtQueue,
    tx_queue: VirtQueue,
    ctrl_queue: Option<VirtQueue>,
    mac: MacAddress,
    features: VirtioNetFeatures,
}

bitflags! {
    pub struct VirtioNetFeatures: u64 {
        const CSUM           = 1 << 0;
        const GUEST_CSUM     = 1 << 1;
        const MAC            = 1 << 5;
        const GSO            = 1 << 6;
        const GUEST_TSO4     = 1 << 7;
        const GUEST_TSO6     = 1 << 8;
        const MRG_RXBUF      = 1 << 15;
        const STATUS         = 1 << 16;
        const CTRL_VQ        = 1 << 17;
        const MQ             = 1 << 22;
    }
}
```

### 10.2 Intel e1000 (Common)

```rust
pub struct E1000 {
    mmio_base: *mut u8,
    rx_ring: RxRing,
    tx_ring: TxRing,
    mac: MacAddress,
    irq: u8,
}

struct RxRing {
    descriptors: *mut E1000RxDesc,
    buffers: Vec<*mut u8>,
    head: u32,
    tail: u32,
    count: u32,
}

#[repr(C)]
struct E1000RxDesc {
    buffer_addr: u64,
    length: u16,
    checksum: u16,
    status: u8,
    errors: u8,
    special: u16,
}
```

### 10.3 Loopback

```rust
pub struct LoopbackDevice {
    mtu: usize,
    rx_queue: VecDeque<Vec<u8>>,
}

impl NetworkDriver for LoopbackDevice {
    fn transmit(&self, packet: &[u8]) -> Result<()> {
        // Loop back to receive queue
        self.rx_queue.push_back(packet.to_vec());
        Ok(())
    }
}
```

### 10.4 Driver Registration & Multi-NIC Support

```rust
/// Global network driver registry
pub struct NetworkDriverRegistry {
    /// Registered driver factories
    factories: Vec<Box<dyn NetworkDriverFactory>>,

    /// Active driver instances (multiple NICs)
    drivers: Vec<Arc<dyn NetworkDriver>>,

    /// Interface index counter
    next_index: AtomicU32,
}

/// Factory trait for creating driver instances
pub trait NetworkDriverFactory: Send + Sync {
    /// Driver name
    fn name(&self) -> &'static str;

    /// Check if this factory can handle the given PCI device
    fn probe(&self, pci: &PciDevice) -> bool;

    /// Create a driver instance for the device
    fn create(&self, pci: &PciDevice) -> Result<Box<dyn NetworkDriver>>;
}

impl NetworkDriverRegistry {
    pub fn new() -> Self {
        Self {
            factories: Vec::new(),
            drivers: Vec::new(),
            next_index: AtomicU32::new(0),
        }
    }

    /// Register a driver factory
    pub fn register_factory(&mut self, factory: Box<dyn NetworkDriverFactory>) {
        log::info!("Registered network driver: {}", factory.name());
        self.factories.push(factory);
    }

    /// Probe and initialize all network devices
    pub fn probe_all(&mut self) -> Result<()> {
        for pci_device in pci_enumerate() {
            // Skip non-network devices
            if pci_device.class_code != PCI_CLASS_NETWORK {
                continue;
            }

            // Try each factory
            for factory in &self.factories {
                if factory.probe(&pci_device) {
                    match factory.create(&pci_device) {
                        Ok(driver) => {
                            let index = self.next_index.fetch_add(1, Ordering::SeqCst);
                            log::info!(
                                "Initialized {} on {:02x}:{:02x}.{} as eth{}",
                                factory.name(),
                                pci_device.bus,
                                pci_device.device,
                                pci_device.function,
                                index
                            );
                            self.drivers.push(Arc::from(driver));
                            break;
                        }
                        Err(e) => {
                            log::warn!(
                                "Failed to initialize {} on {:02x}:{:02x}.{}: {:?}",
                                factory.name(),
                                pci_device.bus,
                                pci_device.device,
                                pci_device.function,
                                e
                            );
                        }
                    }
                }
            }
        }

        // Always create loopback
        self.create_loopback();

        Ok(())
    }

    fn create_loopback(&mut self) {
        let lo = LoopbackDevice::new();
        self.drivers.push(Arc::new(lo));
    }

    /// Get all active drivers
    pub fn drivers(&self) -> &[Arc<dyn NetworkDriver>] {
        &self.drivers
    }

    /// Get driver by index
    pub fn get(&self, index: u32) -> Option<&Arc<dyn NetworkDriver>> {
        self.drivers.get(index as usize)
    }

    /// Get driver by name
    pub fn get_by_name(&self, name: &str) -> Option<&Arc<dyn NetworkDriver>> {
        self.drivers.iter().find(|d| d.name() == name)
    }
}

/// Example driver factories
pub struct VirtioNetFactory;
pub struct E1000Factory;
pub struct E1000EFactory;
pub struct I225Factory;

impl NetworkDriverFactory for VirtioNetFactory {
    fn name(&self) -> &'static str { "virtio-net" }

    fn probe(&self, pci: &PciDevice) -> bool {
        pci.vendor_id == VIRTIO_VENDOR_ID &&
        pci.device_id == VIRTIO_NET_DEVICE_ID
    }

    fn create(&self, pci: &PciDevice) -> Result<Box<dyn NetworkDriver>> {
        Ok(Box::new(VirtioNet::new(pci)?))
    }
}

impl NetworkDriverFactory for E1000Factory {
    fn name(&self) -> &'static str { "e1000" }

    fn probe(&self, pci: &PciDevice) -> bool {
        pci.vendor_id == INTEL_VENDOR_ID &&
        matches!(pci.device_id,
            0x100E | // 82540EM
            0x100F | // 82545EM
            0x1015 | // 82540EM LOM
            0x1019 | // 82547EI
            0x101D   // 82546EB
        )
    }

    fn create(&self, pci: &PciDevice) -> Result<Box<dyn NetworkDriver>> {
        Ok(Box::new(E1000::new(pci)?))
    }
}

impl NetworkDriverFactory for I225Factory {
    fn name(&self) -> &'static str { "igc" }

    fn probe(&self, pci: &PciDevice) -> bool {
        pci.vendor_id == INTEL_VENDOR_ID &&
        matches!(pci.device_id,
            0x15F2 | // I225-LM
            0x15F3 | // I225-V
            0x3100 | // I225-K
            0x3101   // I225-K2
        )
    }

    fn create(&self, pci: &PciDevice) -> Result<Box<dyn NetworkDriver>> {
        Ok(Box::new(I225::new(pci)?))
    }
}
```

### 10.5 Network Interface Manager

```rust
/// Manages network interfaces (higher level than drivers)
pub struct NetworkInterfaceManager {
    interfaces: HashMap<String, NetworkInterface>,
    by_index: HashMap<u32, String>,
    next_eth_index: u32,
    next_wlan_index: u32,
}

impl NetworkInterfaceManager {
    pub fn new() -> Self {
        Self {
            interfaces: HashMap::new(),
            by_index: HashMap::new(),
            next_eth_index: 0,
            next_wlan_index: 0,
        }
    }

    /// Create interface from driver
    pub fn add_driver(&mut self, driver: Arc<dyn NetworkDriver>) -> Result<String> {
        let name = match driver.driver_type() {
            NetworkDriverType::Loopback => "lo".to_string(),
            NetworkDriverType::VirtioNet |
            NetworkDriverType::E1000 |
            NetworkDriverType::E1000E |
            NetworkDriverType::I225 |
            NetworkDriverType::IxgbeVf => {
                let idx = self.next_eth_index;
                self.next_eth_index += 1;
                format!("eth{}", idx)
            }
            NetworkDriverType::TunTap => {
                format!("tun{}", self.interfaces.len())
            }
        };

        let index = self.interfaces.len() as u32;
        let iface = NetworkInterface {
            name: name.clone(),
            index,
            driver: driver.clone(),
            mac: driver.mac_address(),
            mtu: driver.mtu(),
            flags: InterfaceFlags::empty(),
            ipv4_addrs: Vec::new(),
            ipv6_addrs: Vec::new(),
            stats: InterfaceStats::default(),
        };

        self.interfaces.insert(name.clone(), iface);
        self.by_index.insert(index, name.clone());

        Ok(name)
    }

    /// Get interface by name
    pub fn get(&self, name: &str) -> Option<&NetworkInterface> {
        self.interfaces.get(name)
    }

    /// Get interface by index
    pub fn get_by_index(&self, index: u32) -> Option<&NetworkInterface> {
        self.by_index.get(&index).and_then(|name| self.interfaces.get(name))
    }

    /// Get mutable interface
    pub fn get_mut(&mut self, name: &str) -> Option<&mut NetworkInterface> {
        self.interfaces.get_mut(name)
    }

    /// List all interfaces
    pub fn list(&self) -> impl Iterator<Item = &NetworkInterface> {
        self.interfaces.values()
    }

    /// Bring interface up
    pub fn up(&mut self, name: &str) -> Result<()> {
        let iface = self.interfaces.get_mut(name).ok_or(Error::NotFound)?;
        iface.flags.insert(InterfaceFlags::UP | InterfaceFlags::RUNNING);
        iface.driver.set_interrupt_enabled(true);
        Ok(())
    }

    /// Bring interface down
    pub fn down(&mut self, name: &str) -> Result<()> {
        let iface = self.interfaces.get_mut(name).ok_or(Error::NotFound)?;
        iface.flags.remove(InterfaceFlags::UP | InterfaceFlags::RUNNING);
        iface.driver.set_interrupt_enabled(false);
        Ok(())
    }

    /// Set IP address
    pub fn set_ipv4(&mut self, name: &str, config: Ipv4Config) -> Result<()> {
        let iface = self.interfaces.get_mut(name).ok_or(Error::NotFound)?;
        iface.ipv4_addrs.push(config);
        Ok(())
    }

    /// Set MTU
    pub fn set_mtu(&mut self, name: &str, mtu: usize) -> Result<()> {
        let iface = self.interfaces.get_mut(name).ok_or(Error::NotFound)?;
        iface.mtu = mtu;
        Ok(())
    }
}

/// Kernel initialization
pub fn init_networking() -> Result<()> {
    // Create driver registry
    let mut registry = NetworkDriverRegistry::new();

    // Register all driver factories
    registry.register_factory(Box::new(VirtioNetFactory));
    registry.register_factory(Box::new(E1000Factory));
    registry.register_factory(Box::new(E1000EFactory));
    registry.register_factory(Box::new(I225Factory));

    // Probe and initialize all devices
    registry.probe_all()?;

    // Create interface manager
    let mut iface_mgr = NetworkInterfaceManager::new();

    // Add all drivers as interfaces
    for driver in registry.drivers() {
        let name = iface_mgr.add_driver(driver.clone())?;
        log::info!("Created network interface: {}", name);
    }

    // Configure loopback
    iface_mgr.set_ipv4("lo", Ipv4Config {
        address: Ipv4Addr::new(127, 0, 0, 1),
        netmask: Ipv4Addr::new(255, 0, 0, 0),
        broadcast: Ipv4Addr::new(127, 255, 255, 255),
    })?;
    iface_mgr.up("lo")?;

    // Store in global state
    NETWORK_REGISTRY.init(registry);
    INTERFACE_MANAGER.init(iface_mgr);

    Ok(())
}
```

### 10.6 Multi-Queue Support (RSS/Multi-core)

```rust
/// Network device with multiple TX/RX queues
pub trait MultiQueueNetworkDriver: NetworkDriver {
    /// Number of TX queues
    fn tx_queue_count(&self) -> u32;

    /// Number of RX queues
    fn rx_queue_count(&self) -> u32;

    /// Transmit on specific queue
    fn transmit_queue(&self, queue: u32, packet: &[u8]) -> Result<()>;

    /// Receive from specific queue
    fn receive_queue(&self, queue: u32, buf: &mut [u8]) -> Result<usize>;

    /// Set RSS hash key
    fn set_rss_key(&mut self, key: &[u8; 40]);

    /// Set RSS indirection table
    fn set_rss_itable(&mut self, table: &[u8]);

    /// Get queue for CPU affinity
    fn queue_for_cpu(&self, cpu: u32) -> u32 {
        cpu % self.tx_queue_count()
    }
}

/// Per-CPU network processing
pub struct PerCpuNetStack {
    cpu_id: u32,
    tx_queue: u32,
    rx_queue: u32,
    socket_table: SocketTable,
}

impl PerCpuNetStack {
    /// Process incoming packets for this CPU
    pub fn poll_rx(&mut self) {
        // Poll RX queue assigned to this CPU
    }

    /// Send packet from this CPU
    pub fn transmit(&self, packet: &[u8]) {
        // Use TX queue assigned to this CPU
    }
}
```

---

## 11) Configuration

### 11.1 Network Configuration Files

```
/etc/efflux/network/
├── interfaces          # Interface configuration
├── routes              # Static routes
├── resolv.conf         # DNS servers
├── hosts               # Static host mappings
└── firewall.conf       # Firewall rules
```

### 11.2 Interface Configuration

```toml
# /etc/efflux/network/interfaces

[eth0]
type = "ethernet"
mac = "auto"                    # Use hardware MAC

[eth0.ipv4]
method = "dhcp"                 # or "static"
# For static:
# address = "192.168.1.100"
# netmask = "255.255.255.0"
# gateway = "192.168.1.1"

[eth0.ipv6]
method = "auto"                 # SLAAC

[lo]
type = "loopback"
[lo.ipv4]
address = "127.0.0.1"
netmask = "255.0.0.0"
```

---

## 12) /proc and /sys Network Entries

### 12.1 /proc/net

```
/proc/net/
├── tcp                 # TCP connection list
├── udp                 # UDP socket list
├── unix                # Unix domain sockets
├── raw                 # Raw sockets
├── route               # Routing table
├── arp                 # ARP cache
├── dev                 # Interface statistics
├── if_inet6            # IPv6 addresses
├── sockstat            # Socket statistics
└── netstat             # Network statistics
```

### 12.2 /sys/class/net

```
/sys/class/net/
├── eth0/
│   ├── address         # MAC address
│   ├── mtu             # MTU
│   ├── operstate       # up/down
│   ├── speed           # Link speed
│   ├── duplex          # Full/half duplex
│   └── statistics/
│       ├── rx_bytes
│       ├── tx_bytes
│       ├── rx_packets
│       └── tx_packets
└── lo/
    └── ...
```

---

## 13) Exit Criteria

### Phase 12a: Basic Networking
- [ ] Loopback interface works
- [ ] virtio-net driver works
- [ ] ARP resolution works
- [ ] IPv4 ping works
- [ ] UDP sockets work

### Phase 12b: TCP
- [ ] TCP connection establishment
- [ ] TCP data transfer
- [ ] TCP connection teardown
- [ ] Multiple concurrent connections

### Phase 12c: Full Stack
- [ ] DHCP client works
- [ ] DNS resolution works
- [ ] IPv6 basic support
- [ ] Socket options work
- [ ] Works on both arches

### Phase 12d: Advanced
- [ ] Firewall rules work
- [ ] NAT/masquerade works
- [ ] Network namespaces work
- [ ] Routing between interfaces

---

*End of EFFLUX Network Specification*
