//! DHCP Client Integration for TCP/IP Stack
//!
//! Provides DHCP address acquisition at the kernel level.

use alloc::sync::Arc;
use alloc::vec::Vec;

use dhcp::{
    self, DHCP_CLIENT_PORT, DHCP_SERVER_PORT, DhcpClient, DhcpLease, DhcpPacket, DhcpState,
};
use net::{Ipv4Addr, MacAddress, NetError, NetResult, NetworkInterface};

use crate::checksum;
use crate::ethernet::{EtherType, EthernetFrame};
use crate::ip::{IpProtocol, Ipv4Packet, PseudoHeader};
use crate::udp::UDP_HEADER_LEN;

/// Maximum number of DHCP retries
const DHCP_MAX_RETRIES: u32 = 2;

/// DHCP timeout per attempt (in poll iterations)
/// With SPIN_DELAY spin loops per iteration, this gives ~1 second per phase
const DHCP_TIMEOUT_ITERATIONS: u32 = 5000;

/// Spin loops between receive attempts (~0.2ms worth of delay on modern CPUs)
const SPIN_DELAY: u32 = 20000;

/// Send a raw DHCP packet (broadcast UDP without requiring IP address)
///
/// DHCP requires sending packets from 0.0.0.0:68 to 255.255.255.255:67
/// before the interface has an IP address configured.
pub fn send_dhcp_packet(interface: &NetworkInterface, dhcp_data: &[u8]) -> NetResult<()> {
    // Build UDP header
    let src_port = DHCP_CLIENT_PORT;
    let dst_port = DHCP_SERVER_PORT;
    let udp_len = (UDP_HEADER_LEN + dhcp_data.len()) as u16;

    let mut udp_packet = Vec::with_capacity(UDP_HEADER_LEN + dhcp_data.len());
    udp_packet.extend_from_slice(&src_port.to_be_bytes());
    udp_packet.extend_from_slice(&dst_port.to_be_bytes());
    udp_packet.extend_from_slice(&udp_len.to_be_bytes());
    udp_packet.extend_from_slice(&[0, 0]); // Checksum placeholder
    udp_packet.extend_from_slice(dhcp_data);

    // Compute UDP checksum with pseudo-header
    let src_ip = Ipv4Addr::ANY; // 0.0.0.0
    let dst_ip = Ipv4Addr::BROADCAST; // 255.255.255.255
    let pseudo = PseudoHeader::new(src_ip, dst_ip, IpProtocol::Udp, udp_packet.len() as u16);
    let cksum = checksum::checksum_with_pseudo(&pseudo.to_bytes(), &udp_packet);
    let cksum = if cksum == 0 { 0xFFFF } else { cksum };
    udp_packet[6] = (cksum >> 8) as u8;
    udp_packet[7] = cksum as u8;

    // Build IP packet
    let ip_packet = Ipv4Packet::new(src_ip, dst_ip, IpProtocol::Udp, &udp_packet);
    let ip_bytes = ip_packet.to_bytes();

    // Build Ethernet frame (broadcast)
    let src_mac = interface.mac_address();
    let frame = EthernetFrame::new(MacAddress::BROADCAST, src_mac, EtherType::Ipv4, &ip_bytes);

    interface.device.transmit(&frame.to_bytes())
}

/// Receive and parse a DHCP response packet from raw Ethernet frame
///
/// Returns the DHCP packet if this is a valid DHCP response for us.
fn receive_dhcp_packet(
    interface: &NetworkInterface,
    expected_xid: u32,
) -> NetResult<Option<DhcpPacket>> {
    let mut buf = [0u8; 1536];

    // Try to receive a packet
    let len = match interface.device.receive(&mut buf)? {
        Some(n) => n,
        None => return Ok(None),
    };

    if len < 14 + 20 + 8 + 240 {
        // Too short for Ethernet + IP + UDP + DHCP minimum
        return Ok(None);
    }

    // Parse Ethernet header
    let eth_type = u16::from_be_bytes([buf[12], buf[13]]);
    if eth_type != 0x0800 {
        // Not IPv4
        return Ok(None);
    }

    // Parse IP header
    let ip_start = 14;
    let ip_header_len = ((buf[ip_start] & 0x0F) * 4) as usize;
    if ip_header_len < 20 || ip_start + ip_header_len >= len {
        return Ok(None);
    }

    let ip_proto = buf[ip_start + 9];
    if ip_proto != 17 {
        // Not UDP
        return Ok(None);
    }

    // Parse UDP header
    let udp_start = ip_start + ip_header_len;
    if udp_start + 8 >= len {
        return Ok(None);
    }

    let src_port = u16::from_be_bytes([buf[udp_start], buf[udp_start + 1]]);
    let dst_port = u16::from_be_bytes([buf[udp_start + 2], buf[udp_start + 3]]);

    // Check if this is a DHCP response (from port 67 to port 68)
    if src_port != DHCP_SERVER_PORT || dst_port != DHCP_CLIENT_PORT {
        return Ok(None);
    }

    // Parse DHCP payload
    let dhcp_start = udp_start + 8;
    let dhcp_data = &buf[dhcp_start..len];

    if let Some(dhcp_packet) = DhcpPacket::parse(dhcp_data) {
        // Verify transaction ID
        if dhcp_packet.xid == expected_xid {
            return Ok(Some(dhcp_packet));
        }
    }

    Ok(None)
}

/// Acquire an IP address via DHCP
///
/// This performs the full DHCP handshake:
/// 1. Send DISCOVER
/// 2. Receive OFFER
/// 3. Send REQUEST
/// 4. Receive ACK
///
/// On success, configures the interface with the lease information
/// and returns the lease details.
pub fn acquire_lease(interface: Arc<NetworkInterface>) -> NetResult<DhcpLease> {
    let client = DhcpClient::new(interface.clone());

    for attempt in 0..DHCP_MAX_RETRIES {
        // Send DISCOVER
        let discover_data = client.discover()?;
        send_dhcp_packet(&interface, &discover_data)?;

        // Get the transaction ID from the DISCOVER packet we sent
        let xid = if let Some(packet) = DhcpPacket::parse(&discover_data) {
            packet.xid
        } else {
            continue;
        };

        // Wait for OFFER
        let mut offer_received = false;
        for i in 0..DHCP_TIMEOUT_ITERATIONS {
            // Poll for any received packets
            if let Some(packet) = receive_dhcp_packet(&interface, xid)? {
                if let Some(request_data) = client.process_offer(&packet)? {
                    // Got OFFER, send REQUEST
                    send_dhcp_packet(&interface, &request_data)?;
                    offer_received = true;
                    break;
                }
            }

            // Delay between polls
            for _ in 0..SPIN_DELAY {
                core::hint::spin_loop();
            }

            // Retransmit DISCOVER periodically (every ~1 second worth of iterations)
            if i > 0 && i % 10000 == 0 {
                send_dhcp_packet(&interface, &discover_data)?;
            }
        }

        if !offer_received {
            continue; // Retry DISCOVER with new transaction ID
        }

        // Wait for ACK
        for i in 0..DHCP_TIMEOUT_ITERATIONS {
            if let Some(packet) = receive_dhcp_packet(&interface, xid)? {
                // Check for NAK
                client.process_nak(&packet)?;
                if client.state() == DhcpState::Init {
                    break; // Got NAK, retry
                }

                // Check for ACK
                if let Some(lease) = client.process_ack(&packet)? {
                    // Got lease! Configure interface
                    interface.set_ipv4_addr(lease.ip_addr, lease.subnet_mask)?;

                    if let Some(gateway) = lease.gateway {
                        interface.set_ipv4_gateway(gateway)?;
                    }

                    return Ok(lease);
                }
            }

            // Delay between polls
            for _ in 0..SPIN_DELAY {
                core::hint::spin_loop();
            }

            // Retransmit REQUEST periodically
            if i > 0 && i % 10000 == 0 {
                // Resend the request (need to rebuild it)
                if let Some(ip) = client.lease().map(|l| l.ip_addr) {
                    // We're in Requesting state, resend original request
                    let request_data = client.discover()?;
                    send_dhcp_packet(&interface, &request_data).ok();
                }
            }
        }

        // Timeout waiting for ACK, retry from DISCOVER
    }

    Err(NetError::TimedOut)
}

/// Write DHCP lease to file
///
/// Creates a lease file at /var/lib/dhcp/<interface>.lease
/// Format:
/// ```text
/// ip=192.168.1.100
/// netmask=255.255.255.0
/// gateway=192.168.1.1
/// dns=8.8.8.8
/// dns=8.8.4.4
/// server=192.168.1.1
/// lease_time=86400
/// ```
pub fn format_lease_file(lease: &DhcpLease) -> alloc::string::String {
    use alloc::format;
    use alloc::string::String;

    let mut output = String::new();

    output.push_str(&format!("ip={}\n", lease.ip_addr));
    output.push_str(&format!("netmask={}\n", lease.subnet_mask));

    if let Some(gw) = lease.gateway {
        output.push_str(&format!("gateway={}\n", gw));
    }

    for dns in &lease.dns_servers {
        output.push_str(&format!("dns={}\n", dns));
    }

    output.push_str(&format!("server={}\n", lease.server));
    output.push_str(&format!("lease_time={}\n", lease.lease_time));
    output.push_str(&format!("renewal_time={}\n", lease.renewal_time));
    output.push_str(&format!("rebinding_time={}\n", lease.rebinding_time));

    output
}
