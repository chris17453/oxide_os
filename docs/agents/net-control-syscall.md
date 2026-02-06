# NET_CONTROL Syscall Documentation

## Overview

The `NET_CONTROL` syscall (number 310) allows userspace programs to trigger kernel-level network operations, primarily DHCP lease acquisition.

## Background

At boot time, the kernel attempts DHCP with a ~2 second timeout and limited retries. If the network isn't ready (e.g., QEMU's virtio-net hasn't initialized, or the DHCP server is slow), this boot-time DHCP fails silently. Previously, there was no way for userspace to retry DHCP.

## Syscall Interface

### Kernel Side

```rust
// kernel/syscall/syscall/src/lib.rs
pub const NET_CONTROL: u64 = 310;

// kernel/syscall/syscall/src/socket.rs
pub fn sys_net_control(operation: u64, iface_ptr: u64, iface_len: usize) -> i64
```

### Operations

| Operation | Value | Description |
|-----------|-------|-------------|
| `NET_OP_DHCP_REQUEST` | 1 | Trigger DHCP lease acquisition |
| `NET_OP_DHCP_RELEASE` | 2 | Release DHCP lease (future) |
| `NET_OP_DHCP_RENEW` | 3 | Renew DHCP lease (future) |

### Return Values

| Value | Meaning |
|-------|---------|
| 0 | Success |
| -19 (ENODEV) | Interface not found |
| -100 (ENETDOWN) | Network is down |
| -101 (ENETUNREACH) | Network unreachable |
| -110 (ETIMEDOUT) | DHCP timed out |

### Userspace (libc)

```rust
// userspace/libs/libc/src/syscall.rs
pub fn dhcp_request(iface: &str) -> i32
pub fn sys_net_control(op: u64, iface: &str) -> i32
```

## Lease File Format

On successful DHCP, the kernel writes the lease to `/var/lib/dhcp/<interface>.lease`:

```
ip=192.168.1.100
netmask=255.255.255.0
gateway=192.168.1.1
dns=8.8.8.8
dns=8.8.4.4
server=192.168.1.1
lease_time=86400
renewal_time=43200
rebinding_time=75600
```

## Usage Examples

### From Shell (dhclient utility)

```bash
# Request DHCP for eth0
dhclient eth0

# Verbose mode
dhclient -v eth0
```

### From Code (networkd)

```rust
use libc::syscall::dhcp_request;

// Trigger DHCP
let result = dhcp_request("eth0");
if result == 0 {
    // Read lease from /var/lib/dhcp/eth0.lease
}
```

## Implementation Files

- `kernel/syscall/syscall/src/lib.rs` - Syscall number and dispatch
- `kernel/syscall/syscall/src/socket.rs` - Handler implementation
- `userspace/libs/libc/src/syscall.rs` - Libc wrapper
- `userspace/services/networkd/src/manager.rs` - networkd integration
- `userspace/coreutils/src/bin/dhclient.rs` - CLI utility

## Design Decisions

1. **Blocking Operation**: The syscall blocks until DHCP completes or times out. This simplifies the API but means callers should be prepared for delays (~5-10 seconds on timeout).

2. **Kernel Writes Lease File**: The kernel writes the lease file directly rather than returning the lease data to userspace. This ensures networkd can read the same format whether DHCP succeeded at boot or via syscall.

3. **Interface Name Validation**: The syscall validates interface names against the kernel's interface list, returning ENODEV if not found.

---
*— ShadePacket: Because network timeouts at boot shouldn't mean network timeouts forever.*
