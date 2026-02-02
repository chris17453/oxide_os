//! Connection Tracking for Stateful Packet Filtering
//!
//! Tracks TCP, UDP, and ICMP connections to enable stateful firewall rules.
//! Connections are identified by a 5-tuple: (protocol, src_ip, src_port, dst_ip, dst_port)

extern crate alloc;

use alloc::collections::BTreeMap;
use net::Ipv4Addr;
use spin::RwLock;

use crate::filter::ConnState;
use crate::ip::IpProtocol;

/// TCP connection states (simplified)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TcpState {
    /// SYN sent, waiting for SYN-ACK
    SynSent,
    /// SYN received, SYN-ACK sent
    SynReceived,
    /// Connection established
    Established,
    /// FIN sent, waiting for ACK
    FinWait1,
    /// FIN received and ACKed, waiting for FIN
    FinWait2,
    /// Both sides closing
    Closing,
    /// Received FIN, sent ACK, waiting for close
    CloseWait,
    /// Sent FIN after CloseWait
    LastAck,
    /// Waiting for timeout after close
    TimeWait,
    /// Connection closed
    Closed,
}

impl TcpState {
    /// Convert to filter ConnState
    pub fn to_conn_state(self) -> ConnState {
        match self {
            TcpState::Established => ConnState::Established,
            TcpState::SynSent | TcpState::SynReceived => ConnState::New,
            _ => ConnState::Established, // Closing states still count as established
        }
    }
}

/// TCP flags for state tracking
#[derive(Debug, Clone, Copy)]
pub struct TcpFlags {
    pub syn: bool,
    pub ack: bool,
    pub fin: bool,
    pub rst: bool,
}

impl TcpFlags {
    pub fn from_byte(flags: u8) -> Self {
        TcpFlags {
            fin: (flags & 0x01) != 0,
            syn: (flags & 0x02) != 0,
            rst: (flags & 0x04) != 0,
            ack: (flags & 0x10) != 0,
        }
    }
}

/// Connection tuple (identifies a connection)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ConnTuple {
    /// IP protocol
    pub protocol: u8,
    /// Source IP
    pub src_ip: u32,
    /// Source port
    pub src_port: u16,
    /// Destination IP
    pub dst_ip: u32,
    /// Destination port
    pub dst_port: u16,
}

impl ConnTuple {
    /// Create a new connection tuple
    pub fn new(
        protocol: IpProtocol,
        src_ip: Ipv4Addr,
        src_port: u16,
        dst_ip: Ipv4Addr,
        dst_port: u16,
    ) -> Self {
        Self {
            protocol: u8::from(protocol),
            src_ip: u32::from_be_bytes(*src_ip.as_bytes()),
            src_port,
            dst_ip: u32::from_be_bytes(*dst_ip.as_bytes()),
            dst_port,
        }
    }

    /// Create the reverse tuple (for matching reply packets)
    pub fn reverse(&self) -> Self {
        Self {
            protocol: self.protocol,
            src_ip: self.dst_ip,
            src_port: self.dst_port,
            dst_ip: self.src_ip,
            dst_port: self.src_port,
        }
    }

    /// Create tuple for ICMP (no ports, use type/code as "ports")
    pub fn new_icmp(src_ip: Ipv4Addr, dst_ip: Ipv4Addr, icmp_type: u8, icmp_code: u8) -> Self {
        Self {
            protocol: u8::from(IpProtocol::Icmp),
            src_ip: u32::from_be_bytes(*src_ip.as_bytes()),
            src_port: icmp_type as u16,
            dst_ip: u32::from_be_bytes(*dst_ip.as_bytes()),
            dst_port: icmp_code as u16,
        }
    }
}

/// A tracked connection entry
#[derive(Debug, Clone)]
pub struct ConnEntry {
    /// Original direction tuple
    pub original: ConnTuple,
    /// Reply direction tuple
    pub reply: ConnTuple,
    /// TCP state (if TCP)
    pub tcp_state: Option<TcpState>,
    /// Timestamp when connection was created (in ticks)
    pub created: u64,
    /// Timestamp of last packet (in ticks)
    pub last_seen: u64,
    /// Has reply been seen?
    pub reply_seen: bool,
    /// Packet count (original direction)
    pub packets_orig: u64,
    /// Packet count (reply direction)
    pub packets_reply: u64,
    /// Byte count (original direction)
    pub bytes_orig: u64,
    /// Byte count (reply direction)
    pub bytes_reply: u64,
}

impl ConnEntry {
    /// Create a new connection entry
    pub fn new(tuple: ConnTuple, now: u64) -> Self {
        let tcp_state = if tuple.protocol == u8::from(IpProtocol::Tcp) {
            Some(TcpState::SynSent)
        } else {
            None
        };

        Self {
            original: tuple,
            reply: tuple.reverse(),
            tcp_state,
            created: now,
            last_seen: now,
            reply_seen: false,
            packets_orig: 1,
            packets_reply: 0,
            bytes_orig: 0,
            bytes_reply: 0,
        }
    }

    /// Get connection state for firewall
    pub fn conn_state(&self) -> ConnState {
        if let Some(tcp_state) = self.tcp_state {
            tcp_state.to_conn_state()
        } else {
            // UDP/ICMP: established if we've seen a reply
            if self.reply_seen {
                ConnState::Established
            } else {
                ConnState::New
            }
        }
    }

    /// Check if connection has timed out
    pub fn is_expired(&self, now: u64) -> bool {
        let timeout = self.timeout_ticks();
        now.saturating_sub(self.last_seen) > timeout
    }

    /// Get timeout in ticks based on protocol and state
    fn timeout_ticks(&self) -> u64 {
        // Assuming ~100 ticks per second
        const TICKS_PER_SEC: u64 = 100;

        if let Some(tcp_state) = self.tcp_state {
            match tcp_state {
                TcpState::Established => 5 * 24 * 3600 * TICKS_PER_SEC, // 5 days
                TcpState::SynSent | TcpState::SynReceived => 120 * TICKS_PER_SEC, // 2 min
                TcpState::TimeWait => 120 * TICKS_PER_SEC,              // 2 min
                TcpState::FinWait1 | TcpState::FinWait2 => 120 * TICKS_PER_SEC,
                TcpState::CloseWait | TcpState::LastAck => 60 * TICKS_PER_SEC,
                TcpState::Closing => 60 * TICKS_PER_SEC,
                TcpState::Closed => 10 * TICKS_PER_SEC,
            }
        } else if self.original.protocol == u8::from(IpProtocol::Udp) {
            if self.reply_seen {
                180 * TICKS_PER_SEC // 3 min for established UDP
            } else {
                30 * TICKS_PER_SEC // 30 sec for new UDP
            }
        } else {
            // ICMP
            30 * TICKS_PER_SEC
        }
    }

    /// Update TCP state based on flags
    pub fn update_tcp_state(&mut self, flags: TcpFlags, is_reply: bool) {
        let Some(ref mut state) = self.tcp_state else {
            return;
        };

        if flags.rst {
            *state = TcpState::Closed;
            return;
        }

        *state = match (*state, flags.syn, flags.ack, flags.fin, is_reply) {
            // Initial SYN
            (TcpState::SynSent, false, true, false, true) => TcpState::SynReceived,
            // SYN-ACK received, send ACK
            (TcpState::SynReceived, false, true, false, false) => TcpState::Established,
            // Direct to established (simultaneous open)
            (TcpState::SynSent, true, true, false, true) => TcpState::Established,

            // Close initiated
            (TcpState::Established, false, _, true, _) => TcpState::FinWait1,
            (TcpState::FinWait1, false, true, false, true) => TcpState::FinWait2,
            (TcpState::FinWait1, false, _, true, true) => TcpState::Closing,
            (TcpState::FinWait2, false, _, true, true) => TcpState::TimeWait,
            (TcpState::Closing, false, true, false, true) => TcpState::TimeWait,

            // Passive close
            (TcpState::Established, false, _, true, true) => TcpState::CloseWait,
            (TcpState::CloseWait, false, _, true, false) => TcpState::LastAck,
            (TcpState::LastAck, false, true, false, true) => TcpState::Closed,

            // No state change
            _ => *state,
        };
    }
}

/// Connection tracking table
pub struct ConnTrackTable {
    /// Connections indexed by original tuple
    connections: BTreeMap<ConnTuple, ConnEntry>,
    /// Maximum number of connections
    max_connections: usize,
    /// Current tick counter (for timeouts)
    current_tick: u64,
}

impl ConnTrackTable {
    /// Create a new connection tracking table
    pub const fn new() -> Self {
        Self {
            connections: BTreeMap::new(),
            max_connections: 65536,
            current_tick: 0,
        }
    }

    /// Update the tick counter
    pub fn tick(&mut self) {
        self.current_tick = self.current_tick.wrapping_add(1);
    }

    /// Get current tick
    pub fn current_tick(&self) -> u64 {
        self.current_tick
    }

    /// Look up a connection by tuple
    pub fn lookup(&self, tuple: &ConnTuple) -> Option<&ConnEntry> {
        // Try original direction
        if let Some(entry) = self.connections.get(tuple) {
            return Some(entry);
        }

        // Try reply direction
        let reverse = tuple.reverse();
        self.connections.get(&reverse)
    }

    /// Look up a connection mutably
    pub fn lookup_mut(&mut self, tuple: &ConnTuple) -> Option<&mut ConnEntry> {
        // Try original direction first
        if self.connections.contains_key(tuple) {
            return self.connections.get_mut(tuple);
        }

        // Try reply direction
        let reverse = tuple.reverse();
        if self.connections.contains_key(&reverse) {
            return self.connections.get_mut(&reverse);
        }

        None
    }

    /// Check if tuple is in reply direction
    pub fn is_reply(&self, tuple: &ConnTuple) -> bool {
        !self.connections.contains_key(tuple) && self.connections.contains_key(&tuple.reverse())
    }

    /// Create or update a connection
    pub fn track(
        &mut self,
        tuple: ConnTuple,
        tcp_flags: Option<TcpFlags>,
        packet_len: usize,
    ) -> ConnState {
        let now = self.current_tick;

        // Check if this is a reply to existing connection
        let reverse = tuple.reverse();
        if let Some(entry) = self.connections.get_mut(&reverse) {
            // This is a reply packet
            entry.last_seen = now;
            entry.reply_seen = true;
            entry.packets_reply += 1;
            entry.bytes_reply += packet_len as u64;

            if let Some(flags) = tcp_flags {
                entry.update_tcp_state(flags, true);
            }

            return entry.conn_state();
        }

        // Check if we already have this connection
        if let Some(entry) = self.connections.get_mut(&tuple) {
            entry.last_seen = now;
            entry.packets_orig += 1;
            entry.bytes_orig += packet_len as u64;

            if let Some(flags) = tcp_flags {
                entry.update_tcp_state(flags, false);
            }

            return entry.conn_state();
        }

        // New connection - check if we have room
        if self.connections.len() >= self.max_connections {
            // Try to evict expired connections
            self.gc();

            if self.connections.len() >= self.max_connections {
                // Still full, reject
                return ConnState::Invalid;
            }
        }

        // Create new connection
        let mut entry = ConnEntry::new(tuple, now);
        entry.bytes_orig = packet_len as u64;

        if let Some(flags) = tcp_flags {
            // For new TCP connections, only allow SYN
            if !flags.syn || flags.ack {
                return ConnState::Invalid;
            }
        }

        let state = entry.conn_state();
        self.connections.insert(tuple, entry);

        state
    }

    /// Remove expired connections
    pub fn gc(&mut self) {
        let now = self.current_tick;
        self.connections.retain(|_, entry| !entry.is_expired(now));
    }

    /// Get number of tracked connections
    pub fn connection_count(&self) -> usize {
        self.connections.len()
    }

    /// Remove a specific connection
    pub fn remove(&mut self, tuple: &ConnTuple) -> Option<ConnEntry> {
        if let Some(entry) = self.connections.remove(tuple) {
            return Some(entry);
        }
        self.connections.remove(&tuple.reverse())
    }

    /// Iterate over all connections
    pub fn iter(&self) -> impl Iterator<Item = (&ConnTuple, &ConnEntry)> {
        self.connections.iter()
    }
}

impl Default for ConnTrackTable {
    fn default() -> Self {
        Self::new()
    }
}

/// Global connection tracking table
static CONNTRACK: RwLock<ConnTrackTable> = RwLock::new(ConnTrackTable::new());

/// Track a packet and get its connection state
pub fn track_packet(
    protocol: IpProtocol,
    src_ip: Ipv4Addr,
    src_port: u16,
    dst_ip: Ipv4Addr,
    dst_port: u16,
    tcp_flags: Option<TcpFlags>,
    packet_len: usize,
) -> ConnState {
    let tuple = ConnTuple::new(protocol, src_ip, src_port, dst_ip, dst_port);
    CONNTRACK.write().track(tuple, tcp_flags, packet_len)
}

/// Track an ICMP packet
pub fn track_icmp(
    src_ip: Ipv4Addr,
    dst_ip: Ipv4Addr,
    icmp_type: u8,
    icmp_code: u8,
    packet_len: usize,
) -> ConnState {
    let tuple = ConnTuple::new_icmp(src_ip, dst_ip, icmp_type, icmp_code);
    CONNTRACK.write().track(tuple, None, packet_len)
}

/// Look up connection state for a packet
pub fn lookup_state(
    protocol: IpProtocol,
    src_ip: Ipv4Addr,
    src_port: u16,
    dst_ip: Ipv4Addr,
    dst_port: u16,
) -> ConnState {
    let tuple = ConnTuple::new(protocol, src_ip, src_port, dst_ip, dst_port);
    CONNTRACK
        .read()
        .lookup(&tuple)
        .map(|e| e.conn_state())
        .unwrap_or(ConnState::New)
}

/// Update tick counter (call from timer interrupt)
pub fn tick() {
    CONNTRACK.write().tick();
}

/// Run garbage collection on expired connections
pub fn gc() {
    CONNTRACK.write().gc();
}

/// Get number of tracked connections
pub fn connection_count() -> usize {
    CONNTRACK.read().connection_count()
}

/// Remove a connection
pub fn remove_connection(
    protocol: IpProtocol,
    src_ip: Ipv4Addr,
    src_port: u16,
    dst_ip: Ipv4Addr,
    dst_port: u16,
) {
    let tuple = ConnTuple::new(protocol, src_ip, src_port, dst_ip, dst_port);
    CONNTRACK.write().remove(&tuple);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_conn_tuple_reverse() {
        let tuple = ConnTuple::new(
            IpProtocol::Tcp,
            Ipv4Addr::new(192, 168, 1, 1),
            12345,
            Ipv4Addr::new(10, 0, 0, 1),
            80,
        );

        let reverse = tuple.reverse();
        assert_eq!(reverse.src_ip, tuple.dst_ip);
        assert_eq!(reverse.dst_ip, tuple.src_ip);
        assert_eq!(reverse.src_port, tuple.dst_port);
        assert_eq!(reverse.dst_port, tuple.src_port);
    }

    #[test]
    fn test_tcp_tracking() {
        let mut table = ConnTrackTable::new();

        let tuple = ConnTuple::new(
            IpProtocol::Tcp,
            Ipv4Addr::new(192, 168, 1, 1),
            12345,
            Ipv4Addr::new(10, 0, 0, 1),
            80,
        );

        // SYN
        let syn = TcpFlags {
            syn: true,
            ack: false,
            fin: false,
            rst: false,
        };
        let state = table.track(tuple, Some(syn), 60);
        assert_eq!(state, ConnState::New);

        // SYN-ACK (reply)
        let syn_ack = TcpFlags {
            syn: true,
            ack: true,
            fin: false,
            rst: false,
        };
        let state = table.track(tuple.reverse(), Some(syn_ack), 60);
        assert_eq!(state, ConnState::Established);

        // ACK
        let ack = TcpFlags {
            syn: false,
            ack: true,
            fin: false,
            rst: false,
        };
        let state = table.track(tuple, Some(ack), 60);
        assert_eq!(state, ConnState::Established);
    }

    #[test]
    fn test_udp_tracking() {
        let mut table = ConnTrackTable::new();

        let tuple = ConnTuple::new(
            IpProtocol::Udp,
            Ipv4Addr::new(192, 168, 1, 1),
            12345,
            Ipv4Addr::new(8, 8, 8, 8),
            53,
        );

        // First packet (query)
        let state = table.track(tuple, None, 50);
        assert_eq!(state, ConnState::New);

        // Reply
        let state = table.track(tuple.reverse(), None, 100);
        assert_eq!(state, ConnState::Established);

        // Another query on same connection
        let state = table.track(tuple, None, 50);
        assert_eq!(state, ConnState::Established);
    }
}
