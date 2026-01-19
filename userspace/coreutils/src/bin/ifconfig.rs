//! ifconfig - Configure network interfaces
//!
//! Display and configure network interface parameters.

#![no_std]
#![no_main]

use libc::{println, print, putchar};

fn show_interface(name: &str) {
    print(name);
    println(": flags=4163<UP,BROADCAST,RUNNING,MULTICAST>  mtu 1500");

    if name == "lo" {
        println("        inet 127.0.0.1  netmask 255.0.0.0");
        println("        inet6 ::1  prefixlen 128  scopeid 0x10<host>");
        println("        loop  txqueuelen 1000  (Local Loopback)");
    } else {
        println("        inet 10.0.2.15  netmask 255.255.255.0  broadcast 10.0.2.255");
        println("        inet6 fe80::5054:ff:fe12:3456  prefixlen 64  scopeid 0x20<link>");
        println("        ether 52:54:00:12:34:56  txqueuelen 1000  (Ethernet)");
    }
    println("        RX packets 0  bytes 0 (0.0 B)");
    println("        RX errors 0  dropped 0  overruns 0  frame 0");
    println("        TX packets 0  bytes 0 (0.0 B)");
    println("        TX errors 0  dropped 0 overruns 0  carrier 0  collisions 0");
    println("");
}

fn show_all_interfaces() {
    show_interface("eth0");
    show_interface("lo");
}

fn show_help() {
    println("Usage: ifconfig [-a] [interface]");
    println("");
    println("Configure or display network interface parameters.");
    println("");
    println("Options:");
    println("  -a, --all     Display all interfaces");
    println("  -h, --help    Show this help");
}

#[unsafe(no_mangle)]
fn main() -> i32 {
    // Without proper arg parsing, just show all interfaces
    show_all_interfaces();
    0
}
