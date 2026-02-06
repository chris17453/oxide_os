# DNS and Name Resolution - Testing and Validation

## Quick Testing Guide

### 1. Build System

```bash
cd /home/runner/work/oxide_os/oxide_os
make build-full
```

### 2. Test resolvd Service

After booting the system:

```bash
# Check if resolvd is running
ps aux | grep resolvd

# Check logs
cat /var/log/resolvd.log

# Check PID file
cat /var/run/resolvd.pid

# Verify service status
service status resolvd
```

### 3. Test hostctl Utility

```bash
# List existing hosts
hostctl list

# Add a test entry
hostctl add testserver 192.168.1.100

# Look up the entry
hostctl lookup testserver

# Verify in hosts file
cat /etc/hosts

# Remove the entry
hostctl remove testserver

# Verify it's gone
hostctl list
```

### 4. Test DNS Resolution

```bash
# Basic DNS lookup
nslookup google.com

# Check cache behavior (second query should be faster)
time nslookup example.com
time nslookup example.com

# Test reverse DNS
nslookup 8.8.8.8

# Test with different DNS server
nslookup -server=1.1.1.1 google.com
```

### 5. Test /etc/hosts Priority

```bash
# Add local entry that overrides DNS
hostctl add google.com 127.0.0.1

# Verify it resolves to local IP
nslookup google.com

# Remove and verify DNS resolution returns
hostctl remove google.com
nslookup google.com
```

### 6. Verify Enhanced /etc/hosts

```bash
# Check default entries
cat /etc/hosts

# Should see:
# - localhost entries (IPv4 and IPv6)
# - IPv6 special addresses (ip6-localnet, etc.)
```

### 7. Test Service Integration

```bash
# Restart resolvd
service restart resolvd

# Check logs for startup message
tail /var/log/resolvd.log

# Verify DNS servers are read from /etc/resolv.conf
cat /etc/resolv.conf
```

## Expected Results

### resolvd Service
- Should start automatically on boot
- Should log to `/var/log/resolvd.log`
- Should create PID file at `/var/run/resolvd.pid`
- Should display configured DNS servers on startup
- Should print statistics periodically

### hostctl Utility
- `hostctl list` should show formatted table
- `hostctl add` should update /etc/hosts
- `hostctl remove` should remove entries
- `hostctl lookup` should find hostnames
- Should prevent modification of localhost entries

### DNS Resolution
- Local /etc/hosts should take priority
- DNS queries should be cached
- Second query to same hostname should be faster
- Statistics should show cache hits/misses

### Enhanced /etc/hosts
- Should contain comprehensive IPv4/IPv6 localhost entries
- Should include IPv6 special addresses
- Should have comments explaining sections

## Common Issues

### resolvd Not Starting
- Check service manager logs: `journalctl -u resolvd`
- Verify binary exists: `ls -l /bin/resolvd`
- Check permissions: `ls -l /var/run/`

### hostctl Errors
- Verify /etc/hosts exists and is writable
- Check for invalid hostname/IP format
- Ensure not trying to modify localhost

### DNS Not Resolving
- Verify /etc/resolv.conf has nameservers
- Test network connectivity: `ping 8.8.8.8`
- Check resolvd is running: `ps aux | grep resolvd`

## Performance Benchmarks

Expected resolution times:
- Cache hit: < 1ms
- /etc/hosts lookup: < 5ms
- DNS query (first time): 20-100ms
- DNS query (cached): < 1ms

## Security Checks

- [ ] PID file has correct permissions (644)
- [ ] Log file has correct permissions (644)
- [ ] /etc/hosts backup is created before modifications
- [ ] localhost entries cannot be removed
- [ ] Invalid hostnames/IPs are rejected
- [ ] Cache size is limited to prevent exhaustion

## Integration Tests

```bash
# Test full workflow
hostctl add myapp.local 10.0.0.5
ping myapp.local
nslookup myapp.local
hostctl list | grep myapp
hostctl remove myapp.local
```

## Documentation Checklist

- [x] DNS_RESOLUTION.md covers all features
- [x] resolvd README.md explains service
- [x] Usage examples provided
- [x] Troubleshooting guide included
- [x] Security features documented
- [x] Performance characteristics noted

## Known Limitations

1. resolvd currently doesn't listen on socket for IPC (future enhancement)
2. Cache uses simple LRU (oldest entry removed when full)
3. No DNSSEC validation yet
4. No DNS-over-TLS support yet
5. IPv6 AAAA queries supported but not extensively tested

## Future Enhancements

See docs/DNS_RESOLUTION.md section "Future Enhancements" for roadmap.
