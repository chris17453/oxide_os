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

/// TCP window size
pub const TCP_WINDOW_SIZE: u16 = 65535;

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
    pub options: Vec<u8>,
    /// Data
    pub data: Vec<u8>,
}

impl TcpSegment {
    /// Create a new TCP segment
    pub fn new(
        src_port: u16,
        dst_port: u16,
        seq_num: u32,
        ack_num: u32,
        flags: u8,
        window: u16,
        data: &[u8],
    ) -> Self {
        let data_offset = 5u8; // 20 bytes, no options

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
            options: Vec::new(),
            data: data.to_vec(),
        }
    }

    /// Serialize with checksum
    pub fn to_bytes(&self, src_ip: Ipv4Addr, dst_ip: Ipv4Addr) -> Vec<u8> {
        let mut buf = self.header.to_bytes();
        buf.extend_from_slice(&self.options);
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
    /// Retransmit queue
    retransmit_queue: Mutex<Vec<(u32, Vec<u8>)>>,
    /// Last activity timestamp
    last_activity: AtomicU64,
}

impl TcpConnection {
    /// Create a new TCP connection
    pub fn new(
        id: u32,
        local_ip: Ipv4Addr,
        local_port: u16,
        remote_ip: Ipv4Addr,
        remote_port: u16,
    ) -> Self {
        let isn = generate_isn();

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

        // Send SYN
        let seq = self.snd_nxt.load(Ordering::SeqCst);
        let segment = TcpSegment::new(
            self.local_port,
            self.remote_port,
            seq,
            0,
            tcp_flags::SYN,
            TCP_WINDOW_SIZE,
            &[],
        );

        // Would transmit segment here
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

    /// Process incoming segment
    pub fn process_segment(&self, header: &TcpHeader, data: &[u8]) -> NetResult<()> {
        let mut state = self.state.lock();

        match *state {
            TcpState::Closed => {
                // Send RST
            }
            TcpState::Listen => {
                if header.flags & tcp_flags::SYN != 0 {
                    // Handle incoming SYN
                    self.rcv_nxt
                        .store(header.seq_num.wrapping_add(1), Ordering::SeqCst);
                    *state = TcpState::SynReceived;
                    // Send SYN-ACK
                }
            }
            TcpState::SynSent => {
                if header.flags & tcp_flags::SYN != 0 && header.flags & tcp_flags::ACK != 0 {
                    // SYN-ACK received
                    self.snd_una.store(header.ack_num, Ordering::SeqCst);
                    self.rcv_nxt
                        .store(header.seq_num.wrapping_add(1), Ordering::SeqCst);
                    *state = TcpState::Established;
                    // Send ACK
                } else if header.flags & tcp_flags::SYN != 0 {
                    // Simultaneous open
                    self.rcv_nxt
                        .store(header.seq_num.wrapping_add(1), Ordering::SeqCst);
                    *state = TcpState::SynReceived;
                    // Send SYN-ACK
                }
            }
            TcpState::SynReceived => {
                if header.flags & tcp_flags::ACK != 0 {
                    self.snd_una.store(header.ack_num, Ordering::SeqCst);
                    *state = TcpState::Established;
                }
            }
            TcpState::Established => {
                // Process ACKs
                if header.flags & tcp_flags::ACK != 0 {
                    let ack = header.ack_num;
                    let una = self.snd_una.load(Ordering::SeqCst);
                    if Self::seq_gt(ack, una) {
                        self.snd_una.store(ack, Ordering::SeqCst);
                        // Remove acknowledged data from retransmit queue
                    }
                }

                // Process data
                if !data.is_empty() {
                    let expected_seq = self.rcv_nxt.load(Ordering::SeqCst);
                    if header.seq_num == expected_seq {
                        // In-order data
                        self.recv_buf.lock().extend(data);
                        self.rcv_nxt.fetch_add(data.len() as u32, Ordering::SeqCst);
                        // Send ACK
                    }
                    // Out-of-order data would be queued
                }

                // Handle FIN
                if header.flags & tcp_flags::FIN != 0 {
                    self.rcv_nxt.fetch_add(1, Ordering::SeqCst);
                    *state = TcpState::CloseWait;
                    // Send ACK
                }
            }
            TcpState::FinWait1 => {
                if header.flags & tcp_flags::ACK != 0 {
                    *state = TcpState::FinWait2;
                }
                if header.flags & tcp_flags::FIN != 0 {
                    self.rcv_nxt.fetch_add(1, Ordering::SeqCst);
                    if *state == TcpState::FinWait2 {
                        *state = TcpState::TimeWait;
                    } else {
                        *state = TcpState::Closing;
                    }
                    // Send ACK
                }
            }
            TcpState::FinWait2 => {
                if header.flags & tcp_flags::FIN != 0 {
                    self.rcv_nxt.fetch_add(1, Ordering::SeqCst);
                    *state = TcpState::TimeWait;
                    // Send ACK
                }
            }
            TcpState::CloseWait => {
                // Waiting for application to close
            }
            TcpState::Closing => {
                if header.flags & tcp_flags::ACK != 0 {
                    *state = TcpState::TimeWait;
                }
            }
            TcpState::LastAck => {
                if header.flags & tcp_flags::ACK != 0 {
                    *state = TcpState::Closed;
                }
            }
            TcpState::TimeWait => {
                // Wait for 2*MSL before closing
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
        // Handle retransmission, keepalive, TIME_WAIT timeout, etc.
        Ok(())
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
}
