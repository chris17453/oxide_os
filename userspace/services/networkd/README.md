# OXIDE Network Manager (networkd)

## Overview

The OXIDE Network Manager (`networkd`) is a multi-adapter network management service that handles configuration, monitoring, and lifecycle management of all network interfaces in the system.

## Architecture

### Components

#### 1. NetworkAdapter (`adapter.rs`)
- Represents individual network adapters
- Tracks adapter state (Down, Configuring, Up, Error, Removing)
- Manages IPv4 configuration (address, netmask, gateway)
- Collects DNS server information
- Monitors link status and statistics
- Reads hardware information from sysfs

#### 2. AdapterManager (`manager.rs`)
- Central registry for all network adapters
- Handles adapter discovery from `/sys/class/net`
- Coordinates adapter configuration
- Manages DHCP lease processing
- Collects DNS servers across all adapters
- Provides adapter lifecycle operations (up/down/reload)

#### 3. Configuration Parser (`config.rs`)
- Reads interface configuration files
- Parses key=value configuration format
- Supports multiple configuration modes
- Validates IP addresses and network parameters

## Configuration

### Configuration Files

Each network interface can have a configuration file at:
```
/etc/network/<interface>.conf
```

Example: `/etc/network/eth0.conf`

### Configuration Format

```ini
# Network interface configuration
mode=static|dhcp|manual

# Static configuration (required if mode=static)
address=192.168.1.100
netmask=255.255.255.0
gateway=192.168.1.1

# DNS servers (up to 3)
dns=8.8.8.8
dns=8.8.4.4

# MTU (optional, default: 1500)
mtu=1500
```

### Configuration Modes

- **static** - Static IP configuration (requires address, netmask)
- **dhcp** - Dynamic configuration via DHCP
- **manual** - No automatic configuration
- **loopback** - Special mode for loopback interface (auto-configured)

### Special Interfaces

#### Loopback (lo)
- Automatically configured with 127.0.0.1/255.0.0.0
- MTU: 65536
- Always brought up during daemon startup

## Usage

### Running as Daemon

Started automatically by init or service manager:
```bash
networkd daemon
```

### Command-Line Interface

#### List All Adapters
```bash
networkd list
```
Shows a summary table of all network adapters with their state, link status, and IP address.

#### Show Network Status
```bash
networkd status
```
Displays detailed status for all adapters including:
- State and link status
- MAC address
- IPv4 address and netmask
- Gateway configuration
- DNS servers
- TX/RX statistics

#### Show Adapter Configuration
```bash
networkd config eth0
```
Displays the current configuration for a specific adapter.

#### Bring Adapter Up
```bash
networkd up eth0
```
Brings the specified adapter up and applies its configuration.

#### Bring Adapter Down
```bash
networkd down eth0
```
Brings the specified adapter down and removes its configuration.

#### Reload Configuration
```bash
networkd reload [interface]
```
Reloads configuration for all adapters (if no interface specified) or a specific adapter.

#### Show Help
```bash
networkd help
```

## Runtime Behavior

### Startup Sequence

1. Create necessary directories
   - `/var/log/` - For log file
   - `/var/lib/dhcp/` - For DHCP leases
   - `/var/lib/networkd/` - For state persistence
   - `/etc/network/` - For configuration files
   - `/run/network/` - For active runtime configuration

2. Initialize AdapterManager

3. Discover all network adapters from `/sys/class/net`

4. Configure loopback interface

5. Configure each discovered adapter:
   - Read configuration file
   - Apply static configuration or initiate DHCP
   - Set adapter state

6. Write `/etc/resolv.conf` with collected DNS servers

7. Enter monitoring loop

### Monitoring Loop

The daemon runs a continuous monitoring loop (60-second intervals) that:
- Updates adapter statistics
- Monitors link status changes
- Checks for DHCP lease renewals (future)
- Detects adapter hotplug events (future)

## File Locations

### Configuration Files
- `/etc/network/<interface>.conf` - Per-interface configuration

### Runtime State
- `/run/network/<interface>.conf` - Active adapter configuration (for kernel)
- `/var/lib/dhcp/<interface>.lease` - DHCP lease information

### Logs
- `/var/log/networkd.log` - Service log file

### System Information (Read-Only)
- `/sys/class/net/` - Adapter enumeration
- `/sys/class/net/<interface>/carrier` - Link status
- `/sys/class/net/<interface>/address` - MAC address
- `/sys/class/net/<interface>/statistics/` - Network statistics

## Multi-Adapter Support

The network manager is designed to handle multiple adapters simultaneously:

### Adapter Discovery
- Automatic discovery via sysfs
- Fallback to defaults (lo, eth0) if sysfs unavailable
- New adapters detected on reload

### State Management
- Each adapter has independent state tracking
- State transitions are logged for debugging
- Error states are preserved for diagnostics

### Configuration Independence
- Each adapter has its own configuration file
- Adapters can use different configuration modes
- DNS servers are collected from all active adapters

### Statistics Tracking
- Per-adapter RX/TX packet counts
- Per-adapter RX/TX byte counts
- Independent link status monitoring

## DHCP Integration

The network manager supports DHCP through lease files:

1. DHCP client (separate process) obtains lease
2. Writes lease to `/var/lib/dhcp/<interface>.lease`
3. Network manager reads and applies lease
4. Configuration is applied to adapter
5. DNS servers are extracted and written to resolv.conf

Lease file format:
```ini
address=192.168.1.100
netmask=255.255.255.0
gateway=192.168.1.1
dns=192.168.1.1
dns=8.8.8.8
```

## DNS Configuration

The network manager automatically generates `/etc/resolv.conf`:

### DNS Server Collection
1. Collect DNS servers from all adapters in Up state
2. Remove duplicates
3. Add default DNS servers (8.8.8.8, 8.8.4.4) if none configured

### resolv.conf Format
```
# Generated by OXIDE networkd
# Do not edit manually

nameserver 192.168.1.1
nameserver 8.8.8.8
nameserver 8.8.4.4
```

## Adapter State Machine

```
     ┌──────────┐
     │   Down   │
     └────┬─────┘
          │ bring up / configure
          ▼
  ┌──────────────┐
  │ Configuring  │
  └──────┬───────┘
         │ success        error
         ▼                  │
     ┌──────┐             ┌───────┐
     │  Up  │◄────────────│ Error │
     └──┬───┘   reload    └───────┘
        │
        │ bring down
        ▼
    ┌──────────┐
    │   Down   │
    └──────────┘
```

## Examples

### Example 1: Static Configuration

Create `/etc/network/eth0.conf`:
```ini
mode=static
address=192.168.1.100
netmask=255.255.255.0
gateway=192.168.1.1
dns=8.8.8.8
dns=8.8.4.4
```

Apply configuration:
```bash
networkd reload eth0
# or
networkd down eth0
networkd up eth0
```

### Example 2: DHCP Configuration

Create `/etc/network/eth0.conf`:
```ini
mode=dhcp
```

The adapter will obtain configuration from DHCP server.

### Example 3: Multiple Adapters

Configure multiple interfaces:

`/etc/network/eth0.conf`:
```ini
mode=dhcp
```

`/etc/network/eth1.conf`:
```ini
mode=static
address=10.0.0.10
netmask=255.255.255.0
gateway=10.0.0.1
dns=10.0.0.1
```

Both adapters will be configured independently.

## Troubleshooting

### Check Service Status
```bash
networkd status
```

### View Service Log
```bash
cat /var/log/networkd.log
```

### Check Adapter Configuration
```bash
networkd config eth0
```

### Check Active Configuration
```bash
cat /run/network/eth0.conf
```

### Reload Configuration
```bash
networkd reload eth0
```

### Common Issues

1. **Adapter not found**
   - Check `/sys/class/net/` for available adapters
   - Ensure adapter name matches configuration file name

2. **Static IP not applied**
   - Verify configuration file syntax
   - Check that address and netmask are provided
   - Look for errors in `/var/log/networkd.log`

3. **DHCP not working**
   - Ensure DHCP client is running
   - Check for lease file in `/var/lib/dhcp/`
   - Verify network connectivity

4. **DNS not working**
   - Check `/etc/resolv.conf` contents
   - Ensure at least one adapter is Up with DNS configuration
   - Default DNS (8.8.8.8, 8.8.4.4) used if none configured

## Future Enhancements

- [ ] Real-time DHCP lease renewal
- [ ] Adapter hotplug event handling
- [ ] IPv6 support
- [ ] Wireless adapter support
- [ ] VLAN configuration
- [ ] Bridge configuration
- [ ] Control socket for IPC
- [ ] Firewall integration
- [ ] Network monitoring hooks
- [ ] Configuration validation
- [ ] Systemd-style unit files

## Code Structure

```
userspace/services/networkd/
├── src/
│   ├── main.rs      # Entry point, CLI, daemon loop
│   ├── manager.rs   # AdapterManager - multi-adapter coordination
│   ├── adapter.rs   # NetworkAdapter - individual adapter representation
│   └── config.rs    # Configuration file parsing
├── Cargo.toml
└── README.md (this file)
```

## Integration

### Init Integration
The network manager should be started early in the boot process, after mounting filesystems but before network-dependent services.

### Service Manager Integration
Can be managed by a service manager (if available) with automatic restart on failure.

### Kernel Integration
Writes active configuration to `/run/network/` for kernel consumption. Future versions will use ioctl for direct kernel communication.

## Development Notes

### Persona: ShadePacket
This code follows the ShadePacket persona - a networking stack engineer focused on:
- Clean network protocols
- Efficient packet handling
- Robust adapter management
- Clear separation of concerns
- Production-quality reliability

### No-STD Environment
The service runs in a no_std environment and uses:
- Custom libc bindings
- Manual memory management
- Direct system calls
- Sysfs for hardware information

### Testing
Testing should cover:
- Single adapter scenarios
- Multiple adapter configurations
- Mixed static/DHCP configurations
- Link up/down events
- Configuration reload
- Error handling and recovery

---

**ShadePacket** - Networking stack engineer
