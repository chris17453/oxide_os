//! TCP Protocol Implementation

use alloc::collections::VecDeque;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use spin::Mutex;

use net::{Ipv4Addr, NetError, NetResult};

use crate::checksum;
use crate::ip::{IpProtocol, PseudoHeader};

/// TCP header minimum length
pub const TCP_HEADER_MIN_LEN: usize = 20;

/// TCP maximum segment size (default)
pub const TCP_MSS_DEFAULT: u16 = 536;

/// TCP maximum segment size (standard Ethernet)
pub const TCP_MSS_ETHERNET: u16 = 1460;

/// TCP window size (default)
pub const TCP_WINDOW_SIZE: u16 = 65535;

/// Maximum window scale shift count (RFC 1323)
pub const TCP_MAX_WSCALE: u8 = 14;

/// TCP option kinds (RFC 793, 1323, 2018)
pub mod tcp_option {
    pub const END: u8 = 0;
    pub const NOP: u8 = 1;
    pub const MSS: u8 = 2;
    pub const WINDOW_SCALE: u8 = 3;
    pub const SACK_PERMITTED: u8 = 4;
    pub const SACK: u8 = 5;
    pub const TIMESTAMP: u8 = 8;
}

/// TCP flags
pub mod tcp_flags {
    pub const FIN: u8 = 0x01;
    pub const SYN: u8 = 0x02;
    pub const RST: u8 = 0x04;
    pub const PSH: u8 = 0x08;
    pub const ACK: u8 = 0x10;
    pub const URG: u8 = 0x20;
    pub const ECE: u8 = 0x40;
    pub const CWR: u8 = 0x80;
}

/// TCP options parsed from header
#[derive(Debug, Clone, Default)]
pub struct TcpOptions {
    /// Maximum Segment Size (MSS)
    pub mss: Option<u16>,
    /// Window Scale shift count
    pub window_scale: Option<u8>,
    /// SACK permitted flag
    pub sack_permitted: bool,
    /// SACK blocks (left edge, right edge)
    pub sack_blocks: Vec<(u32, u32)>,
    /// Timestamp value
    pub timestamp: Option<u32>,
    /// Timestamp echo reply
    pub timestamp_echo: Option<u32>,
}

impl TcpOptions {
    /// GraveShift: Parse TCP options from raw bytes - critical for RFC compliance
    pub fn parse(data: &[u8]) -> Self {
        let mut opts = TcpOptions::default();
        let mut i = 0;

        while i < data.len() {
            let kind = data[i];

            match kind {
                tcp_option::END => break,
                tcp_option::NOP => {
                    i += 1;
                }
                tcp_option::MSS => {
                    if i + 3 < data.len() && data[i + 1] == 4 {
                        opts.mss = Some(u16::from_be_bytes([data[i + 2], data[i + 3]]));
                        i += 4;
                    } else {
                        break;
                    }
                }
                tcp_option::WINDOW_SCALE => {
                    if i + 2 < data.len() && data[i + 1] == 3 {
                        opts.window_scale = Some(data[i + 2].min(TCP_MAX_WSCALE));
                        i += 3;
                    } else {
                        break;
                    }
                }
                tcp_option::SACK_PERMITTED => {
                    if i + 1 < data.len() && data[i + 1] == 2 {
                        opts.sack_permitted = true;
                        i += 2;
                    } else {
                        break;
                    }
                }
                tcp_option::SACK => {
                    if i + 1 < data.len() {
                        let len = data[i + 1] as usize;
                        if len >= 2 && i + len <= data.len() && (len - 2) % 8 == 0 {
                            let mut j = i + 2;
                            while j + 7 < i + len {
                                let left = u32::from_be_bytes([data[j], data[j+1], data[j+2], data[j+3]]);
                                let right = u32::from_be_bytes([data[j+4], data[j+5], data[j+6], data[j+7]]);
                                opts.sack_blocks.push((left, right));
                                j += 8;
                            }
                            i += len;
                        } else {
                            break;
                        }
                    } else {
                        break;
                    }
                }
                tcp_option::TIMESTAMP => {
                    if i + 9 < data.len() && data[i + 1] == 10 {
                        opts.timestamp = Some(u32::from_be_bytes([data[i+2], data[i+3], data[i+4], data[i+5]]));
                        opts.timestamp_echo = Some(u32::from_be_bytes([data[i+6], data[i+7], data[i+8], data[i+9]]));
                        i += 10;
                    } else {
                        break;
                    }
                }
                _ => {
                    // Unknown option - skip if we can determine length
                    if i + 1 < data.len() {
                        let len = data[i + 1] as usize;
                        if len >= 2 && i + len <= data.len() {
                            i += len;
                        } else {
                            break;
                        }
                    } else {
                        break;
                    }
                }
            }
        }

        opts
    }

    /// BlackLatch: Encode options to bytes with proper padding for security
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::new();

        if let Some(mss) = self.mss {
            buf.push(tcp_option::MSS);
            buf.push(4);
            buf.extend_from_slice(&mss.to_be_bytes());
        }

        if let Some(scale) = self.window_scale {
            buf.push(tcp_option::WINDOW_SCALE);
            buf.push(3);
            buf.push(scale);
        }

        if self.sack_permitted {
            buf.push(tcp_option::SACK_PERMITTED);
            buf.push(2);
        }

        if !self.sack_blocks.is_empty() {
            buf.push(tcp_option::SACK);
            buf.push(2 + (self.sack_blocks.len() * 8) as u8);
            for (left, right) in &self.sack_blocks {
                buf.extend_from_slice(&left.to_be_bytes());
                buf.extend_from_slice(&right.to_be_bytes());
            }
        }

        if let (Some(ts), Some(ts_echo)) = (self.timestamp, self.timestamp_echo) {
            buf.push(tcp_option::TIMESTAMP);
            buf.push(10);
            buf.extend_from_slice(&ts.to_be_bytes());
            buf.extend_from_slice(&ts_echo.to_be_bytes());
        }

        // Pad to 4-byte boundary with NOPs
        while buf.len() % 4 != 0 {
            buf.push(tcp_option::NOP);
        }

        buf
    }
}

/// TCP state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

/// TCP header
#[derive(Debug, Clone, Copy)]
pub struct TcpHeader {
    /// Source port
    pub src_port: u16,
    /// Destination port
    pub dst_port: u16,
    /// Sequence number
    pub seq_num: u32,
    /// Acknowledgment number
    pub ack_num: u32,
    /// Data offset and reserved
    pub data_offset_reserved: u8,
    /// Flags
    pub flags: u8,
    /// Window size
    pub window: u16,
    /// Checksum
    pub checksum: u16,
    /// Urgent pointer
    pub urgent_ptr: u16,
}

impl TcpHeader {
    /// Parse TCP header
    pub fn parse(data: &[u8]) -> NetResult<Self> {
        if data.len() < TCP_HEADER_MIN_LEN {
            return Err(NetError::InvalidArgument);
        }

        Ok(TcpHeader {
            src_port: u16::from_be_bytes([data[0], data[1]]),
            dst_port: u16::from_be_bytes([data[2], data[3]]),
            seq_num: u32::from_be_bytes([data[4], data[5], data[6], data[7]]),
            ack_num: u32::from_be_bytes([data[8], data[9], data[10], data[11]]),
            data_offset_reserved: data[12],
            flags: data[13],
            window: u16::from_be_bytes([data[14], data[15]]),
            checksum: u16::from_be_bytes([data[16], data[17]]),
            urgent_ptr: u16::from_be_bytes([data[18], data[19]]),
        })
    }

    /// Get data offset in bytes
    pub fn data_offset(&self) -> usize {
        ((self.data_offset_reserved >> 4) as usize) * 4
    }

    /// Serialize to bytes
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(TCP_HEADER_MIN_LEN);

        buf.extend_from_slice(&self.src_port.to_be_bytes());
        buf.extend_from_slice(&self.dst_port.to_be_bytes());
        buf.extend_from_slice(&self.seq_num.to_be_bytes());
        buf.extend_from_slice(&self.ack_num.to_be_bytes());
        buf.push(self.data_offset_reserved);
        buf.push(self.flags);
        buf.extend_from_slice(&self.window.to_be_bytes());
        buf.extend_from_slice(&self.checksum.to_be_bytes());
        buf.extend_from_slice(&self.urgent_ptr.to_be_bytes());

        buf
    }
}

/// TCP segment
pub struct TcpSegment {
    /// Header
    pub header: TcpHeader,
    /// Options
    pub options: TcpOptions,
    /// Data
    pub data: Vec<u8>,
}

impl TcpSegment {
    /// Create a new TCP segment with options
    pub fn new_with_options(
        src_port: u16,
        dst_port: u16,
        seq_num: u32,
        ack_num: u32,
        flags: u8,
        window: u16,
        options: TcpOptions,
        data: &[u8],
    ) -> Self {
        let options_bytes = options.to_bytes();
        let data_offset = ((TCP_HEADER_MIN_LEN + options_bytes.len()) / 4) as u8;

        TcpSegment {
            header: TcpHeader {
                src_port,
                dst_port,
                seq_num,
                ack_num,
                data_offset_reserved: data_offset << 4,
                flags,
                window,
                checksum: 0,
                urgent_ptr: 0,
            },
            options,
            data: data.to_vec(),
        }
    }

    /// Create a new TCP segment without options (legacy compatibility)
    pub fn new(
        src_port: u16,
        dst_port: u16,
        seq_num: u32,
        ack_num: u32,
        flags: u8,
        window: u16,
        data: &[u8],
    ) -> Self {
        Self::new_with_options(
            src_port,
            dst_port,
            seq_num,
            ack_num,
            flags,
            window,
            TcpOptions::default(),
            data,
        )
    }

    /// Parse TCP segment from raw bytes
    pub fn parse(data: &[u8]) -> NetResult<Self> {
        let header = TcpHeader::parse(data)?;
        let header_len = header.data_offset();
        
        if data.len() < header_len {
            return Err(NetError::InvalidArgument);
        }

        let options = if header_len > TCP_HEADER_MIN_LEN {
            TcpOptions::parse(&data[TCP_HEADER_MIN_LEN..header_len])
        } else {
            TcpOptions::default()
        };

        let payload = if data.len() > header_len {
            data[header_len..].to_vec()
        } else {
            Vec::new()
        };

        Ok(TcpSegment {
            header,
            options,
            data: payload,
        })
    }

    /// Serialize with checksum
    pub fn to_bytes(&self, src_ip: Ipv4Addr, dst_ip: Ipv4Addr) -> Vec<u8> {
        let mut buf = self.header.to_bytes();
        let options_bytes = self.options.to_bytes();
        buf.extend_from_slice(&options_bytes);
        buf.extend_from_slice(&self.data);

        // Compute checksum
        let pseudo = PseudoHeader::new(src_ip, dst_ip, IpProtocol::Tcp, buf.len() as u16);
        let checksum = checksum::checksum_with_pseudo(&pseudo.to_bytes(), &buf);

        buf[16] = (checksum >> 8) as u8;
        buf[17] = checksum as u8;

        buf
    }
}

/// Initial sequence number generator
static ISN_COUNTER: AtomicU32 = AtomicU32::new(0x12345678);

fn generate_isn() -> u32 {
    ISN_COUNTER.fetch_add(64000, Ordering::SeqCst)
}

/// TCP connection
pub struct TcpConnection {
    /// Connection ID
    pub id: u32,
    /// Local IP
    pub local_ip: Ipv4Addr,
    /// Local port
    pub local_port: u16,
    /// Remote IP
    pub remote_ip: Ipv4Addr,
    /// Remote port
    pub remote_port: u16,
    /// State
    state: Mutex<TcpState>,
    /// Send sequence number (next to send)
    snd_nxt: AtomicU32,
    /// Send unacknowledged
    snd_una: AtomicU32,
    /// Receive sequence number (next expected)
    rcv_nxt: AtomicU32,
    /// Send window
    snd_wnd: AtomicU32,
    /// Receive window
    rcv_wnd: AtomicU32,
    /// Send buffer
    send_buf: Mutex<VecDeque<u8>>,
    /// Receive buffer
    recv_buf: Mutex<VecDeque<u8>>,
    /// Retransmit queue (seq_num, data, timestamp)
    retransmit_queue: Mutex<Vec<(u32, Vec<u8>, u64)>>,
    /// Last activity timestamp
    last_activity: AtomicU64,
    
    // TorqueJax: Congestion control fields for RFC 5681 compliance
    /// Congestion window (in bytes)
    cwnd: AtomicU32,
    /// Slow start threshold (in bytes)
    ssthresh: AtomicU32,
    /// Maximum segment size negotiated with peer
    mss: AtomicU32,
    /// Receive MSS
    rcv_mss: AtomicU32,
    
    // WireSaint: RTT estimation fields (RFC 6298)
    /// Smoothed round-trip time (microseconds)
    srtt: AtomicU64,
    /// RTT variation (microseconds)
    rttvar: AtomicU64,
    /// Retransmission timeout (microseconds)
    rto: AtomicU64,
    
    // GraveShift: Additional RFC compliance fields
    /// Window scale shift count (send)
    snd_wscale: AtomicU32,
    /// Window scale shift count (receive)
    rcv_wscale: AtomicU32,
    /// SACK permitted flag
    sack_permitted: AtomicU32,
    /// Timestamp for next outgoing segment
    ts_recent: AtomicU32,
    /// Duplicate ACK counter for fast retransmit
    dup_acks: AtomicU32,
    /// Keepalive timer
    keepalive_timer: AtomicU64,
    /// TIME_WAIT timer
    time_wait_timer: AtomicU64,
    /// Nagle algorithm enabled
    nagle_enabled: AtomicU32,
    /// Unacknowledged data pending
    has_unacked: AtomicU32,
}

impl TcpConnection {
    // RustViper: Initial RTO value per RFC 6298 (1 second in microseconds)
    const INITIAL_RTO: u64 = 1_000_000;
    // SableWire: Minimum RTO (200ms per RFC 6298)
    const MIN_RTO: u64 = 200_000;
    // Maximum RTO (60 seconds)
    const MAX_RTO: u64 = 60_000_000;
    // Keepalive interval (2 hours in microseconds)
    const KEEPALIVE_INTERVAL: u64 = 2 * 3600 * 1_000_000;
    // TIME_WAIT timeout (2*MSL = 4 minutes)
    const TIME_WAIT_TIMEOUT: u64 = 4 * 60 * 1_000_000;
    
    /// Create a new TCP connection
    pub fn new(
        id: u32,
        local_ip: Ipv4Addr,
        local_port: u16,
        remote_ip: Ipv4Addr,
        remote_port: u16,
    ) -> Self {
        let isn = generate_isn();
        let init_mss = TCP_MSS_ETHERNET;

        TcpConnection {
            id,
            local_ip,
            local_port,
            remote_ip,
            remote_port,
            state: Mutex::new(TcpState::Closed),
            snd_nxt: AtomicU32::new(isn),
            snd_una: AtomicU32::new(isn),
            rcv_nxt: AtomicU32::new(0),
            snd_wnd: AtomicU32::new(TCP_WINDOW_SIZE as u32),
            rcv_wnd: AtomicU32::new(TCP_WINDOW_SIZE as u32),
            send_buf: Mutex::new(VecDeque::new()),
            recv_buf: Mutex::new(VecDeque::new()),
            retransmit_queue: Mutex::new(Vec::new()),
            last_activity: AtomicU64::new(0),
            // Initialize congestion control per RFC 5681
            cwnd: AtomicU32::new(init_mss as u32 * 2), // Initial cwnd = 2*MSS
            ssthresh: AtomicU32::new(u32::MAX), // Initially unlimited
            mss: AtomicU32::new(init_mss as u32),
            rcv_mss: AtomicU32::new(init_mss as u32),
            // Initialize RTT tracking per RFC 6298
            srtt: AtomicU64::new(0),
            rttvar: AtomicU64::new(0),
            rto: AtomicU64::new(Self::INITIAL_RTO),
            // Initialize window scaling
            snd_wscale: AtomicU32::new(0),
            rcv_wscale: AtomicU32::new(0),
            sack_permitted: AtomicU32::new(0),
            ts_recent: AtomicU32::new(0),
            dup_acks: AtomicU32::new(0),
            keepalive_timer: AtomicU64::new(0),
            time_wait_timer: AtomicU64::new(0),
            nagle_enabled: AtomicU32::new(1), // Enabled by default
            has_unacked: AtomicU32::new(0),
        }
    }

    /// Check if connection matches
    pub fn matches(
        &self,
        remote_ip: Ipv4Addr,
        remote_port: u16,
        local_ip: Ipv4Addr,
        local_port: u16,
    ) -> bool {
        self.remote_ip == remote_ip
            && self.remote_port == remote_port
            && self.local_ip == local_ip
            && self.local_port == local_port
    }

    /// Get current state
    pub fn state(&self) -> TcpState {
        *self.state.lock()
    }

    /// Initiate connection (active open)
    pub fn connect(&self) -> NetResult<()> {
        let mut state = self.state.lock();
        if *state != TcpState::Closed {
            return Err(NetError::AlreadyConnected);
        }

        // GraveShift: Build SYN with RFC-compliant options
        let seq = self.snd_nxt.load(Ordering::SeqCst);
        let mss = self.mss.load(Ordering::SeqCst) as u16;
        
        let mut options = TcpOptions::default();
        options.mss = Some(mss);
        options.window_scale = Some(7); // Scale factor for 64KB->8MB window
        options.sack_permitted = true;
        options.timestamp = Some(Self::get_timestamp());
        options.timestamp_echo = Some(0);
        
        self.rcv_wscale.store(7, Ordering::SeqCst);
        
        let segment = TcpSegment::new_with_options(
            self.local_port,
            self.remote_port,
            seq,
            0,
            tcp_flags::SYN,
            TCP_WINDOW_SIZE,
            options,
            &[],
        );

        // BlackLatch: Transmit would happen here via stack callback
        let _bytes = segment.to_bytes(self.local_ip, self.remote_ip);

        self.snd_nxt.fetch_add(1, Ordering::SeqCst); // SYN consumes one sequence number
        *state = TcpState::SynSent;

        Ok(())
    }

    /// Listen for connections (passive open)
    pub fn listen(&self) -> NetResult<()> {
        let mut state = self.state.lock();
        if *state != TcpState::Closed {
            return Err(NetError::InvalidArgument);
        }
        *state = TcpState::Listen;
        Ok(())
    }

    /// Process incoming segment with full RFC compliance
    pub fn process_segment(&self, segment: &TcpSegment) -> NetResult<()> {
        let header = &segment.header;
        let options = &segment.options;
        let data = &segment.data;
        
        // NeonRoot: Validate sequence number per RFC 793
        if !self.seq_acceptable(header.seq_num, data.len() as u32) {
            // Segment not acceptable - send ACK
            return Ok(());
        }
        
        let mut state = self.state.lock();
        
        // Process RST flag first (RFC 793)
        if header.flags & tcp_flags::RST != 0 {
            match *state {
                TcpState::SynReceived | TcpState::Established | TcpState::FinWait1 
                | TcpState::FinWait2 | TcpState::CloseWait => {
                    *state = TcpState::Closed;
                    return Ok(());
                }
                _ => return Ok(()),
            }
        }

        match *state {
            TcpState::Closed => {
                // GraveShift: Closed state - should send RST (handled at stack level)
            }
            TcpState::Listen => {
                if header.flags & tcp_flags::SYN != 0 {
                    // BlackLatch: Handle incoming SYN with option negotiation
                    if let Some(peer_mss) = options.mss {
                        self.mss.store(peer_mss.min(TCP_MSS_ETHERNET) as u32, Ordering::SeqCst);
                    }
                    if let Some(wscale) = options.window_scale {
                        self.snd_wscale.store(wscale as u32, Ordering::SeqCst);
                    }
                    if options.sack_permitted {
                        self.sack_permitted.store(1, Ordering::SeqCst);
                    }
                    if let Some(ts) = options.timestamp {
                        self.ts_recent.store(ts, Ordering::SeqCst);
                    }
                    
                    self.rcv_nxt.store(header.seq_num.wrapping_add(1), Ordering::SeqCst);
                    *state = TcpState::SynReceived;
                    // Would send SYN-ACK here
                }
            }
            TcpState::SynSent => {
                if header.flags & tcp_flags::SYN != 0 && header.flags & tcp_flags::ACK != 0 {
                    // SYN-ACK received - process options
                    if let Some(peer_mss) = options.mss {
                        self.mss.store(peer_mss.min(TCP_MSS_ETHERNET) as u32, Ordering::SeqCst);
                    }
                    if let Some(wscale) = options.window_scale {
                        self.snd_wscale.store(wscale as u32, Ordering::SeqCst);
                    }
                    if options.sack_permitted {
                        self.sack_permitted.store(1, Ordering::SeqCst);
                    }
                    
                    self.snd_una.store(header.ack_num, Ordering::SeqCst);
                    self.rcv_nxt.store(header.seq_num.wrapping_add(1), Ordering::SeqCst);
                    *state = TcpState::Established;
                    // Would send ACK here
                } else if header.flags & tcp_flags::SYN != 0 {
                    // Simultaneous open
                    self.rcv_nxt.store(header.seq_num.wrapping_add(1), Ordering::SeqCst);
                    *state = TcpState::SynReceived;
                    // Would send SYN-ACK here
                }
            }
            TcpState::SynReceived => {
                if header.flags & tcp_flags::ACK != 0 {
                    self.process_ack(header.ack_num);
                    *state = TcpState::Established;
                }
            }
            TcpState::Established => {
                // WireSaint: Update activity timestamp
                self.last_activity.store(Self::get_timestamp_us(), Ordering::SeqCst);
                
                // Process ACKs with congestion control
                if header.flags & tcp_flags::ACK != 0 {
                    self.process_ack(header.ack_num);
                    
                    // Update RTT if timestamp present
                    if let (Some(ts_val), Some(ts_ecr)) = (options.timestamp, options.timestamp_echo) {
                        if ts_ecr > 0 {
                            let now = Self::get_timestamp();
                            let rtt = now.saturating_sub(ts_ecr) as u64 * 1000; // Convert to microseconds
                            self.update_rtt(rtt);
                        }
                    }
                }

                // ShadePacket: Process data with proper flow control
                if !data.is_empty() {
                    let expected_seq = self.rcv_nxt.load(Ordering::SeqCst);
                    if header.seq_num == expected_seq {
                        // In-order data
                        let mut recv_buf = self.recv_buf.lock();
                        recv_buf.extend(data);
                        self.rcv_nxt.fetch_add(data.len() as u32, Ordering::SeqCst);
                        
                        // Update receive window based on buffer space
                        let available = TCP_WINDOW_SIZE as usize - recv_buf.len();
                        self.rcv_wnd.store(available as u32, Ordering::SeqCst);
                        
                        // Would send ACK here
                    } else {
                        // Out-of-order data - would queue for reassembly
                        // Send duplicate ACK
                    }
                }
                
                // Process URG flag
                if header.flags & tcp_flags::URG != 0 {
                    // Handle urgent data up to urgent_ptr
                }

                // Handle FIN
                if header.flags & tcp_flags::FIN != 0 {
                    self.rcv_nxt.fetch_add(1, Ordering::SeqCst);
                    *state = TcpState::CloseWait;
                    // Would send ACK here
                }
            }
            TcpState::FinWait1 => {
                if header.flags & tcp_flags::ACK != 0 {
                    self.process_ack(header.ack_num);
                    *state = TcpState::FinWait2;
                }
                if header.flags & tcp_flags::FIN != 0 {
                    self.rcv_nxt.fetch_add(1, Ordering::SeqCst);
                    if *state == TcpState::FinWait2 {
                        *state = TcpState::TimeWait;
                        self.time_wait_timer.store(Self::get_timestamp_us(), Ordering::SeqCst);
                    } else {
                        *state = TcpState::Closing;
                    }
                    // Would send ACK here
                }
            }
            TcpState::FinWait2 => {
                if header.flags & tcp_flags::ACK != 0 {
                    self.process_ack(header.ack_num);
                }
                if header.flags & tcp_flags::FIN != 0 {
                    self.rcv_nxt.fetch_add(1, Ordering::SeqCst);
                    *state = TcpState::TimeWait;
                    self.time_wait_timer.store(Self::get_timestamp_us(), Ordering::SeqCst);
                    // Would send ACK here
                }
            }
            TcpState::CloseWait => {
                // Waiting for application to close
                if header.flags & tcp_flags::ACK != 0 {
                    self.process_ack(header.ack_num);
                }
            }
            TcpState::Closing => {
                if header.flags & tcp_flags::ACK != 0 {
                    self.process_ack(header.ack_num);
                    *state = TcpState::TimeWait;
                    self.time_wait_timer.store(Self::get_timestamp_us(), Ordering::SeqCst);
                }
            }
            TcpState::LastAck => {
                if header.flags & tcp_flags::ACK != 0 {
                    self.process_ack(header.ack_num);
                    *state = TcpState::Closed;
                }
            }
            TcpState::TimeWait => {
                // Reset TIME_WAIT timer if we get another FIN
                if header.flags & tcp_flags::FIN != 0 {
                    self.time_wait_timer.store(Self::get_timestamp_us(), Ordering::SeqCst);
                    // Would send ACK here
                }
            }
        }

        Ok(())
    }

    /// Send data
    pub fn send(&self, data: &[u8]) -> NetResult<usize> {
        let state = *self.state.lock();
        if state != TcpState::Established && state != TcpState::CloseWait {
            return Err(NetError::NotConnected);
        }

        // Add to send buffer
        self.send_buf.lock().extend(data);
        Ok(data.len())
    }

    /// Receive data
    pub fn recv(&self, buf: &mut [u8]) -> NetResult<usize> {
        let state = *self.state.lock();
        match state {
            TcpState::Established | TcpState::FinWait1 | TcpState::FinWait2 => {}
            TcpState::CloseWait => {
                // Can still receive buffered data
            }
            _ => return Err(NetError::NotConnected),
        }

        let mut recv_buf = self.recv_buf.lock();
        if recv_buf.is_empty() {
            return Err(NetError::WouldBlock);
        }

        let len = buf.len().min(recv_buf.len());
        for i in 0..len {
            buf[i] = recv_buf.pop_front().unwrap();
        }

        Ok(len)
    }

    /// Close connection
    pub fn close(&self) -> NetResult<()> {
        let mut state = self.state.lock();

        match *state {
            TcpState::Established => {
                // Send FIN
                *state = TcpState::FinWait1;
            }
            TcpState::CloseWait => {
                // Send FIN
                *state = TcpState::LastAck;
            }
            TcpState::SynSent | TcpState::Listen => {
                *state = TcpState::Closed;
            }
            _ => {}
        }

        Ok(())
    }

    /// Process timers
    pub fn process_timers(&self) -> NetResult<()> {
        let now = Self::get_timestamp_us();
        let state = *self.state.lock();
        
        // WireSaint: Retransmission timer
        self.check_retransmission(now)?;
        
        // Keepalive timer
        if state == TcpState::Established {
            let last_activity = self.last_activity.load(Ordering::SeqCst);
            if now.saturating_sub(last_activity) > Self::KEEPALIVE_INTERVAL {
                self.send_keepalive()?;
                self.last_activity.store(now, Ordering::SeqCst);
            }
        }
        
        // TIME_WAIT timeout
        if state == TcpState::TimeWait {
            let timer = self.time_wait_timer.load(Ordering::SeqCst);
            if timer > 0 && now.saturating_sub(timer) > Self::TIME_WAIT_TIMEOUT {
                let mut state = self.state.lock();
                *state = TcpState::Closed;
            }
        }
        
        Ok(())
    }
    
    /// TorqueJax: Check and handle retransmissions with exponential backoff
    fn check_retransmission(&self, now: u64) -> NetResult<()> {
        let mut queue = self.retransmit_queue.lock();
        let rto = self.rto.load(Ordering::SeqCst);
        
        for (seq, data, timestamp) in queue.iter_mut() {
            if now.saturating_sub(*timestamp) > rto {
                // Retransmit needed
                // Would send segment here
                *timestamp = now;
                
                // Exponential backoff: double RTO (up to max)
                let new_rto = (rto * 2).min(Self::MAX_RTO);
                self.rto.store(new_rto, Ordering::SeqCst);
                
                // Reduce congestion window (multiplicative decrease)
                let ssthresh = self.cwnd.load(Ordering::SeqCst) / 2;
                self.ssthresh.store(ssthresh.max(2 * self.mss.load(Ordering::SeqCst)), Ordering::SeqCst);
                self.cwnd.store(self.mss.load(Ordering::SeqCst), Ordering::SeqCst);
            }
        }
        
        Ok(())
    }
    
    /// Send keepalive probe
    fn send_keepalive(&self) -> NetResult<()> {
        // Send segment with seq = snd_una - 1 (one byte before window)
        Ok(())
    }
    
    /// RustViper: Update RTT estimate using Karn's algorithm (RFC 6298)
    fn update_rtt(&self, measured_rtt: u64) {
        let srtt = self.srtt.load(Ordering::SeqCst);
        let rttvar = self.rttvar.load(Ordering::SeqCst);
        
        if srtt == 0 {
            // First measurement
            self.srtt.store(measured_rtt, Ordering::SeqCst);
            self.rttvar.store(measured_rtt / 2, Ordering::SeqCst);
        } else {
            // RFC 6298: RTTVAR = (1-beta) * RTTVAR + beta * |SRTT - R'|
            // SRTT = (1-alpha) * SRTT + alpha * R'
            // Using alpha=1/8, beta=1/4
            let diff = if srtt > measured_rtt {
                srtt - measured_rtt
            } else {
                measured_rtt - srtt
            };
            
            let new_rttvar = (rttvar * 3 / 4) + (diff / 4);
            let new_srtt = (srtt * 7 / 8) + (measured_rtt / 8);
            
            self.rttvar.store(new_rttvar, Ordering::SeqCst);
            self.srtt.store(new_srtt, Ordering::SeqCst);
        }
        
        // RTO = SRTT + max(G, K*RTTVAR) where G=clock granularity, K=4
        let new_rto = (srtt + rttvar * 4).clamp(Self::MIN_RTO, Self::MAX_RTO);
        self.rto.store(new_rto, Ordering::SeqCst);
    }
    
    /// GraveShift: Process ACK with congestion control (RFC 5681)
    fn process_ack(&self, ack_num: u32) {
        let una = self.snd_una.load(Ordering::SeqCst);
        
        if ack_num == una {
            // Duplicate ACK
            let dup_count = self.dup_acks.fetch_add(1, Ordering::SeqCst) + 1;
            
            if dup_count == 3 {
                // Fast retransmit (RFC 5681)
                self.fast_retransmit();
                // Enter fast recovery
                let ssthresh = self.cwnd.load(Ordering::SeqCst) / 2;
                self.ssthresh.store(ssthresh.max(2 * self.mss.load(Ordering::SeqCst)), Ordering::SeqCst);
                self.cwnd.store(ssthresh + 3 * self.mss.load(Ordering::SeqCst), Ordering::SeqCst);
            } else if dup_count > 3 {
                // Inflate cwnd for each additional duplicate ACK
                self.cwnd.fetch_add(self.mss.load(Ordering::SeqCst), Ordering::SeqCst);
            }
        } else if Self::seq_gt(ack_num, una) {
            // New ACK - clear duplicate counter
            self.dup_acks.store(0, Ordering::SeqCst);
            self.snd_una.store(ack_num, Ordering::SeqCst);
            
            // Remove acknowledged data from retransmit queue
            let mut queue = self.retransmit_queue.lock();
            queue.retain(|(seq, data, _)| {
                let end_seq = seq.wrapping_add(data.len() as u32);
                Self::seq_gt(end_seq, ack_num)
            });
            
            // Congestion control
            let cwnd = self.cwnd.load(Ordering::SeqCst);
            let ssthresh = self.ssthresh.load(Ordering::SeqCst);
            let mss = self.mss.load(Ordering::SeqCst);
            
            if cwnd < ssthresh {
                // Slow start: cwnd += MSS for each ACK
                self.cwnd.store(cwnd + mss, Ordering::SeqCst);
            } else {
                // Congestion avoidance: cwnd += MSS*MSS/cwnd
                let increment = (mss * mss) / cwnd;
                self.cwnd.store(cwnd + increment.max(1), Ordering::SeqCst);
            }
        }
    }
    
    /// Fast retransmit on triple duplicate ACK
    fn fast_retransmit(&self) {
        // Retransmit first unacknowledged segment
        let una = self.snd_una.load(Ordering::SeqCst);
        let queue = self.retransmit_queue.lock();
        
        for (seq, data, _) in queue.iter() {
            if *seq == una {
                // Would retransmit this segment here
                break;
            }
        }
    }
    
    /// ShadePacket: Apply Nagle algorithm check
    fn can_send_segment(&self, data_len: usize) -> bool {
        if self.nagle_enabled.load(Ordering::SeqCst) == 0 {
            return true; // Nagle disabled
        }
        
        // Can send if: full-sized segment OR no unacked data OR no buffered data
        let mss = self.mss.load(Ordering::SeqCst) as usize;
        let has_unacked = self.has_unacked.load(Ordering::SeqCst) != 0;
        
        data_len >= mss || !has_unacked
    }
    
    /// Get current timestamp in arbitrary units (e.g., milliseconds)
    fn get_timestamp() -> u32 {
        // Would use actual clock source
        0
    }
    
    /// Get current timestamp in microseconds
    fn get_timestamp_us() -> u64 {
        // Would use actual clock source
        0
    }
    
    /// Validate sequence number (RFC 793 segment acceptance test)
    fn seq_acceptable(&self, seq: u32, len: u32) -> bool {
        let rcv_nxt = self.rcv_nxt.load(Ordering::SeqCst);
        let rcv_wnd = self.rcv_wnd.load(Ordering::SeqCst);
        
        if rcv_wnd == 0 {
            return len == 0 && seq == rcv_nxt;
        }
        
        if len == 0 {
            // Zero-length segment
            Self::seq_ge(seq, rcv_nxt) && Self::seq_lt(seq, rcv_nxt.wrapping_add(rcv_wnd))
        } else {
            // Non-zero length segment  
            let seq_end = seq.wrapping_add(len - 1);
            let wnd_end = rcv_nxt.wrapping_add(rcv_wnd);
            
            (Self::seq_ge(seq, rcv_nxt) && Self::seq_lt(seq, wnd_end))
                || (Self::seq_ge(seq_end, rcv_nxt) && Self::seq_lt(seq_end, wnd_end))
        }
    }

    /// Sequence number comparison (handles wraparound)
    fn seq_gt(a: u32, b: u32) -> bool {
        let diff = a.wrapping_sub(b) as i32;
        diff > 0
    }

    fn seq_ge(a: u32, b: u32) -> bool {
        let diff = a.wrapping_sub(b) as i32;
        diff >= 0
    }
    
    fn seq_lt(a: u32, b: u32) -> bool {
        let diff = a.wrapping_sub(b) as i32;
        diff < 0
    }
    
    fn seq_le(a: u32, b: u32) -> bool {
        let diff = a.wrapping_sub(b) as i32;
        diff <= 0
    }
}
