# DNS Resolution Agent Rules

## Overview

This document describes the implementation rules for DNS name resolution and local hostname management in OXIDE OS.

## Core Principles

### Resolution Order (MUST follow)

DNS resolution **MUST** follow this strict priority order:

1. **In-memory cache** (if available and not expired)
2. **`/etc/hosts` file** (local static mappings)
3. **Upstream DNS servers** (from `/etc/resolv.conf`)

**Rationale**: This order ensures:
- Fastest possible resolution for repeated queries
- Local overrides take precedence over external DNS
- Standard Unix behavior for hostname resolution

### Cache Management (MUST implement)

DNS caches **MUST**:

1. Respect TTL values from DNS responses
2. Implement size limits to prevent memory exhaustion
3. Expire entries when TTL is reached
4. Provide cache statistics for monitoring

**Rationale**: Caching dramatically improves performance and reduces network traffic, but must be bounded and time-limited to prevent stale data.

### File Integrity (MUST preserve)

When modifying `/etc/hosts`:

1. **MUST** create backup before modification (`.bak` file)
2. **MUST** preserve localhost entries (127.0.0.1, ::1)
3. **MUST** validate hostname and IP format before writing
4. **MUST NOT** allow removal of localhost entries

**Rationale**: Corrupting `/etc/hosts` can break system functionality. Localhost must always resolve.

### Service Integration (MUST follow)

The resolver daemon:

1. **MUST** be started by servicemgr automatically
2. **MUST** create PID file at `/var/run/resolvd.pid`
3. **MUST** log to `/var/log/resolvd.log`
4. **MUST** be restartable without system reboot
5. **MUST** exit cleanly on SIGTERM

**Rationale**: Standard service behavior for monitoring, management, and debugging.

## Implementation Details

### resolvd Daemon

**Location**: `userspace/services/resolvd/`

**Must Have**:
- Cache with TTL-based expiry
- Statistics tracking (hits/misses/queries)
- Periodic cache cleanup
- Dual logging (file + stdout)
- PID file creation

**Must Not**:
- Block indefinitely on DNS queries
- Exhaust memory with unbounded cache
- Crash on malformed responses
- Allow DNS cache poisoning

### hostctl Utility

**Location**: `userspace/coreutils/src/bin/hostctl.rs`

**Commands Must Support**:
- `add <hostname> <ip>` - Add/update entry
- `remove <hostname>` - Remove entry (except localhost)
- `list` - Show all entries in table format
- `lookup <hostname>` - Find IP for hostname
- `clear` - Remove non-localhost entries

**Must Validate**:
- Hostname format (alphanumeric, dots, hyphens, underscores)
- IP address format (valid IPv4)
- Not modifying protected entries (localhost)

### /etc/hosts Format

**Must Include** (default entries):
```
127.0.0.1       localhost localhost.localdomain
::1             localhost localhost.localdomain ip6-localhost ip6-loopback
fe00::0         ip6-localnet
ff00::0         ip6-mcastprefix
ff02::1         ip6-allnodes
ff02::2         ip6-allrouters
```

**Format Rules**:
- Comments start with `#`
- Format: `<ip> <hostname> [aliases...]`
- One entry per line
- Whitespace separates fields

### /etc/resolv.conf Format

**Must Support**:
- `nameserver <ip>` - DNS server (up to 3)
- Comments with `#`
- Lines without nameserver are ignored

**Read by**: 
- `libc::dns::get_dns_servers()`
- networkd (when writing)
- resolvd (when starting)

## Security Considerations

### ColdCipher Security Rules

**Input Validation** (MUST enforce):
- Hostname length ≤ 253 characters
- IP octets 0-255
- No special characters except `.`, `-`, `_`
- Reject buffer overflow attempts

**Resource Limits** (MUST enforce):
- Cache size ≤ 256 entries
- File size ≤ 4KB for /etc/hosts
- Query timeout ≤ 5 seconds
- Log rotation enabled

**Audit Trail** (MUST log):
- All modifications to /etc/hosts
- DNS query failures
- Cache hits/misses
- Service start/stop

## Error Handling

**Must Handle Gracefully**:
- Missing /etc/hosts (create default)
- Missing /etc/resolv.conf (use default 8.8.8.8)
- Network unreachable (return error, don't crash)
- Malformed DNS responses (log and skip)
- Full cache (evict oldest entry)

## Testing Requirements

**Must Test**:
- Resolution order (cache → hosts → DNS)
- Cache expiry
- hostctl all commands
- Service restart without data loss
- Invalid input rejection
- Localhost protection

## Performance Requirements

**Target Latencies**:
- Cache hit: < 1ms
- /etc/hosts: < 5ms
- DNS query: < 100ms

**Must Monitor**:
- Cache hit rate (should be > 70%)
- Query failures
- Average resolution time

## Compatibility

**Must Be Compatible With**:
- Standard libc DNS functions
- POSIX /etc/hosts format
- Standard /etc/resolv.conf format
- Existing applications (no changes needed)

## References

- RFC 1035: Domain Names - Implementation and Specification
- hosts(5) man page
- resolv.conf(5) man page
- OXIDE OS: `docs/DNS_RESOLUTION.md`
- OXIDE OS: `docs/DNS_TESTING.md`

---

**Persona**: ColdCipher - Security & name resolution specialist

These rules ensure reliable, secure, and performant DNS resolution that integrates seamlessly with existing OXIDE OS infrastructure.
