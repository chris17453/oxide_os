//! Kernel Discovery
//!
//! Two modes of operation:
//! - **Config-driven**: Parse boot.cfg entries, validate paths exist on disk
//! - **Auto-scan**: Enumerate `\EFI\OXIDE\` for kernel*.elf files when no config present
//!
//! — SableWire: probing the firmware's filesystem for signs of intelligent life

use crate::efi::{self, EfiGuid, EfiHandle};
use crate::efi::fs::*;
use crate::efi::guid::*;

use crate::config::{BootConfig, BootEntry, MAX_ENTRIES, MAX_LABEL, MAX_OPTIONS, MAX_PATH};

/// Default initramfs path
const DEFAULT_INITRAMFS: &[u8] = b"\\EFI\\OXIDE\\initramfs.cpio";

/// Maximum directory entries we'll return
const MAX_DIR_ENTRIES: usize = 32;

/// Open the root volume of the ESP
/// — SableWire: the FAT32 root — gateway to everything on the ESP
fn open_root_volume() -> Option<*mut EfiFileProtocol> {
    let fs_handle = efi::locate_handle_for_protocol(&EFI_SIMPLE_FILE_SYSTEM_PROTOCOL_GUID)?;
    let fs: *mut EfiSimpleFileSystemProtocol = efi::handle_protocol(
        fs_handle,
        &EFI_SIMPLE_FILE_SYSTEM_PROTOCOL_GUID,
    )?;

    let mut root: *mut EfiFileProtocol = core::ptr::null_mut();
    let status = unsafe { ((*fs).open_volume)(fs, &mut root) };
    if efi::efi_error(status) || root.is_null() {
        None
    } else {
        Some(root)
    }
}

/// Open a file by path (ASCII bytes → UCS-2 conversion)
/// — SableWire: the bridge between ASCII paths and UEFI's UTF-16 obsession
fn open_file(root: *mut EfiFileProtocol, path: &[u8], path_len: usize) -> Option<*mut EfiFileProtocol> {
    let mut path_u16 = [0u16; MAX_PATH + 1];
    efi::ascii_to_ucs2(&path[..path_len], &mut path_u16);

    let mut file: *mut EfiFileProtocol = core::ptr::null_mut();
    let status = unsafe {
        ((*root).open)(root, &mut file, path_u16.as_ptr(), EFI_FILE_MODE_READ, 0)
    };
    if efi::efi_error(status) || file.is_null() {
        None
    } else {
        Some(file)
    }
}

/// Get file size via GetInfo
fn get_file_size(file: *mut EfiFileProtocol) -> Option<u64> {
    let mut info_buf = [0u8; 512];
    let mut info_size = info_buf.len();
    let status = unsafe {
        ((*file).get_info)(
            file,
            &EFI_FILE_INFO_ID as *const EfiGuid,
            &mut info_size,
            info_buf.as_mut_ptr(),
        )
    };
    if efi::efi_error(status) {
        return None;
    }

    // EfiFileInfo.file_size is at offset 8
    let file_info = unsafe { &*(info_buf.as_ptr() as *const EfiFileInfo) };
    Some(file_info.file_size)
}

/// Try to load and parse boot.cfg from the ESP.
/// Returns Some(BootConfig) if the file exists and parses, None otherwise.
/// — SableWire: reading the sacred scrolls from the FAT32 temple
pub fn load_config_file() -> Option<BootConfig> {
    let root = open_root_volume()?;
    let cfg_path = b"\\EFI\\OXIDE\\boot.cfg";
    let file = open_file(root, cfg_path, cfg_path.len())?;

    let file_size = get_file_size(file)? as usize;
    if file_size == 0 || file_size > 8192 {
        unsafe { ((*file).close)(file) };
        return None;
    }

    // Read into scratch arena
    let mark = efi::scratch::save_mark();
    let data = efi::scratch::alloc_slice(file_size)?;

    let mut read_size = file_size;
    let status = unsafe { ((*file).read)(file, &mut read_size, data.as_mut_ptr()) };
    unsafe { ((*file).close)(file) };

    if efi::efi_error(status) {
        efi::scratch::restore_mark(mark);
        return None;
    }

    let config = crate::config::parse_config(&data[..read_size]);
    efi::scratch::restore_mark(mark);
    Some(config)
}

/// Auto-scan `\EFI\OXIDE\` for kernel ELF files.
/// Populates a BootConfig with discovered entries sorted by version (newest first).
/// — SableWire: when there's no config, we go dumpster-diving in the ESP
pub fn auto_scan_kernels() -> BootConfig {
    let mut config = BootConfig::empty();
    config.from_config_file = false;
    config.timeout_secs = 5; // — SableWire: default patience level

    let root = match open_root_volume() {
        Some(r) => r,
        None => return config,
    };

    // Open the OXIDE directory
    let dir_path = b"\\EFI\\OXIDE";
    let dir = match open_file(root, dir_path, dir_path.len()) {
        Some(d) => d,
        None => return config,
    };

    // Enumerate directory entries looking for kernel*.elf
    // — SableWire: FAT32 directory iteration — pray the firmware got this right
    let mut buf = [0u8; 512];
    loop {
        let mut buf_size = buf.len();
        let status = unsafe {
            ((*dir).read)(dir, &mut buf_size, buf.as_mut_ptr())
        };
        if efi::efi_error(status) || buf_size == 0 {
            break;
        }

        // Parse the EfiFileInfo from the buffer
        let file_info = unsafe { &*(buf.as_ptr() as *const EfiFileInfo) };

        // Get filename from after the fixed EfiFileInfo structure
        // The filename starts at offset sizeof(EfiFileInfo)
        let name_offset = core::mem::size_of::<EfiFileInfo>();
        let name_ptr = unsafe { buf.as_ptr().add(name_offset) as *const u16 };

        let mut name_buf = [0u8; 128];
        let mut name_len = 0;
        for i in 0..64 {
            let ch = unsafe { *name_ptr.add(i) };
            if ch == 0 {
                break;
            }
            if ch < 128 && name_len < 127 {
                name_buf[name_len] = ch as u8;
                name_len += 1;
            }
        }

        if name_len == 0 {
            continue;
        }

        let filename = &name_buf[..name_len];

        // Check if it matches kernel*.elf pattern
        if !is_kernel_file(filename) {
            continue;
        }

        if config.entry_count >= MAX_ENTRIES {
            break; // — SableWire: eight kernels ought to be enough for anybody
        }

        let idx = config.entry_count;
        let entry = &mut config.entries[idx];

        // Build full path: \EFI\OXIDE\<filename>
        let prefix = b"\\EFI\\OXIDE\\";
        let path_len = prefix.len() + name_len;
        if path_len < MAX_PATH {
            entry.path[..prefix.len()].copy_from_slice(prefix);
            entry.path[prefix.len()..prefix.len() + name_len].copy_from_slice(filename);
            entry.path_len = path_len;
        }

        // Parse version from filename
        let (major, minor, patch, has_version) = parse_kernel_filename(filename);
        entry.version_major = major;
        entry.version_minor = minor;
        entry.version_patch = patch;

        // Generate label
        if has_version {
            let label = format_version_label(major, minor, patch);
            let len = label.len().min(MAX_LABEL - 1);
            entry.label[..len].copy_from_slice(&label[..len]);
            entry.label_len = len;
        } else {
            let label = b"OXIDE (current)";
            let len = label.len().min(MAX_LABEL - 1);
            entry.label[..len].copy_from_slice(&label[..len]);
            entry.label_len = len;
        }

        // Default initramfs path
        let ifr_len = DEFAULT_INITRAMFS.len().min(MAX_PATH - 1);
        entry.initramfs_path[..ifr_len].copy_from_slice(&DEFAULT_INITRAMFS[..ifr_len]);
        entry.initramfs_path_len = ifr_len;

        entry.valid = true;
        config.entry_count += 1;
    }

    unsafe { ((*dir).close)(dir) };

    // Sort entries by version descending (simple insertion sort — max 8 entries)
    // — SableWire: O(n²) on 8 elements is O(64) which is O(who cares)
    for i in 1..config.entry_count {
        let mut j = i;
        while j > 0
            && config.entries[j].version_cmp(&config.entries[j - 1])
                == core::cmp::Ordering::Less
        {
            // Swap entries
            let a = config.entries[j].clone();
            let b = config.entries[j - 1].clone();
            config.entries[j] = b;
            config.entries[j - 1] = a;
            j -= 1;
        }
    }

    config
}

/// Validate that paths in a config-file-derived BootConfig actually exist on disk.
/// Marks entries with valid=true/false accordingly.
/// — SableWire: trust but verify — config can lie, filesystem can't (well, usually)
pub fn validate_config_entries(config: &mut BootConfig) {
    let root = match open_root_volume() {
        Some(r) => r,
        None => return,
    };

    for i in 0..config.entry_count {
        let entry = &config.entries[i];
        if entry.path_len == 0 {
            config.entries[i].valid = false;
            continue;
        }

        match open_file(root, &entry.path, entry.path_len) {
            Some(file) => {
                unsafe { ((*file).close)(file) };
                config.entries[i].valid = true;
            }
            None => {
                config.entries[i].valid = false;
            }
        }
    }
}

/// Load a file from the ESP into the scratch arena.
/// Returns a slice referencing the file data in scratch memory.
/// For small files only (config, etc.) — kernel images use load_large_file_from_esp.
/// — SableWire: generic file loader for the pre-boot environment
pub fn load_file_from_esp(path_bytes: &[u8], path_len: usize) -> Option<&'static [u8]> {
    let root = open_root_volume()?;
    let file = open_file(root, path_bytes, path_len)?;

    let file_size = get_file_size(file)? as usize;
    if file_size == 0 {
        unsafe { ((*file).close)(file) };
        return None;
    }

    // Allocate from scratch arena
    let data = efi::scratch::alloc_slice(file_size)?;

    let mut read_size = file_size;
    let status = unsafe { ((*file).read)(file, &mut read_size, data.as_mut_ptr()) };
    unsafe { ((*file).close)(file) };

    if efi::efi_error(status) {
        return None;
    }

    // Return as immutable slice
    Some(&data[..read_size])
}

/// Load a large file from the ESP using UEFI page allocation (not scratch arena).
/// Used for kernel images that can exceed the 2MB scratch arena.
/// Returns a slice backed by LOADER_DATA pages — caller must not free.
/// — SableWire: for when 2MB of scratch just doesn't cut it (33MB debug kernels, anyone?)
pub fn load_large_file_from_esp(path_bytes: &[u8], path_len: usize) -> Option<&'static [u8]> {
    let root = open_root_volume()?;
    let file = open_file(root, path_bytes, path_len)?;

    let file_size = get_file_size(file)? as usize;
    if file_size == 0 {
        unsafe { ((*file).close)(file) };
        return None;
    }

    // Allocate pages directly — no 2MB limit
    let page_count = (file_size + 4095) / 4096;
    let phys_addr = efi::allocate_pages(page_count)?;
    let data = unsafe { core::slice::from_raw_parts_mut(phys_addr as *mut u8, page_count * 4096) };

    let mut read_size = file_size;
    let status = unsafe { ((*file).read)(file, &mut read_size, data.as_mut_ptr()) };
    unsafe { ((*file).close)(file) };

    if efi::efi_error(status) {
        return None;
    }

    Some(unsafe { core::slice::from_raw_parts(phys_addr as *const u8, read_size) })
}

/// List files in a directory on the ESP. Returns fixed-size array + count.
/// Used by the diagnostic console's `ls` command.
/// — SableWire: filesystem archaeology for the desperate
pub fn list_directory(dir_path: &str) -> ([([u8; 128], usize, u64, bool); MAX_DIR_ENTRIES], usize) {
    let mut results = [([0u8; 128], 0usize, 0u64, false); MAX_DIR_ENTRIES];
    let mut count = 0;

    let root = match open_root_volume() {
        Some(r) => r,
        None => return (results, 0),
    };

    let path_bytes = dir_path.as_bytes();
    let dir = match open_file(root, path_bytes, path_bytes.len()) {
        Some(d) => d,
        None => return (results, 0),
    };

    let mut buf = [0u8; 512];
    loop {
        if count >= MAX_DIR_ENTRIES {
            break;
        }

        let mut buf_size = buf.len();
        let status = unsafe {
            ((*dir).read)(dir, &mut buf_size, buf.as_mut_ptr())
        };
        if efi::efi_error(status) || buf_size == 0 {
            break;
        }

        let file_info = unsafe { &*(buf.as_ptr() as *const EfiFileInfo) };

        // Get filename
        let name_offset = core::mem::size_of::<EfiFileInfo>();
        let name_ptr = unsafe { buf.as_ptr().add(name_offset) as *const u16 };

        let mut name_buf = [0u8; 128];
        let mut name_len = 0;
        for i in 0..64 {
            let ch = unsafe { *name_ptr.add(i) };
            if ch == 0 { break; }
            if ch < 128 && name_len < 127 {
                name_buf[name_len] = ch as u8;
                name_len += 1;
            }
        }

        if name_len == 0 { continue; }

        let is_dir = (file_info.attribute & EFI_FILE_DIRECTORY) != 0;
        results[count] = (name_buf, name_len, file_info.file_size, is_dir);
        count += 1;
    }

    unsafe { ((*dir).close)(dir) };
    (results, count)
}

// ── Internal helpers ──

/// Check if a filename matches the kernel*.elf pattern
/// — SableWire: regex would be overkill, pattern matching is just right
fn is_kernel_file(name: &[u8]) -> bool {
    // Must start with "kernel" (case-insensitive for FAT32)
    if name.len() < 10 {
        // "kernel.elf" = 10 chars minimum
        return false;
    }

    let lower_name: [u8; 6] = [
        to_lower(name[0]),
        to_lower(name[1]),
        to_lower(name[2]),
        to_lower(name[3]),
        to_lower(name[4]),
        to_lower(name[5]),
    ];

    if &lower_name != b"kernel" {
        return false;
    }

    // Must end with ".elf"
    if name.len() < 4 {
        return false;
    }
    let suffix = &name[name.len() - 4..];
    to_lower(suffix[0]) == b'.' && to_lower(suffix[1]) == b'e' && to_lower(suffix[2]) == b'l' && to_lower(suffix[3]) == b'f'
}

/// Parse version from kernel filename.
/// "kernel.elf" → (0, 0, 0, false)
/// "kernel-0.2.0.elf" → (0, 2, 0, true)
fn parse_kernel_filename(name: &[u8]) -> (u16, u16, u16, bool) {
    // Find the dash after "kernel"
    if name.len() <= 10 || name[6] != b'-' {
        return (0, 0, 0, false);
    }

    // Version string is between dash and ".elf"
    let ver_start = 7;
    let ver_end = name.len() - 4; // strip ".elf"
    if ver_start >= ver_end {
        return (0, 0, 0, false);
    }

    let ver_str = &name[ver_start..ver_end];
    let mut parts = [0u16; 3];
    let mut part_idx = 0;
    let mut current = 0u16;

    for &b in ver_str {
        if b == b'.' {
            if part_idx < 3 {
                parts[part_idx] = current;
                part_idx += 1;
                current = 0;
            }
        } else if b >= b'0' && b <= b'9' {
            current = current.saturating_mul(10).saturating_add((b - b'0') as u16);
        } else {
            break; // non-numeric suffix (e.g., "-debug")
        }
    }
    if part_idx < 3 {
        parts[part_idx] = current;
    }

    (parts[0], parts[1], parts[2], true)
}

/// Generate a version label like "OXIDE 0.2.0"
fn format_version_label(major: u16, minor: u16, patch: u16) -> [u8; MAX_LABEL] {
    let mut buf = [0u8; MAX_LABEL];
    let prefix = b"OXIDE ";
    buf[..prefix.len()].copy_from_slice(prefix);
    let mut pos = prefix.len();

    pos += write_u16(&mut buf[pos..], major);
    buf[pos] = b'.';
    pos += 1;
    pos += write_u16(&mut buf[pos..], minor);
    buf[pos] = b'.';
    pos += 1;
    pos += write_u16(&mut buf[pos..], patch);

    buf
}

/// Write a u16 as decimal into a buffer, return bytes written
fn write_u16(buf: &mut [u8], mut val: u16) -> usize {
    if val == 0 {
        if !buf.is_empty() {
            buf[0] = b'0';
        }
        return 1;
    }
    let mut tmp = [0u8; 5];
    let mut len = 0;
    while val > 0 {
        tmp[len] = b'0' + (val % 10) as u8;
        val /= 10;
        len += 1;
    }
    for i in 0..len.min(buf.len()) {
        buf[i] = tmp[len - 1 - i];
    }
    len.min(buf.len())
}

fn to_lower(b: u8) -> u8 {
    if b >= b'A' && b <= b'Z' {
        b + 32
    } else {
        b
    }
}
