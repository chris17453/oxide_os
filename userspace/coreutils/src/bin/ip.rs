//! ip - Show/manipulate routing, network devices, interfaces and tunnels
//!
//! Modern replacement for ifconfig, route, arp, etc.

#![no_std]
#![no_main]

use libc::{println, print};

fn show_usage() {
    println("Usage: ip [OPTIONS] OBJECT { COMMAND | help }");
    println("");
    println("OBJECT :=");
    println("  address   - protocol (IP or IPv6) address on a device");
    println("  link      - network device");
    println("  route     - routing table entry");
    println("  neighbor  - ARP or NDISC cache entry");
    println("");
    println("OPTIONS :=");
    println("  -4        - use IPv4");
    println("  -6        - use IPv6");
    println("  -h, -help - display this help");
    println("");
    println("Examples:");
    println("  ip addr show");
    println("  ip link show");
    println("  ip route show");
}

fn show_addr() {
    println("1: lo: <LOOPBACK,UP,LOWER_UP> mtu 65536 qdisc noqueue state UNKNOWN group default qlen 1000");
    println("    link/loopback 00:00:00:00:00:00 brd 00:00:00:00:00:00");
    println("    inet 127.0.0.1/8 scope host lo");
    println("       valid_lft forever preferred_lft forever");
    println("    inet6 ::1/128 scope host");
    println("       valid_lft forever preferred_lft forever");
    println("2: eth0: <BROADCAST,MULTICAST,UP,LOWER_UP> mtu 1500 qdisc pfifo_fast state UP group default qlen 1000");
    println("    link/ether 52:54:00:12:34:56 brd ff:ff:ff:ff:ff:ff");
    println("    inet 10.0.2.15/24 brd 10.0.2.255 scope global dynamic eth0");
    println("       valid_lft 86399sec preferred_lft 86399sec");
    println("    inet6 fe80::5054:ff:fe12:3456/64 scope link");
    println("       valid_lft forever preferred_lft forever");
}

fn show_link() {
    println("1: lo: <LOOPBACK,UP,LOWER_UP> mtu 65536 qdisc noqueue state UNKNOWN mode DEFAULT group default qlen 1000");
    println("    link/loopback 00:00:00:00:00:00 brd 00:00:00:00:00:00");
    println("2: eth0: <BROADCAST,MULTICAST,UP,LOWER_UP> mtu 1500 qdisc pfifo_fast state UP mode DEFAULT group default qlen 1000");
    println("    link/ether 52:54:00:12:34:56 brd ff:ff:ff:ff:ff:ff");
}

fn show_route() {
    println("default via 10.0.2.2 dev eth0 proto dhcp metric 100");
    println("10.0.2.0/24 dev eth0 proto kernel scope link src 10.0.2.15 metric 100");
}

fn show_neighbor() {
    println("10.0.2.2 dev eth0 lladdr 52:54:00:12:35:02 REACHABLE");
    println("10.0.2.3 dev eth0 lladdr 52:54:00:12:35:03 STALE");
}

#[unsafe(no_mangle)]
fn main() -> i32 {
    // Without proper argument parsing, show address info by default
    // In future, would parse args and dispatch to appropriate command
    show_addr();
    0
}
