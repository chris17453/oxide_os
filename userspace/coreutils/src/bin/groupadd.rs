//! groupadd - create a new group
//!
//! --- WireSaint: Storage systems + filesystems ---
//! Implements group creation with proper /etc/group management.
//! Follows UNIX standard group format.
//!
//! Features:
//! - Add group to /etc/group
//! - Specify GID (-g)
//! - System group (-r)

#![no_std]
#![no_main]

use libc::*;

const MAX_LINE: usize = 512;
const GROUP_PATH: &str = "/etc/group";

struct GroupConfig {
    groupname: [u8; 64],
    groupname_len: usize,
    gid: Option<u32>,
    system_group: bool,
}

impl GroupConfig {
    fn new() -> Self {
        GroupConfig {
            groupname: [0; 64],
            groupname_len: 0,
            gid: None,
            system_group: false,
        }
    }
}

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

fn parse_u32(s: &str) -> Option<u32> {
    let bytes = s.as_bytes();
    if bytes.is_empty() {
        return None;
    }
    let mut result = 0u32;
    for &b in bytes {
        if b < b'0' || b > b'9' {
            return None;
        }
        result = result.saturating_mul(10).saturating_add((b - b'0') as u32);
    }
    Some(result)
}

fn copy_str_to_buf(dst: &mut [u8], src: &str) -> usize {
    let bytes = src.as_bytes();
    let len = bytes.len().min(dst.len() - 1);
    dst[..len].copy_from_slice(&bytes[..len]);
    dst[len] = 0;
    len
}

/// Check if group already exists in /etc/group
fn group_exists(groupname: &[u8], groupname_len: usize) -> bool {
    let fd = open2(GROUP_PATH, O_RDONLY);
    if fd < 0 {
        return false;
    }

    let mut buf = [0u8; 4096];
    let mut line = [0u8; MAX_LINE];
    let mut line_len = 0;

    loop {
        let n = read(fd, &mut buf);
        if n <= 0 {
            break;
        }

        for i in 0..(n as usize) {
            let byte = buf[i];
            if byte == b'\n' {
                // Check if line starts with groupname:
                if line_len >= groupname_len {
                    let mut match_found = true;
                    for j in 0..groupname_len {
                        if line[j] != groupname[j] {
                            match_found = false;
                            break;
                        }
                    }
                    if match_found && line_len > groupname_len && line[groupname_len] == b':' {
                        close(fd);
                        return true;
                    }
                }
                line_len = 0;
            } else if line_len < MAX_LINE {
                line[line_len] = byte;
                line_len += 1;
            }
        }
    }

    close(fd);
    false
}

/// Find the next available GID
fn find_next_gid(system_group: bool) -> u32 {
    let fd = open2(GROUP_PATH, O_RDONLY);
    if fd < 0 {
        return if system_group { 100 } else { 1000 };
    }

    let mut buf = [0u8; 4096];
    let mut line = [0u8; MAX_LINE];
    let mut line_len = 0;
    let mut max_gid = if system_group { 99u32 } else { 999u32 };

    loop {
        let n = read(fd, &mut buf);
        if n <= 0 {
            break;
        }

        for i in 0..(n as usize) {
            let byte = buf[i];
            if byte == b'\n' {
                // Parse GID from line (format: groupname:password:gid:members)
                if line_len > 0 {
                    let mut colon_count = 0;
                    let mut gid_start = 0;
                    let mut gid_end = 0;
                    for j in 0..line_len {
                        if line[j] == b':' {
                            colon_count += 1;
                            if colon_count == 2 {
                                gid_start = j + 1;
                            } else if colon_count == 3 {
                                gid_end = j;
                                break;
                            }
                        }
                    }
                    // Handle case where there's no trailing colon (no members)
                    if colon_count == 2 {
                        gid_end = line_len;
                    }

                    if colon_count >= 2 && gid_start < gid_end {
                        let mut gid = 0u32;
                        for j in gid_start..gid_end {
                            if line[j] >= b'0' && line[j] <= b'9' {
                                gid = gid * 10 + (line[j] - b'0') as u32;
                            }
                        }
                        if system_group {
                            // System groups: 0-999
                            if gid > max_gid && gid < 1000 {
                                max_gid = gid;
                            }
                        } else {
                            // User groups: 1000+
                            if gid > max_gid && gid >= 1000 && gid < 60000 {
                                max_gid = gid;
                            }
                        }
                    }
                }
                line_len = 0;
            } else if line_len < MAX_LINE {
                line[line_len] = byte;
                line_len += 1;
            }
        }
    }

    close(fd);
    max_gid + 1
}

/// Append group entry to /etc/group
fn add_group_to_file(config: &GroupConfig) -> i32 {
    // Open file for append
    let fd = open2(GROUP_PATH, O_WRONLY | O_APPEND | O_CREAT);
    if fd < 0 {
        eprintlns("groupadd: cannot open /etc/group");
        return 1;
    }

    // Format: groupname:x:gid:\n
    let mut line = [0u8; MAX_LINE];
    let mut pos = 0;

    // Group name
    for i in 0..config.groupname_len {
        line[pos] = config.groupname[i];
        pos += 1;
    }
    line[pos] = b':';
    pos += 1;

    // Password (use 'x' for shadow password)
    line[pos] = b'x';
    pos += 1;
    line[pos] = b':';
    pos += 1;

    // GID
    let gid = config
        .gid
        .unwrap_or_else(|| find_next_gid(config.system_group));
    let gid_str = format_u32(gid);
    for &b in gid_str.as_bytes() {
        line[pos] = b;
        pos += 1;
    }
    line[pos] = b':';
    pos += 1;

    // Members (empty for now)
    line[pos] = b'\n';
    pos += 1;

    // Write to file
    let written = write(fd, &line[..pos]);
    close(fd);

    if written != pos as isize {
        eprintlns("groupadd: failed to write to /etc/group");
        return 1;
    }

    0
}

fn format_u32(n: u32) -> &'static str {
    static mut BUF: [u8; 16] = [0; 16];
    unsafe {
        let mut val = n;
        let mut pos = 15;
        if val == 0 {
            BUF[pos] = b'0';
            return core::str::from_utf8_unchecked(&BUF[pos..pos + 1]);
        }
        while val > 0 {
            BUF[pos] = b'0' + (val % 10) as u8;
            val /= 10;
            if pos > 0 {
                pos -= 1;
            }
        }
        core::str::from_utf8_unchecked(&BUF[pos + 1..16])
    }
}

fn print_usage() {
    eprintlns("Usage: groupadd [options] GROUPNAME");
    eprintlns("Options:");
    eprintlns("  -g GID        Group ID");
    eprintlns("  -r            Create system group (GID < 1000)");
}

#[unsafe(no_mangle)]
fn main(argc: i32, argv: *const *const u8) -> i32 {
    // Check root privileges
    if getuid() != 0 {
        eprintlns("groupadd: permission denied (must be root)");
        return 1;
    }

    let mut config = GroupConfig::new();
    let mut i = 1;

    // Parse arguments
    while i < argc {
        let arg = unsafe { cstr_to_str(*argv.add(i as usize)) };

        if arg.starts_with("-") {
            match arg {
                "-g" => {
                    i += 1;
                    if i >= argc {
                        eprintlns("groupadd: option requires an argument -- 'g'");
                        return 1;
                    }
                    let gid_str = unsafe { cstr_to_str(*argv.add(i as usize)) };
                    config.gid = parse_u32(gid_str);
                    if config.gid.is_none() {
                        eprintlns("groupadd: invalid GID");
                        return 1;
                    }
                }
                "-r" => {
                    config.system_group = true;
                }
                "-h" | "--help" => {
                    print_usage();
                    return 0;
                }
                _ => {
                    eprints("groupadd: invalid option -- '");
                    eprints(arg);
                    eprintlns("'");
                    print_usage();
                    return 1;
                }
            }
        } else {
            // This is the groupname
            config.groupname_len = copy_str_to_buf(&mut config.groupname, arg);
            break;
        }
        i += 1;
    }

    // Validate groupname
    if config.groupname_len == 0 {
        eprintlns("groupadd: no group name specified");
        print_usage();
        return 1;
    }

    // Check if group already exists
    if group_exists(&config.groupname, config.groupname_len) {
        eprints("groupadd: group '");
        let groupname_str =
            unsafe { core::str::from_utf8_unchecked(&config.groupname[..config.groupname_len]) };
        eprints(groupname_str);
        eprintlns("' already exists");
        return 1;
    }

    // Add group
    if add_group_to_file(&config) != 0 {
        return 1;
    }

    printlns("Group added successfully");
    0
}
