//! ifconfig - Configure network interfaces
//!
//! Display and configure network interface parameters.
//! Reads actual data from /sys/class/net and /run/network/

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
    while len > 0 && (buf[len - 1] == b'\n' || buf[len - 1] == b'\r') {
        len -= 1;
    }
    core::str::from_utf8(&buf[..len]).ok().map(String::from)
}

/// Interface information
struct InterfaceInfo {
    name: String,
    mtu: u32,
    mac: String,
    address: Option<String>,
    netmask: Option<String>,
    broadcast: Option<String>,
    is_up: bool,
    is_loopback: bool,
    rx_packets: u64,
    tx_packets: u64,
    rx_bytes: u64,
    tx_bytes: u64,
    rx_errors: u64,
    tx_errors: u64,
}

/// Parse u64 from string
fn parse_u64(s: &str) -> Option<u64> {
    let mut val: u64 = 0;
    for c in s.bytes() {
        if c.is_ascii_digit() {
            val = val.checked_mul(10)?;
            val = val.checked_add((c - b'0') as u64)?;
        } else {
            break;
        }
    }
    Some(val)
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
        interfaces.push(String::from("lo"));
        interfaces.push(String::from("eth0"));
    }

    // Sort: lo first
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
fn read_interface_info(name: &str) -> InterfaceInfo {
    let mut info = InterfaceInfo {
        name: String::from(name),
        mtu: 1500,
        mac: String::from("00:00:00:00:00:00"),
        address: None,
        netmask: None,
        broadcast: None,
        is_up: false,
        is_loopback: name == "lo",
        rx_packets: 0,
        tx_packets: 0,
        rx_bytes: 0,
        tx_bytes: 0,
        rx_errors: 0,
        tx_errors: 0,
    };

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

    // Read operstate
    if let Some(state) = read_sysfs_value(&format!("{}/operstate", base)) {
        info.is_up = state == "up" || state == "unknown";
    }

    // Read statistics
    let stats_base = format!("{}/statistics", base);
    if let Some(val) = read_sysfs_value(&format!("{}/rx_packets", stats_base)) {
        info.rx_packets = parse_u64(&val).unwrap_or(0);
    }
    if let Some(val) = read_sysfs_value(&format!("{}/tx_packets", stats_base)) {
        info.tx_packets = parse_u64(&val).unwrap_or(0);
    }
    if let Some(val) = read_sysfs_value(&format!("{}/rx_bytes", stats_base)) {
        info.rx_bytes = parse_u64(&val).unwrap_or(0);
    }
    if let Some(val) = read_sysfs_value(&format!("{}/tx_bytes", stats_base)) {
        info.tx_bytes = parse_u64(&val).unwrap_or(0);
    }
    if let Some(val) = read_sysfs_value(&format!("{}/rx_errors", stats_base)) {
        info.rx_errors = parse_u64(&val).unwrap_or(0);
    }
    if let Some(val) = read_sysfs_value(&format!("{}/tx_errors", stats_base)) {
        info.tx_errors = parse_u64(&val).unwrap_or(0);
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
                        "broadcast" => info.broadcast = Some(String::from(value.trim())),
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

    // Calculate broadcast if we have address and netmask but no broadcast
    if info.broadcast.is_none() {
        if let (Some(addr), Some(mask)) = (&info.address, &info.netmask) {
            info.broadcast = Some(calculate_broadcast(addr, mask));
        }
    }

    info
}

/// Calculate broadcast address from IP and netmask
fn calculate_broadcast(addr: &str, mask: &str) -> String {
    let addr_parts: Vec<&str> = addr.split('.').collect();
    let mask_parts: Vec<&str> = mask.split('.').collect();

    if addr_parts.len() != 4 || mask_parts.len() != 4 {
        return String::from("255.255.255.255");
    }

    let mut broadcast = [0u8; 4];
    for i in 0..4 {
        let a = parse_u32(addr_parts[i]).unwrap_or(0) as u8;
        let m = parse_u32(mask_parts[i]).unwrap_or(255) as u8;
        broadcast[i] = a | !m;
    }

    format!("{}.{}.{}.{}", broadcast[0], broadcast[1], broadcast[2], broadcast[3])
}

/// Format bytes as human readable
fn format_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KiB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1} MiB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.1} GiB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

fn show_interface(info: &InterfaceInfo) {
    // Line 1: Name and flags
    prints(&info.name);
    prints(": flags=");

    let mut flags_val: u32 = 0;
    let mut flags_str = String::new();

    if info.is_up {
        flags_val |= 0x1; // IFF_UP
        if !flags_str.is_empty() {
            flags_str.push(',');
        }
        flags_str.push_str("UP");
    }
    if !info.is_loopback {
        flags_val |= 0x2; // IFF_BROADCAST
        if !flags_str.is_empty() {
            flags_str.push(',');
        }
        flags_str.push_str("BROADCAST");
    }
    if info.is_up {
        flags_val |= 0x40; // IFF_RUNNING
        if !flags_str.is_empty() {
            flags_str.push(',');
        }
        flags_str.push_str("RUNNING");
    }
    if !info.is_loopback {
        flags_val |= 0x1000; // IFF_MULTICAST
        if !flags_str.is_empty() {
            flags_str.push(',');
        }
        flags_str.push_str("MULTICAST");
    }
    if info.is_loopback {
        flags_val |= 0x8; // IFF_LOOPBACK
        if !flags_str.is_empty() {
            flags_str.push(',');
        }
        flags_str.push_str("LOOPBACK");
    }

    prints(&format!("{}<{}>  mtu {}\n", flags_val, flags_str, info.mtu));

    // Line 2: IPv4 address
    if let Some(ref addr) = info.address {
        prints("        inet ");
        prints(addr);
        if let Some(ref mask) = info.netmask {
            prints("  netmask ");
            prints(mask);
        }
        if let Some(ref bcast) = info.broadcast {
            if !info.is_loopback {
                prints("  broadcast ");
                prints(bcast);
            }
        }
        printlns("");
    }

    // Line 3: Link info
    if info.is_loopback {
        prints("        loop  txqueuelen 1000  (Local Loopback)\n");
    } else {
        prints("        ether ");
        prints(&info.mac);
        printlns("  txqueuelen 1000  (Ethernet)");
    }

    // Line 4: RX stats
    prints(&format!(
        "        RX packets {}  bytes {} ({})\n",
        info.rx_packets,
        info.rx_bytes,
        format_bytes(info.rx_bytes)
    ));

    // Line 5: RX errors
    prints(&format!(
        "        RX errors {}  dropped 0  overruns 0  frame 0\n",
        info.rx_errors
    ));

    // Line 6: TX stats
    prints(&format!(
        "        TX packets {}  bytes {} ({})\n",
        info.tx_packets,
        info.tx_bytes,
        format_bytes(info.tx_bytes)
    ));

    // Line 7: TX errors
    prints(&format!(
        "        TX errors {}  dropped 0 overruns 0  carrier 0  collisions 0\n",
        info.tx_errors
    ));

    printlns("");
}

fn show_all_interfaces() {
    let interfaces = enumerate_interfaces();
    for name in interfaces {
        let info = read_interface_info(&name);
        show_interface(&info);
    }
}

fn show_one_interface(name: &str) {
    let info = read_interface_info(name);
    show_interface(&info);
}

fn show_help() {
    printlns("Usage: ifconfig [-a] [interface]");
    printlns("");
    printlns("Configure or display network interface parameters.");
    printlns("");
    printlns("Options:");
    printlns("  -a, --all     Display all interfaces");
    printlns("  -h, --help    Show this help");
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
    let mut show_all = true;
    let mut specific_iface: Option<&str> = None;

    let mut i = 1;
    while i < argc {
        let arg = cstr_to_str(unsafe { *argv.add(i as usize) });
        match arg {
            "-h" | "--help" | "help" => {
                show_help();
                return 0;
            }
            "-a" | "--all" => {
                show_all = true;
            }
            _ => {
                if !arg.starts_with('-') {
                    specific_iface = Some(arg);
                    show_all = false;
                }
            }
        }
        i += 1;
    }

    if let Some(iface) = specific_iface {
        show_one_interface(iface);
    } else {
        show_all_interfaces();
    }

    0
}
