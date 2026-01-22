//! ip - Show/manipulate routing, network devices, interfaces and tunnels
//!
//! Modern replacement for ifconfig, route, arp, etc.

#![no_std]
#![no_main]

use libc::{printlns, prints};

fn show_usage() {
    printlns("Usage: ip [OPTIONS] OBJECT { COMMAND | help }");
    printlns("");
    printlns("OBJECT :=");
    printlns("  address   - protocol (IP or IPv6) address on a device");
    printlns("  link      - network device");
    printlns("  route     - routing table entry");
    printlns("  neighbor  - ARP or NDISC cache entry");
    printlns("");
    printlns("OPTIONS :=");
    printlns("  -4        - use IPv4");
    printlns("  -6        - use IPv6");
    printlns("  -h, -help - display this help");
    printlns("");
    printlns("Examples:");
    printlns("  ip addr show");
    printlns("  ip link show");
    printlns("  ip route show");
}

fn show_addr() {
    printlns(
        "1: lo: <LOOPBACK,UP,LOWER_UP> mtu 65536 qdisc noqueue state UNKNOWN group default qlen 1000",
    );
    printlns("    link/loopback 00:00:00:00:00:00 brd 00:00:00:00:00:00");
    printlns("    inet 127.0.0.1/8 scope host lo");
    printlns("       valid_lft forever preferred_lft forever");
    printlns("    inet6 ::1/128 scope host");
    printlns("       valid_lft forever preferred_lft forever");
    printlns(
        "2: eth0: <BROADCAST,MULTICAST,UP,LOWER_UP> mtu 1500 qdisc pfifo_fast state UP group default qlen 1000",
    );
    printlns("    link/ether 52:54:00:12:34:56 brd ff:ff:ff:ff:ff:ff");
    printlns("    inet 10.0.2.15/24 brd 10.0.2.255 scope global dynamic eth0");
    printlns("       valid_lft 86399sec preferred_lft 86399sec");
    printlns("    inet6 fe80::5054:ff:fe12:3456/64 scope link");
    printlns("       valid_lft forever preferred_lft forever");
}

fn show_link() {
    printlns(
        "1: lo: <LOOPBACK,UP,LOWER_UP> mtu 65536 qdisc noqueue state UNKNOWN mode DEFAULT group default qlen 1000",
    );
    printlns("    link/loopback 00:00:00:00:00:00 brd 00:00:00:00:00:00");
    printlns(
        "2: eth0: <BROADCAST,MULTICAST,UP,LOWER_UP> mtu 1500 qdisc pfifo_fast state UP mode DEFAULT group default qlen 1000",
    );
    printlns("    link/ether 52:54:00:12:34:56 brd ff:ff:ff:ff:ff:ff");
}

fn show_route() {
    printlns("default via 10.0.2.2 dev eth0 proto dhcp metric 100");
    printlns("10.0.2.0/24 dev eth0 proto kernel scope link src 10.0.2.15 metric 100");
}

fn show_neighbor() {
    printlns("10.0.2.2 dev eth0 lladdr 52:54:00:12:35:02 REACHABLE");
    printlns("10.0.2.3 dev eth0 lladdr 52:54:00:12:35:03 STALE");
}

#[unsafe(no_mangle)]
fn main() -> i32 {
    // Without proper argument parsing, show address info by default
    // In future, would parse args and dispatch to appropriate command
    show_addr();
    0
}
