//! Boot Configuration Parser
//!
//! Parses `\EFI\OXIDE\boot.cfg` — an INI-style config that tells the boot manager
//! which kernels exist, what options to pass, and how long to stare at the menu
//! before giving up and booting the default. All stack-allocated, no heap required.
//!
//! — BlackLatch: the gatekeeper reads the manifest before opening the gate

/// Maximum number of kernel entries we'll track
pub const MAX_ENTRIES: usize = 8;

/// Maximum path length (FAT32 paths aren't that long... right?)
pub const MAX_PATH: usize = 128;

/// Maximum label length
pub const MAX_LABEL: usize = 64;

/// Maximum options string length
pub const MAX_OPTIONS: usize = 256;

/// A single boot entry — everything the boot manager needs to launch one kernel
/// — BlackLatch: each entry is a contract between config and reality
#[derive(Clone)]
pub struct BootEntry {
    /// Filesystem path to the kernel ELF (e.g., `\EFI\OXIDE\kernel-0.2.0.elf`)
    pub path: [u8; MAX_PATH],
    pub path_len: usize,
    /// Human-readable label for the menu
    pub label: [u8; MAX_LABEL],
    pub label_len: usize,
    /// Boot options / command line to pass to kernel
    pub options: [u8; MAX_OPTIONS],
    pub options_len: usize,
    /// Path to initramfs (optional)
    pub initramfs_path: [u8; MAX_PATH],
    pub initramfs_path_len: usize,
    /// Parsed version for sorting (0.0.0 if unparseable)
    pub version_major: u16,
    pub version_minor: u16,
    pub version_patch: u16,
    /// Whether this entry has been validated (file exists)
    pub valid: bool,
}

impl BootEntry {
    pub const fn empty() -> Self {
        Self {
            path: [0u8; MAX_PATH],
            path_len: 0,
            label: [0u8; MAX_LABEL],
            label_len: 0,
            options: [0u8; MAX_OPTIONS],
            options_len: 0,
            initramfs_path: [0u8; MAX_PATH],
            initramfs_path_len: 0,
            version_major: 0,
            version_minor: 0,
            version_patch: 0,
            valid: false,
        }
    }

    /// Get path as a string slice
    pub fn path_str(&self) -> &str {
        core::str::from_utf8(&self.path[..self.path_len]).unwrap_or("")
    }

    /// Get label as a string slice
    pub fn label_str(&self) -> &str {
        core::str::from_utf8(&self.label[..self.label_len]).unwrap_or("")
    }

    /// Get options as a string slice
    pub fn options_str(&self) -> &str {
        core::str::from_utf8(&self.options[..self.options_len]).unwrap_or("")
    }

    /// Get initramfs path as a string slice
    pub fn initramfs_str(&self) -> &str {
        core::str::from_utf8(&self.initramfs_path[..self.initramfs_path_len]).unwrap_or("")
    }

    /// Compare versions for sorting (descending — newest first)
    pub fn version_cmp(&self, other: &Self) -> core::cmp::Ordering {
        match self.version_major.cmp(&other.version_major) {
            core::cmp::Ordering::Equal => match self.version_minor.cmp(&other.version_minor) {
                core::cmp::Ordering::Equal => self.version_patch.cmp(&other.version_patch),
                ord => ord,
            },
            ord => ord,
        }
        .reverse() // — BlackLatch: newest first, because who boots old kernels on purpose
    }
}

/// The complete boot configuration — stack-allocated, bounded, no surprises
/// — BlackLatch: the entire state of the pre-boot universe fits in here
pub struct BootConfig {
    /// Kernel entries (sorted by version descending after parse)
    pub entries: [BootEntry; MAX_ENTRIES],
    /// Number of valid entries
    pub entry_count: usize,
    /// Index of the default entry to auto-boot
    pub default_index: usize,
    /// Timeout in seconds (-1 = wait forever, 0 = instant boot)
    pub timeout_secs: i32,
    /// Whether config was loaded from file (vs auto-scan)
    pub from_config_file: bool,
}

impl BootConfig {
    pub const fn empty() -> Self {
        Self {
            entries: [
                BootEntry::empty(), BootEntry::empty(), BootEntry::empty(), BootEntry::empty(),
                BootEntry::empty(), BootEntry::empty(), BootEntry::empty(), BootEntry::empty(),
            ],
            entry_count: 0,
            default_index: 0,
            timeout_secs: 5,
            from_config_file: false,
        }
    }
}

/// Copy bytes from src slice into a fixed-size destination buffer.
/// Returns how many bytes were copied.
fn copy_to_buf(dst: &mut [u8], src: &[u8]) -> usize {
    let len = src.len().min(dst.len() - 1); // leave room for null terminator
    dst[..len].copy_from_slice(&src[..len]);
    dst[len] = 0;
    len
}

/// Parse a version string like "0.2.0" into (major, minor, patch)
/// — BlackLatch: semver without the crate dependency — shocking, I know
fn parse_version(s: &[u8]) -> (u16, u16, u16) {
    let mut parts = [0u16; 3];
    let mut part_idx = 0;
    let mut current = 0u16;

    for &b in s {
        if b == b'.' {
            if part_idx < 3 {
                parts[part_idx] = current;
                part_idx += 1;
                current = 0;
            }
        } else if b >= b'0' && b <= b'9' {
            current = current.saturating_mul(10).saturating_add((b - b'0') as u16);
        } else {
            // — BlackLatch: non-numeric garbage in a version string? how quaint.
            break;
        }
    }
    if part_idx < 3 {
        parts[part_idx] = current;
    }

    (parts[0], parts[1], parts[2])
}

/// Parse the boot.cfg file content into a BootConfig.
///
/// Format:
/// ```ini
/// [defaults]
/// timeout = 5
/// default = latest
///
/// [kernel.0.2.0]
/// path = \EFI\OXIDE\kernel-0.2.0.elf
/// label = OXIDE 0.2.0
/// options = debug-all
/// initramfs = \EFI\OXIDE\initramfs.cpio
/// ```
///
/// — BlackLatch: reading tea leaves from a FAT32 partition
pub fn parse_config(data: &[u8]) -> BootConfig {
    let mut config = BootConfig::empty();
    config.from_config_file = true;

    // Track which section we're in
    let mut in_defaults = false;
    let mut current_entry: Option<usize> = None;
    let mut default_str = [0u8; 64];
    let mut default_str_len = 0usize;

    // Process line by line
    let mut line_start = 0;
    let data_len = data.len();

    while line_start < data_len {
        // Find end of line
        let mut line_end = line_start;
        while line_end < data_len && data[line_end] != b'\n' && data[line_end] != b'\r' {
            line_end += 1;
        }

        let line = &data[line_start..line_end];
        let line = trim_bytes(line);

        // Skip empty lines and comments
        if !line.is_empty() && line[0] != b'#' {
            if line[0] == b'[' {
                // Section header
                if let Some(end) = find_byte(line, b']') {
                    let section = &line[1..end];
                    if bytes_eq(section, b"defaults") {
                        in_defaults = true;
                        current_entry = None;
                    } else if starts_with(section, b"kernel.") || starts_with(section, b"kernel") {
                        in_defaults = false;
                        if config.entry_count < MAX_ENTRIES {
                            let idx = config.entry_count;
                            config.entry_count += 1;
                            current_entry = Some(idx);

                            // Extract version from section name (e.g., "kernel.0.2.0")
                            if section.len() > 7 && section[6] == b'.' {
                                let ver_str = &section[7..];
                                let (maj, min, pat) = parse_version(ver_str);
                                config.entries[idx].version_major = maj;
                                config.entries[idx].version_minor = min;
                                config.entries[idx].version_patch = pat;
                            }
                        }
                    }
                }
            } else if let Some(eq_pos) = find_byte(line, b'=') {
                // Key = value pair
                let key = trim_bytes(&line[..eq_pos]);
                let value = trim_bytes(&line[eq_pos + 1..]);

                if in_defaults {
                    if bytes_eq(key, b"timeout") {
                        config.timeout_secs = parse_i32(value);
                    } else if bytes_eq(key, b"default") {
                        default_str_len = copy_to_buf(&mut default_str, value);
                    }
                    // resolution is parsed but we don't act on it yet
                } else if let Some(idx) = current_entry {
                    let entry = &mut config.entries[idx];
                    if bytes_eq(key, b"path") {
                        entry.path_len = copy_to_buf(&mut entry.path, value);
                    } else if bytes_eq(key, b"label") {
                        entry.label_len = copy_to_buf(&mut entry.label, value);
                    } else if bytes_eq(key, b"options") {
                        entry.options_len = copy_to_buf(&mut entry.options, value);
                    } else if bytes_eq(key, b"initramfs") {
                        entry.initramfs_path_len = copy_to_buf(&mut entry.initramfs_path, value);
                    }
                }
            }
        }

        // Advance past the line ending (\r\n or \n)
        line_start = line_end;
        if line_start < data_len && data[line_start] == b'\r' {
            line_start += 1;
        }
        if line_start < data_len && data[line_start] == b'\n' {
            line_start += 1;
        }
    }

    // Resolve default entry
    // — BlackLatch: "latest" means highest version, anything else is an exact label match
    if default_str_len > 0 {
        let default_val = &default_str[..default_str_len];
        if bytes_eq(default_val, b"latest") {
            // Find highest version entry
            let mut best_idx = 0usize;
            for i in 1..config.entry_count {
                if config.entries[i].version_cmp(&config.entries[best_idx])
                    == core::cmp::Ordering::Less
                {
                    // version_cmp is reversed, so Less = higher version
                    best_idx = i;
                }
            }
            config.default_index = best_idx;
        } else {
            // Match by label
            for i in 0..config.entry_count {
                let label = &config.entries[i].label[..config.entries[i].label_len];
                if bytes_eq(label, default_val) {
                    config.default_index = i;
                    break;
                }
            }
        }
    }

    config
}

// ── Byte-level utility functions ──
// — BlackLatch: because str::trim() needs alloc and we're allergic to that here

/// Trim leading and trailing whitespace from a byte slice
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

/// Find the first occurrence of a byte in a slice
fn find_byte(s: &[u8], needle: u8) -> Option<usize> {
    for (i, &b) in s.iter().enumerate() {
        if b == needle {
            return Some(i);
        }
    }
    None
}

/// Compare two byte slices for equality
fn bytes_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    for i in 0..a.len() {
        if a[i] != b[i] {
            return false;
        }
    }
    true
}

/// Check if a byte slice starts with a prefix
fn starts_with(s: &[u8], prefix: &[u8]) -> bool {
    if s.len() < prefix.len() {
        return false;
    }
    bytes_eq(&s[..prefix.len()], prefix)
}

/// Parse a byte slice as an i32 (handles negative for timeout = -1)
fn parse_i32(s: &[u8]) -> i32 {
    let s = trim_bytes(s);
    if s.is_empty() {
        return 0;
    }

    let (negative, start) = if s[0] == b'-' {
        (true, 1)
    } else {
        (false, 0)
    };

    let mut val = 0i32;
    for &b in &s[start..] {
        if b >= b'0' && b <= b'9' {
            val = val.saturating_mul(10).saturating_add((b - b'0') as i32);
        } else {
            break;
        }
    }

    if negative { -val } else { val }
}
