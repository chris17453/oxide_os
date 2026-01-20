//! TCP/IP Stack Implementation
//!
//! A lightweight TCP/IP network stack for Oxide OS.

#![no_std]

extern crate alloc;

pub mod arp;
pub mod checksum;
pub mod ethernet;
pub mod icmp;
pub mod ip;
pub mod tcp;
pub mod udp;

use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU16, AtomicU32, Ordering};
use spin::Mutex;

use net::{
    IpAddr, Ipv4Addr, MacAddress, NetError, NetResult, NetworkDevice, NetworkInterface,
    Socket, SocketAddr, SocketDomain, SocketProtocol, SocketState, SocketType,
};

pub use arp::ArpCache;
pub use ethernet::EtherType;
pub use ip::IpProtocol;
pub use tcp::TcpConnection;
pub use udp::UdpSocket;

/// TCP/IP Stack
pub struct TcpIpStack {
    /// Network interface
    interface: Arc<NetworkInterface>,
    /// ARP cache
    arp_cache: ArpCache,
    /// TCP connections
    tcp_connections: Mutex<BTreeMap<u32, Arc<TcpConnection>>>,
    /// UDP sockets
    udp_sockets: Mutex<BTreeMap<u16, Arc<UdpSocket>>>,
    /// Next connection ID
    next_conn_id: AtomicU32,
    /// Next ephemeral port
    next_ephemeral_port: AtomicU16,
    /// Receive buffer
    rx_buffer: Mutex<Vec<u8>>,
}

impl TcpIpStack {
    /// Ephemeral port range start
    const EPHEMERAL_PORT_START: u16 = 49152;
    /// Ephemeral port range end
    const EPHEMERAL_PORT_END: u16 = 65535;

    /// Create a new TCP/IP stack
    pub fn new(interface: Arc<NetworkInterface>) -> Self {
        TcpIpStack {
            interface,
            arp_cache: ArpCache::new(),
            tcp_connections: Mutex::new(BTreeMap::new()),
            udp_sockets: Mutex::new(BTreeMap::new()),
            next_conn_id: AtomicU32::new(1),
            next_ephemeral_port: AtomicU16::new(Self::EPHEMERAL_PORT_START),
            rx_buffer: Mutex::new(Vec::with_capacity(65536)),
        }
    }

    /// Get interface
    pub fn interface(&self) -> &Arc<NetworkInterface> {
        &self.interface
    }

    /// Allocate an ephemeral port
    pub fn alloc_ephemeral_port(&self) -> u16 {
        let port = self.next_ephemeral_port.fetch_add(1, Ordering::SeqCst);
        if port >= Self::EPHEMERAL_PORT_END {
            self.next_ephemeral_port.store(Self::EPHEMERAL_PORT_START, Ordering::SeqCst);
        }
        port
    }

    /// Process incoming packets
    pub fn poll(&self) -> NetResult<()> {
        let mut buf = [0u8; 1536];

        // Receive from device
        if let Some(len) = self.interface.device.receive(&mut buf)? {
            self.process_packet(&buf[..len])?;
        }

        // Process TCP timers
        self.process_tcp_timers()?;

        Ok(())
    }

    /// Process a received packet
    fn process_packet(&self, packet: &[u8]) -> NetResult<()> {
        if packet.len() < ethernet::ETHERNET_HEADER_LEN {
            return Ok(());
        }

        let eth_header = ethernet::EthernetHeader::parse(packet)?;
        let payload = &packet[ethernet::ETHERNET_HEADER_LEN..];

        match eth_header.ethertype {
            EtherType::Arp => {
                self.process_arp(payload)?;
            }
            EtherType::Ipv4 => {
                self.process_ipv4(payload)?;
            }
            EtherType::Ipv6 => {
                // IPv6 not yet implemented
            }
            _ => {}
        }

        Ok(())
    }

    /// Process ARP packet
    fn process_arp(&self, payload: &[u8]) -> NetResult<()> {
        if let Some(arp_packet) = arp::ArpPacket::parse(payload) {
            // Update cache with sender info
            self.arp_cache.insert(arp_packet.sender_ip, arp_packet.sender_mac);

            // Handle ARP request
            if arp_packet.operation == arp::ARP_REQUEST {
                if let Some(our_ip) = self.interface.ipv4_addr() {
                    if arp_packet.target_ip == our_ip {
                        // Send ARP reply
                        self.send_arp_reply(arp_packet.sender_ip, arp_packet.sender_mac)?;
                    }
                }
            }
        }
        Ok(())
    }

    /// Send ARP reply
    fn send_arp_reply(&self, target_ip: Ipv4Addr, target_mac: MacAddress) -> NetResult<()> {
        if let Some(our_ip) = self.interface.ipv4_addr() {
            let our_mac = self.interface.mac_address();
            let reply = arp::ArpPacket::new_reply(our_mac, our_ip, target_mac, target_ip);
            let packet = reply.to_bytes();

            // Wrap in Ethernet frame
            let frame = ethernet::EthernetFrame::new(
                target_mac,
                our_mac,
                EtherType::Arp,
                &packet,
            );

            self.interface.device.transmit(&frame.to_bytes())?;
        }
        Ok(())
    }

    /// Send ARP request
    pub fn send_arp_request(&self, target_ip: Ipv4Addr) -> NetResult<()> {
        if let Some(our_ip) = self.interface.ipv4_addr() {
            let our_mac = self.interface.mac_address();
            let request = arp::ArpPacket::new_request(our_mac, our_ip, target_ip);
            let packet = request.to_bytes();

            // Wrap in Ethernet frame (broadcast)
            let frame = ethernet::EthernetFrame::new(
                MacAddress::BROADCAST,
                our_mac,
                EtherType::Arp,
                &packet,
            );

            self.interface.device.transmit(&frame.to_bytes())?;
        }
        Ok(())
    }

    /// Process IPv4 packet
    fn process_ipv4(&self, payload: &[u8]) -> NetResult<()> {
        let ip_header = ip::Ipv4Header::parse(payload)?;
        let ip_payload = &payload[ip_header.header_len()..];

        // Verify destination is for us
        if let Some(our_ip) = self.interface.ipv4_addr() {
            if ip_header.dst != our_ip && !ip_header.dst.is_broadcast() {
                return Ok(()); // Not for us
            }
        }

        match ip_header.protocol {
            IpProtocol::Icmp => {
                self.process_icmp(ip_header.src, ip_payload)?;
            }
            IpProtocol::Tcp => {
                self.process_tcp(ip_header.src, ip_header.dst, ip_payload)?;
            }
            IpProtocol::Udp => {
                self.process_udp(ip_header.src, ip_header.dst, ip_payload)?;
            }
            _ => {}
        }

        Ok(())
    }

    /// Process ICMP packet
    fn process_icmp(&self, src_ip: Ipv4Addr, payload: &[u8]) -> NetResult<()> {
        if let Some(icmp_packet) = icmp::IcmpPacket::parse(payload) {
            if icmp_packet.icmp_type == icmp::ICMP_ECHO_REQUEST {
                // Send echo reply
                self.send_icmp_reply(src_ip, icmp_packet.identifier, icmp_packet.sequence, &icmp_packet.data)?;
            }
        }
        Ok(())
    }

    /// Send ICMP echo reply
    fn send_icmp_reply(&self, dst_ip: Ipv4Addr, id: u16, seq: u16, data: &[u8]) -> NetResult<()> {
        let reply = icmp::IcmpPacket::new_echo_reply(id, seq, data);
        self.send_ipv4_packet(dst_ip, IpProtocol::Icmp, &reply.to_bytes())
    }

    /// Send ICMP echo request (ping)
    pub fn send_ping(&self, dst_ip: Ipv4Addr, id: u16, seq: u16, data: &[u8]) -> NetResult<()> {
        let request = icmp::IcmpPacket::new_echo_request(id, seq, data);
        self.send_ipv4_packet(dst_ip, IpProtocol::Icmp, &request.to_bytes())
    }

    /// Process TCP segment
    fn process_tcp(&self, src_ip: Ipv4Addr, dst_ip: Ipv4Addr, payload: &[u8]) -> NetResult<()> {
        let tcp_header = tcp::TcpHeader::parse(payload)?;
        let tcp_data = &payload[tcp_header.data_offset()..];

        // Find connection
        let connections = self.tcp_connections.lock();
        for conn in connections.values() {
            if conn.matches(src_ip, tcp_header.src_port, dst_ip, tcp_header.dst_port) {
                conn.process_segment(&tcp_header, tcp_data)?;
                return Ok(());
            }
        }

        // Check listening sockets for SYN
        if tcp_header.flags & tcp::TcpFlags::SYN != 0 {
            // Handle new connection (would add to pending queue)
        }

        Ok(())
    }

    /// Process UDP datagram
    fn process_udp(&self, src_ip: Ipv4Addr, _dst_ip: Ipv4Addr, payload: &[u8]) -> NetResult<()> {
        let udp_header = udp::UdpHeader::parse(payload)?;
        let udp_data = &payload[8..]; // UDP header is 8 bytes

        // Find socket
        let sockets = self.udp_sockets.lock();
        if let Some(socket) = sockets.get(&udp_header.dst_port) {
            socket.receive(src_ip, udp_header.src_port, udp_data)?;
        }

        Ok(())
    }

    /// Send IPv4 packet
    pub fn send_ipv4_packet(&self, dst_ip: Ipv4Addr, protocol: IpProtocol, payload: &[u8]) -> NetResult<()> {
        let src_ip = self.interface.ipv4_addr().ok_or(NetError::NotConnected)?;

        // Build IP packet
        let ip_packet = ip::Ipv4Packet::new(src_ip, dst_ip, protocol, payload);
        let ip_bytes = ip_packet.to_bytes();

        // Resolve MAC address
        let dst_mac = if dst_ip.is_broadcast() || self.interface.same_network(dst_ip) {
            // Direct delivery
            self.resolve_mac(dst_ip)?
        } else {
            // Use gateway
            let gateway = self.interface.ipv4_gateway().ok_or(NetError::NetworkUnreachable)?;
            self.resolve_mac(gateway)?
        };

        // Build Ethernet frame
        let frame = ethernet::EthernetFrame::new(
            dst_mac,
            self.interface.mac_address(),
            EtherType::Ipv4,
            &ip_bytes,
        );

        self.interface.device.transmit(&frame.to_bytes())
    }

    /// Resolve IP to MAC address
    fn resolve_mac(&self, ip: Ipv4Addr) -> NetResult<MacAddress> {
        // Check cache first
        if let Some(mac) = self.arp_cache.lookup(ip) {
            return Ok(mac);
        }

        // Broadcast address
        if ip.is_broadcast() {
            return Ok(MacAddress::BROADCAST);
        }

        // Send ARP request and wait
        self.send_arp_request(ip)?;

        // For now, return error (real implementation would wait/retry)
        Err(NetError::HostUnreachable)
    }

    /// Process TCP timers (retransmission, keepalive, etc.)
    fn process_tcp_timers(&self) -> NetResult<()> {
        let connections = self.tcp_connections.lock();
        for conn in connections.values() {
            conn.process_timers()?;
        }
        Ok(())
    }

    /// Create TCP connection
    pub fn tcp_connect(&self, dst_addr: SocketAddr) -> NetResult<Arc<TcpConnection>> {
        let dst_ip = match dst_addr.ip {
            IpAddr::V4(ip) => ip,
            IpAddr::V6(_) => return Err(NetError::AddressFamilyNotSupported),
        };

        let src_ip = self.interface.ipv4_addr().ok_or(NetError::NotConnected)?;
        let src_port = self.alloc_ephemeral_port();

        let conn_id = self.next_conn_id.fetch_add(1, Ordering::SeqCst);
        let conn = Arc::new(TcpConnection::new(
            conn_id,
            src_ip,
            src_port,
            dst_ip,
            dst_addr.port,
        ));

        // Add to connections
        self.tcp_connections.lock().insert(conn_id, conn.clone());

        // Initiate connection (send SYN)
        conn.connect()?;

        Ok(conn)
    }

    /// Create UDP socket
    pub fn udp_bind(&self, port: u16) -> NetResult<Arc<UdpSocket>> {
        let src_ip = self.interface.ipv4_addr().ok_or(NetError::NotConnected)?;

        let socket = Arc::new(UdpSocket::new(src_ip, port));
        self.udp_sockets.lock().insert(port, socket.clone());

        Ok(socket)
    }
}

/// Global TCP/IP stack instance
static TCPIP_STACK: Mutex<Option<Arc<TcpIpStack>>> = Mutex::new(None);

/// Initialize the TCP/IP stack
pub fn init(interface: Arc<NetworkInterface>) {
    let stack = Arc::new(TcpIpStack::new(interface));
    *TCPIP_STACK.lock() = Some(stack);
}

/// Get the TCP/IP stack
pub fn stack() -> Option<Arc<TcpIpStack>> {
    TCPIP_STACK.lock().clone()
}

/// Poll the TCP/IP stack
pub fn poll() -> NetResult<()> {
    if let Some(stack) = stack() {
        stack.poll()
    } else {
        Ok(())
    }
}
