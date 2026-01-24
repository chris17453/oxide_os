//! Packet filtering (firewall) for OXIDE OS
//!
//! Implements stateful packet filtering with support for:
//! - INPUT/OUTPUT chains (FORWARD for routing later)
//! - Protocol matching (TCP, UDP, ICMP)
//! - IP address matching with CIDR prefixes
//! - Port matching (single port or range)
//! - Connection state tracking

extern crate alloc;

use alloc::vec::Vec;
use net::Ipv4Addr;
use spin::RwLock;

use crate::ip::IpProtocol;

/// Filter verdict - what to do with a packet
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterVerdict {
    /// Accept the packet
    Accept,
    /// Silently drop the packet
    Drop,
    /// Drop and send ICMP error (for TCP: RST)
    Reject,
}

/// Filter chain - where in the packet flow to apply rules
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterChain {
    /// Incoming packets destined for local processes
    Input,
    /// Outgoing packets originating from local processes
    Output,
    /// Packets being routed through (not yet implemented)
    Forward,
}

/// Connection state for stateful filtering
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnState {
    /// New connection (first packet)
    New,
    /// Established connection (seen reply)
    Established,
    /// Related connection (e.g., ICMP error for existing connection)
    Related,
    /// Invalid packet
    Invalid,
}

/// IP address match specification
#[derive(Debug, Clone, Copy)]
pub struct IpMatch {
    /// IP address
    pub addr: Ipv4Addr,
    /// Prefix length (0-32, 32 = exact match)
    pub prefix: u8,
}

impl IpMatch {
    /// Create a new IP match
    pub fn new(addr: Ipv4Addr, prefix: u8) -> Self {
        Self {
            addr,
            prefix: prefix.min(32),
        }
    }

    /// Match any IP address
    pub fn any() -> Self {
        Self {
            addr: Ipv4Addr::new(0, 0, 0, 0),
            prefix: 0,
        }
    }

    /// Check if an IP address matches
    pub fn matches(&self, ip: Ipv4Addr) -> bool {
        if self.prefix == 0 {
            return true;
        }

        let mask = if self.prefix >= 32 {
            u32::MAX
        } else {
            u32::MAX << (32 - self.prefix)
        };

        let self_bits = u32::from_be_bytes(*self.addr.as_bytes());
        let ip_bits = u32::from_be_bytes(*ip.as_bytes());

        (self_bits & mask) == (ip_bits & mask)
    }
}

/// Port range match specification
#[derive(Debug, Clone, Copy)]
pub struct PortMatch {
    /// Start port (inclusive)
    pub start: u16,
    /// End port (inclusive)
    pub end: u16,
}

impl PortMatch {
    /// Match a single port
    pub fn single(port: u16) -> Self {
        Self {
            start: port,
            end: port,
        }
    }

    /// Match a range of ports
    pub fn range(start: u16, end: u16) -> Self {
        Self { start, end }
    }

    /// Match any port
    pub fn any() -> Self {
        Self {
            start: 0,
            end: 65535,
        }
    }

    /// Check if a port matches
    pub fn matches(&self, port: u16) -> bool {
        port >= self.start && port <= self.end
    }
}

/// A single filter rule
#[derive(Debug, Clone)]
pub struct FilterRule {
    /// Which chain this rule belongs to
    pub chain: FilterChain,
    /// Action to take if rule matches
    pub action: FilterVerdict,
    /// Protocol to match (None = any)
    pub protocol: Option<IpProtocol>,
    /// Source IP to match (None = any)
    pub src_ip: Option<IpMatch>,
    /// Destination IP to match (None = any)
    pub dst_ip: Option<IpMatch>,
    /// Source port to match (None = any, only for TCP/UDP)
    pub src_port: Option<PortMatch>,
    /// Destination port to match (None = any, only for TCP/UDP)
    pub dst_port: Option<PortMatch>,
    /// Connection state to match (None = any)
    pub state: Option<ConnState>,
    /// Rule description/comment
    pub comment: Option<&'static str>,
}

impl FilterRule {
    /// Create a new rule with all fields set to "any"
    pub fn new(chain: FilterChain, action: FilterVerdict) -> Self {
        Self {
            chain,
            action,
            protocol: None,
            src_ip: None,
            dst_ip: None,
            src_port: None,
            dst_port: None,
            state: None,
            comment: None,
        }
    }

    /// Set protocol match
    pub fn protocol(mut self, proto: IpProtocol) -> Self {
        self.protocol = Some(proto);
        self
    }

    /// Set source IP match
    pub fn src_ip(mut self, addr: Ipv4Addr, prefix: u8) -> Self {
        self.src_ip = Some(IpMatch::new(addr, prefix));
        self
    }

    /// Set destination IP match
    pub fn dst_ip(mut self, addr: Ipv4Addr, prefix: u8) -> Self {
        self.dst_ip = Some(IpMatch::new(addr, prefix));
        self
    }

    /// Set source port match (single)
    pub fn src_port(mut self, port: u16) -> Self {
        self.src_port = Some(PortMatch::single(port));
        self
    }

    /// Set source port range
    pub fn src_port_range(mut self, start: u16, end: u16) -> Self {
        self.src_port = Some(PortMatch::range(start, end));
        self
    }

    /// Set destination port match (single)
    pub fn dst_port(mut self, port: u16) -> Self {
        self.dst_port = Some(PortMatch::single(port));
        self
    }

    /// Set destination port range
    pub fn dst_port_range(mut self, start: u16, end: u16) -> Self {
        self.dst_port = Some(PortMatch::range(start, end));
        self
    }

    /// Set connection state match
    pub fn state(mut self, state: ConnState) -> Self {
        self.state = Some(state);
        self
    }

    /// Set comment
    pub fn comment(mut self, comment: &'static str) -> Self {
        self.comment = Some(comment);
        self
    }
}

/// Packet information for matching
#[derive(Debug, Clone)]
pub struct PacketInfo {
    /// Source IP address
    pub src_ip: Ipv4Addr,
    /// Destination IP address
    pub dst_ip: Ipv4Addr,
    /// Protocol
    pub protocol: IpProtocol,
    /// Source port (if TCP/UDP)
    pub src_port: Option<u16>,
    /// Destination port (if TCP/UDP)
    pub dst_port: Option<u16>,
    /// ICMP type (if ICMP)
    pub icmp_type: Option<u8>,
    /// Connection state
    pub state: ConnState,
}

impl PacketInfo {
    /// Create packet info for an IP packet
    pub fn new(src_ip: Ipv4Addr, dst_ip: Ipv4Addr, protocol: IpProtocol) -> Self {
        Self {
            src_ip,
            dst_ip,
            protocol,
            src_port: None,
            dst_port: None,
            icmp_type: None,
            state: ConnState::New,
        }
    }

    /// Set ports (for TCP/UDP)
    pub fn with_ports(mut self, src_port: u16, dst_port: u16) -> Self {
        self.src_port = Some(src_port);
        self.dst_port = Some(dst_port);
        self
    }

    /// Set ICMP type
    pub fn with_icmp_type(mut self, icmp_type: u8) -> Self {
        self.icmp_type = Some(icmp_type);
        self
    }

    /// Set connection state
    pub fn with_state(mut self, state: ConnState) -> Self {
        self.state = state;
        self
    }
}

/// Check if a rule matches a packet
fn rule_matches(rule: &FilterRule, pkt: &PacketInfo) -> bool {
    // Check protocol
    if let Some(proto) = rule.protocol {
        if proto != pkt.protocol {
            return false;
        }
    }

    // Check source IP
    if let Some(ref ip_match) = rule.src_ip {
        if !ip_match.matches(pkt.src_ip) {
            return false;
        }
    }

    // Check destination IP
    if let Some(ref ip_match) = rule.dst_ip {
        if !ip_match.matches(pkt.dst_ip) {
            return false;
        }
    }

    // Check source port (only for TCP/UDP)
    if let Some(ref port_match) = rule.src_port {
        match pkt.src_port {
            Some(port) => {
                if !port_match.matches(port) {
                    return false;
                }
            }
            None => return false, // Rule requires port but packet doesn't have one
        }
    }

    // Check destination port (only for TCP/UDP)
    if let Some(ref port_match) = rule.dst_port {
        match pkt.dst_port {
            Some(port) => {
                if !port_match.matches(port) {
                    return false;
                }
            }
            None => return false, // Rule requires port but packet doesn't have one
        }
    }

    // Check connection state
    if let Some(state) = rule.state {
        if state != pkt.state {
            return false;
        }
    }

    true
}

/// Filter table containing all rules and policies
pub struct FilterTable {
    /// Rules (evaluated in order)
    rules: Vec<FilterRule>,
    /// Default policy for INPUT chain
    input_policy: FilterVerdict,
    /// Default policy for OUTPUT chain
    output_policy: FilterVerdict,
    /// Default policy for FORWARD chain
    forward_policy: FilterVerdict,
}

impl Default for FilterTable {
    fn default() -> Self {
        Self::new()
    }
}

impl FilterTable {
    /// Create a new filter table with ACCEPT policies
    pub fn new() -> Self {
        Self {
            rules: Vec::new(),
            input_policy: FilterVerdict::Accept,
            output_policy: FilterVerdict::Accept,
            forward_policy: FilterVerdict::Drop,
        }
    }

    /// Add a rule to the table
    pub fn add_rule(&mut self, rule: FilterRule) {
        self.rules.push(rule);
    }

    /// Insert a rule at a specific position
    pub fn insert_rule(&mut self, index: usize, rule: FilterRule) {
        if index <= self.rules.len() {
            self.rules.insert(index, rule);
        } else {
            self.rules.push(rule);
        }
    }

    /// Delete a rule by index
    pub fn delete_rule(&mut self, index: usize) -> Option<FilterRule> {
        if index < self.rules.len() {
            Some(self.rules.remove(index))
        } else {
            None
        }
    }

    /// Flush all rules from a chain
    pub fn flush_chain(&mut self, chain: FilterChain) {
        self.rules.retain(|r| r.chain != chain);
    }

    /// Flush all rules
    pub fn flush_all(&mut self) {
        self.rules.clear();
    }

    /// Set default policy for a chain
    pub fn set_policy(&mut self, chain: FilterChain, policy: FilterVerdict) {
        match chain {
            FilterChain::Input => self.input_policy = policy,
            FilterChain::Output => self.output_policy = policy,
            FilterChain::Forward => self.forward_policy = policy,
        }
    }

    /// Get default policy for a chain
    pub fn get_policy(&self, chain: FilterChain) -> FilterVerdict {
        match chain {
            FilterChain::Input => self.input_policy,
            FilterChain::Output => self.output_policy,
            FilterChain::Forward => self.forward_policy,
        }
    }

    /// Get all rules
    pub fn rules(&self) -> &[FilterRule] {
        &self.rules
    }

    /// Get number of rules
    pub fn rule_count(&self) -> usize {
        self.rules.len()
    }

    /// Evaluate the filter table for a packet
    pub fn filter(&self, chain: FilterChain, pkt: &PacketInfo) -> FilterVerdict {
        // Evaluate rules in order
        for rule in &self.rules {
            if rule.chain != chain {
                continue;
            }

            if rule_matches(rule, pkt) {
                return rule.action;
            }
        }

        // No rule matched, use default policy
        self.get_policy(chain)
    }
}

/// Global filter table
static FILTER_TABLE: RwLock<FilterTable> = RwLock::new(FilterTable {
    rules: Vec::new(),
    input_policy: FilterVerdict::Accept,
    output_policy: FilterVerdict::Accept,
    forward_policy: FilterVerdict::Drop,
});

/// Filter an incoming packet (INPUT chain)
pub fn filter_input(pkt: &PacketInfo) -> FilterVerdict {
    FILTER_TABLE.read().filter(FilterChain::Input, pkt)
}

/// Filter an outgoing packet (OUTPUT chain)
pub fn filter_output(pkt: &PacketInfo) -> FilterVerdict {
    FILTER_TABLE.read().filter(FilterChain::Output, pkt)
}

/// Filter a forwarded packet (FORWARD chain)
pub fn filter_forward(pkt: &PacketInfo) -> FilterVerdict {
    FILTER_TABLE.read().filter(FilterChain::Forward, pkt)
}

/// Add a rule to the global filter table
pub fn add_rule(rule: FilterRule) {
    FILTER_TABLE.write().add_rule(rule);
}

/// Insert a rule at a specific position
pub fn insert_rule(index: usize, rule: FilterRule) {
    FILTER_TABLE.write().insert_rule(index, rule);
}

/// Delete a rule by index
pub fn delete_rule(index: usize) -> Option<FilterRule> {
    FILTER_TABLE.write().delete_rule(index)
}

/// Flush a chain
pub fn flush_chain(chain: FilterChain) {
    FILTER_TABLE.write().flush_chain(chain);
}

/// Flush all rules
pub fn flush_all() {
    FILTER_TABLE.write().flush_all();
}

/// Set chain policy
pub fn set_policy(chain: FilterChain, policy: FilterVerdict) {
    FILTER_TABLE.write().set_policy(chain, policy);
}

/// Get chain policy
pub fn get_policy(chain: FilterChain) -> FilterVerdict {
    FILTER_TABLE.read().get_policy(chain)
}

/// Get total rule count
pub fn rule_count() -> usize {
    FILTER_TABLE.read().rule_count()
}

/// Access the filter table for reading rules
pub fn with_rules<F, R>(f: F) -> R
where
    F: FnOnce(&[FilterRule]) -> R,
{
    f(FILTER_TABLE.read().rules())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ip_match() {
        let any = IpMatch::any();
        assert!(any.matches(Ipv4Addr::new(192, 168, 1, 1)));
        assert!(any.matches(Ipv4Addr::new(10, 0, 0, 1)));

        let exact = IpMatch::new(Ipv4Addr::new(192, 168, 1, 1), 32);
        assert!(exact.matches(Ipv4Addr::new(192, 168, 1, 1)));
        assert!(!exact.matches(Ipv4Addr::new(192, 168, 1, 2)));

        let subnet = IpMatch::new(Ipv4Addr::new(192, 168, 1, 0), 24);
        assert!(subnet.matches(Ipv4Addr::new(192, 168, 1, 1)));
        assert!(subnet.matches(Ipv4Addr::new(192, 168, 1, 254)));
        assert!(!subnet.matches(Ipv4Addr::new(192, 168, 2, 1)));
    }

    #[test]
    fn test_port_match() {
        let single = PortMatch::single(22);
        assert!(single.matches(22));
        assert!(!single.matches(23));

        let range = PortMatch::range(1024, 65535);
        assert!(!range.matches(80));
        assert!(range.matches(1024));
        assert!(range.matches(8080));
    }

    #[test]
    fn test_filter_rule() {
        let mut table = FilterTable::new();

        // Allow SSH
        table.add_rule(
            FilterRule::new(FilterChain::Input, FilterVerdict::Accept)
                .protocol(IpProtocol::Tcp)
                .dst_port(22),
        );

        // Drop everything else
        table.set_policy(FilterChain::Input, FilterVerdict::Drop);

        // SSH should be accepted
        let ssh_pkt = PacketInfo::new(
            Ipv4Addr::new(10, 0, 0, 1),
            Ipv4Addr::new(192, 168, 1, 1),
            IpProtocol::Tcp,
        )
        .with_ports(12345, 22);
        assert_eq!(
            table.filter(FilterChain::Input, &ssh_pkt),
            FilterVerdict::Accept
        );

        // HTTP should be dropped
        let http_pkt = PacketInfo::new(
            Ipv4Addr::new(10, 0, 0, 1),
            Ipv4Addr::new(192, 168, 1, 1),
            IpProtocol::Tcp,
        )
        .with_ports(12345, 80);
        assert_eq!(
            table.filter(FilterChain::Input, &http_pkt),
            FilterVerdict::Drop
        );
    }
}
