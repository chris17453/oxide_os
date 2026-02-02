//! DHCP Client Implementation
//!
//! Implements DHCPv4 client (RFC 2131).

#![no_std]

extern crate alloc;

use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::Mutex;

use net::{Ipv4Addr, MacAddress, NetResult, NetworkInterface};

/// DHCP server port
pub const DHCP_SERVER_PORT: u16 = 67;

/// DHCP client port
pub const DHCP_CLIENT_PORT: u16 = 68;

/// DHCP magic cookie
pub const DHCP_MAGIC_COOKIE: u32 = 0x63825363;

/// DHCP message types
pub mod message_type {
    pub const DISCOVER: u8 = 1;
    pub const OFFER: u8 = 2;
    pub const REQUEST: u8 = 3;
    pub const DECLINE: u8 = 4;
    pub const ACK: u8 = 5;
    pub const NAK: u8 = 6;
    pub const RELEASE: u8 = 7;
    pub const INFORM: u8 = 8;
}

/// DHCP options
pub mod options {
    pub const PAD: u8 = 0;
    pub const SUBNET_MASK: u8 = 1;
    pub const ROUTER: u8 = 3;
    pub const DNS_SERVER: u8 = 6;
    pub const HOSTNAME: u8 = 12;
    pub const DOMAIN_NAME: u8 = 15;
    pub const BROADCAST_ADDR: u8 = 28;
    pub const REQUESTED_IP: u8 = 50;
    pub const LEASE_TIME: u8 = 51;
    pub const MESSAGE_TYPE: u8 = 53;
    pub const SERVER_ID: u8 = 54;
    pub const PARAM_REQUEST: u8 = 55;
    pub const RENEWAL_TIME: u8 = 58;
    pub const REBINDING_TIME: u8 = 59;
    pub const CLIENT_ID: u8 = 61;
    pub const END: u8 = 255;
}

/// DHCP op codes
pub mod op {
    pub const BOOTREQUEST: u8 = 1;
    pub const BOOTREPLY: u8 = 2;
}

/// Hardware type
pub const HTYPE_ETHERNET: u8 = 1;

/// DHCP packet
#[derive(Debug, Clone)]
pub struct DhcpPacket {
    /// Op code
    pub op: u8,
    /// Hardware type
    pub htype: u8,
    /// Hardware address length
    pub hlen: u8,
    /// Hops
    pub hops: u8,
    /// Transaction ID
    pub xid: u32,
    /// Seconds
    pub secs: u16,
    /// Flags
    pub flags: u16,
    /// Client IP address
    pub ciaddr: Ipv4Addr,
    /// Your IP address
    pub yiaddr: Ipv4Addr,
    /// Server IP address
    pub siaddr: Ipv4Addr,
    /// Gateway IP address
    pub giaddr: Ipv4Addr,
    /// Client hardware address
    pub chaddr: [u8; 16],
    /// Server hostname
    pub sname: [u8; 64],
    /// Boot filename
    pub file: [u8; 128],
    /// Options
    pub options: Vec<DhcpOption>,
}

/// DHCP option
#[derive(Debug, Clone)]
pub struct DhcpOption {
    /// Option code
    pub code: u8,
    /// Option data
    pub data: Vec<u8>,
}

impl DhcpPacket {
    /// Create a new DHCP Discover packet
    pub fn new_discover(mac: MacAddress, xid: u32) -> Self {
        let mut chaddr = [0u8; 16];
        chaddr[..6].copy_from_slice(&mac.0);

        let mut options = Vec::new();

        // Message type: DISCOVER
        options.push(DhcpOption {
            code: options::MESSAGE_TYPE,
            data: vec![message_type::DISCOVER],
        });

        // Parameter request list
        options.push(DhcpOption {
            code: options::PARAM_REQUEST,
            data: vec![
                options::SUBNET_MASK,
                options::ROUTER,
                options::DNS_SERVER,
                options::DOMAIN_NAME,
                options::BROADCAST_ADDR,
                options::LEASE_TIME,
            ],
        });

        // End option
        options.push(DhcpOption {
            code: options::END,
            data: Vec::new(),
        });

        DhcpPacket {
            op: op::BOOTREQUEST,
            htype: HTYPE_ETHERNET,
            hlen: 6,
            hops: 0,
            xid,
            secs: 0,
            flags: 0x8000, // Broadcast
            ciaddr: Ipv4Addr::ANY,
            yiaddr: Ipv4Addr::ANY,
            siaddr: Ipv4Addr::ANY,
            giaddr: Ipv4Addr::ANY,
            chaddr,
            sname: [0; 64],
            file: [0; 128],
            options,
        }
    }

    /// Create a new DHCP Request packet
    pub fn new_request(
        mac: MacAddress,
        xid: u32,
        requested_ip: Ipv4Addr,
        server_ip: Ipv4Addr,
    ) -> Self {
        let mut chaddr = [0u8; 16];
        chaddr[..6].copy_from_slice(&mac.0);

        let mut options = Vec::new();

        // Message type: REQUEST
        options.push(DhcpOption {
            code: options::MESSAGE_TYPE,
            data: vec![message_type::REQUEST],
        });

        // Requested IP
        options.push(DhcpOption {
            code: options::REQUESTED_IP,
            data: requested_ip.0.to_vec(),
        });

        // Server identifier
        options.push(DhcpOption {
            code: options::SERVER_ID,
            data: server_ip.0.to_vec(),
        });

        // Parameter request list
        options.push(DhcpOption {
            code: options::PARAM_REQUEST,
            data: vec![
                options::SUBNET_MASK,
                options::ROUTER,
                options::DNS_SERVER,
                options::DOMAIN_NAME,
                options::BROADCAST_ADDR,
                options::LEASE_TIME,
            ],
        });

        // End option
        options.push(DhcpOption {
            code: options::END,
            data: Vec::new(),
        });

        DhcpPacket {
            op: op::BOOTREQUEST,
            htype: HTYPE_ETHERNET,
            hlen: 6,
            hops: 0,
            xid,
            secs: 0,
            flags: 0x8000, // Broadcast
            ciaddr: Ipv4Addr::ANY,
            yiaddr: Ipv4Addr::ANY,
            siaddr: Ipv4Addr::ANY,
            giaddr: Ipv4Addr::ANY,
            chaddr,
            sname: [0; 64],
            file: [0; 128],
            options,
        }
    }

    /// Parse a DHCP packet
    pub fn parse(data: &[u8]) -> Option<Self> {
        if data.len() < 240 {
            // Minimum DHCP packet size
            return None;
        }

        let op = data[0];
        let htype = data[1];
        let hlen = data[2];
        let hops = data[3];
        let xid = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
        let secs = u16::from_be_bytes([data[8], data[9]]);
        let flags = u16::from_be_bytes([data[10], data[11]]);

        let ciaddr = Ipv4Addr([data[12], data[13], data[14], data[15]]);
        let yiaddr = Ipv4Addr([data[16], data[17], data[18], data[19]]);
        let siaddr = Ipv4Addr([data[20], data[21], data[22], data[23]]);
        let giaddr = Ipv4Addr([data[24], data[25], data[26], data[27]]);

        let mut chaddr = [0u8; 16];
        chaddr.copy_from_slice(&data[28..44]);

        let mut sname = [0u8; 64];
        sname.copy_from_slice(&data[44..108]);

        let mut file = [0u8; 128];
        file.copy_from_slice(&data[108..236]);

        // Check magic cookie
        let magic = u32::from_be_bytes([data[236], data[237], data[238], data[239]]);
        if magic != DHCP_MAGIC_COOKIE {
            return None;
        }

        // Parse options
        let mut options = Vec::new();
        let mut i = 240;
        while i < data.len() {
            let code = data[i];
            if code == options::END {
                break;
            }
            if code == options::PAD {
                i += 1;
                continue;
            }

            if i + 1 >= data.len() {
                break;
            }
            let len = data[i + 1] as usize;
            if i + 2 + len > data.len() {
                break;
            }

            options.push(DhcpOption {
                code,
                data: data[i + 2..i + 2 + len].to_vec(),
            });

            i += 2 + len;
        }

        Some(DhcpPacket {
            op,
            htype,
            hlen,
            hops,
            xid,
            secs,
            flags,
            ciaddr,
            yiaddr,
            siaddr,
            giaddr,
            chaddr,
            sname,
            file,
            options,
        })
    }

    /// Serialize to bytes
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(576);

        buf.push(self.op);
        buf.push(self.htype);
        buf.push(self.hlen);
        buf.push(self.hops);
        buf.extend_from_slice(&self.xid.to_be_bytes());
        buf.extend_from_slice(&self.secs.to_be_bytes());
        buf.extend_from_slice(&self.flags.to_be_bytes());
        buf.extend_from_slice(&self.ciaddr.0);
        buf.extend_from_slice(&self.yiaddr.0);
        buf.extend_from_slice(&self.siaddr.0);
        buf.extend_from_slice(&self.giaddr.0);
        buf.extend_from_slice(&self.chaddr);
        buf.extend_from_slice(&self.sname);
        buf.extend_from_slice(&self.file);

        // Magic cookie
        buf.extend_from_slice(&DHCP_MAGIC_COOKIE.to_be_bytes());

        // Options
        for opt in &self.options {
            buf.push(opt.code);
            if opt.code != options::END && opt.code != options::PAD {
                buf.push(opt.data.len() as u8);
                buf.extend_from_slice(&opt.data);
            }
        }

        // Pad to minimum size
        while buf.len() < 300 {
            buf.push(0);
        }

        buf
    }

    /// Get message type from options
    pub fn message_type(&self) -> Option<u8> {
        for opt in &self.options {
            if opt.code == options::MESSAGE_TYPE && !opt.data.is_empty() {
                return Some(opt.data[0]);
            }
        }
        None
    }

    /// Get option value
    pub fn get_option(&self, code: u8) -> Option<&[u8]> {
        for opt in &self.options {
            if opt.code == code {
                return Some(&opt.data);
            }
        }
        None
    }
}

/// DHCP lease information
#[derive(Debug, Clone)]
pub struct DhcpLease {
    /// Assigned IP address
    pub ip_addr: Ipv4Addr,
    /// Subnet mask
    pub subnet_mask: Ipv4Addr,
    /// Default gateway
    pub gateway: Option<Ipv4Addr>,
    /// DNS servers
    pub dns_servers: Vec<Ipv4Addr>,
    /// DHCP server
    pub server: Ipv4Addr,
    /// Lease time in seconds
    pub lease_time: u32,
    /// Renewal time in seconds
    pub renewal_time: u32,
    /// Rebinding time in seconds
    pub rebinding_time: u32,
}

/// DHCP client state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DhcpState {
    /// Initial state
    Init,
    /// Selecting (sent DISCOVER)
    Selecting,
    /// Requesting (sent REQUEST)
    Requesting,
    /// Bound (have lease)
    Bound,
    /// Renewing
    Renewing,
    /// Rebinding
    Rebinding,
}

/// DHCP client
pub struct DhcpClient {
    /// Interface
    interface: Arc<NetworkInterface>,
    /// State
    state: Mutex<DhcpState>,
    /// Transaction ID
    xid: AtomicU32,
    /// Current lease
    lease: Mutex<Option<DhcpLease>>,
    /// Offered IP (during negotiation)
    offered_ip: Mutex<Option<Ipv4Addr>>,
    /// Offered server
    offered_server: Mutex<Option<Ipv4Addr>>,
}

impl DhcpClient {
    /// Create a new DHCP client
    pub fn new(interface: Arc<NetworkInterface>) -> Self {
        DhcpClient {
            interface,
            state: Mutex::new(DhcpState::Init),
            xid: AtomicU32::new(0x12345678),
            lease: Mutex::new(None),
            offered_ip: Mutex::new(None),
            offered_server: Mutex::new(None),
        }
    }

    /// Get current state
    pub fn state(&self) -> DhcpState {
        *self.state.lock()
    }

    /// Get current lease
    pub fn lease(&self) -> Option<DhcpLease> {
        self.lease.lock().clone()
    }

    /// Start DHCP discovery
    pub fn discover(&self) -> NetResult<Vec<u8>> {
        let xid = self.xid.fetch_add(1, Ordering::SeqCst);
        let mac = self.interface.mac_address();

        let packet = DhcpPacket::new_discover(mac, xid);
        *self.state.lock() = DhcpState::Selecting;

        Ok(packet.to_bytes())
    }

    /// Process DHCP offer
    pub fn process_offer(&self, packet: &DhcpPacket) -> NetResult<Option<Vec<u8>>> {
        if *self.state.lock() != DhcpState::Selecting {
            return Ok(None);
        }

        if packet.message_type() != Some(message_type::OFFER) {
            return Ok(None);
        }

        // Save offered address
        *self.offered_ip.lock() = Some(packet.yiaddr);

        // Get server ID
        let server = if let Some(data) = packet.get_option(options::SERVER_ID) {
            if data.len() >= 4 {
                Ipv4Addr([data[0], data[1], data[2], data[3]])
            } else {
                packet.siaddr
            }
        } else {
            packet.siaddr
        };
        *self.offered_server.lock() = Some(server);

        // Send REQUEST - use the same XID from the OFFER (and original DISCOVER)
        let xid = packet.xid;
        let mac = self.interface.mac_address();
        let request = DhcpPacket::new_request(mac, xid, packet.yiaddr, server);

        *self.state.lock() = DhcpState::Requesting;

        Ok(Some(request.to_bytes()))
    }

    /// Process DHCP ACK
    pub fn process_ack(&self, packet: &DhcpPacket) -> NetResult<Option<DhcpLease>> {
        if *self.state.lock() != DhcpState::Requesting && *self.state.lock() != DhcpState::Renewing
        {
            return Ok(None);
        }

        if packet.message_type() != Some(message_type::ACK) {
            return Ok(None);
        }

        // Parse lease information
        let ip_addr = packet.yiaddr;

        let subnet_mask = if let Some(data) = packet.get_option(options::SUBNET_MASK) {
            if data.len() >= 4 {
                Ipv4Addr([data[0], data[1], data[2], data[3]])
            } else {
                Ipv4Addr::new(255, 255, 255, 0)
            }
        } else {
            Ipv4Addr::new(255, 255, 255, 0)
        };

        let gateway = if let Some(data) = packet.get_option(options::ROUTER) {
            if data.len() >= 4 {
                Some(Ipv4Addr([data[0], data[1], data[2], data[3]]))
            } else {
                None
            }
        } else {
            None
        };

        let mut dns_servers = Vec::new();
        if let Some(data) = packet.get_option(options::DNS_SERVER) {
            let mut i = 0;
            while i + 4 <= data.len() {
                dns_servers.push(Ipv4Addr([data[i], data[i + 1], data[i + 2], data[i + 3]]));
                i += 4;
            }
        }

        let server = self.offered_server.lock().unwrap_or(packet.siaddr);

        let lease_time = if let Some(data) = packet.get_option(options::LEASE_TIME) {
            if data.len() >= 4 {
                u32::from_be_bytes([data[0], data[1], data[2], data[3]])
            } else {
                86400 // Default 24 hours
            }
        } else {
            86400
        };

        let renewal_time = if let Some(data) = packet.get_option(options::RENEWAL_TIME) {
            if data.len() >= 4 {
                u32::from_be_bytes([data[0], data[1], data[2], data[3]])
            } else {
                lease_time / 2
            }
        } else {
            lease_time / 2
        };

        let rebinding_time = if let Some(data) = packet.get_option(options::REBINDING_TIME) {
            if data.len() >= 4 {
                u32::from_be_bytes([data[0], data[1], data[2], data[3]])
            } else {
                (lease_time * 7) / 8
            }
        } else {
            (lease_time * 7) / 8
        };

        let lease = DhcpLease {
            ip_addr,
            subnet_mask,
            gateway,
            dns_servers,
            server,
            lease_time,
            renewal_time,
            rebinding_time,
        };

        *self.lease.lock() = Some(lease.clone());
        *self.state.lock() = DhcpState::Bound;

        Ok(Some(lease))
    }

    /// Process DHCP NAK
    pub fn process_nak(&self, packet: &DhcpPacket) -> NetResult<()> {
        if packet.message_type() != Some(message_type::NAK) {
            return Ok(());
        }

        // Return to INIT state
        *self.state.lock() = DhcpState::Init;
        *self.lease.lock() = None;
        *self.offered_ip.lock() = None;
        *self.offered_server.lock() = None;

        Ok(())
    }

    /// Release current lease
    pub fn release(&self) -> NetResult<Option<Vec<u8>>> {
        let lease = self.lease.lock().take();

        if let Some(lease) = lease {
            let xid = self.xid.fetch_add(1, Ordering::SeqCst);
            let mac = self.interface.mac_address();

            let mut packet = DhcpPacket::new_request(mac, xid, lease.ip_addr, lease.server);
            packet.ciaddr = lease.ip_addr;

            // Change message type to RELEASE
            for opt in &mut packet.options {
                if opt.code == options::MESSAGE_TYPE {
                    opt.data = vec![message_type::RELEASE];
                    break;
                }
            }

            *self.state.lock() = DhcpState::Init;

            Ok(Some(packet.to_bytes()))
        } else {
            Ok(None)
        }
    }
}
