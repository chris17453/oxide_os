# SSH Server and Firewall Implementation Plan

## Status Overview

| Phase | Description | Status |
|-------|-------------|--------|
| SSH-1 | Fix crypto primitives (SHA-512, HMAC, Ed25519) | ✅ Complete |
| FW-1 | Add filter hooks to network stack | ✅ Complete |
| FW-2 | Filter rule engine | ✅ Complete |
| SSH-2 | SSH daemon structure and transport layer | 🔄 In Progress |
| SSH-3 | Key exchange (curve25519-sha256) | ⏳ Pending |
| SSH-4 | Authentication (password) | ⏳ Pending |
| SSH-5 | Channels, PTY, shell sessions | ⏳ Pending |
| FW-3 | Connection tracking | ⏳ Pending |
| FW-4 | Firewall syscalls | ⏳ Pending |
| FW-5 | fw CLI tool | ⏳ Pending |
| FW-6 | Boot integration | ⏳ Pending |

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

- [ ] Implement curve25519-sha256 key exchange
- [ ] Implement session key derivation
- [ ] Implement host key signing/verification
- [ ] Enable encrypted transport

---

## Phase FW-2: Filter Rule Engine

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

- [ ] Implement FilterRule struct
- [ ] Implement rule matching logic
- [ ] Implement rule chain evaluation
- [ ] Add default policies per chain

---

## Phase SSH-4: Authentication

### Tasks

- [ ] Implement password authentication
- [ ] Integrate with `/etc/passwd` checking
- [ ] Handle authentication failure limits
- [ ] (Future) Public key authentication

---

## Phase SSH-5: Channels and Sessions

### Tasks

- [ ] Implement channel multiplexing (RFC 4254)
- [ ] Implement PTY-req handling
- [ ] Implement shell channel
- [ ] Implement session management

---

## Phase FW-3: Connection Tracking

### Tasks

- [ ] Implement connection state table
- [ ] Track TCP connection states
- [ ] Track UDP "connections" by timeout
- [ ] Enable stateful rule matching

---

## Phase FW-4: Firewall Syscalls

### Syscalls

| Syscall | Purpose |
|---------|---------|
| `sys_fw_add_rule` | Add a filter rule |
| `sys_fw_del_rule` | Delete a filter rule |
| `sys_fw_list_rules` | List all rules |
| `sys_fw_set_policy` | Set chain default policy |
| `sys_fw_flush` | Flush all rules in chain |

### Tasks

- [ ] Add syscall numbers
- [ ] Implement syscall handlers
- [ ] Add root-only permission check

---

## Phase FW-5: fw CLI Tool

### Commands

```bash
fw add input -p tcp --dport 22 -j accept
fw add input -m state --state established -j accept
fw add input -j drop
fw policy input drop
fw list
fw flush input
fw save > /etc/fw.rules
fw restore < /etc/fw.rules
```

### Tasks

- [ ] Create `userspace/coreutils/src/bin/fw.rs`
- [ ] Implement command parsing
- [ ] Implement rule file format

---

## Phase FW-6: Boot Integration

### Tasks

- [ ] Modify `userspace/init/src/main.rs`
- [ ] Load `/etc/fw.rules` at boot
- [ ] Start sshd at boot

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
| Init | `userspace/init/src/main.rs` |
