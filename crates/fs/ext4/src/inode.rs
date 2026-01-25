//! ext4 inode handling

use block::BlockDevice;

use crate::error::{Ext4Error, Ext4Result};
use crate::group_desc::{read_block, BlockGroupTable};
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
        let hi = ((self.i_osd2[0] as u64)
            | ((self.i_osd2[1] as u64) << 8))
            << 32;
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
