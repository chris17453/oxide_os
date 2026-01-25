//! mkfs.ext4 - Create an ext4 filesystem
//!
//! Usage: mkfs.ext4 [-b block_size] [-L label] device
//!
//! Creates an ext4 filesystem on the specified device (file or block device).

#![no_std]
#![no_main]
#![allow(unused)]

use libc::*;

// ext4 constants
const EXT4_MAGIC: u16 = 0xEF53;
const EXT4_BLOCK_SIZE_1K: u32 = 0; // 1024 << 0 = 1024
const EXT4_BLOCK_SIZE_4K: u32 = 2; // 1024 << 2 = 4096

// Feature flags
const COMPAT_HAS_JOURNAL: u32 = 0x0004;
const COMPAT_EXT_ATTR: u32 = 0x0008;
const COMPAT_DIR_INDEX: u32 = 0x0020;

const INCOMPAT_FILETYPE: u32 = 0x0002;
const INCOMPAT_EXTENTS: u32 = 0x0040;
const INCOMPAT_64BIT: u32 = 0x0080;
const INCOMPAT_FLEX_BG: u32 = 0x0200;

const RO_COMPAT_SPARSE_SUPER: u32 = 0x0001;
const RO_COMPAT_LARGE_FILE: u32 = 0x0002;
const RO_COMPAT_HUGE_FILE: u32 = 0x0008;
const RO_COMPAT_GDT_CSUM: u32 = 0x0010;
const RO_COMPAT_DIR_NLINK: u32 = 0x0020;
const RO_COMPAT_EXTRA_ISIZE: u32 = 0x0040;

// Inode flags
const INODE_EXTENTS: u32 = 0x00080000;

// File types
const S_IFDIR: u16 = 0o040000;
const S_IFREG: u16 = 0o100000;

// Directory entry file types
const FT_DIR: u8 = 2;

// Extent magic
const EXT4_EXT_MAGIC: u16 = 0xF30A;

// Well-known inodes
const EXT4_ROOT_INO: u32 = 2;
const EXT4_FIRST_INO: u32 = 11;
const EXT4_LOST_FOUND_INO: u32 = 11;

#[repr(C)]
#[derive(Clone, Copy)]
struct Ext4Superblock {
    s_inodes_count: u32,
    s_blocks_count_lo: u32,
    s_r_blocks_count_lo: u32,
    s_free_blocks_count_lo: u32,
    s_free_inodes_count: u32,
    s_first_data_block: u32,
    s_log_block_size: u32,
    s_log_cluster_size: u32,
    s_blocks_per_group: u32,
    s_clusters_per_group: u32,
    s_inodes_per_group: u32,
    s_mtime: u32,
    s_wtime: u32,
    s_mnt_count: u16,
    s_max_mnt_count: u16,
    s_magic: u16,
    s_state: u16,
    s_errors: u16,
    s_minor_rev_level: u16,
    s_lastcheck: u32,
    s_checkinterval: u32,
    s_creator_os: u32,
    s_rev_level: u32,
    s_def_resuid: u16,
    s_def_resgid: u16,
    s_first_ino: u32,
    s_inode_size: u16,
    s_block_group_nr: u16,
    s_feature_compat: u32,
    s_feature_incompat: u32,
    s_feature_ro_compat: u32,
    s_uuid: [u8; 16],
    s_volume_name: [u8; 16],
    s_last_mounted: [u8; 64],
    s_algorithm_usage_bitmap: u32,
    s_prealloc_blocks: u8,
    s_prealloc_dir_blocks: u8,
    s_reserved_gdt_blocks: u16,
    s_journal_uuid: [u8; 16],
    s_journal_inum: u32,
    s_journal_dev: u32,
    s_last_orphan: u32,
    s_hash_seed: [u32; 4],
    s_def_hash_version: u8,
    s_jnl_backup_type: u8,
    s_desc_size: u16,
    s_default_mount_opts: u32,
    s_first_meta_bg: u32,
    s_mkfs_time: u32,
    s_jnl_blocks: [u32; 17],
    s_blocks_count_hi: u32,
    s_r_blocks_count_hi: u32,
    s_free_blocks_count_hi: u32,
    s_min_extra_isize: u16,
    s_want_extra_isize: u16,
    s_flags: u32,
    s_raid_stride: u16,
    s_mmp_interval: u16,
    s_mmp_block: u64,
    s_raid_stripe_width: u32,
    s_log_groups_per_flex: u8,
    s_checksum_type: u8,
    s_reserved_pad: u16,
    s_kbytes_written: u64,
    // Pad to 1024 bytes
    s_reserved: [u32; 98],
}

#[repr(C)]
#[derive(Clone, Copy)]
struct Ext4GroupDesc {
    bg_block_bitmap_lo: u32,
    bg_inode_bitmap_lo: u32,
    bg_inode_table_lo: u32,
    bg_free_blocks_count_lo: u16,
    bg_free_inodes_count_lo: u16,
    bg_used_dirs_count_lo: u16,
    bg_flags: u16,
    bg_exclude_bitmap_lo: u32,
    bg_block_bitmap_csum_lo: u16,
    bg_inode_bitmap_csum_lo: u16,
    bg_itable_unused_lo: u16,
    bg_checksum: u16,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct Ext4Inode {
    i_mode: u16,
    i_uid: u16,
    i_size_lo: u32,
    i_atime: u32,
    i_ctime: u32,
    i_mtime: u32,
    i_dtime: u32,
    i_gid: u16,
    i_links_count: u16,
    i_blocks_lo: u32,
    i_flags: u32,
    i_osd1: u32,
    i_block: [u32; 15],
    i_generation: u32,
    i_file_acl_lo: u32,
    i_size_hi: u32,
    i_obso_faddr: u32,
    i_osd2: [u8; 12],
    i_extra_isize: u16,
    i_checksum_hi: u16,
    i_ctime_extra: u32,
    i_mtime_extra: u32,
    i_atime_extra: u32,
    i_crtime: u32,
    i_crtime_extra: u32,
    i_version_hi: u32,
    i_projid: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct ExtentHeader {
    eh_magic: u16,
    eh_entries: u16,
    eh_max: u16,
    eh_depth: u16,
    eh_generation: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct Extent {
    ee_block: u32,
    ee_len: u16,
    ee_start_hi: u16,
    ee_start_lo: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct Ext4DirEntry {
    inode: u32,
    rec_len: u16,
    name_len: u8,
    file_type: u8,
    // name follows
}

struct MkfsConfig {
    block_size: u32,
    device_size: u64,
    label: [u8; 16],
}

#[unsafe(no_mangle)]
fn main(argc: i32, argv: *const *const u8) -> i32 {
    let mut block_size: u32 = 4096;
    let mut label: [u8; 16] = [0; 16];
    let mut device_path: Option<&str> = None;

    // Parse arguments
    let mut i = 1;
    while i < argc {
        let arg = unsafe { cstr_to_str(*argv.add(i as usize)) };

        if arg == "-b" {
            i += 1;
            if i >= argc {
                eprintlns("mkfs.ext4: -b requires an argument");
                return 1;
            }
            let bs_str = unsafe { cstr_to_str(*argv.add(i as usize)) };
            block_size = match parse_int(bs_str.as_bytes()) {
                Some(v) => v as u32,
                None => {
                    eprintlns("mkfs.ext4: invalid block size");
                    return 1;
                }
            };
            if block_size != 1024 && block_size != 2048 && block_size != 4096 {
                eprintlns("mkfs.ext4: block size must be 1024, 2048, or 4096");
                return 1;
            }
        } else if arg == "-L" {
            i += 1;
            if i >= argc {
                eprintlns("mkfs.ext4: -L requires an argument");
                return 1;
            }
            let label_str = unsafe { cstr_to_str(*argv.add(i as usize)) };
            let label_bytes = label_str.as_bytes();
            let copy_len = if label_bytes.len() > 16 { 16 } else { label_bytes.len() };
            label[..copy_len].copy_from_slice(&label_bytes[..copy_len]);
        } else if arg == "-h" || arg == "--help" {
            printlns("Usage: mkfs.ext4 [-b block_size] [-L label] device");
            printlns("  -b block_size  Set block size (1024, 2048, or 4096)");
            printlns("  -L label       Set volume label");
            return 0;
        } else if !arg.starts_with('-') {
            device_path = Some(arg);
        } else {
            eprints("mkfs.ext4: unknown option: ");
            eprintlns(arg);
            return 1;
        }
        i += 1;
    }

    let device = match device_path {
        Some(d) => d,
        None => {
            eprintlns("mkfs.ext4: no device specified");
            printlns("Usage: mkfs.ext4 [-b block_size] [-L label] device");
            return 1;
        }
    };

    // Open device
    let fd = open2(device, O_RDWR);
    if fd < 0 {
        eprints("mkfs.ext4: cannot open '");
        eprints(device);
        eprintlns("'");
        return 1;
    }

    // Get device size by seeking to end
    let size = lseek(fd, 0, SEEK_END);
    if size < 0 {
        eprintlns("mkfs.ext4: cannot determine device size");
        close(fd);
        return 1;
    }
    lseek(fd, 0, SEEK_SET);

    let device_size = size as u64;

    // Minimum size: 64KB
    if device_size < 65536 {
        eprintlns("mkfs.ext4: device too small (minimum 64KB)");
        close(fd);
        return 1;
    }

    eprints("Creating ext4 filesystem on ");
    eprints(device);
    prints(" (");
    print_u64(device_size / 1024);
    printlns(" KB)");

    let config = MkfsConfig {
        block_size,
        device_size,
        label,
    };

    if let Err(e) = create_filesystem(fd, &config) {
        eprints("mkfs.ext4: ");
        eprintlns(e);
        close(fd);
        return 1;
    }

    close(fd);
    printlns("Filesystem created successfully");
    0
}

fn create_filesystem(fd: i32, config: &MkfsConfig) -> Result<(), &'static str> {
    let block_size = config.block_size as u64;
    let total_blocks = config.device_size / block_size;

    if total_blocks < 64 {
        return Err("device too small");
    }

    // Calculate filesystem parameters
    let blocks_per_group = 8 * block_size as u32; // 8 blocks worth of bits
    let inodes_per_group = blocks_per_group / 4; // Rough estimate
    let num_groups = ((total_blocks as u32) + blocks_per_group - 1) / blocks_per_group;

    // For simplicity, use single group for small filesystems
    let num_groups = if num_groups < 1 { 1 } else { num_groups };
    let actual_blocks = (num_groups * blocks_per_group) as u64;
    let actual_blocks = if actual_blocks > total_blocks {
        total_blocks
    } else {
        actual_blocks
    };

    let total_inodes = num_groups * inodes_per_group;
    let inode_size: u16 = 256;

    // Calculate metadata block positions (for first/only group)
    let sb_block = if block_size == 1024 { 1 } else { 0 };
    let gdt_block = sb_block + 1;
    let block_bitmap_block = gdt_block + 1;
    let inode_bitmap_block = block_bitmap_block + 1;
    let inode_table_block = inode_bitmap_block + 1;

    // Inode table size (in blocks)
    let inodes_per_block = block_size / inode_size as u64;
    let inode_table_blocks = ((inodes_per_group as u64) + inodes_per_block - 1) / inodes_per_block;

    let first_data_block = inode_table_block + inode_table_blocks as u32;

    // Calculate free blocks (total - metadata)
    let metadata_blocks = first_data_block as u64;
    let free_blocks = if actual_blocks > metadata_blocks {
        actual_blocks - metadata_blocks
    } else {
        0
    };

    // Free inodes (total - reserved - root - lost+found)
    let reserved_inodes = 10;
    let free_inodes = if total_inodes > reserved_inodes + 2 {
        total_inodes - reserved_inodes - 2
    } else {
        0
    };

    prints("  Block size: ");
    print_u64(block_size);
    printlns("");
    prints("  Total blocks: ");
    print_u64(actual_blocks);
    printlns("");
    prints("  Free blocks: ");
    print_u64(free_blocks);
    printlns("");
    prints("  Total inodes: ");
    print_u64(total_inodes as u64);
    printlns("");

    // Create and write superblock
    let mut sb = create_superblock(config, actual_blocks as u32, total_inodes, free_blocks as u32, free_inodes);
    sb.s_first_data_block = if block_size == 1024 { 1 } else { 0 };
    sb.s_blocks_per_group = blocks_per_group;
    sb.s_inodes_per_group = inodes_per_group;

    write_superblock(fd, &sb, block_size)?;

    // Create and write block group descriptor
    let gd = Ext4GroupDesc {
        bg_block_bitmap_lo: block_bitmap_block,
        bg_inode_bitmap_lo: inode_bitmap_block,
        bg_inode_table_lo: inode_table_block,
        bg_free_blocks_count_lo: free_blocks as u16,
        bg_free_inodes_count_lo: free_inodes as u16,
        bg_used_dirs_count_lo: 2, // root + lost+found
        bg_flags: 0,
        bg_exclude_bitmap_lo: 0,
        bg_block_bitmap_csum_lo: 0,
        bg_inode_bitmap_csum_lo: 0,
        bg_itable_unused_lo: free_inodes as u16,
        bg_checksum: 0,
    };

    write_group_desc(fd, &gd, gdt_block as u64, block_size)?;

    // Initialize block bitmap
    write_block_bitmap(fd, block_bitmap_block as u64, block_size, first_data_block as usize)?;

    // Initialize inode bitmap
    write_inode_bitmap(fd, inode_bitmap_block as u64, block_size, reserved_inodes as usize + 2)?;

    // Initialize inode table (zero it out)
    zero_blocks(fd, inode_table_block as u64, inode_table_blocks as u64, block_size)?;

    // Create root directory inode (inode 2)
    let root_data_block = first_data_block;
    create_root_inode(fd, inode_table_block as u64, block_size, inode_size as u64, root_data_block as u64)?;

    // Create root directory data block
    write_root_directory(fd, root_data_block as u64, block_size, EXT4_LOST_FOUND_INO)?;

    // Create lost+found directory inode (inode 11)
    let lf_data_block = first_data_block + 1;
    create_lost_found_inode(fd, inode_table_block as u64, block_size, inode_size as u64, lf_data_block as u64)?;

    // Create lost+found directory data block
    write_lost_found_directory(fd, lf_data_block as u64, block_size)?;

    Ok(())
}

fn create_superblock(config: &MkfsConfig, total_blocks: u32, total_inodes: u32, free_blocks: u32, free_inodes: u32) -> Ext4Superblock {
    let log_block_size = match config.block_size {
        1024 => 0,
        2048 => 1,
        4096 => 2,
        _ => 2,
    };

    Ext4Superblock {
        s_inodes_count: total_inodes,
        s_blocks_count_lo: total_blocks,
        s_r_blocks_count_lo: total_blocks / 20, // 5% reserved
        s_free_blocks_count_lo: free_blocks,
        s_free_inodes_count: free_inodes,
        s_first_data_block: if config.block_size == 1024 { 1 } else { 0 },
        s_log_block_size: log_block_size,
        s_log_cluster_size: log_block_size,
        s_blocks_per_group: 8 * config.block_size,
        s_clusters_per_group: 8 * config.block_size,
        s_inodes_per_group: total_inodes, // Single group
        s_mtime: 0,
        s_wtime: 0,
        s_mnt_count: 0,
        s_max_mnt_count: 0xFFFF,
        s_magic: EXT4_MAGIC,
        s_state: 1, // Clean
        s_errors: 1, // Continue on error
        s_minor_rev_level: 0,
        s_lastcheck: 0,
        s_checkinterval: 0,
        s_creator_os: 0, // Linux
        s_rev_level: 1, // Dynamic
        s_def_resuid: 0,
        s_def_resgid: 0,
        s_first_ino: EXT4_FIRST_INO,
        s_inode_size: 256,
        s_block_group_nr: 0,
        s_feature_compat: COMPAT_EXT_ATTR | COMPAT_DIR_INDEX,
        s_feature_incompat: INCOMPAT_FILETYPE | INCOMPAT_EXTENTS,
        s_feature_ro_compat: RO_COMPAT_SPARSE_SUPER | RO_COMPAT_LARGE_FILE | RO_COMPAT_HUGE_FILE | RO_COMPAT_EXTRA_ISIZE,
        s_uuid: generate_uuid(),
        s_volume_name: config.label,
        s_last_mounted: [0; 64],
        s_algorithm_usage_bitmap: 0,
        s_prealloc_blocks: 0,
        s_prealloc_dir_blocks: 0,
        s_reserved_gdt_blocks: 0,
        s_journal_uuid: [0; 16],
        s_journal_inum: 0,
        s_journal_dev: 0,
        s_last_orphan: 0,
        s_hash_seed: [0; 4],
        s_def_hash_version: 1, // Half MD4
        s_jnl_backup_type: 0,
        s_desc_size: 32,
        s_default_mount_opts: 0,
        s_first_meta_bg: 0,
        s_mkfs_time: 0,
        s_jnl_blocks: [0; 17],
        s_blocks_count_hi: 0,
        s_r_blocks_count_hi: 0,
        s_free_blocks_count_hi: 0,
        s_min_extra_isize: 32,
        s_want_extra_isize: 32,
        s_flags: 0,
        s_raid_stride: 0,
        s_mmp_interval: 0,
        s_mmp_block: 0,
        s_raid_stripe_width: 0,
        s_log_groups_per_flex: 0,
        s_checksum_type: 0,
        s_reserved_pad: 0,
        s_kbytes_written: 0,
        s_reserved: [0; 98],
    }
}

fn generate_uuid() -> [u8; 16] {
    // Simple pseudo-random UUID (not cryptographically secure)
    // In a real implementation, use /dev/urandom
    let mut uuid = [0u8; 16];
    let seed = 0x12345678u32; // Would use time/random
    for i in 0..16 {
        uuid[i] = ((seed.wrapping_mul(i as u32 + 1)) & 0xFF) as u8;
    }
    // Set version (4) and variant bits
    uuid[6] = (uuid[6] & 0x0F) | 0x40;
    uuid[8] = (uuid[8] & 0x3F) | 0x80;
    uuid
}

fn write_superblock(fd: i32, sb: &Ext4Superblock, block_size: u64) -> Result<(), &'static str> {
    // Superblock is always at byte offset 1024
    lseek(fd, 1024, SEEK_SET);

    let sb_bytes = unsafe {
        core::slice::from_raw_parts(sb as *const Ext4Superblock as *const u8, 1024)
    };

    if write(fd, sb_bytes) != 1024 {
        return Err("failed to write superblock");
    }

    Ok(())
}

fn write_group_desc(fd: i32, gd: &Ext4GroupDesc, block: u64, block_size: u64) -> Result<(), &'static str> {
    lseek(fd, (block * block_size) as i64, SEEK_SET);

    let gd_bytes = unsafe {
        core::slice::from_raw_parts(gd as *const Ext4GroupDesc as *const u8, 32)
    };

    // Write group descriptor, pad rest of block with zeros
    let mut buf = [0u8; 4096];
    buf[..32].copy_from_slice(gd_bytes);

    if write(fd, &buf[..block_size as usize]) != block_size as isize {
        return Err("failed to write group descriptor");
    }

    Ok(())
}

fn write_block_bitmap(fd: i32, block: u64, block_size: u64, used_blocks: usize) -> Result<(), &'static str> {
    lseek(fd, (block * block_size) as i64, SEEK_SET);

    let mut buf = [0u8; 4096];

    // Mark used blocks (metadata blocks)
    for i in 0..used_blocks {
        let byte_idx = i / 8;
        let bit_idx = i % 8;
        buf[byte_idx] |= 1 << bit_idx;
    }

    if write(fd, &buf[..block_size as usize]) != block_size as isize {
        return Err("failed to write block bitmap");
    }

    Ok(())
}

fn write_inode_bitmap(fd: i32, block: u64, block_size: u64, used_inodes: usize) -> Result<(), &'static str> {
    lseek(fd, (block * block_size) as i64, SEEK_SET);

    let mut buf = [0u8; 4096];

    // Mark used inodes (reserved + root + lost+found)
    for i in 0..used_inodes {
        let byte_idx = i / 8;
        let bit_idx = i % 8;
        buf[byte_idx] |= 1 << bit_idx;
    }

    if write(fd, &buf[..block_size as usize]) != block_size as isize {
        return Err("failed to write inode bitmap");
    }

    Ok(())
}

fn zero_blocks(fd: i32, start_block: u64, count: u64, block_size: u64) -> Result<(), &'static str> {
    let buf = [0u8; 4096];

    for i in 0..count {
        lseek(fd, ((start_block + i) * block_size) as i64, SEEK_SET);
        if write(fd, &buf[..block_size as usize]) != block_size as isize {
            return Err("failed to zero blocks");
        }
    }

    Ok(())
}

fn create_root_inode(fd: i32, inode_table_block: u64, block_size: u64, inode_size: u64, data_block: u64) -> Result<(), &'static str> {
    // Root inode is inode 2 (index 1 in table)
    let inode_offset = (inode_table_block * block_size) + (1 * inode_size);

    let mut inode = Ext4Inode {
        i_mode: S_IFDIR | 0o755,
        i_uid: 0,
        i_size_lo: block_size as u32,
        i_atime: 0,
        i_ctime: 0,
        i_mtime: 0,
        i_dtime: 0,
        i_gid: 0,
        i_links_count: 3, // ., .., lost+found/..
        i_blocks_lo: (block_size / 512) as u32,
        i_flags: INODE_EXTENTS,
        i_osd1: 0,
        i_block: [0; 15],
        i_generation: 0,
        i_file_acl_lo: 0,
        i_size_hi: 0,
        i_obso_faddr: 0,
        i_osd2: [0; 12],
        i_extra_isize: 32,
        i_checksum_hi: 0,
        i_ctime_extra: 0,
        i_mtime_extra: 0,
        i_atime_extra: 0,
        i_crtime: 0,
        i_crtime_extra: 0,
        i_version_hi: 0,
        i_projid: 0,
    };

    // Set up extent header in i_block
    init_extent(&mut inode, data_block);

    lseek(fd, inode_offset as i64, SEEK_SET);

    let inode_bytes = unsafe {
        core::slice::from_raw_parts(&inode as *const Ext4Inode as *const u8, inode_size as usize)
    };

    if write(fd, inode_bytes) != inode_size as isize {
        return Err("failed to write root inode");
    }

    Ok(())
}

fn create_lost_found_inode(fd: i32, inode_table_block: u64, block_size: u64, inode_size: u64, data_block: u64) -> Result<(), &'static str> {
    // lost+found is inode 11 (index 10 in table)
    let inode_offset = (inode_table_block * block_size) + (10 * inode_size);

    let mut inode = Ext4Inode {
        i_mode: S_IFDIR | 0o700,
        i_uid: 0,
        i_size_lo: block_size as u32,
        i_atime: 0,
        i_ctime: 0,
        i_mtime: 0,
        i_dtime: 0,
        i_gid: 0,
        i_links_count: 2, // . and ..
        i_blocks_lo: (block_size / 512) as u32,
        i_flags: INODE_EXTENTS,
        i_osd1: 0,
        i_block: [0; 15],
        i_generation: 0,
        i_file_acl_lo: 0,
        i_size_hi: 0,
        i_obso_faddr: 0,
        i_osd2: [0; 12],
        i_extra_isize: 32,
        i_checksum_hi: 0,
        i_ctime_extra: 0,
        i_mtime_extra: 0,
        i_atime_extra: 0,
        i_crtime: 0,
        i_crtime_extra: 0,
        i_version_hi: 0,
        i_projid: 0,
    };

    init_extent(&mut inode, data_block);

    lseek(fd, inode_offset as i64, SEEK_SET);

    let inode_bytes = unsafe {
        core::slice::from_raw_parts(&inode as *const Ext4Inode as *const u8, inode_size as usize)
    };

    if write(fd, inode_bytes) != inode_size as isize {
        return Err("failed to write lost+found inode");
    }

    Ok(())
}

fn init_extent(inode: &mut Ext4Inode, data_block: u64) {
    // Write extent header
    let header = ExtentHeader {
        eh_magic: EXT4_EXT_MAGIC,
        eh_entries: 1,
        eh_max: 4,
        eh_depth: 0,
        eh_generation: 0,
    };

    let header_bytes = unsafe {
        core::slice::from_raw_parts(&header as *const ExtentHeader as *const u8, 12)
    };

    let i_block_bytes = unsafe {
        core::slice::from_raw_parts_mut(inode.i_block.as_mut_ptr() as *mut u8, 60)
    };

    i_block_bytes[..12].copy_from_slice(header_bytes);

    // Write extent
    let extent = Extent {
        ee_block: 0,
        ee_len: 1,
        ee_start_hi: (data_block >> 32) as u16,
        ee_start_lo: data_block as u32,
    };

    let extent_bytes = unsafe {
        core::slice::from_raw_parts(&extent as *const Extent as *const u8, 12)
    };

    i_block_bytes[12..24].copy_from_slice(extent_bytes);
}

fn write_root_directory(fd: i32, block: u64, block_size: u64, lost_found_ino: u32) -> Result<(), &'static str> {
    lseek(fd, (block * block_size) as i64, SEEK_SET);

    let mut buf = [0u8; 4096];
    let mut offset = 0usize;

    // . entry
    let dot = Ext4DirEntry {
        inode: EXT4_ROOT_INO,
        rec_len: 12,
        name_len: 1,
        file_type: FT_DIR,
    };
    let dot_bytes = unsafe {
        core::slice::from_raw_parts(&dot as *const Ext4DirEntry as *const u8, 8)
    };
    buf[offset..offset + 8].copy_from_slice(dot_bytes);
    buf[offset + 8] = b'.';
    offset += 12;

    // .. entry
    let dotdot = Ext4DirEntry {
        inode: EXT4_ROOT_INO,
        rec_len: 12,
        name_len: 2,
        file_type: FT_DIR,
    };
    let dotdot_bytes = unsafe {
        core::slice::from_raw_parts(&dotdot as *const Ext4DirEntry as *const u8, 8)
    };
    buf[offset..offset + 8].copy_from_slice(dotdot_bytes);
    buf[offset + 8] = b'.';
    buf[offset + 9] = b'.';
    offset += 12;

    // lost+found entry (takes rest of block)
    let lf_rec_len = (block_size as usize) - offset;
    let lf = Ext4DirEntry {
        inode: lost_found_ino,
        rec_len: lf_rec_len as u16,
        name_len: 10,
        file_type: FT_DIR,
    };
    let lf_bytes = unsafe {
        core::slice::from_raw_parts(&lf as *const Ext4DirEntry as *const u8, 8)
    };
    buf[offset..offset + 8].copy_from_slice(lf_bytes);
    buf[offset + 8..offset + 18].copy_from_slice(b"lost+found");

    if write(fd, &buf[..block_size as usize]) != block_size as isize {
        return Err("failed to write root directory");
    }

    Ok(())
}

fn write_lost_found_directory(fd: i32, block: u64, block_size: u64) -> Result<(), &'static str> {
    lseek(fd, (block * block_size) as i64, SEEK_SET);

    let mut buf = [0u8; 4096];
    let mut offset = 0usize;

    // . entry
    let dot = Ext4DirEntry {
        inode: EXT4_LOST_FOUND_INO,
        rec_len: 12,
        name_len: 1,
        file_type: FT_DIR,
    };
    let dot_bytes = unsafe {
        core::slice::from_raw_parts(&dot as *const Ext4DirEntry as *const u8, 8)
    };
    buf[offset..offset + 8].copy_from_slice(dot_bytes);
    buf[offset + 8] = b'.';
    offset += 12;

    // .. entry (takes rest of block)
    let dotdot_rec_len = (block_size as usize) - offset;
    let dotdot = Ext4DirEntry {
        inode: EXT4_ROOT_INO,
        rec_len: dotdot_rec_len as u16,
        name_len: 2,
        file_type: FT_DIR,
    };
    let dotdot_bytes = unsafe {
        core::slice::from_raw_parts(&dotdot as *const Ext4DirEntry as *const u8, 8)
    };
    buf[offset..offset + 8].copy_from_slice(dotdot_bytes);
    buf[offset + 8] = b'.';
    buf[offset + 9] = b'.';

    if write(fd, &buf[..block_size as usize]) != block_size as isize {
        return Err("failed to write lost+found directory");
    }

    Ok(())
}

// Helper functions

fn parse_int(s: &[u8]) -> Option<i64> {
    if s.is_empty() {
        return None;
    }
    let mut result: i64 = 0;
    for &c in s {
        if c < b'0' || c > b'9' {
            return None;
        }
        result = result * 10 + (c - b'0') as i64;
    }
    Some(result)
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
