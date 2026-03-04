//! UEFI Diagnostic Console
//!
//! A minimal command-line shell running in UEFI text mode (SimpleTextOutput).
//! For when the graphical boot menu isn't enough and you need to poke around
//! the ESP like a pre-boot forensic investigator.
//!
//! Commands:
//! - `ls [path]` — list files on ESP
//! - `info` — firmware info, memory map summary, GOP modes, ACPI RSDP
//! - `boot <path> [options]` — manually boot a kernel
//! - `set <key> <value>` — override config values for this boot
//! - `reboot` — reset system
//! - `exit` — return to graphical boot menu
//!
//! — WireSaint: the pre-boot emergency room — for when everything else fails

use core::fmt::Write;

use crate::efi::{self, FmtBuf, EfiInputKey, EfiGraphicsOutputProtocol};
use crate::efi::text::*;
use crate::efi::runtime::*;

use crate::config::BootConfig;
use crate::discovery;

/// Result of running the diagnostic console
pub enum ConsoleResult {
    /// Return to graphical boot menu
    ReturnToMenu,
    /// Boot a specific kernel (path index in config, or manual path)
    Boot(usize),
    /// Boot a kernel by manual path with options
    ManualBoot {
        path: [u8; 128],
        path_len: usize,
        options: [u8; 256],
        options_len: usize,
    },
}

/// Run the UEFI diagnostic console.
/// — WireSaint: entering the bunker — text mode, no frills, maximum control
pub fn run_console(config: &mut BootConfig) -> ConsoleResult {
    // Switch to text mode
    efi::clear_screen();

    print_banner();

    let mut line_buf = [0u8; 256];
    let mut line_len: usize;

    loop {
        print_str("oxide> ");

        line_len = read_line(&mut line_buf);
        let line = &line_buf[..line_len];

        // Trim the line
        let line = trim_bytes(line);
        if line.is_empty() {
            continue;
        }

        // Parse command and arguments
        let (cmd, args) = split_first_word(line);

        match cmd {
            b"exit" | b"quit" => return ConsoleResult::ReturnToMenu,
            b"reboot" => {
                print_line("Rebooting...");
                reboot();
            }
            b"ls" => cmd_ls(args),
            b"info" => cmd_info(),
            b"help" | b"?" => cmd_help(),
            b"boot" => {
                if let Some(result) = cmd_boot(args, config) {
                    return result;
                }
            }
            b"set" => cmd_set(args, config),
            b"entries" => cmd_entries(config),
            _ => {
                print_str("Unknown command: ");
                print_bytes(cmd);
                print_line("");
                print_line("Type 'help' for available commands.");
            }
        }
    }
}

fn print_banner() {
    print_line("");
    print_line("+==========================================+");
    print_line("|   OXIDE Diagnostic Console               |");
    print_line("|   -- WireSaint: pre-boot emergency room  |");
    print_line("+==========================================+");
    print_line("");
    print_line("Type 'help' for available commands.");
    print_line("");
}

fn cmd_help() {
    print_line("Commands:");
    print_line("  ls [path]          List files (default: \\EFI\\OXIDE\\)");
    print_line("  info               System firmware & hardware info");
    print_line("  entries            Show configured boot entries");
    print_line("  boot <N>           Boot entry number N");
    print_line("  boot <path> [opts] Boot kernel at path with options");
    print_line("  set timeout <N>    Set auto-boot timeout");
    print_line("  set default <N>    Set default entry index");
    print_line("  reboot             Reset system");
    print_line("  exit               Return to graphical boot menu");
    print_line("");
}

fn cmd_ls(args: &[u8]) {
    let path = if args.is_empty() {
        "\\EFI\\OXIDE"
    } else {
        core::str::from_utf8(args).unwrap_or("\\EFI\\OXIDE")
    };

    print_str("Directory: ");
    print_line(path);
    print_line("-----------------------------------------");

    let (entries, entry_count) = discovery::list_directory(path);
    if entry_count == 0 {
        print_line("  (empty or not found)");
    } else {
        for i in 0..entry_count {
            let (name, name_len, size, is_dir) = &entries[i];
            let name_str = core::str::from_utf8(&name[..*name_len]).unwrap_or("?");
            if *is_dir {
                print_str("  [DIR]  ");
                print_line(name_str);
            } else {
                // Format size — SableWire: FmtBuf replaces format!()
                let size_kb = size / 1024;
                let mut buf = FmtBuf::<64>::new();
                write!(buf, "  {:>8} KB  {}", size_kb, name_str).ok();
                print_line(buf.as_str());
            }
        }
    }
    print_line("");
}

fn cmd_info() {
    print_line("System Information:");
    print_line("-----------------------------------------");

    // Firmware info
    let mut vendor_buf = [0u8; 64];
    let vendor_len = efi::firmware_vendor_ascii(&mut vendor_buf);
    if vendor_len > 0 {
        let vendor_str = core::str::from_utf8(&vendor_buf[..vendor_len]).unwrap_or("?");
        let mut buf = FmtBuf::<128>::new();
        write!(buf, "  Firmware: {}", vendor_str).ok();
        print_line(buf.as_str());
    }

    let rev = efi::firmware_revision();
    {
        let mut buf = FmtBuf::<64>::new();
        write!(buf, "  Revision: {}.{}", rev >> 16, rev & 0xFFFF).ok();
        print_line(buf.as_str());
    }

    // UEFI spec version
    let (major, minor) = efi::uefi_revision();
    {
        let mut buf = FmtBuf::<64>::new();
        write!(buf, "  UEFI: {}.{}", major, minor).ok();
        print_line(buf.as_str());
    }

    // Configuration tables
    let ct_count = efi::config_table_count();
    {
        let mut buf = FmtBuf::<64>::new();
        write!(buf, "  Config tables: {}", ct_count).ok();
        print_line(buf.as_str());
    }

    // RSDP
    let rsdp = crate::find_rsdp_in_config_tables();
    if rsdp != 0 {
        let mut buf = FmtBuf::<64>::new();
        write!(buf, "  ACPI RSDP: 0x{:016x}", rsdp).ok();
        print_line(buf.as_str());
    } else {
        print_line("  ACPI RSDP: not found");
    }

    // GOP info
    if let Some(gop_handle) = efi::locate_handle_for_protocol(&efi::EFI_GRAPHICS_OUTPUT_PROTOCOL_GUID) {
        let gop: Option<*mut EfiGraphicsOutputProtocol> = efi::handle_protocol(
            gop_handle,
            &efi::EFI_GRAPHICS_OUTPUT_PROTOCOL_GUID,
        );
        if let Some(gop) = gop {
            unsafe {
                let mode = &*(*gop).mode;
                let info = &*mode.info;
                let w = info.horizontal_resolution;
                let h = info.vertical_resolution;
                let stride = info.pixels_per_scan_line;

                let mut buf = FmtBuf::<64>::new();
                write!(buf, "  GOP: {}x{} (stride: {})", w, h, stride).ok();
                print_line(buf.as_str());

                let mut buf2 = FmtBuf::<64>::new();
                write!(buf2, "  GOP modes: {}", mode.max_mode).ok();
                print_line(buf2.as_str());
            }
        }
    }

    // Memory summary
    {
        let mut mmap_buf = [0u8; 8192];
        if let Some(bs) = efi::boot_services() {
            let mut map_size = mmap_buf.len();
            let mut map_key: usize = 0;
            let mut desc_size: usize = 0;
            let mut desc_version: u32 = 0;

            let status = unsafe {
                (bs.get_memory_map)(
                    &mut map_size,
                    mmap_buf.as_mut_ptr() as *mut efi::EfiMemoryDescriptor,
                    &mut map_key,
                    &mut desc_size,
                    &mut desc_version,
                )
            };

            if !efi::efi_error(status) && desc_size > 0 {
                let mut total_usable = 0u64;
                let mut total_reserved = 0u64;
                let count = map_size / desc_size;

                for i in 0..count {
                    let desc = unsafe {
                        &*(mmap_buf.as_ptr().add(i * desc_size) as *const efi::EfiMemoryDescriptor)
                    };
                    let size = desc.number_of_pages * 4096;
                    if desc.memory_type == efi::boot_services::EFI_CONVENTIONAL_MEMORY {
                        total_usable += size;
                    } else {
                        total_reserved += size;
                    }
                }

                let mut buf = FmtBuf::<128>::new();
                write!(buf, "  Memory: {} MB usable, {} MB reserved ({} regions)",
                    total_usable / (1024 * 1024),
                    total_reserved / (1024 * 1024),
                    count
                ).ok();
                print_line(buf.as_str());
            }
        }
    }

    print_line("");
}

fn cmd_entries(config: &BootConfig) {
    print_line("Boot Entries:");
    print_line("-----------------------------------------");

    if config.entry_count == 0 {
        print_line("  (no entries)");
    } else {
        for i in 0..config.entry_count {
            let entry = &config.entries[i];
            let default_tag = if i == config.default_index {
                " [default]"
            } else {
                ""
            };
            let valid_tag = if entry.valid { "" } else { " [MISSING]" };

            let label = entry.label_str();
            let path = entry.path_str();
            let opts = entry.options_str();

            let mut buf = FmtBuf::<128>::new();
            write!(buf, "  {}. {}{}{}", i, label, default_tag, valid_tag).ok();
            print_line(buf.as_str());

            let mut buf2 = FmtBuf::<128>::new();
            write!(buf2, "     Path: {}", path).ok();
            print_line(buf2.as_str());

            if !opts.is_empty() {
                let mut buf3 = FmtBuf::<128>::new();
                write!(buf3, "     Options: {}", opts).ok();
                print_line(buf3.as_str());
            }
        }
    }

    let mut buf = FmtBuf::<64>::new();
    write!(buf, "  Timeout: {}s", config.timeout_secs).ok();
    print_line(buf.as_str());
    print_line("");
}

fn cmd_boot(args: &[u8], config: &BootConfig) -> Option<ConsoleResult> {
    if args.is_empty() {
        print_line("Usage: boot <entry_number> or boot <path> [options]");
        return None;
    }

    // Try to parse as entry number first
    if let Some(idx) = parse_usize(args) {
        if idx < config.entry_count {
            let mut buf = FmtBuf::<64>::new();
            write!(buf, "Booting entry {}...", idx).ok();
            print_line(buf.as_str());
            return Some(ConsoleResult::Boot(idx));
        } else {
            print_line("Entry index out of range.");
            return None;
        }
    }

    // Otherwise treat as a path
    let (path_bytes, opts_bytes) = split_first_word(args);
    let mut path = [0u8; 128];
    let path_len = path_bytes.len().min(127);
    path[..path_len].copy_from_slice(&path_bytes[..path_len]);

    let mut options = [0u8; 256];
    let options_len = opts_bytes.len().min(255);
    options[..options_len].copy_from_slice(&opts_bytes[..options_len]);

    let path_str = core::str::from_utf8(&path[..path_len]).unwrap_or("?");
    let mut buf = FmtBuf::<128>::new();
    write!(buf, "Booting {}...", path_str).ok();
    print_line(buf.as_str());

    Some(ConsoleResult::ManualBoot {
        path,
        path_len,
        options,
        options_len,
    })
}

fn cmd_set(args: &[u8], config: &mut BootConfig) {
    let (key, value) = split_first_word(args);

    match key {
        b"timeout" => {
            if let Some(n) = parse_usize(value) {
                config.timeout_secs = n as i32;
                let mut buf = FmtBuf::<64>::new();
                write!(buf, "Timeout set to {}s", n).ok();
                print_line(buf.as_str());
            } else {
                print_line("Usage: set timeout <seconds>");
            }
        }
        b"default" => {
            if let Some(n) = parse_usize(value) {
                if n < config.entry_count {
                    config.default_index = n;
                    let mut buf = FmtBuf::<64>::new();
                    write!(buf, "Default set to entry {}", n).ok();
                    print_line(buf.as_str());
                } else {
                    print_line("Entry index out of range.");
                }
            } else {
                print_line("Usage: set default <entry_index>");
            }
        }
        _ => {
            print_line("Usage: set <timeout|default> <value>");
        }
    }
}

// ── I/O Helpers ──
// — WireSaint: UEFI SimpleTextOutput is our canvas, SimpleTextInput is our brush

fn print_str(s: &str) {
    efi::print_ascii(s);
}

fn print_line(s: &str) {
    efi::println_ascii(s);
}

fn print_bytes(b: &[u8]) {
    if let Ok(s) = core::str::from_utf8(b) {
        print_str(s);
    }
}

/// Read a line from UEFI SimpleTextInput with basic line editing
/// — WireSaint: readline for the firmware age
fn read_line(buf: &mut [u8; 256]) -> usize {
    let mut len = 0usize;

    loop {
        // Poll for key
        if let Some(key) = efi::read_key() {
            // Printable character
            if key.scan_code == SCAN_NULL && key.unicode_char != 0 {
                let c = key.unicode_char;
                if c == 0x000D || c == 0x000A {
                    // Enter
                    print_str("\r\n");
                    return len;
                } else if c == 8 || c == 127 {
                    // Backspace
                    if len > 0 {
                        len -= 1;
                        print_str("\x08 \x08");
                    }
                } else if c >= 32 && c < 127 && len < 255 {
                    buf[len] = c as u8;
                    len += 1;
                    // Echo character
                    let mut echo = [0u8; 4];
                    let ch = char::from(c as u8);
                    let s = ch.encode_utf8(&mut echo);
                    print_str(s);
                }
            } else if key.scan_code == SCAN_ESC {
                // Clear line
                for _ in 0..len {
                    print_str("\x08 \x08");
                }
                len = 0;
            }
            continue;
        }

        efi::stall(10_000); // 10ms between polls
    }
}

// ── Parsing Helpers ──

fn trim_bytes(s: &[u8]) -> &[u8] {
    let mut start = 0;
    let mut end = s.len();
    while start < end && (s[start] == b' ' || s[start] == b'\t') {
        start += 1;
    }
    while end > start && (s[end - 1] == b' ' || s[end - 1] == b'\t') {
        end -= 1;
    }
    &s[start..end]
}

fn split_first_word(s: &[u8]) -> (&[u8], &[u8]) {
    let s = trim_bytes(s);
    for (i, &b) in s.iter().enumerate() {
        if b == b' ' || b == b'\t' {
            return (&s[..i], trim_bytes(&s[i + 1..]));
        }
    }
    (s, &[])
}

fn parse_usize(s: &[u8]) -> Option<usize> {
    let s = trim_bytes(s);
    if s.is_empty() {
        return None;
    }
    let mut val = 0usize;
    for &b in s {
        if b >= b'0' && b <= b'9' {
            val = val.checked_mul(10)?.checked_add((b - b'0') as usize)?;
        } else {
            return None;
        }
    }
    Some(val)
}

/// Reboot the system via UEFI RuntimeServices
/// — WireSaint: the nuclear option
fn reboot() -> ! {
    unsafe {
        let st = efi::system_table().expect("No system table");
        let rt = st.runtime_services;
        ((*rt).reset_system)(EFI_RESET_COLD, efi::EFI_SUCCESS, 0, core::ptr::null());
    }
}
