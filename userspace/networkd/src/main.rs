//! OXIDE Network Daemon (networkd)
//!
//! Manages network interface configuration including:
//! - Static IP configuration from /etc/network/<iface>.conf
//! - DHCP client for dynamic IP assignment
//! - DNS server configuration in /etc/resolv.conf
//!
//! Config file format (/etc/network/eth0.conf):
//! ```text
//! mode=static|dhcp|manual
//! address=192.168.1.100
//! netmask=255.255.255.0
//! gateway=192.168.1.1
//! dns=8.8.8.8
//! dns=8.8.4.4
//! mtu=1500
//! ```

#![no_std]
#![no_main]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use libc::dirent::{closedir, opendir, readdir};
use libc::time::usleep;
use libc::*;

/// Network configuration directory
const NET_CONFIG_DIR: &str = "/etc/network";

/// Resolv.conf path
const RESOLV_CONF: &str = "/etc/resolv.conf";

/// Sysfs network directory (for interface enumeration)
const SYSFS_NET_DIR: &str = "/sys/class/net";

/// Default DNS servers (used if no config and no DHCP)
const DEFAULT_DNS: &[&str] = &["8.8.8.8", "8.8.4.4"];

/// Log file
const LOG_FILE: &str = "/var/log/networkd.log";

/// Configuration mode
#[derive(Clone, Copy, PartialEq, Eq)]
enum ConfigMode {
    Static,
    Dhcp,
    Manual,
    Loopback,
}

/// Interface configuration
struct InterfaceConfig {
    name: [u8; 32],
    name_len: usize,
    mode: ConfigMode,
    address: [u8; 4],
    netmask: [u8; 4],
    gateway: [u8; 4],
    dns_servers: [[u8; 4]; 3],
    dns_count: usize,
    mtu: u32,
    has_address: bool,
    has_gateway: bool,
}

impl InterfaceConfig {
    const fn empty() -> Self {
        InterfaceConfig {
            name: [0; 32],
            name_len: 0,
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

    fn name_str(&self) -> &str {
        core::str::from_utf8(&self.name[..self.name_len]).unwrap_or("")
    }
}

/// DHCP lease information
struct DhcpLease {
    address: [u8; 4],
    netmask: [u8; 4],
    gateway: [u8; 4],
    dns_servers: [[u8; 4]; 3],
    dns_count: usize,
    has_gateway: bool,
}

/// Print to log file
fn log(msg: &str) {
    // Try to log to file
    let fd = open(LOG_FILE, (O_WRONLY | O_CREAT | O_APPEND) as u32, 0o644);
    if fd >= 0 {
        let prefix = b"[networkd] ";
        let _ = write(fd, prefix);
        let _ = write(fd, msg.as_bytes());
        let _ = write(fd, b"\n");
        close(fd);
    }

    // Also print to console
    prints("[networkd] ");
    prints(msg);
    prints("\n");
}

/// Log with interface name
fn log_iface(iface: &str, msg: &str) {
    prints("[networkd] ");
    prints(iface);
    prints(": ");
    prints(msg);
    prints("\n");
}

/// Enumerate network interfaces from /sys/class/net
fn enumerate_interfaces() -> Vec<String> {
    let mut interfaces = Vec::new();

    let dir = opendir(SYSFS_NET_DIR);
    if let Some(mut dir) = dir {
        while let Some(entry) = readdir(&mut dir) {
            let name = entry.name();
            // Skip . and ..
            if name == "." || name == ".." {
                continue;
            }
            interfaces.push(String::from(name));
        }
        closedir(dir);
    } else {
        // Fallback: try to read from /proc/net/dev or use defaults
        log("No /sys/class/net, using default interfaces");
        interfaces.push(String::from("lo"));
        interfaces.push(String::from("eth0"));
    }

    interfaces
}

/// Read config file for an interface
fn read_interface_config(iface_name: &str) -> Option<InterfaceConfig> {
    let mut config = InterfaceConfig::empty();

    // Copy interface name
    let name_bytes = iface_name.as_bytes();
    let len = name_bytes.len().min(31);
    config.name[..len].copy_from_slice(&name_bytes[..len]);
    config.name_len = len;

    // Build config file path
    let mut path_buf = [0u8; 256];
    let prefix = NET_CONFIG_DIR.as_bytes();
    let suffix = b".conf";

    if prefix.len() + 1 + len + suffix.len() >= 256 {
        return None;
    }

    path_buf[..prefix.len()].copy_from_slice(prefix);
    path_buf[prefix.len()] = b'/';
    path_buf[prefix.len() + 1..prefix.len() + 1 + len].copy_from_slice(&name_bytes[..len]);
    path_buf[prefix.len() + 1 + len..prefix.len() + 1 + len + suffix.len()]
        .copy_from_slice(suffix);
    let path_len = prefix.len() + 1 + len + suffix.len();

    let path_str = core::str::from_utf8(&path_buf[..path_len]).ok()?;

    // Open and read config file
    let fd = open2(path_str, O_RDONLY);
    if fd < 0 {
        // No config file - use defaults
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

        // Skip comments and empty lines
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

/// Configure interface with ioctl
fn configure_interface(config: &InterfaceConfig) -> bool {
    log_iface(config.name_str(), "Configuring interface");

    // For now, we'll use a simplified approach:
    // Write configuration to a known location that the kernel can read
    // In a full implementation, this would use ioctl() calls

    // Create interface config in /run/network/
    let _ = mkdir("/run", 0o755);
    let _ = mkdir("/run/network", 0o755);

    // Build path
    let mut path_buf = [0u8; 256];
    let prefix = b"/run/network/";
    let suffix = b".conf";
    let name = config.name_str().as_bytes();

    if prefix.len() + name.len() + suffix.len() >= 256 {
        return false;
    }

    path_buf[..prefix.len()].copy_from_slice(prefix);
    path_buf[prefix.len()..prefix.len() + name.len()].copy_from_slice(name);
    path_buf[prefix.len() + name.len()..prefix.len() + name.len() + suffix.len()]
        .copy_from_slice(suffix);
    let path_len = prefix.len() + name.len() + suffix.len();

    let path_str = core::str::from_utf8(&path_buf[..path_len]).unwrap_or("");

    // Write active configuration
    let fd = open(path_str, (O_WRONLY | O_CREAT | O_TRUNC) as u32, 0o644);
    if fd < 0 {
        log_iface(config.name_str(), "Failed to write active config");
        return false;
    }

    // Write address
    if config.has_address {
        let _ = write(fd, b"address=");
        write_ipv4(fd, &config.address);
        let _ = write(fd, b"\n");

        let _ = write(fd, b"netmask=");
        write_ipv4(fd, &config.netmask);
        let _ = write(fd, b"\n");
    }

    // Write gateway
    if config.has_gateway {
        let _ = write(fd, b"gateway=");
        write_ipv4(fd, &config.gateway);
        let _ = write(fd, b"\n");
    }

    // Write DNS servers
    for i in 0..config.dns_count {
        let _ = write(fd, b"dns=");
        write_ipv4(fd, &config.dns_servers[i]);
        let _ = write(fd, b"\n");
    }

    close(fd);

    if config.has_address {
        log_iface(config.name_str(), "Configured with address ");
        print_ipv4(&config.address);
        prints("\n");
    }

    true
}

/// Write IPv4 address to fd
fn write_ipv4(fd: i32, addr: &[u8; 4]) {
    let mut buf = [0u8; 16];
    let mut pos = 0;

    for (i, &octet) in addr.iter().enumerate() {
        if i > 0 {
            buf[pos] = b'.';
            pos += 1;
        }
        pos += format_u8(octet, &mut buf[pos..]);
    }

    let _ = write(fd, &buf[..pos]);
}

/// Print IPv4 address to console
fn print_ipv4(addr: &[u8; 4]) {
    for (i, &octet) in addr.iter().enumerate() {
        if i > 0 {
            prints(".");
        }
        print_i64(octet as i64);
    }
}

/// Format u8 to buffer, returns length
fn format_u8(mut val: u8, buf: &mut [u8]) -> usize {
    if val == 0 {
        buf[0] = b'0';
        return 1;
    }

    let mut len = 0;
    let mut tmp = [0u8; 3];
    while val > 0 {
        tmp[len] = b'0' + (val % 10);
        val /= 10;
        len += 1;
    }

    for i in 0..len {
        buf[i] = tmp[len - 1 - i];
    }
    len
}

/// Run DHCP client for interface
fn run_dhcp(iface_name: &str) -> Option<DhcpLease> {
    log_iface(iface_name, "Starting DHCP");

    // In a full implementation, this would:
    // 1. Open a raw socket
    // 2. Send DHCP DISCOVER
    // 3. Receive DHCP OFFER
    // 4. Send DHCP REQUEST
    // 5. Receive DHCP ACK
    // 6. Parse lease information

    // For now, we'll try to read from a DHCP lease file if available
    // (e.g., if another component has already done DHCP)

    let mut path_buf = [0u8; 256];
    let prefix = b"/var/lib/dhcp/";
    let suffix = b".lease";
    let name = iface_name.as_bytes();

    if prefix.len() + name.len() + suffix.len() >= 256 {
        return None;
    }

    path_buf[..prefix.len()].copy_from_slice(prefix);
    path_buf[prefix.len()..prefix.len() + name.len()].copy_from_slice(name);
    path_buf[prefix.len() + name.len()..prefix.len() + name.len() + suffix.len()]
        .copy_from_slice(suffix);
    let path_len = prefix.len() + name.len() + suffix.len();

    let path_str = core::str::from_utf8(&path_buf[..path_len]).ok()?;

    let fd = open2(path_str, O_RDONLY);
    if fd < 0 {
        log_iface(iface_name, "No DHCP lease file found");
        return None;
    }

    let mut buf = [0u8; 1024];
    let n = read(fd, &mut buf);
    close(fd);

    if n <= 0 {
        return None;
    }

    // Parse lease file (same format as config)
    let mut lease = DhcpLease {
        address: [0; 4],
        netmask: [255, 255, 255, 0],
        gateway: [0; 4],
        dns_servers: [[0; 4]; 3],
        dns_count: 0,
        has_gateway: false,
    };

    let text = core::str::from_utf8(&buf[..n as usize]).ok()?;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        if let Some((key, value)) = line.split_once('=') {
            let key = key.trim();
            let value = value.trim();

            match key {
                "address" | "ip" => {
                    if let Some(addr) = parse_ipv4(value) {
                        lease.address = addr;
                    }
                }
                "netmask" | "mask" => {
                    if let Some(addr) = parse_ipv4(value) {
                        lease.netmask = addr;
                    }
                }
                "gateway" | "gw" => {
                    if let Some(addr) = parse_ipv4(value) {
                        lease.gateway = addr;
                        lease.has_gateway = true;
                    }
                }
                "dns" | "nameserver" => {
                    if let Some(addr) = parse_ipv4(value) {
                        if lease.dns_count < 3 {
                            lease.dns_servers[lease.dns_count] = addr;
                            lease.dns_count += 1;
                        }
                    }
                }
                _ => {}
            }
        }
    }

    log_iface(iface_name, "DHCP lease obtained");
    Some(lease)
}

/// Write /etc/resolv.conf with DNS servers
fn write_resolv_conf(configs: &[InterfaceConfig]) {
    log("Writing /etc/resolv.conf");

    // Collect all DNS servers
    let mut dns_servers: Vec<[u8; 4]> = Vec::new();

    for config in configs {
        for i in 0..config.dns_count {
            let dns = config.dns_servers[i];
            // Skip if already added
            if !dns_servers.iter().any(|d| d == &dns) {
                dns_servers.push(dns);
            }
        }
    }

    // If no DNS servers configured, use defaults
    if dns_servers.is_empty() {
        for &default_dns in DEFAULT_DNS {
            if let Some(addr) = parse_ipv4(default_dns) {
                dns_servers.push(addr);
            }
        }
    }

    // Create /etc directory if needed
    let _ = mkdir("/etc", 0o755);

    // Write resolv.conf
    let fd = open(RESOLV_CONF, (O_WRONLY | O_CREAT | O_TRUNC) as u32, 0o644);
    if fd < 0 {
        log("Failed to create /etc/resolv.conf");
        return;
    }

    // Header
    let _ = write(fd, b"# Generated by OXIDE networkd\n");
    let _ = write(fd, b"# Do not edit manually\n\n");

    // Write nameservers
    for dns in &dns_servers {
        let _ = write(fd, b"nameserver ");
        write_ipv4(fd, dns);
        let _ = write(fd, b"\n");
    }

    close(fd);

    log("Wrote ");
    print_i64(dns_servers.len() as i64);
    prints(" DNS servers to /etc/resolv.conf\n");
}

/// Configure loopback interface with default settings
fn configure_loopback() {
    log("Configuring loopback interface");

    let config = InterfaceConfig {
        name: *b"lo\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
        name_len: 2,
        mode: ConfigMode::Loopback,
        address: [127, 0, 0, 1],
        netmask: [255, 0, 0, 0],
        gateway: [0; 4],
        dns_servers: [[0; 4]; 3],
        dns_count: 0,
        mtu: 65536,
        has_address: true,
        has_gateway: false,
    };

    configure_interface(&config);
}

/// Main daemon loop
fn run_daemon() {
    log("Starting network daemon");

    // Create necessary directories
    let _ = mkdir("/var", 0o755);
    let _ = mkdir("/var/log", 0o755);
    let _ = mkdir("/var/lib", 0o755);
    let _ = mkdir("/var/lib/dhcp", 0o755);
    let _ = mkdir("/etc", 0o755);
    let _ = mkdir(NET_CONFIG_DIR, 0o755);
    let _ = mkdir("/run", 0o755);
    let _ = mkdir("/run/network", 0o755);

    // Configure loopback first
    configure_loopback();

    // Enumerate interfaces
    let interfaces = enumerate_interfaces();
    log("Found ");
    print_i64(interfaces.len() as i64);
    prints(" interfaces\n");

    // Configure each interface
    let mut configs: Vec<InterfaceConfig> = Vec::new();

    for iface_name in &interfaces {
        if iface_name == "lo" {
            // Already configured loopback
            continue;
        }

        log_iface(iface_name, "Processing interface");

        if let Some(mut config) = read_interface_config(iface_name) {
            match config.mode {
                ConfigMode::Static => {
                    if config.has_address {
                        configure_interface(&config);
                    } else {
                        log_iface(iface_name, "Static mode but no address configured");
                    }
                }
                ConfigMode::Dhcp => {
                    if let Some(lease) = run_dhcp(iface_name) {
                        // Apply DHCP lease
                        config.address = lease.address;
                        config.netmask = lease.netmask;
                        config.gateway = lease.gateway;
                        config.has_address = true;
                        config.has_gateway = lease.has_gateway;

                        // Copy DNS servers
                        for i in 0..lease.dns_count {
                            if config.dns_count < 3 {
                                config.dns_servers[config.dns_count] = lease.dns_servers[i];
                                config.dns_count += 1;
                            }
                        }

                        configure_interface(&config);
                    } else {
                        log_iface(iface_name, "DHCP failed, interface not configured");
                    }
                }
                ConfigMode::Manual | ConfigMode::Loopback => {
                    log_iface(iface_name, "Manual mode, skipping auto-configuration");
                }
            }

            configs.push(config);
        }
    }

    // Write /etc/resolv.conf with collected DNS servers
    write_resolv_conf(&configs);

    log("Network configuration complete");

    // Main loop - monitor for changes and DHCP renewal
    loop {
        // Sleep for 60 seconds before checking again
        usleep(60_000_000);

        // TODO: Check for DHCP lease renewal
        // TODO: Monitor for interface state changes
    }
}

/// Show usage
fn show_usage() {
    prints("Usage: networkd [command]\n");
    prints("\n");
    prints("Commands:\n");
    prints("  daemon    Run as daemon (started by init/servicemgr)\n");
    prints("  status    Show network status\n");
    prints("  reload    Reload configuration\n");
    prints("  help      Show this help\n");
}

/// Show network status
fn show_status() {
    prints("Network Status:\n\n");

    let interfaces = enumerate_interfaces();

    for iface_name in &interfaces {
        prints(iface_name);
        prints(":\n");

        // Try to read active config
        let mut path_buf = [0u8; 256];
        let prefix = b"/run/network/";
        let suffix = b".conf";
        let name = iface_name.as_bytes();

        if prefix.len() + name.len() + suffix.len() < 256 {
            path_buf[..prefix.len()].copy_from_slice(prefix);
            path_buf[prefix.len()..prefix.len() + name.len()].copy_from_slice(name);
            path_buf[prefix.len() + name.len()..prefix.len() + name.len() + suffix.len()]
                .copy_from_slice(suffix);
            let path_len = prefix.len() + name.len() + suffix.len();

            if let Ok(path_str) = core::str::from_utf8(&path_buf[..path_len]) {
                let fd = open2(path_str, O_RDONLY);
                if fd >= 0 {
                    let mut buf = [0u8; 512];
                    let n = read(fd, &mut buf);
                    close(fd);

                    if n > 0 {
                        if let Ok(content) = core::str::from_utf8(&buf[..n as usize]) {
                            for line in content.lines() {
                                prints("  ");
                                prints(line);
                                prints("\n");
                            }
                        }
                    }
                } else {
                    prints("  (not configured)\n");
                }
            }
        }
        prints("\n");
    }
}

/// Main entry point
#[unsafe(no_mangle)]
fn main(argc: i32, argv: *const *const u8) -> i32 {
    let cmd = if argc >= 2 {
        cstr_to_str(unsafe { *argv.add(1) })
    } else {
        "daemon"
    };

    match cmd {
        "daemon" => {
            run_daemon();
            0
        }
        "status" => {
            show_status();
            0
        }
        "reload" => {
            log("Reload not yet implemented");
            0
        }
        "help" | "--help" | "-h" => {
            show_usage();
            0
        }
        _ => {
            prints("Unknown command: ");
            prints(cmd);
            prints("\n");
            show_usage();
            1
        }
    }
}

/// Convert C string to str
fn cstr_to_str(ptr: *const u8) -> &'static str {
    if ptr.is_null() {
        return "";
    }
    let mut len = 0;
    unsafe {
        while *ptr.add(len) != 0 {
            len += 1;
        }
        core::str::from_utf8_unchecked(core::slice::from_raw_parts(ptr, len))
    }
}
