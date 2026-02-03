//! Network Adapter Management Module
//!
//! Handles individual network adapter lifecycle, configuration, and state.
//! Persona: ShadePacket - Networking stack engineer

use alloc::string::String;
use alloc::vec::Vec;
use libc::*;

/// Adapter state tracking
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdapterState {
    /// Adapter is down (not configured)
    Down,
    /// Adapter is being configured
    Configuring,
    /// Adapter is up and running
    Up,
    /// Adapter is in error state
    Error,
    /// Adapter is being removed
    Removing,
}

/// Configuration mode for adapters
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ConfigMode {
    Static,
    Dhcp,
    Manual,
    Loopback,
}

/// Network adapter representation
pub struct NetworkAdapter {
    /// Interface name (e.g., "eth0", "wlan0")
    pub name: String,
    /// MAC address as bytes
    pub mac: [u8; 6],
    /// Current adapter state
    pub state: AdapterState,
    /// Configuration mode
    pub mode: ConfigMode,
    /// IPv4 address (if configured)
    pub ipv4_addr: Option<[u8; 4]>,
    /// IPv4 netmask
    pub ipv4_netmask: Option<[u8; 4]>,
    /// IPv4 gateway
    pub ipv4_gateway: Option<[u8; 4]>,
    /// DNS servers (up to 3)
    pub dns_servers: Vec<[u8; 4]>,
    /// MTU (Maximum Transmission Unit)
    pub mtu: u32,
    /// Link status (true = up, false = down)
    pub link_up: bool,
    /// RX packets count
    pub rx_packets: u64,
    /// TX packets count
    pub tx_packets: u64,
    /// RX bytes count
    pub rx_bytes: u64,
    /// TX bytes count
    pub tx_bytes: u64,
}

impl NetworkAdapter {
    /// Create a new network adapter with default configuration
    pub fn new(name: String) -> Self {
        NetworkAdapter {
            name,
            mac: [0; 6],
            state: AdapterState::Down,
            mode: ConfigMode::Manual,
            ipv4_addr: None,
            ipv4_netmask: Some([255, 255, 255, 0]),
            ipv4_gateway: None,
            dns_servers: Vec::new(),
            mtu: 1500,
            link_up: false,
            rx_packets: 0,
            tx_packets: 0,
            rx_bytes: 0,
            tx_bytes: 0,
        }
    }

    /// Check if adapter is configured with an IP address
    pub fn is_configured(&self) -> bool {
        self.ipv4_addr.is_some()
    }

    /// Set IPv4 configuration
    pub fn set_ipv4(&mut self, addr: [u8; 4], netmask: [u8; 4]) {
        self.ipv4_addr = Some(addr);
        self.ipv4_netmask = Some(netmask);
    }

    /// Set gateway
    pub fn set_gateway(&mut self, gateway: [u8; 4]) {
        self.ipv4_gateway = Some(gateway);
    }

    /// Add DNS server
    pub fn add_dns(&mut self, dns: [u8; 4]) {
        // ShadePacket: Limit to 3 DNS servers max - keeps resolv.conf clean
        if self.dns_servers.len() < 3 && !self.dns_servers.contains(&dns) {
            self.dns_servers.push(dns);
        }
    }

    /// Clear DNS servers
    pub fn clear_dns(&mut self) {
        self.dns_servers.clear();
    }

    /// Get adapter name as a C-compatible string
    pub fn name_cstr(&self) -> &str {
        &self.name
    }

    /// Transition to a new state
    pub fn set_state(&mut self, new_state: AdapterState) {
        self.state = new_state;
    }

    /// Read link status from sysfs (if available)
    pub fn update_link_status(&mut self) -> bool {
        // ShadePacket: Read from /sys/class/net/<iface>/carrier
        let mut path_buf = [0u8; 128];
        let prefix = b"/sys/class/net/";
        let suffix = b"/carrier";
        let name_bytes = self.name.as_bytes();

        if prefix.len() + name_bytes.len() + suffix.len() >= 128 {
            return self.link_up;
        }

        path_buf[..prefix.len()].copy_from_slice(prefix);
        path_buf[prefix.len()..prefix.len() + name_bytes.len()].copy_from_slice(name_bytes);
        path_buf[prefix.len() + name_bytes.len()..prefix.len() + name_bytes.len() + suffix.len()]
            .copy_from_slice(suffix);
        let path_len = prefix.len() + name_bytes.len() + suffix.len();

        if let Ok(path_str) = core::str::from_utf8(&path_buf[..path_len]) {
            let fd = open2(path_str, O_RDONLY);
            if fd >= 0 {
                let mut buf = [0u8; 2];
                let n = read(fd, &mut buf);
                close(fd);

                if n > 0 && buf[0] == b'1' {
                    self.link_up = true;
                } else {
                    self.link_up = false;
                }
            }
        }

        self.link_up
    }

    /// Read MAC address from sysfs
    pub fn update_mac_address(&mut self) -> bool {
        // ShadePacket: Pull MAC from /sys/class/net/<iface>/address
        let mut path_buf = [0u8; 128];
        let prefix = b"/sys/class/net/";
        let suffix = b"/address";
        let name_bytes = self.name.as_bytes();

        if prefix.len() + name_bytes.len() + suffix.len() >= 128 {
            return false;
        }

        path_buf[..prefix.len()].copy_from_slice(prefix);
        path_buf[prefix.len()..prefix.len() + name_bytes.len()].copy_from_slice(name_bytes);
        path_buf[prefix.len() + name_bytes.len()..prefix.len() + name_bytes.len() + suffix.len()]
            .copy_from_slice(suffix);
        let path_len = prefix.len() + name_bytes.len() + suffix.len();

        if let Ok(path_str) = core::str::from_utf8(&path_buf[..path_len]) {
            let fd = open2(path_str, O_RDONLY);
            if fd >= 0 {
                let mut buf = [0u8; 32];
                let n = read(fd, &mut buf);
                close(fd);

                if n >= 17 {
                    // Parse MAC address (format: "aa:bb:cc:dd:ee:ff\n")
                    if let Some(mac) = parse_mac_address(&buf[..n as usize]) {
                        self.mac = mac;
                        return true;
                    }
                }
            }
        }

        false
    }

    /// Update statistics from sysfs
    pub fn update_stats(&mut self) {
        // ShadePacket: Stats live in /sys/class/net/<iface>/statistics/
        let base_path = format_path("/sys/class/net/", &self.name, "/statistics/");

        // Read RX packets
        if let Some(val) = read_sysfs_u64(&format_path(&base_path, "", "rx_packets")) {
            self.rx_packets = val;
        }

        // Read TX packets
        if let Some(val) = read_sysfs_u64(&format_path(&base_path, "", "tx_packets")) {
            self.tx_packets = val;
        }

        // Read RX bytes
        if let Some(val) = read_sysfs_u64(&format_path(&base_path, "", "rx_bytes")) {
            self.rx_bytes = val;
        }

        // Read TX bytes
        if let Some(val) = read_sysfs_u64(&format_path(&base_path, "", "tx_bytes")) {
            self.tx_bytes = val;
        }
    }
}

/// Parse MAC address from string (format: "aa:bb:cc:dd:ee:ff")
fn parse_mac_address(buf: &[u8]) -> Option<[u8; 6]> {
    let mut mac = [0u8; 6];
    let mut idx = 0;
    let mut byte_idx = 0;
    let mut current: u8 = 0;
    let mut nibble_count = 0;

    for &b in buf {
        if b == b':' || b == b'-' || b == b'\n' || b == b'\r' {
            if nibble_count > 0 {
                if byte_idx >= 6 {
                    return None;
                }
                mac[byte_idx] = current;
                byte_idx += 1;
                current = 0;
                nibble_count = 0;
            }
        } else if b.is_ascii_hexdigit() {
            let nibble = if b.is_ascii_digit() {
                b - b'0'
            } else if b >= b'a' && b <= b'f' {
                10 + (b - b'a')
            } else if b >= b'A' && b <= b'F' {
                10 + (b - b'A')
            } else {
                return None;
            };
            current = (current << 4) | nibble;
            nibble_count += 1;
            if nibble_count > 2 {
                return None;
            }
        }
    }

    // Handle last byte if there's no trailing separator
    if nibble_count > 0 && byte_idx < 6 {
        mac[byte_idx] = current;
        byte_idx += 1;
    }

    if byte_idx == 6 {
        Some(mac)
    } else {
        None
    }
}

/// Format a path by concatenating strings
fn format_path(prefix: &str, middle: &str, suffix: &str) -> String {
    let mut result = String::new();
    result.push_str(prefix);
    result.push_str(middle);
    result.push_str(suffix);
    result
}

/// Read a u64 value from a sysfs file
fn read_sysfs_u64(path: &str) -> Option<u64> {
    let fd = open2(path, O_RDONLY);
    if fd < 0 {
        return None;
    }

    let mut buf = [0u8; 32];
    let n = read(fd, &mut buf);
    close(fd);

    if n <= 0 {
        return None;
    }

    // Parse decimal number
    let text = core::str::from_utf8(&buf[..n as usize]).ok()?;
    let text = text.trim();
    
    let mut val: u64 = 0;
    for c in text.bytes() {
        if c.is_ascii_digit() {
            val = val.checked_mul(10)?;
            val = val.checked_add((c - b'0') as u64)?;
        } else {
            break;
        }
    }

    Some(val)
}
