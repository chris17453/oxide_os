//! ext4 inode handling

use block::BlockDevice;

use crate::error::{Ext4Error, Ext4Result};
use crate::group_desc::{BlockGroupTable, read_block};
use crate::superblock::Ext4Superblock;

/// ext4 inode flags
pub mod flags {
    pub const SECRM: u32 = 0x00000001;
    pub const UNRM: u32 = 0x00000002;
    pub const COMPR: u32 = 0x00000004;
    pub const SYNC: u32 = 0x00000008;
    pub const IMMUTABLE: u32 = 0x00000010;
    pub const APPEND: u32 = 0x00000020;
    pub const NODUMP: u32 = 0x00000040;
    pub const NOATIME: u32 = 0x00000080;
    pub const DIRTY: u32 = 0x00000100;
    pub const COMPRBLK: u32 = 0x00000200;
    pub const NOCOMPR: u32 = 0x00000400;
    pub const ENCRYPT: u32 = 0x00000800;
    pub const INDEX: u32 = 0x00001000;
    pub const IMAGIC: u32 = 0x00002000;
    pub const JOURNAL_DATA: u32 = 0x00004000;
    pub const NOTAIL: u32 = 0x00008000;
    pub const DIRSYNC: u32 = 0x00010000;
    pub const TOPDIR: u32 = 0x00020000;
    pub const HUGE_FILE: u32 = 0x00040000;
    pub const EXTENTS: u32 = 0x00080000;
    pub const VERITY: u32 = 0x00100000;
    pub const EA_INODE: u32 = 0x00200000;
    pub const INLINE_DATA: u32 = 0x10000000;
    pub const PROJINHERIT: u32 = 0x20000000;
    pub const CASEFOLD: u32 = 0x40000000;
}

/// ext4 file type (from i_mode)
pub mod file_type {
    pub const S_IFIFO: u16 = 0o010000;
    pub const S_IFCHR: u16 = 0o020000;
    pub const S_IFDIR: u16 = 0o040000;
    pub const S_IFBLK: u16 = 0o060000;
    pub const S_IFREG: u16 = 0o100000;
    pub const S_IFLNK: u16 = 0o120000;
    pub const S_IFSOCK: u16 = 0o140000;
    pub const S_IFMT: u16 = 0o170000;
}

/// ext4 inode structure (128 bytes base, up to 512 bytes with extra)
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Ext4Inode {
    // 0x00
    pub i_mode: u16,
    pub i_uid: u16,
    pub i_size_lo: u32,
    pub i_atime: u32,
    pub i_ctime: u32,
    // 0x10
    pub i_mtime: u32,
    pub i_dtime: u32,
    pub i_gid: u16,
    pub i_links_count: u16,
    pub i_blocks_lo: u32,
    // 0x20
    pub i_flags: u32,
    pub i_osd1: u32,
    pub i_block: [u32; 15], // Extent tree or block pointers (60 bytes)
    // 0x64
    pub i_generation: u32,
    pub i_file_acl_lo: u32,
    pub i_size_hi: u32, // or i_dir_acl for directories (pre-ext4)
    // 0x70
    pub i_obso_faddr: u32,
    // OS-dependent 2 (12 bytes)
    pub i_osd2: [u8; 12],
    // 0x80 - Extra inode fields (if inode_size > 128)
    pub i_extra_isize: u16,
    pub i_checksum_hi: u16,
    pub i_ctime_extra: u32,
    pub i_mtime_extra: u32,
    pub i_atime_extra: u32,
    pub i_crtime: u32,
    pub i_crtime_extra: u32,
    pub i_version_hi: u32,
    pub i_projid: u32,
}

impl Ext4Inode {
    /// Get file type from mode
    pub fn file_type(&self) -> u16 {
        self.i_mode & file_type::S_IFMT
    }

    /// Check if this is a regular file
    pub fn is_file(&self) -> bool {
        self.file_type() == file_type::S_IFREG
    }

    /// Check if this is a directory
    pub fn is_dir(&self) -> bool {
        self.file_type() == file_type::S_IFDIR
    }

    /// Check if this is a symbolic link
    pub fn is_symlink(&self) -> bool {
        self.file_type() == file_type::S_IFLNK
    }

    /// Check if this is a character device
    pub fn is_char_device(&self) -> bool {
        self.file_type() == file_type::S_IFCHR
    }

    /// Check if this is a block device
    pub fn is_block_device(&self) -> bool {
        self.file_type() == file_type::S_IFBLK
    }

    /// Check if this is a FIFO
    pub fn is_fifo(&self) -> bool {
        self.file_type() == file_type::S_IFIFO
    }

    /// Check if this is a socket
    pub fn is_socket(&self) -> bool {
        self.file_type() == file_type::S_IFSOCK
    }

    /// Get file size (64-bit)
    pub fn size(&self) -> u64 {
        self.i_size_lo as u64 | ((self.i_size_hi as u64) << 32)
    }

    /// Check if inode uses extents
    pub fn uses_extents(&self) -> bool {
        self.i_flags & flags::EXTENTS != 0
    }

    /// Get permission bits
    pub fn permissions(&self) -> u16 {
        self.i_mode & 0o7777
    }

    /// Get UID (32-bit with extension from osd2)
    pub fn uid(&self) -> u32 {
        self.i_uid as u32 | ((self.i_osd2[4] as u32) << 16) | ((self.i_osd2[5] as u32) << 24)
    }

    /// Get GID (32-bit with extension from osd2)
    pub fn gid(&self) -> u32 {
        self.i_gid as u32 | ((self.i_osd2[6] as u32) << 16) | ((self.i_osd2[7] as u32) << 24)
    }

    /// Get block count in 512-byte units (with huge_file extension)
    pub fn blocks(&self) -> u64 {
        let lo = self.i_blocks_lo as u64;
        let hi = ((self.i_osd2[0] as u64) | ((self.i_osd2[1] as u64) << 8)) << 32;
        lo | hi
    }
}

/// Read an inode from the filesystem
pub fn read_inode(
    device: &dyn BlockDevice,
    sb: &Ext4Superblock,
    group_table: &BlockGroupTable,
    ino: u32,
) -> Ext4Result<Ext4Inode> {
    if ino == 0 {
        return Err(Ext4Error::InvalidInode);
    }

    // Inode numbers start at 1
    let ino_index = ino - 1;

    // Calculate which block group contains this inode
    let group = ino_index / sb.s_inodes_per_group;
    let index_in_group = ino_index % sb.s_inodes_per_group;

    // Get the block group descriptor
    let desc = group_table.get(group).ok_or(Ext4Error::InvalidInode)?;

    // Calculate the inode's position within the inode table
    let inode_size = sb.inode_size();
    let block_size = sb.block_size();
    let inodes_per_block = block_size / inode_size as u64;

    let block_in_table = index_in_group as u64 / inodes_per_block;
    let offset_in_block = (index_in_group as u64 % inodes_per_block) * inode_size as u64;

    // Read the block containing the inode
    let block_num = desc.inode_table + block_in_table;
    let mut buf = alloc::vec![0u8; block_size as usize];
    read_block(device, sb, block_num, &mut buf)?;

    // Parse the inode
    let inode: Ext4Inode = unsafe {
        core::ptr::read_unaligned(buf[offset_in_block as usize..].as_ptr() as *const Ext4Inode)
    };

    Ok(inode)
}

/// Well-known inode numbers
pub mod ino {
    pub const BAD_INO: u32 = 1;
    pub const ROOT_INO: u32 = 2;
    pub const USER_QUOTA_INO: u32 = 3;
    pub const GROUP_QUOTA_INO: u32 = 4;
    pub const BOOT_LOADER_INO: u32 = 5;
    pub const UNDEL_DIR_INO: u32 = 6;
    pub const RESIZE_INO: u32 = 7;
    pub const JOURNAL_INO: u32 = 8;
    pub const EXCLUDE_INO: u32 = 9;
    pub const REPLICA_INO: u32 = 10;
    pub const FIRST_INO: u32 = 11;
}

/// Write an inode back to the filesystem
pub fn write_inode(
    device: &dyn BlockDevice,
    sb: &Ext4Superblock,
    group_table: &BlockGroupTable,
    ino: u32,
    inode: &Ext4Inode,
) -> Ext4Result<()> {
    if ino == 0 {
        return Err(Ext4Error::InvalidInode);
    }

    // Inode numbers start at 1
    let ino_index = ino - 1;

    // Calculate which block group contains this inode
    let group = ino_index / sb.s_inodes_per_group;
    let index_in_group = ino_index % sb.s_inodes_per_group;

    // Get the block group descriptor
    let desc = group_table.get(group).ok_or(Ext4Error::InvalidInode)?;

    // Calculate the inode's position within the inode table
    let inode_size = sb.inode_size();
    let block_size = sb.block_size();
    let inodes_per_block = block_size / inode_size as u64;

    let block_in_table = index_in_group as u64 / inodes_per_block;
    let offset_in_block = (index_in_group as u64 % inodes_per_block) * inode_size as u64;

    // Read the block containing the inode
    let block_num = desc.inode_table + block_in_table;
    let mut buf = alloc::vec![0u8; block_size as usize];
    read_block(device, sb, block_num, &mut buf)?;

    // Write the inode to the buffer
    let inode_bytes = unsafe {
        core::slice::from_raw_parts(
            inode as *const Ext4Inode as *const u8,
            core::mem::size_of::<Ext4Inode>().min(inode_size as usize),
        )
    };
    buf[offset_in_block as usize..offset_in_block as usize + inode_bytes.len()]
        .copy_from_slice(inode_bytes);

    // Write the block back
    crate::group_desc::write_block(device, sb, block_num, &buf)?;

    Ok(())
}

/// Create a new inode with default values
pub fn new_inode(mode: u16, uid: u32, gid: u32) -> Ext4Inode {
    // — WireSaint: stamping creation time from the os_core clock bridge
    let now = os_core::wall_clock_secs() as u32;

    Ext4Inode {
        i_mode: mode,
        i_uid: uid as u16,
        i_size_lo: 0,
        i_atime: now,
        i_ctime: now,
        i_mtime: now,
        i_dtime: 0,
        i_gid: gid as u16,
        i_links_count: 1,
        i_blocks_lo: 0,
        i_flags: flags::EXTENTS, // Use extents by default
        i_osd1: 0,
        i_block: [0; 15],
        i_generation: 0,
        i_file_acl_lo: 0,
        i_size_hi: 0,
        i_obso_faddr: 0,
        i_osd2: [0; 12],
        i_extra_isize: 32, // Extra fields size
        i_checksum_hi: 0,
        i_ctime_extra: 0,
        i_mtime_extra: 0,
        i_atime_extra: 0,
        i_crtime: now,
        i_crtime_extra: 0,
        i_version_hi: 0,
        i_projid: 0,
    }
}

/// Initialize extent header in inode i_block
pub fn init_extent_header(inode: &mut Ext4Inode) {
    // Extent header is 12 bytes, fits at start of i_block
    let header = crate::extent::ExtentHeader {
        eh_magic: crate::extent::EXT4_EXT_MAGIC,
        eh_entries: 0,
        eh_max: 4, // Max 4 extents in inode (60 bytes - 12 header = 48, 48/12 = 4)
        eh_depth: 0,
        eh_generation: 0,
    };

    // Write header to i_block
    let header_bytes = unsafe {
        core::slice::from_raw_parts(
            &header as *const crate::extent::ExtentHeader as *const u8,
            12,
        )
    };

    let i_block_bytes =
        unsafe { core::slice::from_raw_parts_mut(inode.i_block.as_mut_ptr() as *mut u8, 60) };

    i_block_bytes[..12].copy_from_slice(header_bytes);
}

/// Get the block number and offset for an inode
pub fn inode_location(
    sb: &Ext4Superblock,
    group_table: &BlockGroupTable,
    ino: u32,
) -> Ext4Result<(u64, u64)> {
    if ino == 0 {
        return Err(Ext4Error::InvalidInode);
    }

    let ino_index = ino - 1;
    let group = ino_index / sb.s_inodes_per_group;
    let index_in_group = ino_index % sb.s_inodes_per_group;

    let desc = group_table.get(group).ok_or(Ext4Error::InvalidInode)?;

    let inode_size = sb.inode_size();
    let block_size = sb.block_size();
    let inodes_per_block = block_size / inode_size as u64;

    let block_in_table = index_in_group as u64 / inodes_per_block;
    let offset_in_block = (index_in_group as u64 % inodes_per_block) * inode_size as u64;

    let block_num = desc.inode_table + block_in_table;
    Ok((block_num, offset_in_block))
}

/// Write an inode and return the modified block data for journaling
pub fn write_inode_data(
    device: &dyn BlockDevice,
    sb: &Ext4Superblock,
    group_table: &BlockGroupTable,
    ino: u32,
    inode: &Ext4Inode,
) -> Ext4Result<(u64, alloc::vec::Vec<u8>)> {
    let (block_num, offset_in_block) = inode_location(sb, group_table, ino)?;
    let block_size = sb.block_size();
    let inode_size = sb.inode_size();

    // Read the block containing the inode
    let mut buf = alloc::vec![0u8; block_size as usize];
    read_block(device, sb, block_num, &mut buf)?;

    // Write the inode to the buffer
    let inode_bytes = unsafe {
        core::slice::from_raw_parts(
            inode as *const Ext4Inode as *const u8,
            core::mem::size_of::<Ext4Inode>().min(inode_size as usize),
        )
    };
    buf[offset_in_block as usize..offset_in_block as usize + inode_bytes.len()]
        .copy_from_slice(inode_bytes);

    // Write the block back
    crate::group_desc::write_block(device, sb, block_num, &buf)?;

    Ok((block_num, buf))
}

impl Ext4Inode {
    /// Set file size (64-bit)
    pub fn set_size(&mut self, size: u64) {
        self.i_size_lo = size as u32;
        self.i_size_hi = (size >> 32) as u32;
    }

    /// Update modification time
    pub fn touch_mtime(&mut self, time: u32) {
        self.i_mtime = time;
    }

    /// Update change time
    pub fn touch_ctime(&mut self, time: u32) {
        self.i_ctime = time;
    }

    /// Update access time
    pub fn touch_atime(&mut self, time: u32) {
        self.i_atime = time;
    }

    /// Update all times
    pub fn touch_all(&mut self, time: u32) {
        self.i_atime = time;
        self.i_mtime = time;
        self.i_ctime = time;
    }

    /// Set block count (in 512-byte units)
    pub fn set_blocks(&mut self, blocks: u64) {
        self.i_blocks_lo = blocks as u32;
        // High 16 bits go into osd2[0..2]
        let hi = (blocks >> 32) as u16;
        self.i_osd2[0] = hi as u8;
        self.i_osd2[1] = (hi >> 8) as u8;
    }

    /// Increment link count
    pub fn inc_links(&mut self) {
        self.i_links_count = self.i_links_count.saturating_add(1);
    }

    /// Decrement link count
    pub fn dec_links(&mut self) {
        self.i_links_count = self.i_links_count.saturating_sub(1);
    }

    /// Set deletion time (marks inode as deleted)
    pub fn set_dtime(&mut self, time: u32) {
        self.i_dtime = time;
    }
}
