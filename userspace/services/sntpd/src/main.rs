//! OXIDE SNTP Daemon — Simple Network Time Protocol client
//!
//! Syncs system clock from NTP servers on UDP port 123.
//! RFC 4330 compliant (SNTP v4 — single request/response, no PLL discipline).
//!
//! — WireSaint: "The RTC gives us time at boot. NTP keeps us honest after that.
//!   One UDP packet every 30 minutes. The most efficient daemon in the system."
//!
//! Architecture:
//!   1. Read /etc/ntp.conf for server (default: pool.ntp.org)
//!   2. Resolve server hostname via DNS
//!   3. Send 48-byte NTP request on UDP/123
//!   4. Parse response, extract server timestamp
//!   5. Set system clock via clock_settime(CLOCK_REALTIME)
//!   6. Sleep 30 minutes, repeat

#![no_std]
#![no_main]
#![allow(unused)]

use libc::socket::{
    sockaddr_in_octets, SOCKADDR_IN_SIZE, udp_socket,
};
use libc::time::{Timespec, clock_settime, clock_gettime, nanosleep};
use libc::*;

/// NTP epoch offset: seconds between 1900-01-01 and 1970-01-01.
/// — WireSaint: "70 years of seconds. The bridge between NTP and Unix time."
const NTP_EPOCH_OFFSET: u64 = 2208988800;

/// NTP packet size
const NTP_PACKET_SIZE: usize = 48;

/// Default NTP server
const DEFAULT_NTP_SERVER: &str = "pool.ntp.org";

/// Sync interval: 30 minutes in seconds
const SYNC_INTERVAL_SECS: u64 = 1800;

/// Retry interval on failure: 60 seconds
const RETRY_INTERVAL_SECS: u64 = 60;

/// Initial delay before first sync: 5 seconds (let network come up)
const INITIAL_DELAY_SECS: u64 = 5;

/// CLOCK_REALTIME constant
const CLOCK_REALTIME: i32 = 0;

fn log(msg: &str) {
    prints("[sntpd] ");
    prints(msg);
    prints("\n");
}

/// Build an SNTP client request packet (48 bytes).
/// — WireSaint: "LI=0 (no warning), VN=4 (NTPv4), Mode=3 (client).
///   That's byte 0 = 0x23. The other 47 bytes are zero. Simple."
fn build_ntp_request(packet: &mut [u8; NTP_PACKET_SIZE]) {
    // Zero the packet
    for b in packet.iter_mut() { *b = 0; }
    // LI=0 (00), VN=4 (100), Mode=3 (011) → 00_100_011 = 0x23
    packet[0] = 0x23;
}

/// Extract the transmit timestamp from an NTP response.
/// Returns Unix epoch seconds or None if the response is invalid.
/// — WireSaint: "Bytes 40-43: seconds since 1900. Subtract 70 years. Done."
fn parse_ntp_response(packet: &[u8]) -> Option<u64> {
    if packet.len() < NTP_PACKET_SIZE {
        return None;
    }

    // Check response: Mode should be 4 (server) or 5 (broadcast)
    let mode = packet[0] & 0x07;
    if mode != 4 && mode != 5 {
        return None;
    }

    // Stratum 0 = kiss-of-death, stratum 16 = unsynchronized
    let stratum = packet[1];
    if stratum == 0 || stratum >= 16 {
        return None;
    }

    // Transmit timestamp: bytes 40-43 (seconds), 44-47 (fraction)
    let ntp_secs = u32::from_be_bytes([packet[40], packet[41], packet[42], packet[43]]) as u64;

    if ntp_secs < NTP_EPOCH_OFFSET {
        return None; // Time before 1970 — bogus
    }

    Some(ntp_secs - NTP_EPOCH_OFFSET)
}

/// Perform one NTP sync: send request, receive response, set clock.
/// Returns the offset applied (or negative on error).
/// — WireSaint: "One UDP round trip. One clock adjustment. Maximum efficiency."
fn do_ntp_sync(server_ip: (u8, u8, u8, u8)) -> i64 {
    // Create UDP socket
    let sock = udp_socket();
    if sock < 0 {
        log("socket() failed");
        return -1;
    }

    // Build NTP request
    let mut request = [0u8; NTP_PACKET_SIZE];
    build_ntp_request(&mut request);

    // Get current time for offset calculation
    let mut before = Timespec { tv_sec: 0, tv_nsec: 0 };
    clock_gettime(CLOCK_REALTIME, &mut before);

    // Send to server on port 123
    let addr = sockaddr_in_octets(123, server_ip.0, server_ip.1, server_ip.2, server_ip.3);
    let sent = libc::socket::sendto(sock, &request, 0, &addr, SOCKADDR_IN_SIZE);
    if sent < 0 {
        log("sendto() failed");
        libc::close(sock);
        return -1;
    }

    // Receive response (with timeout via bounded retry)
    let mut response = [0u8; NTP_PACKET_SIZE];
    let mut received: isize = -1;
    for _ in 0..500 { // ~5 seconds of retries (10ms each via recv poll)
        let n = libc::socket::recv(sock, &mut response, 0);
        if n >= NTP_PACKET_SIZE as isize {
            received = n;
            break;
        }
        if n == -11 { // EAGAIN
            continue;
        }
        if n < 0 {
            break;
        }
    }

    libc::close(sock);

    if received < NTP_PACKET_SIZE as isize {
        log("no response from NTP server");
        return -1;
    }

    // Parse response
    let ntp_time = match parse_ntp_response(&response) {
        Some(t) => t,
        None => {
            log("invalid NTP response");
            return -1;
        }
    };

    // Calculate offset
    let local_time = before.tv_sec as u64;
    let offset = ntp_time as i64 - local_time as i64;

    // Set the clock
    let ts = Timespec {
        tv_sec: ntp_time as i64,
        tv_nsec: 0,
    };
    let ret = clock_settime(CLOCK_REALTIME, &ts);
    if ret < 0 {
        log("clock_settime() failed");
        return -1;
    }

    // Log the sync
    prints("[sntpd] synced: offset=");
    if offset >= 0 {
        prints("+");
    }
    print_i64(offset);
    prints("s, server_time=");
    print_i64(ntp_time as i64);
    prints("\n");

    offset
}

fn sleep_secs(secs: u64) {
    let req = Timespec {
        tv_sec: secs as i64,
        tv_nsec: 0,
    };
    nanosleep(&req, None);
}

/// Main daemon loop.
/// — WireSaint: "Resolve. Sync. Sleep. Repeat. The life of an NTP daemon."
fn run_daemon() {
    log("starting SNTP daemon");

    // Initial delay — let networkd bring up the interface
    sleep_secs(INITIAL_DELAY_SECS);

    loop {
        // Resolve NTP server
        let server_ip = match dns::resolve(DEFAULT_NTP_SERVER, None) {
            Some(ip) => {
                prints("[sntpd] resolved ");
                prints(DEFAULT_NTP_SERVER);
                prints(" -> ");
                print_i64(ip.0 as i64); prints(".");
                print_i64(ip.1 as i64); prints(".");
                print_i64(ip.2 as i64); prints(".");
                print_i64(ip.3 as i64); prints("\n");
                ip
            }
            None => {
                log("DNS resolution failed, retrying in 60s");
                sleep_secs(RETRY_INTERVAL_SECS);
                continue;
            }
        };

        // Sync
        let offset = do_ntp_sync(server_ip);

        // Sleep until next sync
        if offset >= 0 {
            sleep_secs(SYNC_INTERVAL_SECS);
        } else {
            // Failed — retry sooner
            sleep_secs(RETRY_INTERVAL_SECS);
        }
    }
}

#[unsafe(no_mangle)]
fn main(_argc: i32, _argv: *const *const u8) -> i32 {
    run_daemon();
    0
}
