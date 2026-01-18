# Phase 12: Networking

**Stage:** 3 - Hardware
**Status:** Not Started
**Dependencies:** Phase 11 (Storage)

---

## Goal

Implement TCP/IP networking with socket API.

---

## Deliverables

| Item | Status |
|------|--------|
| Network device interface | [ ] |
| virtio-net driver | [ ] |
| smoltcp TCP/IP stack | [ ] |
| Socket syscalls | [ ] |
| DHCP client | [ ] |
| DNS resolver | [ ] |
| Loopback interface | [ ] |

---

## Architecture Status

| Arch | NetDev | virtio | TCP/IP | Sockets | Done |
|------|--------|--------|--------|---------|------|
| x86_64 | [ ] | [ ] | [ ] | [ ] | [ ] |
| i686 | [ ] | [ ] | [ ] | [ ] | [ ] |
| aarch64 | [ ] | [ ] | [ ] | [ ] | [ ] |
| arm | [ ] | [ ] | [ ] | [ ] | [ ] |
| mips64 | [ ] | [ ] | [ ] | [ ] | [ ] |
| mips32 | [ ] | [ ] | [ ] | [ ] | [ ] |
| riscv64 | [ ] | [ ] | [ ] | [ ] | [ ] |
| riscv32 | [ ] | [ ] | [ ] | [ ] | [ ] |

---

## Network Stack

```
┌─────────────────────────────┐
│      Application            │
│   (socket API)              │
└──────────────┬──────────────┘
               │
┌──────────────▼──────────────┐
│     Socket Layer            │
│  (TCP, UDP, RAW)            │
└──────────────┬──────────────┘
               │
┌──────────────▼──────────────┐
│    Transport Layer          │
│     (TCP, UDP)              │
└──────────────┬──────────────┘
               │
┌──────────────▼──────────────┐
│    Network Layer            │
│     (IPv4, IPv6)            │
└──────────────┬──────────────┘
               │
┌──────────────▼──────────────┐
│     Link Layer              │
│   (Ethernet, ARP)           │
└──────────────┬──────────────┘
               │
┌──────────────▼──────────────┐
│    Network Driver           │
│   (virtio-net, e1000)       │
└─────────────────────────────┘
```

---

## Socket Syscalls

| Number | Name | Args | Return |
|--------|------|------|--------|
| 60 | sys_socket | domain, type, protocol | fd or -errno |
| 61 | sys_bind | fd, addr, addrlen | 0 or -errno |
| 62 | sys_listen | fd, backlog | 0 or -errno |
| 63 | sys_accept | fd, addr, addrlen | newfd or -errno |
| 64 | sys_connect | fd, addr, addrlen | 0 or -errno |
| 65 | sys_send | fd, buf, len, flags | bytes or -errno |
| 66 | sys_recv | fd, buf, len, flags | bytes or -errno |
| 67 | sys_sendto | fd, buf, len, flags, addr, addrlen | bytes or -errno |
| 68 | sys_recvfrom | fd, buf, len, flags, addr, addrlen | bytes or -errno |
| 69 | sys_shutdown | fd, how | 0 or -errno |
| 70 | sys_setsockopt | fd, level, optname, optval, optlen | 0 or -errno |
| 71 | sys_getsockopt | fd, level, optname, optval, optlen | 0 or -errno |
| 72 | sys_getpeername | fd, addr, addrlen | 0 or -errno |
| 73 | sys_getsockname | fd, addr, addrlen | 0 or -errno |

---

## Network Device Interface

```rust
pub trait NetworkDevice: Send + Sync {
    /// Get MAC address
    fn mac_address(&self) -> [u8; 6];

    /// Maximum transmission unit
    fn mtu(&self) -> usize;

    /// Transmit a packet
    fn transmit(&self, packet: &[u8]) -> Result<()>;

    /// Receive a packet (non-blocking)
    fn receive(&self, buf: &mut [u8]) -> Result<Option<usize>>;

    /// Check link status
    fn link_up(&self) -> bool;
}
```

---

## virtio-net Driver

```rust
// virtio-net header prepended to packets
#[repr(C)]
struct VirtioNetHdr {
    flags: u8,
    gso_type: u8,
    hdr_len: u16,
    gso_size: u16,
    csum_start: u16,
    csum_offset: u16,
    num_buffers: u16,
}

// Virtqueues:
// - Queue 0: Receive
// - Queue 1: Transmit
// - Queue 2: Control (optional)
```

---

## TCP State Machine

```
                    ┌─────────────┐
                    │   CLOSED    │
                    └──────┬──────┘
           ┌───────────────┼───────────────┐
           │               │               │
           ▼               ▼               ▼
    ┌─────────────┐ ┌─────────────┐ ┌─────────────┐
    │   LISTEN    │ │  SYN_SENT   │ │  SYN_RCVD   │
    └──────┬──────┘ └──────┬──────┘ └──────┬──────┘
           │               │               │
           └───────────────┼───────────────┘
                           │
                    ┌──────▼──────┐
                    │ ESTABLISHED │
                    └──────┬──────┘
           ┌───────────────┼───────────────┐
           ▼               │               ▼
    ┌─────────────┐        │        ┌─────────────┐
    │  FIN_WAIT_1 │        │        │ CLOSE_WAIT  │
    └──────┬──────┘        │        └──────┬──────┘
           │               │               │
           ▼               │               ▼
    ┌─────────────┐        │        ┌─────────────┐
    │  FIN_WAIT_2 │        │        │  LAST_ACK   │
    └──────┬──────┘        │        └──────┬──────┘
           │               │               │
           └───────────────┼───────────────┘
                           │
                    ┌──────▼──────┐
                    │  TIME_WAIT  │
                    └──────┬──────┘
                           │
                    ┌──────▼──────┐
                    │   CLOSED    │
                    └─────────────┘
```

---

## Key Files

```
crates/net/efflux-net/src/
├── lib.rs
├── device.rs          # Network device trait
├── socket.rs          # Socket abstraction
└── interface.rs       # Network interface

crates/net/efflux-smoltcp/src/
├── lib.rs             # smoltcp integration
└── glue.rs            # Device/time glue

crates/drivers/net/efflux-virtio-net/src/
└── lib.rs

crates/net/efflux-dhcp/src/
└── lib.rs             # DHCP client

crates/net/efflux-dns/src/
└── lib.rs             # DNS resolver
```

---

## Socket Address Structures

```rust
#[repr(C)]
pub struct SockAddrIn {
    pub sin_family: u16,    // AF_INET
    pub sin_port: u16,      // Port (network byte order)
    pub sin_addr: u32,      // IPv4 address
    pub sin_zero: [u8; 8],  // Padding
}

#[repr(C)]
pub struct SockAddrIn6 {
    pub sin6_family: u16,   // AF_INET6
    pub sin6_port: u16,     // Port
    pub sin6_flowinfo: u32, // Flow info
    pub sin6_addr: [u8; 16],// IPv6 address
    pub sin6_scope_id: u32, // Scope ID
}
```

---

## Exit Criteria

- [ ] virtio-net transmits/receives packets
- [ ] DHCP obtains IP address
- [ ] TCP connections established
- [ ] UDP datagrams sent/received
- [ ] DNS resolves hostnames
- [ ] Loopback (127.0.0.1) works
- [ ] Works on all 8 architectures

---

## Test Program

```c
int main() {
    // TCP client
    int sock = socket(AF_INET, SOCK_STREAM, 0);

    struct sockaddr_in addr = {
        .sin_family = AF_INET,
        .sin_port = htons(80),
        .sin_addr.s_addr = inet_addr("93.184.216.34"), // example.com
    };

    if (connect(sock, (struct sockaddr*)&addr, sizeof(addr)) < 0) {
        perror("connect");
        return 1;
    }

    const char *request = "GET / HTTP/1.0\r\nHost: example.com\r\n\r\n";
    send(sock, request, strlen(request), 0);

    char buf[4096];
    int n = recv(sock, buf, sizeof(buf) - 1, 0);
    buf[n] = '\0';
    printf("%s", buf);

    close(sock);
    return 0;
}
```

---

## Notes

*Add implementation notes here as work progresses*

---

*Phase 12 of EFFLUX Implementation*
