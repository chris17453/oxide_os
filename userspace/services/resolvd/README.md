# OXIDE Resolver Daemon (resolvd)

## Overview

`resolvd` is a local caching DNS resolver service for OXIDE OS that provides:

- **DNS Caching**: Reduces network traffic by caching DNS query results with TTL expiry
- **Local Resolution**: Checks `/etc/hosts` before querying upstream DNS servers
- **Statistics**: Tracks cache hits, misses, and resolution attempts
- **Automatic Cleanup**: Periodically expires old cache entries

## Architecture

ColdCipher: Security-focused DNS resolution with defense in depth — GhostPatch

The resolver follows this resolution order:
1. Check in-memory cache (with TTL validation)
2. Check `/etc/hosts` for static mappings
3. Query upstream DNS servers from `/etc/resolv.conf`
4. Cache successful results for future lookups

## Usage

### Starting the Service

The resolver daemon is automatically started by the init system. To start manually:

```bash
/sbin/resolvd &
```

### Configuration

- **DNS Servers**: Configure in `/etc/resolv.conf`
- **Static Mappings**: Configure in `/etc/hosts`
- **Cache Settings**: Compile-time constants in `src/main.rs`

### Monitoring

The daemon logs to `/var/log/resolvd.log` and periodically prints statistics:

```
[resolvd] Statistics:
  Cache hits:     42
  Cache misses:   15
  Hosts lookups:  8
  DNS queries:    15
  Failed queries: 2
  Cache entries:  15
```

### PID File

The daemon writes its PID to `/var/run/resolvd.pid` for service management.

## Cache Configuration

Default settings (can be modified in source):

- **MAX_CACHE_ENTRIES**: 256 entries
- **DEFAULT_TTL_SECS**: 300 seconds (5 minutes)
- **CLEANUP_INTERVAL_US**: 60 seconds

## Integration

The resolver uses the existing libc DNS functions:
- `libc::dns::resolve()` - Query DNS servers
- `libc::dns::lookup_hosts_file()` - Check /etc/hosts
- `libc::dns::get_dns_servers()` - Read /etc/resolv.conf

Applications continue to use the standard `libc::dns` functions, which benefit from the daemon's cache.

## Security

ColdCipher: Defense mechanisms implemented:
- Cache size limits prevent memory exhaustion
- Input validation on all hostname lookups
- Backup creation before modifying configuration
- Separate logging for audit trails

## Performance

The caching resolver significantly reduces DNS query latency:
- Cache hits: < 1ms (memory lookup)
- /etc/hosts: < 5ms (file read)
- DNS queries: 20-100ms (network round-trip)

## Future Enhancements

- Socket-based IPC for resolver queries
- Advanced cache eviction policies (LRU)
- DNS-over-TLS support
- DNSSEC validation
- Negative caching (NXDOMAIN)
- Query deduplication
