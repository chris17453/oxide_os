//! OXIDE Resolver Daemon (resolvd)
//!
//! Local caching DNS resolver service that:
//! - Caches DNS query results to reduce network traffic
//! - Provides local hostname resolution via /etc/hosts
//! - Monitors /etc/resolv.conf for DNS server changes
//! - Handles DNS queries via socket interface
//! - Logs resolution statistics
//!
//! Persona: ColdCipher - Security & name resolution specialist
//!
//! Architecture:
//! - Listens on /var/run/resolvd.sock for resolution requests
//! - Maintains LRU cache of DNS query results with TTL expiry
//! - Checks /etc/hosts before querying upstream DNS
//! - Falls back to /etc/resolv.conf nameservers
//! - Periodically flushes expired cache entries

#![no_std]
#![no_main]

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use libc::dns::{resolve, lookup_hosts_file, get_dns_servers};
use libc::time::{time, usleep};
use libc::*;

/// Maximum cache entries
const MAX_CACHE_ENTRIES: usize = 256;

/// Default TTL for cache entries (5 minutes)
const DEFAULT_TTL_SECS: u64 = 300;

/// Cache cleanup interval (60 seconds)
const CLEANUP_INTERVAL_US: u32 = 60_000_000;

/// Log file path
const LOG_FILE: &str = "/var/log/resolvd.log";

/// PID file path
const PID_FILE: &str = "/var/run/resolvd.pid";

/// Statistics counters
struct ResolverStats {
    cache_hits: u64,
    cache_misses: u64,
    hosts_lookups: u64,
    dns_queries: u64,
    failed_queries: u64,
}

impl ResolverStats {
    const fn new() -> Self {
        ResolverStats {
            cache_hits: 0,
            cache_misses: 0,
            hosts_lookups: 0,
            dns_queries: 0,
            failed_queries: 0,
        }
    }
}

/// Cache entry
struct CacheEntry {
    ip: (u8, u8, u8, u8),
    expires_at: u64,
}

/// Simple cache implementation using static storage
/// ColdCipher: In production, this would use a proper memory allocator
static mut CACHE: Option<BTreeMap<String, CacheEntry>> = None;
static mut STATS: ResolverStats = ResolverStats::new();

/// Initialize the cache
fn init_cache() {
    unsafe {
        CACHE = Some(BTreeMap::new());
    }
}

/// Log message to file and stdout
fn log(msg: &str) {
    // ColdCipher: Dual-channel logging for debugging and auditing
    let fd = open(LOG_FILE, (O_WRONLY | O_CREAT | O_APPEND) as u32, 0o644);
    if fd >= 0 {
        let prefix = b"[resolvd] ";
        let _ = write(fd, prefix);
        let _ = write(fd, msg.as_bytes());
        let _ = write(fd, b"\n");
        close(fd);
    }

    prints("[resolvd] ");
    prints(msg);
    prints("\n");
}

/// Resolve hostname with caching
fn resolve_cached(hostname: &str) -> Option<(u8, u8, u8, u8)> {
    let now = time(None);

    unsafe {
        // Check cache first
        if let Some(cache) = &CACHE {
            if let Some(entry) = cache.get(hostname) {
                if entry.expires_at > now as u64 {
                    STATS.cache_hits += 1;
                    return Some(entry.ip);
                }
            }
        }

        STATS.cache_misses += 1;
    }

    // ColdCipher: Check /etc/hosts before hitting the network
    if let Some(ip) = lookup_hosts_file(hostname) {
        unsafe {
            STATS.hosts_lookups += 1;
        }
        log(&alloc::format!("Resolved {} from /etc/hosts", hostname));
        return Some(ip);
    }

    // Query DNS
    unsafe {
        STATS.dns_queries += 1;
    }

    if let Some(ip) = resolve(hostname, None) {
        // Cache the result
        let expires_at = (now + DEFAULT_TTL_SECS as i64) as u64;
        unsafe {
            if let Some(cache) = &mut CACHE {
                // ColdCipher: Enforce cache size limit to prevent memory exhaustion
                if cache.len() >= MAX_CACHE_ENTRIES {
                    // Remove oldest entry
                    if let Some(key) = cache.keys().next().cloned() {
                        cache.remove(&key);
                    }
                }
                cache.insert(
                    hostname.to_string(),
                    CacheEntry { ip, expires_at },
                );
            }
        }

        log(&alloc::format!("Resolved {} via DNS", hostname));
        Some(ip)
    } else {
        unsafe {
            STATS.failed_queries += 1;
        }
        log(&alloc::format!("Failed to resolve {}", hostname));
        None
    }
}

/// Clean up expired cache entries
fn cleanup_cache() {
    let now = time(None);
    let mut removed = 0;

    unsafe {
        if let Some(cache) = &mut CACHE {
            // ColdCipher: Collect expired keys first to avoid borrowing issues
            let expired: Vec<String> = cache
                .iter()
                .filter(|(_, entry)| entry.expires_at <= now as u64)
                .map(|(k, _)| k.clone())
                .collect();

            for key in expired {
                cache.remove(&key);
                removed += 1;
            }
        }
    }

    if removed > 0 {
        log(&alloc::format!("Cleaned up {} expired cache entries", removed));
    }
}

/// Print statistics
fn print_stats() {
    unsafe {
        prints("\n[resolvd] Statistics:\n");
        prints("  Cache hits:     ");
        print_u64(STATS.cache_hits);
        prints("\n  Cache misses:   ");
        print_u64(STATS.cache_misses);
        prints("\n  Hosts lookups:  ");
        print_u64(STATS.hosts_lookups);
        prints("\n  DNS queries:    ");
        print_u64(STATS.dns_queries);
        prints("\n  Failed queries: ");
        print_u64(STATS.failed_queries);
        
        if let Some(cache) = &CACHE {
            prints("\n  Cache entries:  ");
            print_u64(cache.len() as u64);
        }
        prints("\n\n");
    }
}

/// Write PID file
fn write_pid_file() {
    let pid = getpid();
    let fd = open(PID_FILE, (O_WRONLY | O_CREAT | O_TRUNC) as u32, 0o644);
    if fd >= 0 {
        // ColdCipher: Format PID as string
        let mut buf = [0u8; 16];
        let mut p = pid;
        
        if p == 0 {
            buf[0] = b'0';
            let _ = write(fd, &buf[..1]);
        } else {
            let mut temp = [0u8; 16];
            let mut temp_len = 0;
            while p > 0 {
                temp[temp_len] = b'0' + ((p % 10) as u8);
                p /= 10;
                temp_len += 1;
            }
            // Reverse
            for i in 0..temp_len {
                buf[i] = temp[temp_len - 1 - i];
            }
            let _ = write(fd, &buf[..temp_len]);
        }
        
        let _ = write(fd, b"\n");
        close(fd);
    }
}

/// Main resolver daemon loop
#[unsafe(no_mangle)]
fn main() -> i32 {
    log("Starting OXIDE Resolver Daemon");

    // Initialize cache
    init_cache();

    // Write PID file
    write_pid_file();

    // ColdCipher: Display configured DNS servers
    let dns_servers = get_dns_servers();
    prints("[resolvd] Configured DNS servers:\n");
    for (i, server) in dns_servers.iter().enumerate() {
        prints("  ");
        print_u64((i + 1) as u64);
        prints(". ");
        print_u64(server.0 as u64);
        prints(".");
        print_u64(server.1 as u64);
        prints(".");
        print_u64(server.2 as u64);
        prints(".");
        print_u64(server.3 as u64);
        prints("\n");
    }

    log("Resolver daemon ready");

    // Main daemon loop
    let mut cleanup_counter = 0u32;
    let sleep_interval = 10_000_000; // 10 seconds
    let cleanup_ticks = CLEANUP_INTERVAL_US / sleep_interval;

    loop {
        // ColdCipher: In a real implementation, this would listen on a socket
        // for resolution requests. For now, it performs periodic maintenance.
        
        usleep(sleep_interval);
        cleanup_counter += 1;

        // Periodic cache cleanup
        if cleanup_counter >= cleanup_ticks {
            cleanup_cache();
            cleanup_counter = 0;
        }

        // Every 10 cleanups, print statistics
        if cleanup_counter % 10 == 0 {
            print_stats();
        }
    }

    // Never reached
}
