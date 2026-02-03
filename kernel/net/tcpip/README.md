# TCP/IP Network Stack

A comprehensive, RFC-compliant TCP/IP network stack implementation for Oxide OS.

## Features

### Core TCP (RFC 793)
- ✅ Complete state machine (11 states)
- ✅ Three-way handshake (SYN, SYN-ACK, ACK)
- ✅ Graceful connection termination (FIN, TIME_WAIT)
- ✅ Flow control with sliding windows
- ✅ Sequence number validation
- ✅ Retransmission with exponential backoff

### TCP Extensions
- ✅ **RFC 1323** - Window Scaling (up to 1GB windows)
- ✅ **RFC 1323** - Timestamps for RTT measurement
- ⚠️ **RFC 2018** - SACK (parsing only, no generation)

### Congestion Control (RFC 5681)
- ✅ Slow Start
- ✅ Congestion Avoidance
- ✅ Fast Retransmit
- ✅ Fast Recovery
- ✅ Initial Window = 2*MSS

### RTT Estimation (RFC 6298)
- ✅ Karn's algorithm
- ✅ Smoothed RTT (SRTT)
- ✅ RTT Variation (RTTVAR)
- ✅ Dynamic RTO calculation
- ✅ Min RTO: 200ms, Max RTO: 60s

### Additional Features
- ✅ Nagle algorithm (configurable)
- ✅ Keep-alive (2-hour interval)
- ✅ Zero window probes
- ✅ Dynamic window management
- ✅ MSS negotiation

## Architecture

```
┌─────────────────────────────────────────────┐
│              Application Layer               │
│         (Socket API via syscalls)            │
└───────────────┬─────────────────────────────┘
                │
┌───────────────▼─────────────────────────────┐
│            TCP/IP Stack (tcpip)              │
│  ┌─────────────────────────────────────┐    │
│  │ TCP Connection Management           │    │
│  │  • State machine                    │    │
│  │  • Congestion control               │    │
│  │  • Flow control                     │    │
│  │  • Retransmission                   │    │
│  └─────────────────────────────────────┘    │
│  ┌─────────────────────────────────────┐    │
│  │ UDP Socket Management               │    │
│  └─────────────────────────────────────┘    │
│  ┌─────────────────────────────────────┐    │
│  │ IP Layer (IPv4)                     │    │
│  │  • Routing                          │    │
│  │  • Fragmentation                    │    │
│  │  • ICMP (ping)                      │    │
│  └─────────────────────────────────────┘    │
│  ┌─────────────────────────────────────┐    │
│  │ Link Layer                          │    │
│  │  • Ethernet                         │    │
│  │  • ARP cache                        │    │
│  └─────────────────────────────────────┘    │
└───────────────┬─────────────────────────────┘
                │
┌───────────────▼─────────────────────────────┐
│        Network Device Drivers                │
│       (virtio-net, e1000, etc.)              │
└─────────────────────────────────────────────┘
```

## Usage

### Initialize the stack

```rust
use tcpip::TcpIpStack;
use net::NetworkInterface;

// Create interface
let interface = Arc::new(NetworkInterface::new(device, mac, ip, netmask, gateway));

// Initialize stack
tcpip::init(interface);

// Poll for packets
loop {
    tcpip::poll()?;
}
```

### TCP Connection

```rust
use net::SocketAddr;

// Get stack instance
let stack = tcpip::stack().unwrap();

// Connect to remote host
let conn = stack.tcp_connect(SocketAddr::new(remote_ip, 80))?;

// Send data
conn.send(b"GET / HTTP/1.1\r\n\r\n")?;

// Receive response
let mut buf = [0u8; 1024];
let n = conn.recv(&mut buf)?;

// Close connection
conn.close()?;
```

### UDP Socket

```rust
// Bind UDP socket
let socket = stack.udp_bind(12345)?;

// Send datagram
socket.send_to(remote_ip, remote_port, b"Hello, UDP!")?;
```

### Ping (ICMP Echo)

```rust
// Send ping
stack.send_ping(remote_ip, id, seq, b"PING")?;
```

## TCP Connection States

```
     ┌─────────┐
     │ CLOSED  │◄─────────────────────────┐
     └────┬────┘                          │
          │                               │
     [passive open]                  [close/RST]
          │                               │
     ┌────▼────┐                          │
     │ LISTEN  │                          │
     └────┬────┘                          │
          │                               │
   [rcv SYN/send SYN-ACK]                │
          │                               │
     ┌────▼──────────┐                    │
     │ SYN_RECEIVED  ├────────────────────┤
     └────┬──────────┘                    │
          │                               │
     [rcv ACK]                            │
          │                               │
   ┌──────▼──────────┐              [send FIN]
   │  ESTABLISHED    ├─────────────────►  │
   └──────┬──────────┘                    │
          │                               │
    [close/send FIN]                 ┌────▼────────┐
          │                          │  FIN_WAIT_1 │
     ┌────▼────────┐                 └────┬────────┘
     │ CLOSE_WAIT  │                      │
     └────┬────────┘                 [rcv ACK]
          │                               │
    [close/send FIN]                 ┌────▼────────┐
          │                          │  FIN_WAIT_2 │
     ┌────▼────────┐                 └────┬────────┘
     │  LAST_ACK   │                      │
     └────┬────────┘                 [rcv FIN]
          │                               │
      [rcv ACK]                      ┌────▼────────┐
          │                          │  TIME_WAIT  │
          │                          │  (2*MSL)    │
          │                          └────┬────────┘
          │                               │
          └───────────────────────────────┘
```

## Congestion Control Algorithm

### Slow Start Phase
```rust
while cwnd < ssthresh {
    // For each ACK received:
    cwnd += MSS;
}
```

### Congestion Avoidance Phase
```rust
while cwnd >= ssthresh {
    // For each ACK received:
    cwnd += (MSS * MSS) / cwnd;
}
```

### Fast Retransmit & Recovery
```rust
if dup_ack_count == 3 {
    // Enter fast recovery
    ssthresh = max(cwnd / 2, 2 * MSS);
    cwnd = ssthresh + 3 * MSS;
    
    // Retransmit lost segment
    retransmit_first_unacked();
    
    // For each additional dup ACK:
    cwnd += MSS;
}
```

### Timeout Recovery
```rust
on_timeout() {
    ssthresh = max(cwnd / 2, 2 * MSS);
    cwnd = MSS;
    rto = rto * 2;  // Exponential backoff
}
```

## Performance Characteristics

### Per-Connection Memory
- TcpConnection struct: ~200 bytes
- Send buffer: up to 64KB
- Receive buffer: up to 64KB
- Retransmit queue: variable (≤ cwnd)

### Throughput
- Limited by window size and RTT
- Theoretical max with window scaling: ~1 Gbps at 1ms RTT

### CPU Usage
- No zero-copy (segments copied multiple times)
- BTreeMap for connection lookup: O(log n)
- Timer processing: O(n) per poll cycle

## Limitations

### Current Limitations
1. **No clock source** - Timestamps return 0
2. **No SACK generation** - Can parse but not generate SACK blocks
3. **No OOO reassembly** - Out-of-order segments not queued
4. **No Path MTU Discovery** - Uses fixed MSS
5. **No ECN support** - Congestion notification not implemented

### Integration Required
- Timer infrastructure hookup
- Network device driver integration
- Syscall interface for userspace
- Loopback device

## Testing

### Manual Testing
```bash
# Build
cargo build -p tcpip

# Run clippy
cargo clippy -p tcpip --no-deps
```

### Integration Testing
Requires full kernel build and QEMU network setup.

## Code Organization

```
src/
├── lib.rs          # Stack management, packet routing
├── tcp.rs          # TCP protocol implementation
├── udp.rs          # UDP protocol implementation
├── ip.rs           # IPv4 packet handling
├── icmp.rs         # ICMP protocol (ping)
├── ethernet.rs     # Ethernet frame handling
├── arp.rs          # ARP cache and protocol
├── checksum.rs     # Internet checksum (RFC 1071)
├── conntrack.rs    # Connection tracking
├── filter.rs       # Packet filtering (firewall)
└── dhcp_client.rs  # DHCP client
```

## RFC Compliance

See [docs/subsystems/tcp_compliance.md](../../docs/subsystems/tcp_compliance.md) for detailed compliance information.

## Contributing

When making changes:
1. Maintain RFC compliance
2. Add inline documentation with persona signatures
3. Update compliance documentation
4. Test with `cargo build -p tcpip` and `cargo clippy`

## License

Part of Oxide OS - see LICENSE in repository root.

---

**Maintainers:**
- GraveShift - Kernel systems architect
- BlackLatch - OS hardening + exploit defense
- SableWire - Firmware + hardware interface
- TorqueJax - Driver engineer
- WireSaint - Storage systems + filesystems
- ShadePacket - Networking stack engineer
- NeonRoot - System integration + platform stability
