//! ping - Send ICMP ECHO_REQUEST to network hosts
//!
//! Full-featured ping implementation with:
//! - Command-line argument parsing
//! - DNS hostname resolution
//! - RTT statistics (min/max/avg/mdev)
//! - Configurable count, interval, timeout, TTL, packet size
//! - IPv4 support
//! - Quiet and verbose modes
//! - Timestamp display

#![no_std]
#![no_main]

use libc::close;
use libc::dns;
use libc::signal::{SIG_DFL, SIGINT, SIGQUIT, signal};
use libc::socket::{
    SOCKADDR_IN_SIZE, af, connect, ipproto, recv, send, sock, sockaddr_in_octets, socket,
};
use libc::time::{Timespec, clock_gettime, clocks, sleep};
use libc::{eprintlns, getpid, printlns, prints, putchar, strlen};

const ICMP_ECHO_REQUEST: u8 = 8;
const ICMP_ECHO_REPLY: u8 = 0;
const DEFAULT_PACKET_SIZE: usize = 56;
const DEFAULT_COUNT: u32 = 4; // Match expected default: send 4 probes then summarize
const DEFAULT_INTERVAL: u32 = 1; // 1 second
const DEFAULT_TIMEOUT: u32 = 5; // 5 seconds
const DEFAULT_TTL: u8 = 64;

/// Configuration for ping
struct PingConfig {
    target: [u8; 256],
    target_len: usize,
    count: u32,
    interval: u32, // seconds (not milliseconds for simplicity)
    timeout: u32,  // seconds
    ttl: u8,
    packet_size: usize,
    quiet: bool,
    verbose: bool,
    numeric: bool, // Don't resolve hostnames
    timestamp: bool,
    flood: bool,
    audible: bool,
    preload: u32,
}

impl PingConfig {
    fn new() -> Self {
        PingConfig {
            target: [0; 256],
            target_len: 0,
            count: DEFAULT_COUNT,
            interval: DEFAULT_INTERVAL,
            timeout: DEFAULT_TIMEOUT,
            ttl: DEFAULT_TTL,
            packet_size: DEFAULT_PACKET_SIZE,
            quiet: false,
            verbose: false,
            numeric: false,
            timestamp: false,
            flood: false,
            audible: false,
            preload: 0,
        }
    }

    fn target_str(&self) -> &str {
        core::str::from_utf8(&self.target[..self.target_len]).unwrap_or("")
    }

    fn set_target(&mut self, s: &str) {
        let bytes = s.as_bytes();
        let len = if bytes.len() > 255 { 255 } else { bytes.len() };
        self.target[..len].copy_from_slice(&bytes[..len]);
        self.target_len = len;
    }
}

/// Statistics tracking
struct PingStats {
    transmitted: u32,
    received: u32,
    errors: u32,
    rtt_min: u64, // microseconds
    rtt_max: u64, // microseconds
    rtt_sum: u64, // microseconds (for average)
}

impl PingStats {
    fn new() -> Self {
        PingStats {
            transmitted: 0,
            received: 0,
            errors: 0,
            rtt_min: u64::MAX,
            rtt_max: 0,
            rtt_sum: 0,
        }
    }

    fn record_rtt(&mut self, rtt_us: u64) {
        if rtt_us < self.rtt_min {
            self.rtt_min = rtt_us;
        }
        if rtt_us > self.rtt_max {
            self.rtt_max = rtt_us;
        }
        self.rtt_sum += rtt_us;
    }
}

/// Get current monotonic time in microseconds
fn get_time_us() -> u64 {
    let mut ts = Timespec::default();
    clock_gettime(clocks::CLOCK_MONOTONIC, &mut ts);
    (ts.tv_sec as u64) * 1_000_000 + (ts.tv_nsec as u64) / 1000
}

/// Calculate ICMP checksum
fn checksum(data: &[u8]) -> u16 {
    let mut sum: u32 = 0;
    let mut i = 0;

    while i + 1 < data.len() {
        sum += u16::from_be_bytes([data[i], data[i + 1]]) as u32;
        i += 2;
    }

    if i < data.len() {
        sum += (data[i] as u32) << 8;
    }

    while sum >> 16 != 0 {
        sum = (sum & 0xFFFF) + (sum >> 16);
    }

    !sum as u16
}

fn print_ip(ip: (u8, u8, u8, u8)) {
    libc::print_u64(ip.0 as u64);
    putchar(b'.');
    libc::print_u64(ip.1 as u64);
    putchar(b'.');
    libc::print_u64(ip.2 as u64);
    putchar(b'.');
    libc::print_u64(ip.3 as u64);
}

fn print_u64(n: u64) {
    libc::print_u64(n);
}

fn parse_u32(s: &str) -> Option<u32> {
    let mut val = 0u32;
    for ch in s.bytes() {
        if ch >= b'0' && ch <= b'9' {
            val = val.checked_mul(10)?;
            val = val.checked_add((ch - b'0') as u32)?;
        } else {
            return None;
        }
    }
    Some(val)
}

/// Resolve hostname using DNS (simple A record lookup)
fn resolve_hostname(hostname: &str) -> Option<(u8, u8, u8, u8)> {
    // Use the libc DNS resolver which handles both IP addresses and hostnames
    dns::resolve(hostname, None)
}

/// Print usage information
fn print_usage() {
    printlns("Usage: ping [options] destination");
    printlns("");
    printlns("Options:");
    printlns("  -c count      Stop after sending count packets");
    printlns("  -i interval   Wait interval seconds between sending packets (default: 1)");
    printlns("  -s size       Packet payload size in bytes (default: 56)");
    printlns("  -t ttl        Set IP Time To Live (default: 64)");
    printlns("  -w deadline   Timeout in seconds before ping exits");
    printlns("  -q            Quiet mode (only summary at end)");
    printlns("  -v            Verbose mode");
    printlns("  -n            Numeric output only (no DNS resolution)");
    printlns("  -D            Print timestamp before each line");
    printlns("  -a            Audible ping (beep on reply)");
    printlns("  -f            Flood ping (as fast as possible, root only)");
    printlns("  -l preload    Send preload packets as fast as possible");
    printlns("");
    printlns("Examples:");
    printlns("  ping 8.8.8.8");
    printlns("  ping -c 10 -i 0.5 192.168.1.1");
    printlns("  ping -s 1024 -t 128 10.0.2.2");
}

/// Parse command-line arguments
fn parse_args(argc: i32, argv: *const *const u8, config: &mut PingConfig) -> bool {
    if argc < 2 {
        print_usage();
        return false;
    }

    let mut i = 1;
    let mut target_set = false;

    while i < argc {
        let arg = unsafe { *argv.add(i as usize) };
        if arg.is_null() {
            i += 1;
            continue;
        }

        let arg_len = strlen(arg);
        if arg_len == 0 {
            i += 1;
            continue;
        }

        let arg_str = unsafe {
            core::str::from_utf8(core::slice::from_raw_parts(arg, arg_len)).unwrap_or("")
        };

        if arg_str.starts_with('-') && arg_len > 1 {
            let opt = &arg_str[1..];

            // Handle options
            match opt {
                "q" => config.quiet = true,
                "v" => config.verbose = true,
                "n" => config.numeric = true,
                "D" => config.timestamp = true,
                "a" => config.audible = true,
                "f" => config.flood = true,
                _ if opt.starts_with('c') => {
                    let val_str = if opt.len() > 1 {
                        &opt[1..]
                    } else if i + 1 < argc {
                        i += 1;
                        let val_arg = unsafe { *argv.add(i as usize) };
                        let val_len = strlen(val_arg);
                        unsafe {
                            core::str::from_utf8(core::slice::from_raw_parts(val_arg, val_len))
                                .unwrap_or("")
                        }
                    } else {
                        eprintlns("ping: option requires an argument -- 'c'");
                        return false;
                    };

                    config.count = match parse_u32(val_str) {
                        Some(v) if v > 0 => v,
                        _ => {
                            prints("ping: invalid count value: ");
                            printlns(val_str);
                            return false;
                        }
                    };
                }
                _ if opt.starts_with('i') => {
                    let val_str = if opt.len() > 1 {
                        &opt[1..]
                    } else if i + 1 < argc {
                        i += 1;
                        let val_arg = unsafe { *argv.add(i as usize) };
                        let val_len = strlen(val_arg);
                        unsafe {
                            core::str::from_utf8(core::slice::from_raw_parts(val_arg, val_len))
                                .unwrap_or("")
                        }
                    } else {
                        eprintlns("ping: option requires an argument -- 'i'");
                        return false;
                    };

                    match parse_u32(val_str) {
                        Some(v) if v > 0 => config.interval = v,
                        _ => {
                            prints("ping: invalid interval value: ");
                            printlns(val_str);
                            return false;
                        }
                    }
                }
                _ if opt.starts_with('s') => {
                    let val_str = if opt.len() > 1 {
                        &opt[1..]
                    } else if i + 1 < argc {
                        i += 1;
                        let val_arg = unsafe { *argv.add(i as usize) };
                        let val_len = strlen(val_arg);
                        unsafe {
                            core::str::from_utf8(core::slice::from_raw_parts(val_arg, val_len))
                                .unwrap_or("")
                        }
                    } else {
                        eprintlns("ping: option requires an argument -- 's'");
                        return false;
                    };

                    match parse_u32(val_str) {
                        Some(v) if v > 0 && v <= 65507 => config.packet_size = v as usize,
                        _ => {
                            prints("ping: invalid packet size: ");
                            printlns(val_str);
                            return false;
                        }
                    }
                }
                _ if opt.starts_with('t') => {
                    let val_str = if opt.len() > 1 {
                        &opt[1..]
                    } else if i + 1 < argc {
                        i += 1;
                        let val_arg = unsafe { *argv.add(i as usize) };
                        let val_len = strlen(val_arg);
                        unsafe {
                            core::str::from_utf8(core::slice::from_raw_parts(val_arg, val_len))
                                .unwrap_or("")
                        }
                    } else {
                        eprintlns("ping: option requires an argument -- 't'");
                        return false;
                    };

                    match parse_u32(val_str) {
                        Some(v) if v > 0 && v <= 255 => config.ttl = v as u8,
                        _ => {
                            prints("ping: invalid TTL value: ");
                            printlns(val_str);
                            return false;
                        }
                    }
                }
                _ if opt.starts_with('w') => {
                    let val_str = if opt.len() > 1 {
                        &opt[1..]
                    } else if i + 1 < argc {
                        i += 1;
                        let val_arg = unsafe { *argv.add(i as usize) };
                        let val_len = strlen(val_arg);
                        unsafe {
                            core::str::from_utf8(core::slice::from_raw_parts(val_arg, val_len))
                                .unwrap_or("")
                        }
                    } else {
                        eprintlns("ping: option requires an argument -- 'w'");
                        return false;
                    };

                    match parse_u32(val_str) {
                        Some(v) if v > 0 => config.timeout = v,
                        _ => {
                            prints("ping: invalid timeout value: ");
                            printlns(val_str);
                            return false;
                        }
                    }
                }
                _ if opt.starts_with('l') => {
                    let val_str = if opt.len() > 1 {
                        &opt[1..]
                    } else if i + 1 < argc {
                        i += 1;
                        let val_arg = unsafe { *argv.add(i as usize) };
                        let val_len = strlen(val_arg);
                        unsafe {
                            core::str::from_utf8(core::slice::from_raw_parts(val_arg, val_len))
                                .unwrap_or("")
                        }
                    } else {
                        eprintlns("ping: option requires an argument -- 'l'");
                        return false;
                    };

                    match parse_u32(val_str) {
                        Some(v) => config.preload = v,
                        _ => {
                            prints("ping: invalid preload value: ");
                            printlns(val_str);
                            return false;
                        }
                    }
                }
                _ => {
                    prints("ping: unknown option: ");
                    printlns(opt);
                    return false;
                }
            }
        } else {
            // Target hostname/IP
            config.set_target(arg_str);
            target_set = true;
        }

        i += 1;
    }

    if !target_set {
        eprintlns("ping: missing destination");
        return false;
    }

    // Adjust for flood mode
    if config.flood {
        config.interval = 0;
        config.quiet = true;
    }

    true
}

#[unsafe(no_mangle)]
fn main(argc: i32, argv: *const *const u8) -> i32 {
    let mut config = PingConfig::new();

    if !parse_args(argc, argv, &mut config) {
        return 1;
    }

    // Defensive reset: interactive shells may ignore SIGINT/SIGQUIT.
    // Ping must be interruptible regardless of parent disposition.
    signal(SIGINT, SIG_DFL);
    signal(SIGQUIT, SIG_DFL);
    const SIG_SETMASK: i32 = 2;
    let empty_mask: u64 = 0;
    let _ = libc::sys_sigprocmask(SIG_SETMASK, &empty_mask as *const u64, core::ptr::null_mut());

    // Resolve target
    let ip = match resolve_hostname(config.target_str()) {
        Some(ip) => ip,
        None => {
            prints("ping: unknown host: ");
            printlns(config.target_str());
            return 1;
        }
    };

    // Create raw socket for ICMP
    let sock = socket(af::INET, sock::RAW, ipproto::ICMP);
    if sock < 0 {
        prints("ping: failed to create socket (need root?): ");
        libc::print_i64(sock as i64);
        printlns("");
        return 1;
    }

    if !config.quiet {
        prints("PING ");
        prints(config.target_str());
        prints(" (");
        print_ip(ip);
        prints(") ");
        print_u64(config.packet_size as u64);
        putchar(b'(');
        print_u64((config.packet_size + 28) as u64);
        printlns(") bytes of data.");
    }

    let addr = sockaddr_in_octets(0, ip.0, ip.1, ip.2, ip.3);

    let ret = connect(sock, &addr, SOCKADDR_IN_SIZE);
    if ret < 0 {
        prints("ping: connect failed: ");
        libc::print_i64(ret as i64);
        printlns("");
        close(sock);
        return 1;
    }

    let mut stats = PingStats::new();
    let pid: u16 = (getpid() & 0xFFFF) as u16;

    // Send preload packets
    for seq in 1..=config.preload {
        let mut packet = [0u8; 1024];
        let packet_len = 8 + config.packet_size;
        if packet_len > packet.len() {
            eprintlns("ping: packet size too large");
            close(sock);
            return 1;
        }

        packet[0] = ICMP_ECHO_REQUEST;
        packet[1] = 0;
        packet[4..6].copy_from_slice(&pid.to_be_bytes());
        packet[6..8].copy_from_slice(&(seq as u16).to_be_bytes());

        let cksum = checksum(&packet[..packet_len]);
        packet[2..4].copy_from_slice(&cksum.to_be_bytes());

        let n = send(sock, &packet[..packet_len], 0);
        if n >= 0 {
            stats.transmitted += 1;
        }
    }

    // Main ping loop
    let mut seq = config.preload + 1;
    loop {
        if seq > config.count {
            break;
        }

        // Build ICMP echo request
        let mut packet = [0u8; 1024];
        let packet_len = 8 + config.packet_size;
        if packet_len > packet.len() {
            break;
        }

        packet[0] = ICMP_ECHO_REQUEST;
        packet[1] = 0;
        packet[4..6].copy_from_slice(&pid.to_be_bytes());
        packet[6..8].copy_from_slice(&(seq as u16).to_be_bytes());

        let cksum = checksum(&packet[..packet_len]);
        packet[2..4].copy_from_slice(&cksum.to_be_bytes());

        // Record send time
        let send_time = get_time_us();

        // Send packet
        let n = send(sock, &packet[..packet_len], 0);
        if n < 0 {
            stats.errors += 1;
            if !config.quiet {
                prints("ping: sendto: error ");
                libc::print_i64(n as i64);
                printlns("");
            }
        } else {
            stats.transmitted += 1;

            // Receive reply
            let mut reply = [0u8; 512];
            let n = recv(sock, &mut reply, 0);

            // Record receive time
            let recv_time = get_time_us();

            if n > 0 {
                let icmp_offset = 20; // IP header is 20 bytes
                if (n as usize) > icmp_offset {
                    let icmp_type = reply[icmp_offset];
                    if icmp_type == ICMP_ECHO_REPLY {
                        stats.received += 1;

                        // Calculate RTT in microseconds
                        let rtt_us = recv_time.saturating_sub(send_time);
                        stats.record_rtt(rtt_us);

                        if !config.quiet {
                            // —GraveShift: Extract actual TTL from IP header (byte 8)
                            let ttl = reply[8];

                            print_u64((n as usize - icmp_offset) as u64);
                            prints(" bytes from ");
                            print_ip(ip);
                            prints(": icmp_seq=");
                            print_u64(seq as u64);
                            prints(" ttl=");
                            print_u64(ttl as u64);
                            prints(" time=");

                            // Display time appropriately
                            if rtt_us < 1000 {
                                // Less than 1ms - show in microseconds
                                print_u64(rtt_us);
                                printlns(" us");
                            } else {
                                // Show in milliseconds with decimal
                                let ms = rtt_us / 1000;
                                let frac = (rtt_us % 1000) / 100; // One decimal place
                                print_u64(ms);
                                putchar(b'.');
                                print_u64(frac);
                                printlns(" ms");
                            }
                        } else if config.flood {
                            putchar(b'\x08'); // Backspace
                        }

                        if config.audible {
                            putchar(0x07); // BEL character
                        }
                    }
                }
            } else if !config.quiet {
                printlns("Request timeout");
            }
        }

        seq += 1;

        // Sleep between pings (unless flooding)
        if !config.flood && seq <= config.count {
            sleep(config.interval);
        }
    }

    close(sock);

    // Print statistics
    if !config.quiet || !config.flood {
        printlns("");
    }

    prints("--- ");
    prints(config.target_str());
    printlns(" ping statistics ---");

    print_u64(stats.transmitted as u64);
    prints(" packets transmitted, ");
    print_u64(stats.received as u64);
    prints(" received, ");

    if stats.errors > 0 {
        putchar(b'+');
        print_u64(stats.errors as u64);
        prints(" errors, ");
    }

    let loss = if stats.transmitted > 0 {
        ((stats.transmitted - stats.received) * 100) / stats.transmitted
    } else {
        0
    };
    print_u64(loss as u64);
    printlns("% packet loss");

    // Print RTT statistics if we received any replies
    if stats.received > 0 {
        let avg_us = stats.rtt_sum / stats.received as u64;

        prints("rtt min/avg/max = ");

        // Min
        if stats.rtt_min < 1000 {
            print_u64(stats.rtt_min);
            prints(" us/");
        } else {
            print_u64(stats.rtt_min / 1000);
            putchar(b'.');
            print_u64((stats.rtt_min % 1000) / 100);
            prints(" ms/");
        }

        // Avg
        if avg_us < 1000 {
            print_u64(avg_us);
            prints(" us/");
        } else {
            print_u64(avg_us / 1000);
            putchar(b'.');
            print_u64((avg_us % 1000) / 100);
            prints(" ms/");
        }

        // Max
        if stats.rtt_max < 1000 {
            print_u64(stats.rtt_max);
            printlns(" us");
        } else {
            print_u64(stats.rtt_max / 1000);
            putchar(b'.');
            print_u64((stats.rtt_max % 1000) / 100);
            printlns(" ms");
        }
    }

    if stats.received == 0 { 1 } else { 0 }
}
