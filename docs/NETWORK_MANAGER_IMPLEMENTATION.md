# OXIDE Network Manager Implementation Guide

## Overview

This document describes the implementation of the OXIDE Network Manager (`networkd`), a multi-adapter network management service designed to handle complex network configurations in the OXIDE OS environment.

## Design Goals

1. **Multi-Adapter Support** - Handle multiple network interfaces simultaneously with independent configuration
2. **State Management** - Track adapter state through a well-defined state machine
3. **Flexible Configuration** - Support static, DHCP, and manual configuration modes
4. **Modular Architecture** - Clean separation of concerns with reusable components
5. **Production Quality** - Robust error handling, logging, and monitoring

## Architecture

### Module Structure

```
networkd/
├── adapter.rs      # NetworkAdapter struct and adapter-level operations
├── config.rs       # Configuration file parsing
├── manager.rs      # AdapterManager for multi-adapter coordination
└── main.rs         # Entry point, CLI interface, daemon loop
```

### Data Flow

```
┌─────────────────┐
│  Configuration  │ (/etc/network/*.conf)
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│ Config Parser   │ (config.rs)
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│ Adapter Manager │ (manager.rs)
└────────┬────────┘
         │
         ├─────► NetworkAdapter (adapter.rs)
         │
         ├─────► NetworkAdapter (adapter.rs)
         │
         └─────► NetworkAdapter (adapter.rs)
                      │
                      ▼
              ┌──────────────┐
              │ Apply Config │ (/run/network/*.conf)
              └──────────────┘
```

## Key Components

### NetworkAdapter (adapter.rs)

**Purpose:** Represents a single network adapter with its configuration and state.

**Key Features:**
- State tracking (Down, Configuring, Up, Error, Removing)
- IPv4 configuration storage
- DNS server list (up to 3)
- Link status monitoring
- MAC address tracking
- Statistics collection (RX/TX packets and bytes)

**State Machine:**
```
Down → Configuring → Up
  ↑                   ↓
  └─── Error ←────────┘
```

**Interface:**
```rust
pub struct NetworkAdapter {
    pub name: String,
    pub mac: [u8; 6],
    pub state: AdapterState,
    pub mode: ConfigMode,
    pub ipv4_addr: Option<[u8; 4]>,
    pub ipv4_netmask: Option<[u8; 4]>,
    pub ipv4_gateway: Option<[u8; 4]>,
    pub dns_servers: Vec<[u8; 4]>,
    pub mtu: u32,
    pub link_up: bool,
    pub rx_packets: u64,
    pub tx_packets: u64,
    pub rx_bytes: u64,
    pub tx_bytes: u64,
}
```

### AdapterManager (manager.rs)

**Purpose:** Central coordinator for all network adapters in the system.

**Key Responsibilities:**
- Adapter discovery from `/sys/class/net`
- Adapter lifecycle management
- Configuration application
- DHCP lease processing
- DNS server aggregation

**Interface:**
```rust
pub struct AdapterManager {
    adapters: Vec<NetworkAdapter>,
}

impl AdapterManager {
    pub fn discover_adapters(&mut self);
    pub fn configure_all(&mut self);
    pub fn bring_up(&mut self, name: &str) -> bool;
    pub fn bring_down(&mut self, name: &str) -> bool;
    pub fn reload_adapter(&mut self, name: &str) -> bool;
    pub fn collect_dns_servers(&self) -> Vec<[u8; 4]>;
}
```

### Configuration Parser (config.rs)

**Purpose:** Parse network interface configuration files.

**Supported Keys:**
- `mode` - Configuration mode (static, dhcp, manual, loopback)
- `address` / `ip` / `ipaddr` - IPv4 address
- `netmask` / `mask` - Network mask
- `gateway` / `gw` - Default gateway
- `dns` / `nameserver` - DNS servers (multiple allowed)
- `mtu` - Maximum transmission unit

**Example:**
```ini
mode=static
address=192.168.1.100
netmask=255.255.255.0
gateway=192.168.1.1
dns=8.8.8.8
dns=8.8.4.4
mtu=1500
```

## Implementation Details

### Adapter Discovery

The manager discovers adapters by reading `/sys/class/net/`:

```rust
fn discover_adapters(&mut self) {
    let dir = opendir("/sys/class/net");
    if let Some(mut dir) = dir {
        while let Some(entry) = readdir(&mut dir) {
            let name = entry.name();
            if name != "." && name != ".." && !self.has_adapter(name) {
                let mut adapter = NetworkAdapter::new(String::from(name));
                adapter.update_mac_address();
                adapter.update_link_status();
                self.adapters.push(adapter);
            }
        }
    }
}
```

### Configuration Application

Static configuration is applied directly:

```rust
adapter.set_ipv4(config.address, config.netmask);
adapter.set_gateway(config.gateway);
for dns in &config.dns_servers {
    adapter.add_dns(dns);
}
self.apply_configuration(index);
```

DHCP configuration reads lease files:

```rust
fn configure_dhcp(&mut self, index: usize) -> bool {
    let lease_path = format!("/var/lib/dhcp/{}.lease", adapter.name);
    // Read lease file
    // Parse lease information
    // Apply to adapter
    // Return success
}
```

### State Management

Each adapter transitions through well-defined states:

```rust
pub enum AdapterState {
    Down,         // Not configured
    Configuring,  // Configuration in progress
    Up,           // Fully operational
    Error,        // Configuration failed
    Removing,     // Being removed
}
```

State transitions are logged for debugging:
```
[networkd] eth0: Configuring interface
[networkd] eth0: Configured (static)
```

### DNS Management

DNS servers are collected from all active adapters:

```rust
pub fn collect_dns_servers(&self) -> Vec<[u8; 4]> {
    let mut dns_list = Vec::new();
    for adapter in &self.adapters {
        if adapter.state == AdapterState::Up {
            for dns in &adapter.dns_servers {
                if !dns_list.contains(dns) {
                    dns_list.push(*dns);
                }
            }
        }
    }
    // Add defaults if empty
    if dns_list.is_empty() {
        dns_list.push(parse_ipv4("8.8.8.8").unwrap());
        dns_list.push(parse_ipv4("8.8.4.4").unwrap());
    }
    dns_list
}
```

Then written to `/etc/resolv.conf`:

```rust
fn write_resolv_conf(manager: &AdapterManager) {
    let dns_servers = manager.collect_dns_servers();
    let fd = open("/etc/resolv.conf", O_WRONLY | O_CREAT | O_TRUNC, 0o644);
    write(fd, b"# Generated by OXIDE networkd\n");
    for dns in &dns_servers {
        write(fd, b"nameserver ");
        write_ipv4(fd, dns);
        write(fd, b"\n");
    }
    close(fd);
}
```

## Runtime Behavior

### Daemon Startup

1. **Initialize directories**
   ```rust
   mkdir("/var/log", 0o755);
   mkdir("/var/lib/dhcp", 0o755);
   mkdir("/etc/network", 0o755);
   mkdir("/run/network", 0o755);
   ```

2. **Create adapter manager**
   ```rust
   let mut manager = AdapterManager::new();
   ```

3. **Discover adapters**
   ```rust
   manager.discover_adapters();
   ```

4. **Configure all adapters**
   ```rust
   manager.configure_all();
   ```

5. **Write DNS configuration**
   ```rust
   write_resolv_conf(&manager);
   ```

6. **Enter monitoring loop**
   ```rust
   loop {
       usleep(POLL_INTERVAL_US);
       manager.update_all_stats();
   }
   ```

### CLI Commands

Each command creates a manager instance, discovers adapters, and performs the requested operation:

```rust
"status" => {
    let mut manager = AdapterManager::new();
    manager.discover_adapters();
    show_status(&manager);
}

"up" => {
    let iface = get_arg(2);
    let mut manager = AdapterManager::new();
    manager.discover_adapters();
    manager.bring_up(iface);
    write_resolv_conf(&manager);
}
```

## Integration Points

### Kernel Interface

Currently uses file-based communication:
- Write configuration to `/run/network/<iface>.conf`
- Kernel reads configuration files

Future: Direct ioctl calls for configuration.

### DHCP Client

External DHCP client writes lease files:
- Location: `/var/lib/dhcp/<iface>.lease`
- Format: Same as interface configuration
- Network manager reads and applies lease

### DNS Resolution

Generates `/etc/resolv.conf` for system-wide DNS:
- Collects from all active adapters
- Removes duplicates
- Adds defaults if needed

## Testing Strategy

### Unit Testing

Test individual components:
- IPv4 address parsing
- Configuration file parsing
- State transitions

### Integration Testing

Test component interaction:
- Adapter discovery and registration
- Configuration application
- DNS collection

### System Testing

Test in QEMU:
- Single adapter configuration
- Multiple adapters (eth0, eth1)
- Mixed static/DHCP
- Adapter up/down operations
- Configuration reload

## Performance Considerations

### Memory Usage
- Fixed-size adapter structures
- Bounded DNS server lists (3 per adapter)
- No dynamic memory for critical paths

### CPU Usage
- 60-second monitoring intervals
- Efficient sysfs reading
- Minimal string processing

### I/O Operations
- Batched configuration writes
- Cached adapter information
- Read-only sysfs access

## Security Considerations

### File Permissions
- Configuration files: 0644 (readable by all)
- Log files: 0644 (readable by all)
- Runtime configs: 0644 (readable by all)

### Input Validation
- IPv4 address validation
- MTU range checking (68-65535)
- Configuration key validation

### Error Handling
- All I/O operations checked
- Invalid configurations logged
- Graceful degradation

## Future Enhancements

### Short Term
1. **Real-time DHCP renewal** - Monitor lease expiration
2. **Hotplug support** - Detect adapter addition/removal
3. **Link state monitoring** - React to link up/down events

### Medium Term
1. **Control socket** - IPC interface for other services
2. **Event notifications** - Publish adapter state changes
3. **Configuration validation** - Pre-flight checks

### Long Term
1. **IPv6 support** - Dual-stack networking
2. **Wireless adapters** - WiFi configuration
3. **Advanced features** - VLANs, bridges, bonding
4. **Firewall integration** - Coordinate with packet filter

## Troubleshooting Guide

### Problem: Adapter not discovered

**Symptoms:**
- `networkd list` doesn't show adapter
- `networkd status` missing adapter

**Diagnosis:**
```bash
ls /sys/class/net/
cat /var/log/networkd.log
```

**Solution:**
- Verify adapter exists in sysfs
- Check kernel driver loaded
- Restart networkd

### Problem: Static IP not applied

**Symptoms:**
- `networkd status` shows Down or Error
- No IP address shown

**Diagnosis:**
```bash
networkd config eth0
cat /etc/network/eth0.conf
cat /var/log/networkd.log
```

**Solution:**
- Verify configuration file syntax
- Ensure address and netmask present
- Check file permissions
- Reload configuration

### Problem: DHCP not working

**Symptoms:**
- Adapter stuck in Configuring state
- No IP address assigned

**Diagnosis:**
```bash
ls /var/lib/dhcp/
cat /var/lib/dhcp/eth0.lease
cat /var/log/networkd.log
```

**Solution:**
- Verify DHCP client running
- Check network connectivity
- Review DHCP client logs
- Try static configuration temporarily

## Code Style and Conventions

### Naming
- `snake_case` for functions and variables
- `CamelCase` for types and enums
- `SCREAMING_SNAKE_CASE` for constants

### Comments
- Use persona-based comments (ShadePacket)
- Explain "why" not "what"
- Document safety assumptions

### Error Handling
- Check all I/O operations
- Log errors with context
- Return meaningful error codes

### Example:
```rust
// ShadePacket: Read MAC from /sys/class/net/<iface>/address
pub fn update_mac_address(&mut self) -> bool {
    let path = format!("/sys/class/net/{}/address", self.name);
    let fd = open2(&path, O_RDONLY);
    if fd < 0 {
        return false;  // Log handled by caller
    }
    
    let mut buf = [0u8; 32];
    let n = read(fd, &mut buf);
    close(fd);
    
    if n >= 17 {
        if let Some(mac) = parse_mac_address(&buf[..n as usize]) {
            self.mac = mac;
            return true;
        }
    }
    
    false
}
```

## Conclusion

The OXIDE Network Manager provides a robust, modular foundation for network adapter management. Its clean architecture, comprehensive state tracking, and support for multiple adapters make it suitable for both simple and complex network configurations.

The implementation follows production-quality practices while maintaining compatibility with the no_std environment and OXIDE OS architecture.

---

**Author:** ShadePacket - Networking stack engineer  
**Date:** 2026-02-03  
**Version:** 1.0
