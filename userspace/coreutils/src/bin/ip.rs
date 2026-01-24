//! ip - Show/manipulate routing, network devices, interfaces and tunnels
//!
//! Modern replacement for ifconfig, route, arp, etc.
//! Reads actual network configuration from /sys/class/net and /run/network/

#![no_std]
#![no_main]

extern crate alloc;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use libc::dirent::{closedir, opendir, readdir};
use libc::*;

/// Read file content into buffer
fn read_file(path: &str, buf: &mut [u8]) -> isize {
    let fd = open2(path, O_RDONLY);
    if fd < 0 {
        return -1;
    }
    let n = read(fd, buf);
    close(fd);
    n
}

/// Read a single line from a sysfs file (trim newline)
fn read_sysfs_value(path: &str) -> Option<String> {
    let mut buf = [0u8; 256];
    let n = read_file(path, &mut buf);
    if n <= 0 {
        return None;
    }
    let mut len = n as usize;
    // Trim trailing newline
    while len > 0 && (buf[len - 1] == b'\n' || buf[len - 1] == b'\r') {
        len -= 1;
    }
    core::str::from_utf8(&buf[..len]).ok().map(String::from)
}

/// Interface information
struct InterfaceInfo {
    name: String,
    index: u32,
    mtu: u32,
    mac: String,
    address: Option<String>,
    netmask: Option<String>,
    gateway: Option<String>,
    is_up: bool,
    is_loopback: bool,
}

/// Enumerate network interfaces from /sys/class/net
fn enumerate_interfaces() -> Vec<String> {
    let mut interfaces = Vec::new();

    let dir = opendir("/sys/class/net");
    if let Some(mut dir) = dir {
        while let Some(entry) = readdir(&mut dir) {
            let name = entry.name();
            if name == "." || name == ".." {
                continue;
            }
            interfaces.push(String::from(name));
        }
        closedir(dir);
    } else {
        // Fallback
        interfaces.push(String::from("lo"));
        interfaces.push(String::from("eth0"));
    }

    // Sort: lo first, then alphabetically
    interfaces.sort_by(|a, b| {
        if a == "lo" {
            core::cmp::Ordering::Less
        } else if b == "lo" {
            core::cmp::Ordering::Greater
        } else {
            a.cmp(b)
        }
    });

    interfaces
}

/// Read interface info
fn read_interface_info(name: &str, index: u32) -> InterfaceInfo {
    let mut info = InterfaceInfo {
        name: String::from(name),
        index,
        mtu: 1500,
        mac: String::from("00:00:00:00:00:00"),
        address: None,
        netmask: None,
        gateway: None,
        is_up: false,
        is_loopback: name == "lo",
    };

    // Build sysfs base path
    let base = format!("/sys/class/net/{}", name);

    // Read MTU
    if let Some(mtu_str) = read_sysfs_value(&format!("{}/mtu", base)) {
        if let Some(mtu) = parse_u32(&mtu_str) {
            info.mtu = mtu;
        }
    }

    // Read MAC address
    if let Some(mac) = read_sysfs_value(&format!("{}/address", base)) {
        info.mac = mac;
    }

    // Read operstate (up/down)
    if let Some(state) = read_sysfs_value(&format!("{}/operstate", base)) {
        info.is_up = state == "up" || state == "unknown";
    }

    // Read active config from /run/network/<name>.conf
    let config_path = format!("/run/network/{}.conf", name);
    let mut buf = [0u8; 512];
    let n = read_file(&config_path, &mut buf);
    if n > 0 {
        if let Ok(content) = core::str::from_utf8(&buf[..n as usize]) {
            for line in content.lines() {
                let line = line.trim();
                if let Some((key, value)) = line.split_once('=') {
                    match key.trim() {
                        "address" => info.address = Some(String::from(value.trim())),
                        "netmask" => info.netmask = Some(String::from(value.trim())),
                        "gateway" => info.gateway = Some(String::from(value.trim())),
                        _ => {}
                    }
                }
            }
        }
    }

    // Loopback defaults
    if info.is_loopback && info.address.is_none() {
        info.address = Some(String::from("127.0.0.1"));
        info.netmask = Some(String::from("255.0.0.0"));
        info.mtu = 65536;
        info.mac = String::from("00:00:00:00:00:00");
    }

    info
}

/// Parse u32 from string
fn parse_u32(s: &str) -> Option<u32> {
    let mut val: u32 = 0;
    for c in s.bytes() {
        if c.is_ascii_digit() {
            val = val.checked_mul(10)?;
            val = val.checked_add((c - b'0') as u32)?;
        } else {
            break;
        }
    }
    Some(val)
}

/// Calculate prefix length from netmask
fn netmask_to_prefix(netmask: &str) -> u8 {
    let parts: Vec<&str> = netmask.split('.').collect();
    if parts.len() != 4 {
        return 24; // default
    }

    let mut prefix = 0u8;
    for part in parts {
        if let Some(octet) = parse_u32(part) {
            let mut val = octet as u8;
            while val & 0x80 != 0 {
                prefix += 1;
                val <<= 1;
            }
        }
    }
    prefix
}

/// Format for alloc
fn format(args: core::fmt::Arguments) -> String {
    use alloc::fmt::Write;
    let mut s = String::new();
    let _ = s.write_fmt(args);
    s
}

/// Show address information (ip addr)
fn show_addr() {
    let interfaces = enumerate_interfaces();

    for (idx, name) in interfaces.iter().enumerate() {
        let info = read_interface_info(name, (idx + 1) as u32);

        // Line 1: interface header
        prints(&format!("{}: {}: <", info.index, info.name));

        // Flags
        let mut flags = Vec::new();
        if info.is_loopback {
            flags.push("LOOPBACK");
        } else {
            flags.push("BROADCAST");
            flags.push("MULTICAST");
        }
        if info.is_up {
            flags.push("UP");
            flags.push("LOWER_UP");
        }

        for (i, flag) in flags.iter().enumerate() {
            if i > 0 {
                prints(",");
            }
            prints(flag);
        }

        prints(&format!("> mtu {} state ", info.mtu));
        if info.is_up {
            prints("UP");
        } else {
            prints("DOWN");
        }
        printlns("");

        // Line 2: link info
        if info.is_loopback {
            prints("    link/loopback ");
        } else {
            prints("    link/ether ");
        }
        prints(&info.mac);
        prints(" brd ");
        if info.is_loopback {
            printlns("00:00:00:00:00:00");
        } else {
            printlns("ff:ff:ff:ff:ff:ff");
        }

        // Line 3: IPv4 address
        if let Some(ref addr) = info.address {
            prints("    inet ");
            prints(addr);
            if let Some(ref mask) = info.netmask {
                let prefix = netmask_to_prefix(mask);
                prints(&format!("/{}", prefix));
            }
            prints(" scope ");
            if info.is_loopback {
                prints("host");
            } else {
                prints("global");
            }
            prints(" ");
            printlns(&info.name);
        }
    }
}

/// Show link information (ip link)
fn show_link() {
    let interfaces = enumerate_interfaces();

    for (idx, name) in interfaces.iter().enumerate() {
        let info = read_interface_info(name, (idx + 1) as u32);

        // Line 1: interface header
        prints(&format!("{}: {}: <", info.index, info.name));

        let mut flags = Vec::new();
        if info.is_loopback {
            flags.push("LOOPBACK");
        } else {
            flags.push("BROADCAST");
            flags.push("MULTICAST");
        }
        if info.is_up {
            flags.push("UP");
            flags.push("LOWER_UP");
        }

        for (i, flag) in flags.iter().enumerate() {
            if i > 0 {
                prints(",");
            }
            prints(flag);
        }

        prints(&format!("> mtu {} state ", info.mtu));
        if info.is_up {
            printlns("UP");
        } else {
            printlns("DOWN");
        }

        // Line 2: link info
        if info.is_loopback {
            prints("    link/loopback ");
        } else {
            prints("    link/ether ");
        }
        prints(&info.mac);
        prints(" brd ");
        if info.is_loopback {
            printlns("00:00:00:00:00:00");
        } else {
            printlns("ff:ff:ff:ff:ff:ff");
        }
    }
}

/// Show route information (ip route)
fn show_route() {
    let interfaces = enumerate_interfaces();

    // Find interface with gateway for default route
    for name in &interfaces {
        if name == "lo" {
            continue;
        }

        let info = read_interface_info(name, 0);

        // Show default route if gateway exists
        if let Some(ref gw) = info.gateway {
            prints("default via ");
            prints(gw);
            prints(" dev ");
            printlns(name);
        }

        // Show network route if address exists
        if let Some(ref addr) = info.address {
            if let Some(ref mask) = info.netmask {
                // Calculate network address
                let prefix = netmask_to_prefix(mask);
                // Simplified: just show the /prefix
                prints(addr);
                prints(&format!("/{} dev {} scope link\n", prefix, name));
            }
        }
    }
}

/// Show neighbor/ARP information (ip neighbor)
fn show_neighbor() {
    // Read from /proc/net/arp if available
    let mut buf = [0u8; 1024];
    let n = read_file("/proc/net/arp", &mut buf);

    if n > 0 {
        if let Ok(content) = core::str::from_utf8(&buf[..n as usize]) {
            let mut first = true;
            for line in content.lines() {
                // Skip header line
                if first {
                    first = false;
                    continue;
                }

                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 6 {
                    // IP HW_Type Flags HW_Address Mask Device
                    prints(parts[0]); // IP
                    prints(" dev ");
                    prints(parts[5]); // Device
                    prints(" lladdr ");
                    prints(parts[3]); // MAC
                    printlns(" REACHABLE");
                }
            }
        }
    } else {
        printlns("(no ARP entries)");
    }
}

fn show_usage() {
    printlns("Usage: ip [OPTIONS] OBJECT { COMMAND | help }");
    printlns("");
    printlns("OBJECT :=");
    printlns("  address   - protocol (IP or IPv6) address on a device");
    printlns("  addr      - alias for address");
    printlns("  link      - network device");
    printlns("  route     - routing table entry");
    printlns("  neighbor  - ARP or NDISC cache entry");
    printlns("  neigh     - alias for neighbor");
    printlns("");
    printlns("OPTIONS :=");
    printlns("  -h, --help - display this help");
    printlns("");
    printlns("Examples:");
    printlns("  ip addr");
    printlns("  ip link");
    printlns("  ip route");
    printlns("  ip neigh");
}

/// Convert C string to str
fn cstr_to_str(ptr: *const u8) -> &'static str {
    if ptr.is_null() {
        return "";
    }
    let mut len = 0;
    unsafe {
        while *ptr.add(len) != 0 {
            len += 1;
        }
        core::str::from_utf8_unchecked(core::slice::from_raw_parts(ptr, len))
    }
}

#[unsafe(no_mangle)]
fn main(argc: i32, argv: *const *const u8) -> i32 {
    // Parse arguments
    let mut object = "";

    let mut i = 1;
    while i < argc {
        let arg = cstr_to_str(unsafe { *argv.add(i as usize) });
        match arg {
            "-h" | "--help" | "help" => {
                show_usage();
                return 0;
            }
            "addr" | "address" | "a" => object = "addr",
            "link" | "l" => object = "link",
            "route" | "r" => object = "route",
            "neighbor" | "neigh" | "n" => object = "neigh",
            "show" | "list" => {} // ignore subcommand
            _ => {
                if object.is_empty() && !arg.starts_with('-') {
                    object = arg;
                }
            }
        }
        i += 1;
    }

    // Default to showing addresses
    if object.is_empty() {
        object = "addr";
    }

    match object {
        "addr" | "address" | "a" => show_addr(),
        "link" | "l" => show_link(),
        "route" | "r" => show_route(),
        "neigh" | "neighbor" | "n" => show_neighbor(),
        _ => {
            prints("Unknown object: ");
            printlns(object);
            show_usage();
            return 1;
        }
    }

    0
}
