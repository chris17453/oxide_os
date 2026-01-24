//! Firewall syscall handlers
//!
//! Provides syscalls for managing packet filter rules:
//! - FW_ADD_RULE: Add a filter rule
//! - FW_DEL_RULE: Delete a filter rule by index
//! - FW_LIST_RULES: List all rules (copy to userspace buffer)
//! - FW_SET_POLICY: Set default policy for a chain
//! - FW_FLUSH: Flush all rules from a chain
//! - FW_GET_CONNTRACK: Get connection tracking info

use crate::errno;
use net::Ipv4Addr;
use os_core::VirtAddr;
use proc::process_table;
use tcpip::{
    add_rule, connection_count, delete_rule, flush_chain, get_policy, rule_count, set_policy,
    with_rules, ConnState, FilterChain, FilterRule, FilterVerdict, IpMatch, IpProtocol, PortMatch,
};

/// Rule structure for userspace communication
#[repr(C)]
#[derive(Clone, Copy)]
pub struct FwRule {
    /// Chain: 0=Input, 1=Output, 2=Forward
    pub chain: u8,
    /// Action: 0=Accept, 1=Drop, 2=Reject
    pub action: u8,
    /// Protocol: 0=Any, 1=ICMP, 6=TCP, 17=UDP
    pub protocol: u8,
    /// Connection state: 0=Any, 1=New, 2=Established, 3=Related, 4=Invalid
    pub state: u8,
    /// Source IP address (network byte order)
    pub src_ip: u32,
    /// Source IP prefix length (0-32)
    pub src_prefix: u8,
    /// Destination IP address (network byte order)
    pub dst_ip: u32,
    /// Destination IP prefix length (0-32)
    pub dst_prefix: u8,
    /// Source port start
    pub src_port_start: u16,
    /// Source port end
    pub src_port_end: u16,
    /// Destination port start
    pub dst_port_start: u16,
    /// Destination port end
    pub dst_port_end: u16,
    /// Padding for alignment
    pub _pad: [u8; 2],
}

impl FwRule {
    /// Convert to internal FilterRule
    pub fn to_filter_rule(&self) -> FilterRule {
        let chain = match self.chain {
            0 => FilterChain::Input,
            1 => FilterChain::Output,
            2 => FilterChain::Forward,
            _ => FilterChain::Input,
        };

        let action = match self.action {
            0 => FilterVerdict::Accept,
            1 => FilterVerdict::Drop,
            2 => FilterVerdict::Reject,
            _ => FilterVerdict::Drop,
        };

        let mut rule = FilterRule::new(chain, action);

        // Protocol
        if self.protocol != 0 {
            rule.protocol = Some(IpProtocol::from(self.protocol));
        }

        // Source IP
        if self.src_prefix > 0 {
            let bytes = self.src_ip.to_be_bytes();
            rule.src_ip = Some(IpMatch::new(
                Ipv4Addr::new(bytes[0], bytes[1], bytes[2], bytes[3]),
                self.src_prefix,
            ));
        }

        // Destination IP
        if self.dst_prefix > 0 {
            let bytes = self.dst_ip.to_be_bytes();
            rule.dst_ip = Some(IpMatch::new(
                Ipv4Addr::new(bytes[0], bytes[1], bytes[2], bytes[3]),
                self.dst_prefix,
            ));
        }

        // Source port
        if self.src_port_start > 0 || self.src_port_end > 0 {
            rule.src_port = Some(PortMatch::range(self.src_port_start, self.src_port_end));
        }

        // Destination port
        if self.dst_port_start > 0 || self.dst_port_end > 0 {
            rule.dst_port = Some(PortMatch::range(self.dst_port_start, self.dst_port_end));
        }

        // Connection state
        if self.state > 0 {
            rule.state = Some(match self.state {
                1 => ConnState::New,
                2 => ConnState::Established,
                3 => ConnState::Related,
                4 => ConnState::Invalid,
                _ => ConnState::New,
            });
        }

        rule
    }

    /// Convert from internal FilterRule
    pub fn from_filter_rule(rule: &FilterRule) -> Self {
        let chain = match rule.chain {
            FilterChain::Input => 0,
            FilterChain::Output => 1,
            FilterChain::Forward => 2,
        };

        let action = match rule.action {
            FilterVerdict::Accept => 0,
            FilterVerdict::Drop => 1,
            FilterVerdict::Reject => 2,
        };

        let protocol = rule.protocol.map(|p| u8::from(p)).unwrap_or(0);

        let (src_ip, src_prefix) = rule.src_ip.as_ref().map(|m| {
            let bytes = m.addr.as_bytes();
            (u32::from_be_bytes(*bytes), m.prefix)
        }).unwrap_or((0, 0));

        let (dst_ip, dst_prefix) = rule.dst_ip.as_ref().map(|m| {
            let bytes = m.addr.as_bytes();
            (u32::from_be_bytes(*bytes), m.prefix)
        }).unwrap_or((0, 0));

        let (src_port_start, src_port_end) = rule.src_port.as_ref()
            .map(|p| (p.start, p.end))
            .unwrap_or((0, 0));

        let (dst_port_start, dst_port_end) = rule.dst_port.as_ref()
            .map(|p| (p.start, p.end))
            .unwrap_or((0, 0));

        let state = rule.state.map(|s| match s {
            ConnState::New => 1,
            ConnState::Established => 2,
            ConnState::Related => 3,
            ConnState::Invalid => 4,
        }).unwrap_or(0);

        FwRule {
            chain,
            action,
            protocol,
            state,
            src_ip,
            src_prefix,
            dst_ip,
            dst_prefix,
            src_port_start,
            src_port_end,
            dst_port_start,
            dst_port_end,
            _pad: [0; 2],
        }
    }
}

/// Check if current process is root (UID 0)
fn is_root() -> bool {
    let table = process_table();
    if let Some(current) = table.current() {
        current.lock().credentials().uid == 0
    } else {
        false
    }
}

/// Add a firewall rule
///
/// Arguments:
/// - rule_ptr: Pointer to FwRule structure
///
/// Returns: 0 on success, negative errno on error
pub fn sys_fw_add_rule(rule_ptr: VirtAddr) -> i64 {
    // Check root permission
    if !is_root() {
        return errno::EPERM;
    }

    // Read rule from userspace
    let rule_data = unsafe {
        let ptr = rule_ptr.as_ptr::<FwRule>();
        if ptr.is_null() {
            return errno::EFAULT;
        }
        *ptr
    };

    // Convert and add rule
    let rule = rule_data.to_filter_rule();
    add_rule(rule);

    0
}

/// Delete a firewall rule by index
///
/// Arguments:
/// - index: Rule index to delete
///
/// Returns: 0 on success, negative errno on error
pub fn sys_fw_del_rule(index: usize) -> i64 {
    if !is_root() {
        return errno::EPERM;
    }

    match delete_rule(index) {
        Some(_) => 0,
        None => errno::EINVAL,
    }
}

/// List firewall rules
///
/// Arguments:
/// - buf_ptr: Pointer to buffer for FwRule array
/// - buf_len: Maximum number of rules to return
///
/// Returns: Number of rules copied, or negative errno on error
pub fn sys_fw_list_rules(buf_ptr: VirtAddr, buf_len: usize) -> i64 {
    if buf_ptr.as_u64() == 0 {
        // Just return count
        return rule_count() as i64;
    }

    let buf = unsafe {
        let ptr = buf_ptr.as_mut_ptr::<FwRule>();
        if ptr.is_null() {
            return errno::EFAULT;
        }
        core::slice::from_raw_parts_mut(ptr, buf_len)
    };

    with_rules(|rules| {
        let count = rules.len().min(buf_len);
        for (i, rule) in rules.iter().take(count).enumerate() {
            buf[i] = FwRule::from_filter_rule(rule);
        }
        count as i64
    })
}

/// Set chain policy
///
/// Arguments:
/// - chain: 0=Input, 1=Output, 2=Forward
/// - policy: 0=Accept, 1=Drop, 2=Reject
///
/// Returns: 0 on success, negative errno on error
pub fn sys_fw_set_policy(chain: u8, policy: u8) -> i64 {
    if !is_root() {
        return errno::EPERM;
    }

    let chain = match chain {
        0 => FilterChain::Input,
        1 => FilterChain::Output,
        2 => FilterChain::Forward,
        _ => return errno::EINVAL,
    };

    let policy = match policy {
        0 => FilterVerdict::Accept,
        1 => FilterVerdict::Drop,
        2 => FilterVerdict::Reject,
        _ => return errno::EINVAL,
    };

    set_policy(chain, policy);
    0
}

/// Flush rules from a chain
///
/// Arguments:
/// - chain: 0=Input, 1=Output, 2=Forward, 255=All
///
/// Returns: 0 on success, negative errno on error
pub fn sys_fw_flush(chain: u8) -> i64 {
    if !is_root() {
        return errno::EPERM;
    }

    if chain == 255 {
        tcpip::flush_all();
    } else {
        let chain = match chain {
            0 => FilterChain::Input,
            1 => FilterChain::Output,
            2 => FilterChain::Forward,
            _ => return errno::EINVAL,
        };
        flush_chain(chain);
    }

    0
}

/// Get connection tracking statistics
///
/// Arguments:
/// - stats_ptr: Pointer to ConntrackStats structure (optional, can be NULL)
///
/// Returns: Number of tracked connections, or negative errno on error
pub fn sys_fw_get_conntrack(stats_ptr: VirtAddr) -> i64 {
    // Connection tracking info is readable by anyone
    let count = connection_count();

    if stats_ptr.as_u64() != 0 {
        // If buffer provided, write stats
        #[repr(C)]
        struct ConntrackStats {
            count: u64,
            max: u64,
        }

        let stats = unsafe {
            let ptr = stats_ptr.as_mut_ptr::<ConntrackStats>();
            if ptr.is_null() {
                return errno::EFAULT;
            }
            &mut *ptr
        };

        stats.count = count as u64;
        stats.max = 65536; // MAX_CONNECTIONS from conntrack
    }

    count as i64
}
