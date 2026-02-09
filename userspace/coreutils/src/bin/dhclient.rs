//! dhclient - DHCP client for network configuration
//!
//! Triggers DHCP lease acquisition for network interfaces.
//! —ShadePacket: Because sometimes the network gods need a second ask.
//!
//! Usage: dhclient <interface>
//!        dhclient eth0
//!        dhclient -v eth0      # verbose mode
//!        dhclient -h           # help

#![no_std]
#![no_main]

use libc::syscall::dhcp_request;
use libc::{print_i64, printlns, prints};

/// Error codes returned by DHCP syscall
mod err {
    pub const ENODEV: i32 = -19;
    pub const ENETDOWN: i32 = -100;
    pub const ENETUNREACH: i32 = -101;
    pub const ETIMEDOUT: i32 = -110;
}

/// Print usage information
fn print_usage() {
    printlns("Usage: dhclient [OPTIONS] <interface>");
    printlns("");
    printlns("Trigger DHCP lease acquisition for a network interface.");
    printlns("");
    printlns("Options:");
    printlns("  -v, --verbose    Show detailed progress");
    printlns("  -h, --help       Show this help message");
    printlns("");
    printlns("Examples:");
    printlns("  dhclient eth0    Request DHCP lease for eth0");
    printlns("  dhclient -v lo   Verbose DHCP request for lo");
    printlns("");
    printlns("Exit Status:");
    printlns("  0  Success - DHCP lease acquired");
    printlns("  1  Error - DHCP request failed");
    printlns("");
    // —ShadePacket: The kernel writes leases to /var/lib/dhcp/<iface>.lease
    printlns("On success, lease is written to /var/lib/dhcp/<interface>.lease");
}

/// Parse command line arguments
struct Args {
    interface: [u8; 64],
    interface_len: usize,
    verbose: bool,
    help: bool,
}

impl Args {
    fn new() -> Self {
        Args {
            interface: [0; 64],
            interface_len: 0,
            verbose: false,
            help: false,
        }
    }

    fn interface_str(&self) -> &str {
        core::str::from_utf8(&self.interface[..self.interface_len]).unwrap_or("")
    }

    fn set_interface(&mut self, s: &str) {
        let bytes = s.as_bytes();
        let len = if bytes.len() > 63 { 63 } else { bytes.len() };
        self.interface[..len].copy_from_slice(&bytes[..len]);
        self.interface_len = len;
    }
}

fn parse_args(argc: usize, argv: *const *const u8) -> Args {
    let mut args = Args::new();

    for i in 1..argc {
        let arg = unsafe {
            let ptr = *argv.add(i);
            let mut len = 0;
            while *ptr.add(len) != 0 {
                len += 1;
            }
            core::str::from_utf8_unchecked(core::slice::from_raw_parts(ptr, len))
        };

        if arg == "-h" || arg == "--help" {
            args.help = true;
        } else if arg == "-v" || arg == "--verbose" {
            args.verbose = true;
        } else if !arg.starts_with('-') && args.interface_len == 0 {
            args.set_interface(arg);
        } else if arg.starts_with('-') {
            prints("dhclient: unknown option: ");
            printlns(arg);
        }
    }

    args
}

/// Describe error code
fn error_description(code: i32) -> &'static str {
    match code {
        err::ENODEV => "Interface not found",
        err::ENETDOWN => "Network is down",
        err::ENETUNREACH => "Network is unreachable",
        err::ETIMEDOUT => "DHCP timed out (no response from server)",
        _ => "Unknown error",
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn main(argc: usize, argv: *const *const u8) -> i32 {
    let args = parse_args(argc, argv);

    // —ShadePacket: Handle help first
    if args.help {
        print_usage();
        return 0;
    }

    // —ShadePacket: Interface is required
    if args.interface_len == 0 {
        printlns("dhclient: error: no interface specified");
        printlns("Usage: dhclient <interface>");
        return 1;
    }

    let iface = args.interface_str();

    if args.verbose {
        prints("dhclient: Requesting DHCP lease for ");
        printlns(iface);
    }

    // —ShadePacket: Call the kernel to do the heavy lifting
    let result = dhcp_request(iface);

    if result == 0 {
        if args.verbose {
            prints("dhclient: ");
            prints(iface);
            printlns(": DHCP lease acquired successfully");
            prints("dhclient: Lease written to /var/lib/dhcp/");
            prints(iface);
            printlns(".lease");
        } else {
            prints(iface);
            printlns(": DHCP OK");
        }
        0
    } else {
        let desc = error_description(result);
        prints("dhclient: ");
        prints(iface);
        prints(": ");
        prints(desc);
        prints(" (");
        print_i64(result as i64);
        printlns(")");
        1
    }
}
