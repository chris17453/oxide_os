//! useradd - create a new user account
//!
//! --- WireSaint: Storage systems + filesystems ---
//! Implements user account creation with proper /etc/passwd management
//! and home directory initialization. Follows UNIX standard passwd format.
//!
//! Features:
//! - Add user to /etc/passwd
//! - Create home directory (-m)
//! - Specify UID (-u)
//! - Specify GID (-g)
//! - Specify shell (-s)
//! - Specify home directory (-d)
//! - Specify comment/GECOS (-c)

#![no_std]
#![no_main]

use libc::*;

const MAX_LINE: usize = 512;
const PASSWD_PATH: &str = "/etc/passwd";

struct UserConfig {
    username: [u8; 64],
    username_len: usize,
    uid: Option<u32>,
    gid: Option<u32>,
    gecos: [u8; 128],
    gecos_len: usize,
    home: [u8; 128],
    home_len: usize,
    shell: [u8; 64],
    shell_len: usize,
    create_home: bool,
}

impl UserConfig {
    fn new() -> Self {
        UserConfig {
            username: [0; 64],
            username_len: 0,
            uid: None,
            gid: None,
            gecos: [0; 128],
            gecos_len: 0,
            home: [0; 128],
            home_len: 0,
            shell: [0; 64],
            shell_len: 0,
            create_home: false,
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

/// Check if username already exists in /etc/passwd
fn user_exists(username: &[u8], username_len: usize) -> bool {
    let fd = open2(PASSWD_PATH, O_RDONLY);
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
                // Check if line starts with username:
                if line_len >= username_len {
                    let mut match_found = true;
                    for j in 0..username_len {
                        if line[j] != username[j] {
                            match_found = false;
                            break;
                        }
                    }
                    if match_found && line_len > username_len && line[username_len] == b':' {
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

/// Find the next available UID
fn find_next_uid() -> u32 {
    let fd = open2(PASSWD_PATH, O_RDONLY);
    if fd < 0 {
        return 1000; // Default starting UID for regular users
    }

    let mut buf = [0u8; 4096];
    let mut line = [0u8; MAX_LINE];
    let mut line_len = 0;
    let mut max_uid = 999u32; // Start from 999, next will be 1000

    loop {
        let n = read(fd, &mut buf);
        if n <= 0 {
            break;
        }

        for i in 0..(n as usize) {
            let byte = buf[i];
            if byte == b'\n' {
                // Parse UID from line (format: user:pass:uid:gid:...)
                if line_len > 0 {
                    let mut colon_count = 0;
                    let mut uid_start = 0;
                    let mut uid_end = 0;
                    for j in 0..line_len {
                        if line[j] == b':' {
                            colon_count += 1;
                            if colon_count == 2 {
                                uid_start = j + 1;
                            } else if colon_count == 3 {
                                uid_end = j;
                                break;
                            }
                        }
                    }
                    if colon_count >= 3 && uid_start < uid_end {
                        let mut uid = 0u32;
                        for j in uid_start..uid_end {
                            if line[j] >= b'0' && line[j] <= b'9' {
                                uid = uid * 10 + (line[j] - b'0') as u32;
                            }
                        }
                        if uid > max_uid && uid < 60000 {
                            // Ignore system reserved UIDs above 60000
                            max_uid = uid;
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
    max_uid + 1
}

/// Create home directory for user
fn create_home_dir(home: &[u8], home_len: usize, uid: u32, gid: u32) -> i32 {
    // Create directory
    let home_str = unsafe { core::str::from_utf8_unchecked(&home[..home_len]) };

    if sys_mkdir(home_str, 0o755) != 0 {
        // Check if it already exists
        let fd = open(home_str, O_RDONLY | O_DIRECTORY, 0);
        if fd < 0 {
            eprints("useradd: cannot create home directory '");
            eprints(home_str);
            eprintlns("'");
            return 1;
        }
        close(fd);
    }

    // Change ownership to the new user
    let result = chown(home.as_ptr(), home_len, uid as i32, gid as i32);
    if result < 0 {
        eprints("useradd: warning: cannot change ownership of '");
        eprints(home_str);
        eprintlns("'");
    }

    0
}

/// Append user entry to /etc/passwd
fn add_user_to_passwd(config: &UserConfig) -> i32 {
    // Open file for append
    let fd = open2(PASSWD_PATH, O_WRONLY | O_APPEND | O_CREAT);
    if fd < 0 {
        eprintlns("useradd: cannot open /etc/passwd");
        return 1;
    }

    // Format: username:x:uid:gid:gecos:home:shell\n
    let mut line = [0u8; MAX_LINE];
    let mut pos = 0;

    // Username
    for i in 0..config.username_len {
        line[pos] = config.username[i];
        pos += 1;
    }
    line[pos] = b':';
    pos += 1;

    // Password (use 'x' for shadow password)
    line[pos] = b'x';
    pos += 1;
    line[pos] = b':';
    pos += 1;

    // UID
    let uid = config.uid.unwrap_or_else(find_next_uid);
    let uid_str = format_u32(uid);
    for &b in uid_str.as_bytes() {
        line[pos] = b;
        pos += 1;
    }
    line[pos] = b':';
    pos += 1;

    // GID
    let gid = config.gid.unwrap_or(uid);
    let gid_str = format_u32(gid);
    for &b in gid_str.as_bytes() {
        line[pos] = b;
        pos += 1;
    }
    line[pos] = b':';
    pos += 1;

    // GECOS
    for i in 0..config.gecos_len {
        line[pos] = config.gecos[i];
        pos += 1;
    }
    line[pos] = b':';
    pos += 1;

    // Home directory
    for i in 0..config.home_len {
        line[pos] = config.home[i];
        pos += 1;
    }
    line[pos] = b':';
    pos += 1;

    // Shell
    for i in 0..config.shell_len {
        line[pos] = config.shell[i];
        pos += 1;
    }
    line[pos] = b'\n';
    pos += 1;

    // Write to file
    let written = write(fd, &line[..pos]);
    close(fd);

    if written != pos as isize {
        eprintlns("useradd: failed to write to /etc/passwd");
        return 1;
    }

    // Create home directory if requested
    if config.create_home {
        create_home_dir(&config.home, config.home_len, uid, gid);
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
    eprintlns("Usage: useradd [options] USERNAME");
    eprintlns("Options:");
    eprintlns("  -u UID        User ID");
    eprintlns("  -g GID        Primary group ID");
    eprintlns("  -d HOME       Home directory");
    eprintlns("  -s SHELL      Login shell");
    eprintlns("  -c COMMENT    GECOS comment field");
    eprintlns("  -m            Create home directory");
}

#[unsafe(no_mangle)]
fn main(argc: i32, argv: *const *const u8) -> i32 {
    // Check root privileges
    if getuid() != 0 {
        eprintlns("useradd: permission denied (must be root)");
        return 1;
    }

    let mut config = UserConfig::new();
    let mut i = 1;

    // Parse arguments
    while i < argc {
        let arg = unsafe { cstr_to_str(*argv.add(i as usize)) };

        if arg.starts_with("-") {
            match arg {
                "-u" => {
                    i += 1;
                    if i >= argc {
                        eprintlns("useradd: option requires an argument -- 'u'");
                        return 1;
                    }
                    let uid_str = unsafe { cstr_to_str(*argv.add(i as usize)) };
                    config.uid = parse_u32(uid_str);
                    if config.uid.is_none() {
                        eprintlns("useradd: invalid UID");
                        return 1;
                    }
                }
                "-g" => {
                    i += 1;
                    if i >= argc {
                        eprintlns("useradd: option requires an argument -- 'g'");
                        return 1;
                    }
                    let gid_str = unsafe { cstr_to_str(*argv.add(i as usize)) };
                    config.gid = parse_u32(gid_str);
                    if config.gid.is_none() {
                        eprintlns("useradd: invalid GID");
                        return 1;
                    }
                }
                "-d" => {
                    i += 1;
                    if i >= argc {
                        eprintlns("useradd: option requires an argument -- 'd'");
                        return 1;
                    }
                    let home_str = unsafe { cstr_to_str(*argv.add(i as usize)) };
                    config.home_len = copy_str_to_buf(&mut config.home, home_str);
                }
                "-s" => {
                    i += 1;
                    if i >= argc {
                        eprintlns("useradd: option requires an argument -- 's'");
                        return 1;
                    }
                    let shell_str = unsafe { cstr_to_str(*argv.add(i as usize)) };
                    config.shell_len = copy_str_to_buf(&mut config.shell, shell_str);
                }
                "-c" => {
                    i += 1;
                    if i >= argc {
                        eprintlns("useradd: option requires an argument -- 'c'");
                        return 1;
                    }
                    let gecos_str = unsafe { cstr_to_str(*argv.add(i as usize)) };
                    config.gecos_len = copy_str_to_buf(&mut config.gecos, gecos_str);
                }
                "-m" => {
                    config.create_home = true;
                }
                "-h" | "--help" => {
                    print_usage();
                    return 0;
                }
                _ => {
                    eprints("useradd: invalid option -- '");
                    eprints(arg);
                    eprintlns("'");
                    print_usage();
                    return 1;
                }
            }
        } else {
            // This is the username
            config.username_len = copy_str_to_buf(&mut config.username, arg);
            break;
        }
        i += 1;
    }

    // Validate username
    if config.username_len == 0 {
        eprintlns("useradd: no username specified");
        print_usage();
        return 1;
    }

    // Check if user already exists
    if user_exists(&config.username, config.username_len) {
        eprints("useradd: user '");
        let username_str =
            unsafe { core::str::from_utf8_unchecked(&config.username[..config.username_len]) };
        eprints(username_str);
        eprintlns("' already exists");
        return 1;
    }

    // Set defaults
    if config.home_len == 0 {
        // Default home: /home/username
        let home_prefix = "/home/";
        let prefix_len = home_prefix.len();
        for (i, &b) in home_prefix.as_bytes().iter().enumerate() {
            config.home[i] = b;
        }
        for i in 0..config.username_len {
            config.home[prefix_len + i] = config.username[i];
        }
        config.home_len = prefix_len + config.username_len;
        config.home[config.home_len] = 0;
    }

    if config.shell_len == 0 {
        // Default shell: /bin/esh
        config.shell_len = copy_str_to_buf(&mut config.shell, "/bin/esh");
    }

    // Add user
    if add_user_to_passwd(&config) != 0 {
        return 1;
    }

    printlns("User added successfully");
    0
}
