# SSH Server and Firewall Implementation Plan

## Status Overview

| Phase | Description | Status |
|-------|-------------|--------|
| SSH-1 | Fix crypto primitives (SHA-512, HMAC, Ed25519) | ✅ Complete |
| FW-1 | Add filter hooks to network stack | ✅ Complete |
| FW-2 | Filter rule engine | ✅ Complete |
| SSH-2 | SSH daemon structure and transport layer | ✅ Complete |
| SSH-3 | Key exchange (curve25519-sha256) | ✅ Complete |
| SSH-4 | Authentication (password) | ✅ Complete |
| SSH-5 | Channels, PTY, shell sessions | ✅ Complete |
| SVCMGR | Service manager for OXIDE | ✅ Complete (binary renamed to `service`) |
| FW-3 | Connection tracking | ✅ Complete |
| FW-4 | Firewall syscalls | ✅ Complete |
| FW-5 | fw CLI tool | ✅ Complete |
| FW-6 | Boot integration | ✅ Complete |
| SSH-C | SSH client | ✅ Complete |

---

## Current State Analysis

### Crypto Crate Status

| Module | Status | Notes |
|--------|--------|-------|
| `lib.rs` | ✅ Complete | Error types, file headers |
| `aes.rs` | ✅ Complete | AES-256-GCM fully implemented |
| `chacha.rs` | ✅ Complete | ChaCha20-Poly1305 fully implemented |
| `random.rs` | ✅ Complete | ChaCha-based CSPRNG |
| `x25519.rs` | ✅ Complete | X25519 key exchange |
| `ed25519.rs` | ❌ Placeholder | SHA-512, curve ops are stubs |
| `argon2.rs` | ❌ Placeholder | Blake2b simplified |

**Critical Blockers:**
- `sha512()` in ed25519.rs is XOR-based, not FIPS 180-4
- All Ed25519 curve operations are placeholder stubs
- No HMAC implementation exists

### Network Stack Structure

**Inbound Packet Flow:**
```
poll() → process_packet() → process_ipv4() → process_tcp/udp/icmp()
```

**Outbound Packet Flow:**
```
send_ipv4_packet() → EthernetFrame::new() → device.transmit()
```

**Filter Hook Insertion Points:**
- Inbound: `process_ipv4()` at line ~184 (after header parse)
- Outbound: `send_ipv4_packet()` at line ~281 (before packet build)

---

## Phase SSH-1: Fix Crypto Primitives ✅

### Tasks

- [x] Implement proper SHA-512 (FIPS 180-4)
- [x] Implement SHA-256 (using SHA-512/256 or standalone)
- [x] Implement HMAC construction
- [x] Fix Ed25519 curve operations

### Files to Create

| File | Purpose |
|------|---------|
| `crates/security/crypto/src/sha512.rs` | FIPS 180-4 SHA-512/256 |
| `crates/security/crypto/src/hmac.rs` | HMAC-SHA-256/512 |

### Files to Modify

| File | Changes |
|------|---------|
| `crates/security/crypto/src/ed25519.rs` | Replace placeholder curve ops |
| `crates/security/crypto/src/lib.rs` | Add new module exports |

---

## Phase FW-1: Filter Hooks ✅

### Tasks

- [x] Add FilterVerdict enum (Accept, Drop, Reject)
- [x] Add filter hook in `process_ipv4()`
- [x] Add filter hook in `send_ipv4_packet()`
- [x] Create basic filter.rs module structure

### Files Created

| File | Purpose |
|------|---------|
| `crates/net/tcpip/src/filter.rs` | Filter rules and engine |

### Files Modified

| File | Changes |
|------|---------|
| `crates/net/tcpip/src/lib.rs` | Added filter hooks |

## Phase FW-2: Filter Rule Engine ✅

The filter rule engine was implemented as part of FW-1 in `filter.rs`. Includes:
- FilterRule struct with protocol, IP, port matching
- FilterTable with add/delete/flush operations
- Chain policies (INPUT, OUTPUT, FORWARD)
- Global filter table with thread-safe access

---

## Phase SSH-2: SSH Daemon Structure

### Directory Structure

```
userspace/sshd/
├── Cargo.toml
└── src/
    ├── main.rs      # Daemon entry, accept loop
    ├── transport.rs # Binary packet protocol (RFC 4253)
    ├── kex.rs       # Key exchange
    ├── auth.rs      # Authentication
    ├── channel.rs   # Channel multiplexing
    └── session.rs   # PTY allocation, shell exec
```

### Tasks

- [ ] Create sshd crate structure
- [ ] Implement SSH binary packet protocol
- [ ] Implement version exchange
- [ ] Implement algorithm negotiation

---

## Phase SSH-3: Key Exchange

### Supported Algorithms

| Type | Algorithm |
|------|-----------|
| KEX | curve25519-sha256 |
| Host Key | ssh-ed25519 |
| Cipher | chacha20-poly1305@openssh.com |
| MAC | (implicit with AEAD) |

### Tasks

- [x] Implement curve25519-sha256 key exchange
- [x] Implement session key derivation
- [x] Implement host key signing/verification
- [x] Enable encrypted transport

---

## Phase FW-2: Filter Rule Engine ✅

### Rule Structure

```rust
pub struct FilterRule {
    chain: FilterChain,      // Input, Output, Forward
    action: FilterVerdict,   // Accept, Drop, Reject
    protocol: Option<u8>,    // TCP=6, UDP=17, ICMP=1
    src_ip: Option<(Ipv4Addr, u8)>,  // IP + prefix
    dst_ip: Option<(Ipv4Addr, u8)>,
    src_port: Option<(u16, u16)>,    // Range
    dst_port: Option<(u16, u16)>,
    state: Option<ConnState>,        // NEW, ESTABLISHED, RELATED
}
```

### Tasks

- [x] Implement FilterRule struct
- [x] Implement rule matching logic
- [x] Implement rule chain evaluation
- [x] Add default policies per chain

---

## Phase SSH-4: Authentication ✅

### Tasks

- [x] Implement password authentication
- [x] Integrate with `/etc/passwd` checking
- [x] Handle authentication failure limits
- [ ] (Future) Public key authentication

---

## Phase SSH-5: Channels and Sessions ✅

### Tasks

- [x] Implement channel multiplexing (RFC 4254)
- [x] Implement PTY-req handling
- [x] Implement shell channel
- [x] Implement session management

---

## Service Manager ✅

Added `userspace/servicemgr/` with:
- Service definition loading from `/etc/services.d/`
- Start/stop/restart/status commands
- Daemon mode with auto-restart
- Default sshd service configuration

### Usage
```bash
servicemgr daemon        # Run as daemon (started by init)
servicemgr start sshd    # Start a service
servicemgr stop sshd     # Stop a service
servicemgr status        # Show all service status
servicemgr list          # List services
```

---

## Phase FW-3: Connection Tracking ✅

### Tasks

- [x] Implement connection state table
- [x] Track TCP connection states
- [x] Track UDP "connections" by timeout
- [x] Enable stateful rule matching

Implemented in `crates/net/tcpip/src/conntrack.rs` with full TCP state machine,
UDP timeout tracking, ICMP tracking, and integration with packet filter hooks.

---

## Phase FW-4: Firewall Syscalls ✅

### Syscalls

| Syscall | Purpose |
|---------|---------|
| `sys_fw_add_rule` | Add a filter rule |
| `sys_fw_del_rule` | Delete a filter rule |
| `sys_fw_list_rules` | List all rules |
| `sys_fw_set_policy` | Set chain default policy |
| `sys_fw_flush` | Flush all rules in chain |
| `sys_fw_get_conntrack` | Get connection tracking stats |

### Tasks

- [x] Add syscall numbers (200-205)
- [x] Implement syscall handlers
- [x] Add root-only permission check

Implemented in `crates/syscall/syscall/src/firewall.rs`.

---

## Phase FW-5: fw CLI Tool ✅

### Commands

```bash
fw add input -p tcp --dport 22 -j accept
fw add input -m state --state established -j accept
fw add input -j drop
fw policy input drop
fw list
fw flush input
fw save
fw restore /etc/fw.rules
fw conntrack
```

### Tasks

- [x] Create `userspace/coreutils/src/bin/fw.rs`
- [x] Implement command parsing
- [x] Implement rule file format
- [x] Implement restore from file

Full implementation with IP/CIDR matching, port ranges, stateful matching,
and save/restore functionality.

---

## Phase FW-6: Boot Integration ✅

### Tasks

- [x] Modify `userspace/init/src/main.rs`
- [x] Load `/etc/fw.rules` at boot
- [x] Start servicemgr daemon at boot

Init now loads firewall rules early in boot and starts the service manager
which can manage sshd and other services.

---

## Verification Tests

### SSH Testing

```bash
# From host with QEMU port forwarding
ssh -o StrictHostKeyChecking=no -p 2222 root@localhost
```

### Firewall Testing

```bash
# In OXIDE
fw add input -p tcp --dport 12345 -j drop
nc -l 12345 &
# Connection should fail
fw flush input
# Connection should succeed
```

---

## Phase SSH-C: SSH Client ✅

### Tasks

- [x] Implement client transport layer (version exchange order)
- [x] Implement client-side key exchange (send ECDH_INIT, receive ECDH_REPLY)
- [x] Implement host key verification (Ed25519)
- [x] Implement password authentication
- [x] Implement interactive shell session with PTY

### Directory Structure

```
userspace/ssh/
├── Cargo.toml
└── src/
    ├── main.rs      # CLI entry, argument parsing
    ├── transport.rs # Binary packet protocol (client side)
    ├── kex.rs       # Client-side key exchange
    ├── crypto.rs    # Crypto operations
    └── session.rs   # Channel, PTY, interactive session
```

### Usage

```bash
# Basic connection
ssh root@192.168.1.1

# Specify port
ssh -p 2222 user@hostname

# Verbose mode
ssh -v user@hostname

# Specify username
ssh -l root hostname
```

---

## Key Files Summary

| Purpose | File |
|---------|------|
| SHA-512 | `crates/security/crypto/src/sha512.rs` |
| HMAC | `crates/security/crypto/src/hmac.rs` |
| Ed25519 fix | `crates/security/crypto/src/ed25519.rs` |
| Filter hooks | `crates/net/tcpip/src/lib.rs` |
| Filter rules | `crates/net/tcpip/src/filter.rs` |
| Conntrack | `crates/net/tcpip/src/conntrack.rs` |
| FW syscalls | `crates/syscall/syscall/src/firewall.rs` |
| FW CLI | `userspace/coreutils/src/bin/fw.rs` |
| SSH daemon | `userspace/sshd/` |
| SSH client | `userspace/ssh/` |
| Init | `userspace/init/src/main.rs` |
