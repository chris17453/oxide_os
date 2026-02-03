//! Network Adapter Manager Module
//!
//! Central registry and coordinator for all network adapters in the system.
//! Handles adapter discovery, registration, configuration, and lifecycle.
//! Persona: ShadePacket - Networking stack engineer

use alloc::string::String;
use alloc::vec::Vec;
use libc::dirent::{closedir, opendir, readdir};
use libc::*;

use crate::adapter::{AdapterState, ConfigMode, NetworkAdapter};
use crate::config::{read_interface_config, InterfaceConfig};

/// Global network adapter manager
pub struct AdapterManager {
    /// List of all registered adapters
    adapters: Vec<NetworkAdapter>,
}

impl AdapterManager {
    /// Create a new adapter manager
    pub fn new() -> Self {
        AdapterManager {
            adapters: Vec::new(),
        }
    }

    /// Discover network interfaces from /sys/class/net
    pub fn discover_adapters(&mut self) {
        log("[manager] Discovering network adapters");

        let dir = opendir("/sys/class/net");
        if let Some(mut dir) = dir {
            while let Some(entry) = readdir(&mut dir) {
                let name = entry.name();
                
                // ShadePacket: Skip directories and hidden entries
                if name == "." || name == ".." {
                    continue;
                }

                // Check if we already know about this adapter
                if !self.has_adapter(name) {
                    let mut adapter = NetworkAdapter::new(String::from(name));
                    adapter.update_mac_address();
                    adapter.update_link_status();
                    
                    log_adapter(name, "Discovered new adapter");
                    self.adapters.push(adapter);
                } else {
                    log_adapter(name, "Already registered");
                }
            }
            closedir(dir);
        } else {
            // ShadePacket: Fallback when sysfs not available - use known defaults
            log("[manager] /sys/class/net unavailable, using defaults");
            if !self.has_adapter("lo") {
                self.adapters.push(NetworkAdapter::new(String::from("lo")));
            }
            if !self.has_adapter("eth0") {
                self.adapters.push(NetworkAdapter::new(String::from("eth0")));
            }
        }

        log_count("[manager] Total adapters:", self.adapters.len());
    }

    /// Check if an adapter with the given name exists
    pub fn has_adapter(&self, name: &str) -> bool {
        self.adapters.iter().any(|a| a.name == name)
    }

    /// Get adapter by name (immutable)
    pub fn get_adapter(&self, name: &str) -> Option<&NetworkAdapter> {
        self.adapters.iter().find(|a| a.name == name)
    }

    /// Get adapter by name (mutable)
    pub fn get_adapter_mut(&mut self, name: &str) -> Option<&mut NetworkAdapter> {
        self.adapters.iter_mut().find(|a| a.name == name)
    }

    /// Get all adapters
    pub fn get_all_adapters(&self) -> &[NetworkAdapter] {
        &self.adapters
    }

    /// Configure all adapters from their config files
    pub fn configure_all(&mut self) {
        log("[manager] Configuring all adapters");

        for i in 0..self.adapters.len() {
            let name = self.adapters[i].name.clone();
            
            // ShadePacket: Special handling for loopback - always configure
            if name == "lo" {
                self.configure_loopback(i);
                continue;
            }

            self.configure_adapter_by_index(i);
        }
    }

    /// Configure a specific adapter by its index
    fn configure_adapter_by_index(&mut self, index: usize) {
        if index >= self.adapters.len() {
            return;
        }

        let name = self.adapters[index].name.clone();
        log_adapter(&name, "Reading configuration");

        // Read configuration from /etc/network/<iface>.conf
        if let Some(config) = read_interface_config(&name) {
            let mode = config.mode;
            let has_address = config.has_address;
            
            // ShadePacket: Apply configuration based on mode
            {
                let adapter = &mut self.adapters[index];
                adapter.mode = mode;
                adapter.mtu = config.mtu;
            }
            
            match mode {
                ConfigMode::Static => {
                    if has_address {
                        let adapter = &mut self.adapters[index];
                        adapter.set_ipv4(config.address, config.netmask);
                        if config.has_gateway {
                            adapter.set_gateway(config.gateway);
                        }
                        
                        // ShadePacket: Copy DNS servers
                        adapter.clear_dns();
                        for i in 0..config.dns_count {
                            adapter.add_dns(config.dns_servers[i]);
                        }
                        
                        adapter.set_state(AdapterState::Configuring);
                    }
                        
                    // Apply configuration (separate call to avoid borrow issues)
                    self.apply_configuration(index);
                        
                    self.adapters[index].set_state(AdapterState::Up);
                    log_adapter(&name, "Configured (static)");
                    
                    if !has_address {
                        log_adapter(&name, "Static mode but no address in config");
                        self.adapters[index].set_state(AdapterState::Error);
                    }
                }
                ConfigMode::Dhcp => {
                    log_adapter(&name, "Starting DHCP");
                    self.adapters[index].set_state(AdapterState::Configuring);
                    
                    // ShadePacket: DHCP handled by separate function
                    if self.configure_dhcp(index) {
                        self.adapters[index].set_state(AdapterState::Up);
                        log_adapter(&name, "Configured (DHCP)");
                    } else {
                        self.adapters[index].set_state(AdapterState::Error);
                        log_adapter(&name, "DHCP failed");
                    }
                }
                ConfigMode::Manual => {
                    log_adapter(&name, "Manual mode - skipping auto-configuration");
                    self.adapters[index].set_state(AdapterState::Down);
                }
                ConfigMode::Loopback => {
                    // Should be handled separately
                    self.configure_loopback(index);
                }
            }
        } else {
            log_adapter(&name, "No configuration file found");
        }
    }

    /// Configure loopback interface
    fn configure_loopback(&mut self, index: usize) {
        if index >= self.adapters.len() {
            return;
        }

        let name = self.adapters[index].name.clone();
        
        {
            let adapter = &mut self.adapters[index];
            adapter.mode = ConfigMode::Loopback;
            adapter.set_ipv4([127, 0, 0, 1], [255, 0, 0, 0]);
            adapter.mtu = 65536;
            adapter.link_up = true;
            adapter.set_state(AdapterState::Configuring);
        }
        
        self.apply_configuration(index);
        
        self.adapters[index].set_state(AdapterState::Up);
        log_adapter(&name, "Configured (loopback)");
    }

    /// Apply configuration to the kernel/hardware
    fn apply_configuration(&self, index: usize) {
        if index >= self.adapters.len() {
            return;
        }

        let adapter = &self.adapters[index];
        
        // ShadePacket: Write active config to /run/network/ for kernel consumption
        let _ = mkdir("/run", 0o755);
        let _ = mkdir("/run/network", 0o755);

        let path = format_run_path(&adapter.name);
        let fd = open(&path, (O_WRONLY | O_CREAT | O_TRUNC) as u32, 0o644);
        if fd < 0 {
            log_adapter(&adapter.name, "Failed to write active config");
            return;
        }

        if let Some(addr) = adapter.ipv4_addr {
            let _ = write(fd, b"address=");
            write_ipv4(fd, &addr);
            let _ = write(fd, b"\n");

            if let Some(netmask) = adapter.ipv4_netmask {
                let _ = write(fd, b"netmask=");
                write_ipv4(fd, &netmask);
                let _ = write(fd, b"\n");
            }
        }

        if let Some(gateway) = adapter.ipv4_gateway {
            let _ = write(fd, b"gateway=");
            write_ipv4(fd, &gateway);
            let _ = write(fd, b"\n");
        }

        for dns in &adapter.dns_servers {
            let _ = write(fd, b"dns=");
            write_ipv4(fd, dns);
            let _ = write(fd, b"\n");
        }

        close(fd);
    }

    /// Configure adapter using DHCP
    fn configure_dhcp(&mut self, index: usize) -> bool {
        if index >= self.adapters.len() {
            return false;
        }

        let name = self.adapters[index].name.clone();

        // ShadePacket: Check for existing DHCP lease in /var/lib/dhcp/<iface>.lease
        let lease_path = format_dhcp_lease_path(&name);
        let fd = open2(&lease_path, O_RDONLY);
        
        if fd < 0 {
            log_adapter(&name, "No DHCP lease file found");
            return false;
        }

        let mut buf = [0u8; 1024];
        let n = read(fd, &mut buf);
        close(fd);

        if n <= 0 {
            return false;
        }

        // Parse lease file
        if let Ok(text) = core::str::from_utf8(&buf[..n as usize]) {
            let mut addr: Option<[u8; 4]> = None;
            let mut netmask: Option<[u8; 4]> = None;
            let mut gateway: Option<[u8; 4]> = None;
            let mut dns_list: Vec<[u8; 4]> = Vec::new();

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
                            addr = parse_ipv4(value);
                        }
                        "netmask" | "mask" => {
                            netmask = parse_ipv4(value);
                        }
                        "gateway" | "gw" => {
                            gateway = parse_ipv4(value);
                        }
                        "dns" | "nameserver" => {
                            if let Some(dns) = parse_ipv4(value) {
                                dns_list.push(dns);
                            }
                        }
                        _ => {}
                    }
                }
            }

            // Apply DHCP lease to adapter
            if let Some(addr) = addr {
                let adapter = &mut self.adapters[index];
                adapter.set_ipv4(addr, netmask.unwrap_or([255, 255, 255, 0]));
                
                if let Some(gw) = gateway {
                    adapter.set_gateway(gw);
                }

                adapter.clear_dns();
                for dns in dns_list {
                    adapter.add_dns(dns);
                }

                self.apply_configuration(index);
                return true;
            }
        }

        false
    }

    /// Bring an adapter up
    pub fn bring_up(&mut self, name: &str) -> bool {
        log_adapter(name, "Bringing up");
        
        if let Some(adapter) = self.get_adapter_mut(name) {
            if adapter.state == AdapterState::Down || adapter.state == AdapterState::Error {
                adapter.set_state(AdapterState::Configuring);
                
                // ShadePacket: Reconfigure from config file
                let idx = self.adapters.iter().position(|a| a.name == name);
                if let Some(idx) = idx {
                    self.configure_adapter_by_index(idx);
                    return true;
                }
            }
        }
        
        false
    }

    /// Bring an adapter down
    pub fn bring_down(&mut self, name: &str) -> bool {
        log_adapter(name, "Bringing down");
        
        if let Some(adapter) = self.get_adapter_mut(name) {
            adapter.set_state(AdapterState::Down);
            adapter.ipv4_addr = None;
            adapter.ipv4_gateway = None;
            adapter.clear_dns();
            
            // ShadePacket: Remove active configuration
            let path = format_run_path(name);
            let _ = unlink(&path);
            
            return true;
        }
        
        false
    }

    /// Reload adapter configuration
    pub fn reload_adapter(&mut self, name: &str) -> bool {
        log_adapter(name, "Reloading configuration");
        
        // ShadePacket: Bring down and bring back up
        self.bring_down(name);
        self.bring_up(name)
    }

    /// Update all adapter statistics
    pub fn update_all_stats(&mut self) {
        for adapter in &mut self.adapters {
            adapter.update_stats();
            adapter.update_link_status();
        }
    }

    /// Get DNS servers from all configured adapters
    pub fn collect_dns_servers(&self) -> Vec<[u8; 4]> {
        let mut dns_list: Vec<[u8; 4]> = Vec::new();
        
        for adapter in &self.adapters {
            if adapter.state == AdapterState::Up {
                for dns in &adapter.dns_servers {
                    if !dns_list.contains(dns) {
                        dns_list.push(*dns);
                    }
                }
            }
        }

        // ShadePacket: Add defaults if nothing configured
        if dns_list.is_empty() {
            if let Some(dns) = parse_ipv4("8.8.8.8") {
                dns_list.push(dns);
            }
            if let Some(dns) = parse_ipv4("8.8.4.4") {
                dns_list.push(dns);
            }
        }

        dns_list
    }
}

/// Format a path for /run/network/<iface>.conf
fn format_run_path(name: &str) -> String {
    let mut path = String::new();
    path.push_str("/run/network/");
    path.push_str(name);
    path.push_str(".conf");
    path
}

/// Format DHCP lease path
fn format_dhcp_lease_path(name: &str) -> String {
    let mut path = String::new();
    path.push_str("/var/lib/dhcp/");
    path.push_str(name);
    path.push_str(".lease");
    path
}

/// Write IPv4 address to file descriptor
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

/// Log a message
fn log(msg: &str) {
    prints("[networkd] ");
    prints(msg);
    prints("\n");
}

/// Log with adapter name
fn log_adapter(iface: &str, msg: &str) {
    prints("[networkd] ");
    prints(iface);
    prints(": ");
    prints(msg);
    prints("\n");
}

/// Log with count
fn log_count(msg: &str, count: usize) {
    prints("[networkd] ");
    prints(msg);
    prints(" ");
    print_i64(count as i64);
    prints("\n");
}
