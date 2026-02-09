//! TCP/IP Stack Implementation
//!
//! A lightweight TCP/IP network stack for Oxide OS.

#![no_std]
#![allow(unused)]

extern crate alloc;

pub mod arp;
pub mod checksum;
pub mod conntrack;
pub mod dhcp_client;
pub mod ethernet;
pub mod filter;
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
    IpAddr, Ipv4Addr, MacAddress, NetError, NetResult, NetworkDevice, NetworkInterface, SocketAddr,
};

pub use arp::ArpCache;
pub use conntrack::{
    ConnEntry, ConnTrackTable, ConnTuple, TcpFlags, TcpState, connection_count, gc as conntrack_gc,
    lookup_state, remove_connection, tick as conntrack_tick, track_icmp, track_packet,
};
pub use dhcp_client::{acquire_lease, format_lease_file, send_dhcp_packet};
pub use ethernet::EtherType;
pub use filter::{
    ConnState, FilterChain, FilterRule, FilterVerdict, IpMatch, PacketInfo, PortMatch, add_rule,
    delete_rule, filter_input, filter_output, flush_all, flush_chain, get_policy, rule_count,
    set_policy, with_rules,
};
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
        if port == Self::EPHEMERAL_PORT_END {
            self.next_ephemeral_port
                .store(Self::EPHEMERAL_PORT_START, Ordering::SeqCst);
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
            self.arp_cache
                .insert(arp_packet.sender_ip, arp_packet.sender_mac);

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
            let frame = ethernet::EthernetFrame::new(target_mac, our_mac, EtherType::Arp, &packet);

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

        // Build packet info for filtering
        let mut pkt_info =
            filter::PacketInfo::new(ip_header.src, ip_header.dst, ip_header.protocol);

        // Extract port information and track connection state
        let conn_state = match ip_header.protocol {
            IpProtocol::Tcp if ip_payload.len() >= 14 => {
                let src_port = u16::from_be_bytes([ip_payload[0], ip_payload[1]]);
                let dst_port = u16::from_be_bytes([ip_payload[2], ip_payload[3]]);
                pkt_info = pkt_info.with_ports(src_port, dst_port);

                // Extract TCP flags for connection tracking
                let flags = conntrack::TcpFlags::from_byte(ip_payload[13]);
                conntrack::track_packet(
                    ip_header.protocol,
                    ip_header.src,
                    src_port,
                    ip_header.dst,
                    dst_port,
                    Some(flags),
                    payload.len(),
                )
            }
            IpProtocol::Udp if ip_payload.len() >= 4 => {
                let src_port = u16::from_be_bytes([ip_payload[0], ip_payload[1]]);
                let dst_port = u16::from_be_bytes([ip_payload[2], ip_payload[3]]);
                pkt_info = pkt_info.with_ports(src_port, dst_port);

                conntrack::track_packet(
                    ip_header.protocol,
                    ip_header.src,
                    src_port,
                    ip_header.dst,
                    dst_port,
                    None,
                    payload.len(),
                )
            }
            IpProtocol::Icmp if ip_payload.len() >= 2 => {
                let icmp_type = ip_payload[0];
                let icmp_code = ip_payload[1];
                pkt_info = pkt_info.with_icmp_type(icmp_type);

                conntrack::track_icmp(
                    ip_header.src,
                    ip_header.dst,
                    icmp_type,
                    icmp_code,
                    payload.len(),
                )
            }
            _ => filter::ConnState::New,
        };

        // Set connection state for filtering
        pkt_info = pkt_info.with_state(conn_state);

        // Apply INPUT chain filter
        match filter::filter_input(&pkt_info) {
            filter::FilterVerdict::Accept => {}
            filter::FilterVerdict::Drop => return Ok(()), // Silently drop
            filter::FilterVerdict::Reject => {
                // Send ICMP unreachable for UDP/others, TCP RST for TCP
                match ip_header.protocol {
                    IpProtocol::Tcp => {
                        // Parse TCP header to send proper RST
                        if let Ok(tcp_header) = tcp::TcpHeader::parse(ip_payload) {
                            let header_len = tcp_header.data_offset() * 4;
                            let payload_len = ip_payload.len().saturating_sub(header_len);
                            self.send_tcp_rst(
                                &tcp_header,
                                ip_header.src,
                                ip_header.dst,
                                payload_len,
                            )?;
                        }
                    }
                    _ => {
                        // Destination unreachable (port unreachable)
                        let icmp = icmp::IcmpPacket::new_dest_unreachable(
                            icmp::dest_unreachable::PORT_UNREACHABLE,
                            payload,
                        );
                        let _ = self.send_ipv4_packet(
                            ip_header.src,
                            IpProtocol::Icmp,
                            &icmp.to_bytes(),
                        );
                    }
                }
                return Ok(());
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
                self.send_icmp_reply(
                    src_ip,
                    icmp_packet.identifier,
                    icmp_packet.sequence,
                    &icmp_packet.data,
                )?;
            } else if icmp_packet.icmp_type == icmp::ICMP_ECHO_REPLY {
                // —ShadePacket: Buffer the reply for raw ICMP sockets
                // This allows userspace ping to receive responses
                buffer_icmp_reply(src_ip, payload);
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

    /// Send TCP RST in response to unexpected packets
    fn send_tcp_rst(
        &self,
        header: &tcp::TcpHeader,
        src_ip: Ipv4Addr,
        dst_ip: Ipv4Addr,
        payload_len: usize,
    ) -> NetResult<()> {
        // Compute acknowledgment for received data + SYN/FIN
        let mut ack = header.seq_num.wrapping_add(payload_len as u32);
        if header.flags & tcp::tcp_flags::SYN != 0 {
            ack = ack.wrapping_add(1);
        }
        if header.flags & tcp::tcp_flags::FIN != 0 {
            ack = ack.wrapping_add(1);
        }

        // Build minimal RST+ACK segment with seq=0 and zero window
        let segment = tcp::TcpSegment::new(
            header.dst_port,
            header.src_port,
            0,
            ack,
            tcp::tcp_flags::RST | tcp::tcp_flags::ACK,
            0,
            &[],
        );
        let bytes = segment.to_bytes(dst_ip, src_ip);
        self.send_ipv4_packet(src_ip, IpProtocol::Tcp, &bytes)
    }

    /// Process TCP segment
    fn process_tcp(&self, src_ip: Ipv4Addr, dst_ip: Ipv4Addr, payload: &[u8]) -> NetResult<()> {
        // SableWire: Parse complete segment including options
        let segment = tcp::TcpSegment::parse(payload)?;

        // Find connection
        let connections = self.tcp_connections.lock();
        for conn in connections.values() {
            if conn.matches(
                src_ip,
                segment.header.src_port,
                dst_ip,
                segment.header.dst_port,
            ) {
                conn.process_segment(&segment)?;
                return Ok(());
            }
        }

        // Check listening sockets for SYN
        if segment.header.flags & tcp::tcp_flags::SYN != 0 {
            // Handle new connection (would add to pending queue)
        }

        // No matching connection; send RST to refuse
        let header_len = segment.header.data_offset();
        let payload_len = payload.len().saturating_sub(header_len);
        self.send_tcp_rst(&segment.header, src_ip, dst_ip, payload_len)?;
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
    pub fn send_ipv4_packet(
        &self,
        dst_ip: Ipv4Addr,
        protocol: IpProtocol,
        payload: &[u8],
    ) -> NetResult<()> {
        // For loopback addresses, use 127.0.0.1 as source
        let src_ip = if dst_ip.is_loopback() {
            Ipv4Addr::LOCALHOST
        } else {
            self.interface.ipv4_addr().ok_or(NetError::NotConnected)?
        };

        // Build packet info for filtering
        let mut pkt_info = filter::PacketInfo::new(src_ip, dst_ip, protocol);

        // Extract port information for TCP/UDP
        match protocol {
            IpProtocol::Tcp if payload.len() >= 4 => {
                let src_port = u16::from_be_bytes([payload[0], payload[1]]);
                let dst_port = u16::from_be_bytes([payload[2], payload[3]]);
                pkt_info = pkt_info.with_ports(src_port, dst_port);
            }
            IpProtocol::Udp if payload.len() >= 4 => {
                let src_port = u16::from_be_bytes([payload[0], payload[1]]);
                let dst_port = u16::from_be_bytes([payload[2], payload[3]]);
                pkt_info = pkt_info.with_ports(src_port, dst_port);
            }
            IpProtocol::Icmp if !payload.is_empty() => {
                pkt_info = pkt_info.with_icmp_type(payload[0]);
            }
            _ => {}
        }

        // Apply OUTPUT chain filter
        match filter::filter_output(&pkt_info) {
            filter::FilterVerdict::Accept => {}
            filter::FilterVerdict::Drop => return Ok(()), // Silently drop
            filter::FilterVerdict::Reject => return Err(NetError::ConnectionRefused),
        }

        // Build IP packet
        let ip_packet = ip::Ipv4Packet::new(src_ip, dst_ip, protocol, payload);
        let ip_bytes = ip_packet.to_bytes();

        // Handle loopback: feed packet directly back to receive path
        // This includes 127.0.0.0/8 addresses and our own interface IP
        if dst_ip.is_loopback() || Some(dst_ip) == self.interface.ipv4_addr() {
            return self.process_ipv4(&ip_bytes);
        }

        // Resolve MAC address
        let dst_mac = if dst_ip.is_broadcast() || self.interface.same_network(dst_ip) {
            // Direct delivery
            self.resolve_mac(dst_ip)?
        } else {
            // Use gateway
            let gateway = self
                .interface
                .ipv4_gateway()
                .ok_or(NetError::NetworkUnreachable)?;
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
    /// —ShadePacket: Real network stacks wait for ARP resolution, so do we now
    fn resolve_mac(&self, ip: Ipv4Addr) -> NetResult<MacAddress> {
        // Check cache first
        if let Some(mac) = self.arp_cache.lookup(ip) {
            return Ok(mac);
        }

        // Broadcast address
        if ip.is_broadcast() {
            return Ok(MacAddress::BROADCAST);
        }

        // —ShadePacket: ARP resolution with retry loop
        // Send ARP request and poll for reply, like a real network stack
        // Using iteration count instead of time to avoid dependencies
        const ARP_MAX_ATTEMPTS: u32 = 3;
        const POLLS_PER_ATTEMPT: u32 = 5000; // Poll ~5000 times per attempt
        const SPINS_PER_POLL: u32 = 1000; // Spin loop iterations between polls

        for attempt in 0..ARP_MAX_ATTEMPTS {
            // Send ARP request
            let _ = self.send_arp_request(ip);

            // Poll for ARP reply
            for _ in 0..POLLS_PER_ATTEMPT {
                // Check if already resolved
                if let Some(mac) = self.arp_cache.lookup(ip) {
                    return Ok(mac);
                }

                // Poll for incoming packets (including ARP replies)
                let mut buf = [0u8; 1536];
                if let Ok(Some(len)) = self.interface.device.receive(&mut buf) {
                    let _ = self.process_packet(&buf[..len]);

                    // Check again after processing
                    if let Some(mac) = self.arp_cache.lookup(ip) {
                        return Ok(mac);
                    }
                }

                // Brief spin to avoid hammering the device
                for _ in 0..SPINS_PER_POLL {
                    core::hint::spin_loop();
                }
            }
        }

        // —ShadePacket: All attempts exhausted, host truly unreachable
        Err(NetError::HostUnreachable)
    }

    /// Process TCP timers (retransmission, keepalive, etc.)
    fn process_tcp_timers(&self) -> NetResult<()> {
        let connections = self.tcp_connections.lock();
        for conn in connections.values() {
            conn.process_timers()?;

            // SableWire: Transmit any queued segments
            let segments = conn.dequeue_segments();
            for segment_bytes in segments {
                self.send_ipv4_packet(conn.remote_ip, IpProtocol::Tcp, &segment_bytes)?;
            }
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

        // NeonRoot: Immediately transmit the SYN
        let segments = conn.dequeue_segments();
        for segment_bytes in segments {
            self.send_ipv4_packet(dst_ip, IpProtocol::Tcp, &segment_bytes)?;
        }

        Ok(conn)
    }

    /// Create UDP socket
    pub fn udp_bind(&self, port: u16) -> NetResult<Arc<UdpSocket>> {
        let src_ip = self.interface.ipv4_addr().ok_or(NetError::NotConnected)?;

        let socket = Arc::new(UdpSocket::new(src_ip, port));
        self.udp_sockets.lock().insert(port, socket.clone());

        Ok(socket)
    }

    /// Get TCP connection by ID
    /// —ShadePacket: Used by syscall layer to access connection for send/recv
    pub fn get_tcp_connection(&self, conn_id: u32) -> Option<Arc<TcpConnection>> {
        self.tcp_connections.lock().get(&conn_id).cloned()
    }

    /// Transmit pending segments for a TCP connection
    /// —ShadePacket: Called after send() to actually transmit the queued segments
    pub fn transmit_tcp_segments(&self, conn: &TcpConnection) -> NetResult<()> {
        let segments = conn.dequeue_segments();
        for segment_bytes in segments {
            self.send_ipv4_packet(conn.remote_ip, IpProtocol::Tcp, &segment_bytes)?;
        }
        Ok(())
    }
}

/// Global TCP/IP stack instance
static TCPIP_STACK: Mutex<Option<Arc<TcpIpStack>>> = Mutex::new(None);

// ============================================================================
// ICMP Reply Buffer for Raw Sockets
// ============================================================================

/// Buffered ICMP packet (source IP + raw ICMP data)
pub struct IcmpReply {
    pub src_ip: Ipv4Addr,
    pub data: Vec<u8>,
}

/// Buffer for incoming ICMP replies (for raw socket receive)
/// —ShadePacket: The TCP/IP stack buffers ICMP echo replies here so that
/// raw ICMP sockets can receive them via sys_recv()
static ICMP_REPLY_BUFFER: Mutex<Vec<IcmpReply>> = Mutex::new(Vec::new());

/// Maximum buffered ICMP replies
const MAX_ICMP_REPLIES: usize = 64;

/// Get a pending ICMP reply (for raw socket receive)
///
/// Returns the oldest buffered ICMP reply, or None if buffer is empty
pub fn get_icmp_reply() -> Option<IcmpReply> {
    let mut buf = ICMP_REPLY_BUFFER.lock();
    if buf.is_empty() {
        None
    } else {
        Some(buf.remove(0))
    }
}

/// Check if there are pending ICMP replies
pub fn has_icmp_replies() -> bool {
    !ICMP_REPLY_BUFFER.lock().is_empty()
}

/// Buffer an incoming ICMP reply
fn buffer_icmp_reply(src_ip: Ipv4Addr, data: &[u8]) {
    let mut buf = ICMP_REPLY_BUFFER.lock();
    if buf.len() < MAX_ICMP_REPLIES {
        buf.push(IcmpReply {
            src_ip,
            data: data.to_vec(),
        });
    }
    // —ShadePacket: Drop oldest if full (could also drop new, but FIFO is simpler)
}

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
    // ShadePacket: Use try_lock to avoid deadlock if already polling
    // This can happen if poll is called recursively or from timer interrupt
    if let Some(guard) = TCPIP_STACK.try_lock() {
        if let Some(ref stack) = *guard {
            stack.poll()
        } else {
            Ok(())
        }
    } else {
        // Lock held - skip this poll cycle to avoid deadlock
        Ok(())
    }
}
