//! Configuration File Parsing Module
//!
//! Handles reading and parsing network interface configuration files.
//! Persona: ShadePacket - Networking stack engineer

use alloc::string::String;
use libc::*;

use crate::adapter::ConfigMode;

/// Network configuration directory
const NET_CONFIG_DIR: &str = "/etc/network";

/// Interface configuration
pub struct InterfaceConfig {
    pub mode: ConfigMode,
    pub address: [u8; 4],
    pub netmask: [u8; 4],
    pub gateway: [u8; 4],
    pub dns_servers: [[u8; 4]; 3],
    pub dns_count: usize,
    pub mtu: u32,
    pub has_address: bool,
    pub has_gateway: bool,
}

impl InterfaceConfig {
    pub const fn new() -> Self {
        InterfaceConfig {
            mode: ConfigMode::Manual,
            address: [0; 4],
            netmask: [255, 255, 255, 0],
            gateway: [0; 4],
            dns_servers: [[0; 4]; 3],
            dns_count: 0,
            mtu: 1500,
            has_address: false,
            has_gateway: false,
        }
    }
}

/// Read config file for an interface
pub fn read_interface_config(iface_name: &str) -> Option<InterfaceConfig> {
    let mut config = InterfaceConfig::new();

    // ShadePacket: Build config file path /etc/network/<iface>.conf
    let mut path_buf = [0u8; 256];
    let prefix = NET_CONFIG_DIR.as_bytes();
    let suffix = b".conf";
    let name_bytes = iface_name.as_bytes();

    if prefix.len() + 1 + name_bytes.len() + suffix.len() >= 256 {
        return None;
    }

    path_buf[..prefix.len()].copy_from_slice(prefix);
    path_buf[prefix.len()] = b'/';
    path_buf[prefix.len() + 1..prefix.len() + 1 + name_bytes.len()].copy_from_slice(name_bytes);
    path_buf[prefix.len() + 1 + name_bytes.len()..prefix.len() + 1 + name_bytes.len() + suffix.len()]
        .copy_from_slice(suffix);
    let path_len = prefix.len() + 1 + name_bytes.len() + suffix.len();

    let path_str = core::str::from_utf8(&path_buf[..path_len]).ok()?;

    // Open and read config file
    let fd = open2(path_str, O_RDONLY);
    if fd < 0 {
        // ShadePacket: No config file - use defaults based on interface type
        if iface_name == "lo" {
            config.mode = ConfigMode::Loopback;
            config.address = [127, 0, 0, 1];
            config.netmask = [255, 0, 0, 0];
            config.has_address = true;
        }
        return Some(config);
    }

    let mut buf = [0u8; 1024];
    let n = read(fd, &mut buf);
    close(fd);

    if n <= 0 {
        return Some(config);
    }

    // Parse config content
    parse_config(&buf[..n as usize], &mut config);

    Some(config)
}

/// Parse configuration file content
fn parse_config(content: &[u8], config: &mut InterfaceConfig) {
    let text = core::str::from_utf8(content).unwrap_or("");

    for line in text.lines() {
        let line = line.trim();

        // ShadePacket: Skip comments and empty lines
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        // Parse key=value
        if let Some((key, value)) = line.split_once('=') {
            let key = key.trim().to_ascii_lowercase();
            let value = value.trim();

            match key.as_str() {
                "mode" => {
                    config.mode = match value.to_ascii_lowercase().as_str() {
                        "static" => ConfigMode::Static,
                        "dhcp" => ConfigMode::Dhcp,
                        "loopback" => ConfigMode::Loopback,
                        _ => ConfigMode::Manual,
                    };
                }
                "address" | "ip" | "ipaddr" => {
                    if let Some(addr) = parse_ipv4(value) {
                        config.address = addr;
                        config.has_address = true;
                    }
                }
                "netmask" | "mask" => {
                    if let Some(addr) = parse_ipv4(value) {
                        config.netmask = addr;
                    }
                }
                "gateway" | "gw" => {
                    if let Some(addr) = parse_ipv4(value) {
                        config.gateway = addr;
                        config.has_gateway = true;
                    }
                }
                "dns" | "nameserver" => {
                    if let Some(addr) = parse_ipv4(value) {
                        if config.dns_count < 3 {
                            config.dns_servers[config.dns_count] = addr;
                            config.dns_count += 1;
                        }
                    }
                }
                "mtu" => {
                    if let Some(mtu) = parse_u32(value) {
                        if mtu >= 68 && mtu <= 65535 {
                            config.mtu = mtu;
                        }
                    }
                }
                _ => {}
            }
        }
    }
}

/// Parse IPv4 address from string
fn parse_ipv4(s: &str) -> Option<[u8; 4]> {
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

    Some(octets)
}

/// Parse u32 from string
fn parse_u32(s: &str) -> Option<u32> {
    let mut val: u32 = 0;
    for c in s.bytes() {
        if c.is_ascii_digit() {
            val = val.checked_mul(10)?;
            val = val.checked_add((c - b'0') as u32)?;
        } else {
            return None;
        }
    }
    Some(val)
}

/// Trait extension for str
trait ToAsciiLowercase {
    fn to_ascii_lowercase(&self) -> String;
}

impl ToAsciiLowercase for str {
    fn to_ascii_lowercase(&self) -> String {
        self.chars().map(|c| c.to_ascii_lowercase()).collect()
    }
}
