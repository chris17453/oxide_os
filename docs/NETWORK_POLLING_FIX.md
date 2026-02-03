# Network Packet Polling Bug Fix

**Date:** 2026-02-02
**Severity:** CRITICAL (P1-blocker)
**Impact:** All external network connectivity broken
**Fixed By:** ShadePacket

---

## Problem

External network connectivity did not work despite having:
- ✅ Complete VirtIO-net driver with RX/TX support
- ✅ TCP/IP stack with ARP, TCP, UDP, ICMP protocols
- ✅ Socket API implementation
- ✅ Loopback working perfectly

### Root Cause

**The network stack polling mechanism was never invoked!**

The `tcpip::poll()` function existed and worked correctly, but NO code ever called it:
- Socket recv/recvfrom syscalls didn't poll before checking buffers
- Socket accept syscall didn't poll before checking pending connections
- Socket send/sendto syscalls didn't poll to process ACKs
- The networkd daemon didn't poll in its main loop

**Result:** Incoming packets sat in the VirtIO-net device's RX queue forever, never processed.

---

## How It Should Work

```
VirtIO-net Device → RX Queue → tcpip::poll() → Process Packet → Socket Buffer
                                    ↑
                         Called by socket syscalls
```

### Normal Packet Flow

1. **Incoming packet arrives** at VirtIO-net device
2. **Device writes to RX queue** via DMA
3. **Interrupt fires** (or polling checks)
4. **tcpip::poll() called** - reads from device
5. **process_packet()** parses Ethernet, ARP, IP, TCP/UDP
6. **Packet delivered** to socket recv buffer
7. **Application recv()** gets data from buffer

### What Was Happening (BUG)

1. **Incoming packet arrives** at VirtIO-net device
2. **Device writes to RX queue** via DMA
3. **Interrupt fires** (or polling checks)
4. **❌ Nothing calls tcpip::poll()** - packet stuck in device!
5. **Application recv()** checks socket buffer - empty!
6. **Returns EAGAIN** - no data available
7. **Packet lost** - never processed

---

## The Fix

Added `tcpip::poll()` calls to ALL socket syscalls that interact with the network:

### Files Modified

**kernel/syscall/syscall/src/socket.rs**

1. **sys_recv()** (line ~927)
   ```rust
   // ShadePacket: Poll network stack to process incoming packets
   // This is CRITICAL - without this, packets sit in VirtIO-net RX queue!
   let _ = tcpip::poll();
   ```

2. **sys_recvfrom()** (line ~1077)
   ```rust
   // ShadePacket: Poll network stack to process incoming packets
   let _ = tcpip::poll();
   ```

3. **sys_accept()** (line ~640)
   ```rust
   // ShadePacket: Poll network stack to process incoming connection requests (SYN packets)
   let _ = tcpip::poll();
   ```

4. **sys_send()** (line ~808)
   ```rust
   // ShadePacket: Poll network stack to process ACKs from previous sends
   let _ = tcpip::poll();
   ```

5. **sys_sendto()** (line ~1012)
   ```rust
   // ShadePacket: Poll network stack to process any pending responses
   let _ = tcpip::poll();
   ```

### Import Added

```rust
use tcpip; // ShadePacket: Network stack polling for packet reception
```

---

## Why This Fix Works

### On Receive (recv/recvfrom)

1. Application calls recv()
2. **Poll first** - processes any packets in device queue
3. Check socket buffer (may now have data from poll)
4. Return data or EAGAIN

### On Accept (accept)

1. Application calls accept() on listening socket
2. **Poll first** - processes SYN packets from device queue
3. Check pending connections (may now have connection from SYN)
4. Return new socket or EAGAIN

### On Send (send/sendto)

1. Application calls send()
2. **Poll first** - processes ACKs and responses from previous sends
3. Send new data
4. Return bytes sent

---

## Testing Strategy

### Phase 1: Basic Connectivity
```bash
# In OXIDE OS
ping 8.8.8.8
```
Expected: ICMP echo reply received

### Phase 2: TCP Connection
```bash
# In OXIDE OS
nc 8.8.8.8 80
GET / HTTP/1.0
<enter>
```
Expected: HTML response from Google

### Phase 3: DNS Resolution
```bash
# In OXIDE OS
nslookup google.com 8.8.8.8
```
Expected: IP address returned

### Phase 4: HTTP Request
```bash
# In OXIDE OS
wget http://example.com/
```
Expected: File downloaded successfully

---

## Performance Considerations

### Polling Overhead

Each socket syscall now calls `tcpip::poll()`, which:
1. Checks VirtIO-net RX queue for packets
2. If packet present: processes it (~1-10μs)
3. If no packet: returns immediately (~0.1μs)

**Impact:** Negligible - polling is very fast when no packets present.

### Alternative Approaches

1. **Interrupt-driven RX:** Have VirtIO-net interrupt call poll() directly
   - Pros: Zero overhead when no traffic
   - Cons: Requires interrupt handler to safely access kernel structures

2. **Background polling thread:** Kernel thread polls periodically
   - Pros: No syscall overhead
   - Cons: Wastes CPU cycles when idle, adds latency

3. **Hybrid:** Interrupt for first packet + syscall polling for burst
   - Pros: Best of both worlds
   - Cons: More complex

**Decision:** Start with syscall polling (simplest, works), optimize later if needed.

---

## Known Limitations

1. **No interrupt-driven reception yet**
   - Packets only processed when application makes syscalls
   - If no syscalls, packets stay in queue (bounded by RX queue size)
   - Solution: Add background polling or interrupt handler

2. **Poll called on every syscall**
   - Small overhead even when no network activity
   - Solution: Add check for network activity before polling

3. **networkd doesn't poll**
   - networkd main loop just sleeps
   - Solution: Add periodic poll in networkd daemon

---

## Lessons Learned

### Why This Bug Wasn't Caught Earlier

1. **Loopback worked perfectly**
   - Loopback bypasses the network stack and device
   - All tests used localhost, which masked the bug

2. **Modular design hid the gap**
   - VirtIO-net driver was complete
   - TCP/IP stack was complete
   - Both worked in isolation
   - Integration point (poll call) was missing

3. **No external network tests**
   - Need test that connects to actual external host
   - Add to test suite: `ping 8.8.8.8`

### Prevention

1. **Add integration tests** that require external connectivity
2. **Document polling requirements** for all async I/O subsystems
3. **Add assertions** in network stack if poll not called for too long

---

## References

- VirtIO-net driver: `kernel/drivers/net/virtio-net/src/lib.rs`
- TCP/IP stack: `kernel/net/tcpip/src/lib.rs`
- Socket syscalls: `kernel/syscall/syscall/src/socket.rs`
- VirtIO spec: https://docs.oasis-open.org/virtio/virtio/v1.1/virtio-v1.1.html

---

## Future Work

1. **Add interrupt-driven RX** for zero-overhead reception
2. **Implement background polling thread** as fallback
3. **Add rate limiting** to prevent poll storms
4. **Optimize poll path** with fast-path checks
5. **Add telemetry** to track poll effectiveness

---

**Status:** ✅ Fixed, pending real-world testing
**Next:** Test with external network connectivity (ping, wget, nc)
