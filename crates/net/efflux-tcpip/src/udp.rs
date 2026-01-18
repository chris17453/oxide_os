//! UDP Protocol Implementation

use alloc::collections::VecDeque;
use alloc::vec::Vec;
use spin::Mutex;

use efflux_net::{Ipv4Addr, NetError, NetResult};

use crate::checksum;
use crate::ip::{IpProtocol, PseudoHeader};

/// UDP header length
pub const UDP_HEADER_LEN: usize = 8;

/// UDP header
#[derive(Debug, Clone, Copy)]
pub struct UdpHeader {
    /// Source port
    pub src_port: u16,
    /// Destination port
    pub dst_port: u16,
    /// Length (header + data)
    pub length: u16,
    /// Checksum
    pub checksum: u16,
}

impl UdpHeader {
    /// Parse UDP header
    pub fn parse(data: &[u8]) -> NetResult<Self> {
        if data.len() < UDP_HEADER_LEN {
            return Err(NetError::InvalidArgument);
        }

        Ok(UdpHeader {
            src_port: u16::from_be_bytes([data[0], data[1]]),
            dst_port: u16::from_be_bytes([data[2], data[3]]),
            length: u16::from_be_bytes([data[4], data[5]]),
            checksum: u16::from_be_bytes([data[6], data[7]]),
        })
    }

    /// Serialize to bytes
    pub fn to_bytes(&self) -> [u8; UDP_HEADER_LEN] {
        let mut buf = [0u8; UDP_HEADER_LEN];
        buf[0..2].copy_from_slice(&self.src_port.to_be_bytes());
        buf[2..4].copy_from_slice(&self.dst_port.to_be_bytes());
        buf[4..6].copy_from_slice(&self.length.to_be_bytes());
        buf[6..8].copy_from_slice(&self.checksum.to_be_bytes());
        buf
    }
}

/// UDP datagram
pub struct UdpDatagram {
    /// Header
    pub header: UdpHeader,
    /// Data
    pub data: Vec<u8>,
}

impl UdpDatagram {
    /// Create a new UDP datagram
    pub fn new(src_port: u16, dst_port: u16, data: &[u8]) -> Self {
        let length = (UDP_HEADER_LEN + data.len()) as u16;

        UdpDatagram {
            header: UdpHeader {
                src_port,
                dst_port,
                length,
                checksum: 0,
            },
            data: data.to_vec(),
        }
    }

    /// Serialize with checksum
    pub fn to_bytes(&self, src_ip: Ipv4Addr, dst_ip: Ipv4Addr) -> Vec<u8> {
        let mut buf = Vec::with_capacity(UDP_HEADER_LEN + self.data.len());

        buf.extend_from_slice(&self.header.src_port.to_be_bytes());
        buf.extend_from_slice(&self.header.dst_port.to_be_bytes());
        buf.extend_from_slice(&self.header.length.to_be_bytes());
        // Checksum placeholder
        buf.extend_from_slice(&[0, 0]);
        buf.extend_from_slice(&self.data);

        // Compute checksum
        let pseudo = PseudoHeader::new(src_ip, dst_ip, IpProtocol::Udp, buf.len() as u16);
        let checksum = checksum::checksum_with_pseudo(&pseudo.to_bytes(), &buf);

        // UDP allows 0 checksum to mean "no checksum", so use 0xFFFF if computed is 0
        let checksum = if checksum == 0 { 0xFFFF } else { checksum };

        buf[6] = (checksum >> 8) as u8;
        buf[7] = checksum as u8;

        buf
    }
}

/// Received UDP packet info
#[derive(Debug, Clone)]
pub struct ReceivedPacket {
    /// Source IP
    pub src_ip: Ipv4Addr,
    /// Source port
    pub src_port: u16,
    /// Data
    pub data: Vec<u8>,
}

/// UDP socket
pub struct UdpSocket {
    /// Local IP
    pub local_ip: Ipv4Addr,
    /// Local port
    pub local_port: u16,
    /// Receive queue
    recv_queue: Mutex<VecDeque<ReceivedPacket>>,
    /// Maximum receive queue size
    max_queue_size: usize,
}

impl UdpSocket {
    /// Create a new UDP socket
    pub fn new(local_ip: Ipv4Addr, local_port: u16) -> Self {
        UdpSocket {
            local_ip,
            local_port,
            recv_queue: Mutex::new(VecDeque::new()),
            max_queue_size: 64,
        }
    }

    /// Receive a packet (called by stack)
    pub fn receive(&self, src_ip: Ipv4Addr, src_port: u16, data: &[u8]) -> NetResult<()> {
        let mut queue = self.recv_queue.lock();

        if queue.len() >= self.max_queue_size {
            // Drop packet if queue is full
            return Ok(());
        }

        queue.push_back(ReceivedPacket {
            src_ip,
            src_port,
            data: data.to_vec(),
        });

        Ok(())
    }

    /// Receive from the socket
    pub fn recvfrom(&self, buf: &mut [u8]) -> NetResult<(usize, Ipv4Addr, u16)> {
        let mut queue = self.recv_queue.lock();

        if let Some(packet) = queue.pop_front() {
            let len = buf.len().min(packet.data.len());
            buf[..len].copy_from_slice(&packet.data[..len]);
            Ok((len, packet.src_ip, packet.src_port))
        } else {
            Err(NetError::WouldBlock)
        }
    }

    /// Check if there's data available
    pub fn has_data(&self) -> bool {
        !self.recv_queue.lock().is_empty()
    }

    /// Get local address
    pub fn local_addr(&self) -> (Ipv4Addr, u16) {
        (self.local_ip, self.local_port)
    }
}
