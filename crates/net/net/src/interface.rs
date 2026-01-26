//! Network Interface Management

use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use spin::Mutex;

use crate::{IpAddr, Ipv4Addr, Ipv6Addr, MacAddress, NetError, NetResult, NetworkDevice};

/// Interface configuration mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigMode {
    /// Static IP configuration
    Static,
    /// DHCP client
    Dhcp,
    /// Manual (no automatic configuration)
    Manual,
    /// Loopback interface
    Loopback,
}

impl Default for ConfigMode {
    fn default() -> Self {
        ConfigMode::Manual
    }
}

/// Interface configuration
#[derive(Debug, Clone)]
pub struct InterfaceConfig {
    /// Interface name
    pub name: String,
    /// Configuration mode
    pub mode: ConfigMode,
    /// IPv4 address
    pub ipv4_addr: Option<Ipv4Addr>,
    /// IPv4 netmask
    pub ipv4_netmask: Option<Ipv4Addr>,
    /// IPv4 broadcast
    pub ipv4_broadcast: Option<Ipv4Addr>,
    /// IPv4 gateway
    pub ipv4_gateway: Option<Ipv4Addr>,
    /// IPv6 addresses
    pub ipv6_addrs: Vec<Ipv6Addr>,
    /// MTU
    pub mtu: usize,
    /// DNS servers (up to 3)
    pub dns_servers: Vec<Ipv4Addr>,
}

impl Default for InterfaceConfig {
    fn default() -> Self {
        InterfaceConfig {
            name: String::new(),
            mode: ConfigMode::Manual,
            ipv4_addr: None,
            ipv4_netmask: None,
            ipv4_broadcast: None,
            ipv4_gateway: None,
            ipv6_addrs: Vec::new(),
            mtu: 1500,
            dns_servers: Vec::new(),
        }
    }
}

/// Network interface
pub struct NetworkInterface {
    /// Device
    pub device: Arc<dyn NetworkDevice>,
    /// Configuration
    config: Mutex<InterfaceConfig>,
}

impl NetworkInterface {
    /// Create a new network interface
    pub fn new(device: Arc<dyn NetworkDevice>) -> Self {
        let config = InterfaceConfig {
            name: String::from(device.name()),
            mtu: device.mtu(),
            ..Default::default()
        };

        NetworkInterface {
            device,
            config: Mutex::new(config),
        }
    }

    /// Get interface name
    pub fn name(&self) -> String {
        self.config.lock().name.clone()
    }

    /// Get MAC address
    pub fn mac_address(&self) -> MacAddress {
        self.device.mac_address()
    }

    /// Get MTU
    pub fn mtu(&self) -> usize {
        self.config.lock().mtu
    }

    /// Set MTU
    pub fn set_mtu(&self, mtu: usize) -> NetResult<()> {
        if mtu < 68 || mtu > 65535 {
            return Err(NetError::InvalidArgument);
        }
        self.config.lock().mtu = mtu;
        Ok(())
    }

    /// Get IPv4 address
    pub fn ipv4_addr(&self) -> Option<Ipv4Addr> {
        self.config.lock().ipv4_addr
    }

    /// Set IPv4 address
    pub fn set_ipv4_addr(&self, addr: Ipv4Addr, netmask: Ipv4Addr) -> NetResult<()> {
        let mut config = self.config.lock();
        config.ipv4_addr = Some(addr);
        config.ipv4_netmask = Some(netmask);

        // Calculate broadcast
        let addr_u32 = addr.to_u32();
        let mask_u32 = netmask.to_u32();
        let broadcast = Ipv4Addr::from_u32(addr_u32 | !mask_u32);
        config.ipv4_broadcast = Some(broadcast);

        Ok(())
    }

    /// Get IPv4 netmask
    pub fn ipv4_netmask(&self) -> Option<Ipv4Addr> {
        self.config.lock().ipv4_netmask
    }

    /// Get IPv4 gateway
    pub fn ipv4_gateway(&self) -> Option<Ipv4Addr> {
        self.config.lock().ipv4_gateway
    }

    /// Set IPv4 gateway
    pub fn set_ipv4_gateway(&self, gateway: Ipv4Addr) -> NetResult<()> {
        self.config.lock().ipv4_gateway = Some(gateway);
        Ok(())
    }

    /// Add IPv6 address
    pub fn add_ipv6_addr(&self, addr: Ipv6Addr) -> NetResult<()> {
        self.config.lock().ipv6_addrs.push(addr);
        Ok(())
    }

    /// Get IPv6 addresses
    pub fn ipv6_addrs(&self) -> Vec<Ipv6Addr> {
        self.config.lock().ipv6_addrs.clone()
    }

    /// Check if link is up
    pub fn link_up(&self) -> bool {
        self.device.link_up()
    }

    /// Get configuration
    pub fn config(&self) -> InterfaceConfig {
        self.config.lock().clone()
    }

    /// Check if address belongs to this interface
    pub fn has_addr(&self, addr: IpAddr) -> bool {
        let config = self.config.lock();
        match addr {
            IpAddr::V4(v4) => config.ipv4_addr == Some(v4),
            IpAddr::V6(v6) => config.ipv6_addrs.contains(&v6),
        }
    }

    /// Check if address is on the same network
    pub fn same_network(&self, addr: Ipv4Addr) -> bool {
        let config = self.config.lock();
        if let (Some(my_addr), Some(netmask)) = (config.ipv4_addr, config.ipv4_netmask) {
            let my_net = my_addr.to_u32() & netmask.to_u32();
            let their_net = addr.to_u32() & netmask.to_u32();
            my_net == their_net
        } else {
            false
        }
    }
}

/// Global interface list
static INTERFACES: Mutex<Vec<Arc<NetworkInterface>>> = Mutex::new(Vec::new());

/// Add an interface
pub fn add_interface(iface: Arc<NetworkInterface>) {
    INTERFACES.lock().push(iface);
}

/// Get interface by name
pub fn get_interface(name: &str) -> Option<Arc<NetworkInterface>> {
    INTERFACES.lock().iter().find(|i| i.name() == name).cloned()
}

/// Get interface by index
pub fn get_interface_by_index(index: usize) -> Option<Arc<NetworkInterface>> {
    INTERFACES.lock().get(index).cloned()
}

/// Get all interfaces
pub fn interfaces() -> Vec<Arc<NetworkInterface>> {
    INTERFACES.lock().clone()
}

/// Find interface for destination
pub fn find_route(dest: Ipv4Addr) -> Option<Arc<NetworkInterface>> {
    let ifaces = INTERFACES.lock();

    // Check if destination is on a directly connected network
    for iface in ifaces.iter() {
        if iface.same_network(dest) {
            return Some(iface.clone());
        }
    }

    // Use default gateway
    for iface in ifaces.iter() {
        if iface.ipv4_gateway().is_some() {
            return Some(iface.clone());
        }
    }

    None
}

// ============================================================================
// Network Configuration File Parsing
// ============================================================================

/// Config file path pattern
pub const NET_CONFIG_DIR: &str = "/etc/network";

/// Parse an interface configuration file
///
/// Configuration file format (e.g., /etc/network/eth0.conf):
/// ```text
/// # Interface configuration
/// mode=static|dhcp|manual
/// address=192.168.1.100
/// netmask=255.255.255.0
/// gateway=192.168.1.1
/// dns=8.8.8.8
/// dns=8.8.4.4
/// mtu=1500
/// ```
pub fn parse_config_file(content: &str) -> InterfaceConfig {
    let mut config = InterfaceConfig::default();

    for line in content.lines() {
        let line = line.trim();

        // Skip comments and empty lines
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        // Parse key=value pairs
        if let Some((key, value)) = line.split_once('=') {
            let key = key.trim();
            let value = value.trim();

            // Case-insensitive key matching
            let key_lower: String = key.chars().map(|c| c.to_ascii_lowercase()).collect();

            match key_lower.as_str() {
                "mode" => {
                    let value_lower: String =
                        value.chars().map(|c| c.to_ascii_lowercase()).collect();
                    config.mode = match value_lower.as_str() {
                        "static" => ConfigMode::Static,
                        "dhcp" => ConfigMode::Dhcp,
                        "loopback" => ConfigMode::Loopback,
                        _ => ConfigMode::Manual,
                    };
                }
                "address" | "ip" | "ipaddr" => {
                    if let Some(addr) = parse_ipv4(value) {
                        config.ipv4_addr = Some(addr);
                    }
                }
                "netmask" | "mask" => {
                    if let Some(addr) = parse_ipv4(value) {
                        config.ipv4_netmask = Some(addr);
                    }
                }
                "gateway" | "gw" => {
                    if let Some(addr) = parse_ipv4(value) {
                        config.ipv4_gateway = Some(addr);
                    }
                }
                "dns" | "nameserver" => {
                    if let Some(addr) = parse_ipv4(value) {
                        if config.dns_servers.len() < 3 {
                            config.dns_servers.push(addr);
                        }
                    }
                }
                "mtu" => {
                    if let Some(mtu) = parse_usize(value) {
                        if mtu >= 68 && mtu <= 65535 {
                            config.mtu = mtu;
                        }
                    }
                }
                "broadcast" => {
                    if let Some(addr) = parse_ipv4(value) {
                        config.ipv4_broadcast = Some(addr);
                    }
                }
                _ => {}
            }
        }
    }

    // Calculate broadcast if not specified
    if config.ipv4_broadcast.is_none() {
        if let (Some(addr), Some(mask)) = (config.ipv4_addr, config.ipv4_netmask) {
            let broadcast = Ipv4Addr::from_u32(addr.to_u32() | !mask.to_u32());
            config.ipv4_broadcast = Some(broadcast);
        }
    }

    config
}

/// Parse IPv4 address from string
fn parse_ipv4(s: &str) -> Option<Ipv4Addr> {
    let mut octets = [0u8; 4];
    let mut idx = 0;
    let mut current: u16 = 0;
    let mut has_digit = false;

    for c in s.bytes() {
        if c == b'.' {
            if !has_digit || idx >= 3 || current > 255 {
                return None;
            }
            octets[idx] = current as u8;
            idx += 1;
            current = 0;
            has_digit = false;
        } else if c.is_ascii_digit() {
            current = current * 10 + (c - b'0') as u16;
            has_digit = true;
            if current > 255 {
                return None;
            }
        } else {
            return None;
        }
    }

    if !has_digit || idx != 3 || current > 255 {
        return None;
    }
    octets[idx] = current as u8;

    Some(Ipv4Addr::new(octets[0], octets[1], octets[2], octets[3]))
}

/// Parse usize from string
fn parse_usize(s: &str) -> Option<usize> {
    let mut val: usize = 0;
    for c in s.bytes() {
        if c.is_ascii_digit() {
            val = val.checked_mul(10)?;
            val = val.checked_add((c - b'0') as usize)?;
        } else {
            return None;
        }
    }
    Some(val)
}

/// Format configuration to write to file
pub fn format_config_file(config: &InterfaceConfig) -> String {
    let mut output = String::new();

    output.push_str("# Network interface configuration\n");
    output.push_str("# Auto-generated by OXIDE networkd\n\n");

    // Mode
    let mode_str = match config.mode {
        ConfigMode::Static => "static",
        ConfigMode::Dhcp => "dhcp",
        ConfigMode::Manual => "manual",
        ConfigMode::Loopback => "loopback",
    };
    output.push_str(&alloc::format!("mode={}\n", mode_str));

    // IP configuration
    if let Some(addr) = config.ipv4_addr {
        output.push_str(&alloc::format!("address={}\n", addr));
    }
    if let Some(mask) = config.ipv4_netmask {
        output.push_str(&alloc::format!("netmask={}\n", mask));
    }
    if let Some(gw) = config.ipv4_gateway {
        output.push_str(&alloc::format!("gateway={}\n", gw));
    }
    if let Some(bcast) = config.ipv4_broadcast {
        output.push_str(&alloc::format!("broadcast={}\n", bcast));
    }

    // DNS servers
    for dns in &config.dns_servers {
        output.push_str(&alloc::format!("dns={}\n", dns));
    }

    // MTU
    output.push_str(&alloc::format!("mtu={}\n", config.mtu));

    output
}
