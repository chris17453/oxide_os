# Networking Stack

## Crates

| Crate | Purpose |
|-------|---------|
| `net` | Core networking abstractions (sockets, addresses) |
| `tcpip` | TCP/IP protocol stack (TCP, UDP, IP, ICMP, ARP) |
| `dhcp` | DHCP client for automatic IP configuration |
| `dns` | DNS resolver |
| `smb` | SMB protocol support (stub) |
| `nfs` | NFS client (stub) |
| `rdp` | Remote Desktop Protocol (7 sub-crates) |

## Architecture

The network stack follows a layered model: raw Ethernet frames from drivers
(virtio-net) feed into the `tcpip` crate which handles IP routing, TCP
connection state, and UDP. The `net` crate provides the socket API that
userspace calls via syscalls.

DHCP runs early in boot via `networkd` to obtain an IP address.
DNS resolution is available to userspace through libc's `getaddrinfo`.
