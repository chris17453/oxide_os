//! OXIDEFS - OXIDE Native Filesystem
//!
//! A modern filesystem designed for OXIDE OS with:
//! - 64-bit block addresses
//! - Extended attributes
//! - Metadata checksums
//! - Journal for crash recovery
//! - Efficient directory handling

#![no_std]

extern crate alloc;

pub mod superblock;
pub mod inode;
pub mod dir;
pub mod file;
pub mod bitmap;
pub mod journal;

use alloc::boxed::Box;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use spin::{Mutex, RwLock};

use block::{BlockDevice, BlockError, BlockResult};
use vfs::{DirEntry, Mode, Stat, VfsError, VfsResult, VnodeOps, VnodeType};

pub use superblock::Superblock;
pub use inode::{Inode, InodeData};

/// OXIDEFS magic number ("OXIDE" + version)
pub const OXIDEFS_MAGIC: u64 = 0x4546464C5558_0001; // "OXIDE" + version 1

/// Block size (4KB)
pub const BLOCK_SIZE: u32 = 4096;

/// Inode size
pub const INODE_SIZE: u32 = 256;

/// Root inode number
pub const ROOT_INO: u64 = 2;

/// Maximum filename length
pub const MAX_NAME_LEN: usize = 255;

/// Maximum file size (16 EB with 64-bit addressing)
pub const MAX_FILE_SIZE: u64 = u64::MAX;

/// OXIDEFS error types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OxidefsError {
    /// Invalid magic number
    InvalidMagic,
    /// Corrupted superblock
    CorruptedSuperblock,
    /// Corrupted inode
    CorruptedInode,
    /// No space left
    NoSpace,
    /// No inodes left
    NoInodes,
    /// Name too long
    NameTooLong,
    /// Not a directory
    NotDirectory,
    /// Is a directory
    IsDirectory,
    /// Directory not empty
    NotEmpty,
    /// File exists
    FileExists,
    /// File not found
    NotFound,
    /// I/O error
    IoError,
    /// Read-only filesystem
    ReadOnly,
    /// Invalid argument
    InvalidArgument,
}

impl From<BlockError> for OxidefsError {
    fn from(_: BlockError) -> Self {
        OxidefsError::IoError
    }
}

impl From<OxidefsError> for VfsError {
    fn from(e: OxidefsError) -> Self {
        match e {
            OxidefsError::NotFound => VfsError::NotFound,
            OxidefsError::NoSpace => VfsError::NoSpace,
            OxidefsError::NotDirectory => VfsError::NotDirectory,
            OxidefsError::IsDirectory => VfsError::IsDirectory,
            OxidefsError::NotEmpty => VfsError::NotEmpty,
            OxidefsError::FileExists => VfsError::AlreadyExists,
            OxidefsError::NameTooLong => VfsError::NameTooLong,
            OxidefsError::ReadOnly => VfsError::ReadOnly,
            OxidefsError::IoError => VfsError::IoError,
            _ => VfsError::IoError,
        }
    }
}

/// Result type for oxidefs operations
pub type OxidefsResult<T> = Result<T, OxidefsError>;

/// OXIDEFS filesystem instance
pub struct Oxidefs {
    /// Block device
    device: Arc<dyn BlockDevice>,
    /// Superblock
    superblock: RwLock<Superblock>,
    /// Block bitmap
    block_bitmap: Mutex<bitmap::Bitmap>,
    /// Inode bitmap
    inode_bitmap: Mutex<bitmap::Bitmap>,
    /// Read-only flag
    read_only: bool,
}

impl Oxidefs {
    /// Mount an OXIDEFS filesystem from a block device
    pub fn mount(device: Arc<dyn BlockDevice>, read_only: bool) -> OxidefsResult<Arc<Self>> {
        let block_size = device.block_size() as usize;

        // Read superblock (block 0)
        let mut buf = alloc::vec![0u8; block_size];
        device.read(0, &mut buf)?;

        let sb = Superblock::parse(&buf)?;

        // Verify magic
        if sb.magic != OXIDEFS_MAGIC {
            return Err(OxidefsError::InvalidMagic);
        }

        // Load block bitmap
        let block_bitmap_blocks = (sb.total_blocks + 8 * block_size as u64 - 1) / (8 * block_size as u64);
        let block_bitmap = bitmap::Bitmap::load(
            &*device,
            sb.block_bitmap_start,
            block_bitmap_blocks as usize,
            sb.total_blocks as usize,
        )?;

        // Load inode bitmap
        let inode_bitmap_blocks = (sb.total_inodes + 8 * block_size as u64 - 1) / (8 * block_size as u64);
        let inode_bitmap = bitmap::Bitmap::load(
            &*device,
            sb.inode_bitmap_start,
            inode_bitmap_blocks as usize,
            sb.total_inodes as usize,
        )?;

        Ok(Arc::new(Oxidefs {
            device,
            superblock: RwLock::new(sb),
            block_bitmap: Mutex::new(block_bitmap),
            inode_bitmap: Mutex::new(inode_bitmap),
            read_only,
        }))
    }

    /// Create a new OXIDEFS filesystem on a block device
    pub fn mkfs(device: Arc<dyn BlockDevice>) -> OxidefsResult<()> {
        let block_size = device.block_size() as usize;
        let total_blocks = device.block_count();

        // Calculate layout
        let inode_ratio = 16384; // One inode per 16KB
        let total_inodes = (total_blocks * block_size as u64 / inode_ratio).max(128);

        let superblock_blocks = 1u64;
        let block_bitmap_blocks = (total_blocks + 8 * block_size as u64 - 1) / (8 * block_size as u64);
        let inode_bitmap_blocks = (total_inodes + 8 * block_size as u64 - 1) / (8 * block_size as u64);
        let inode_table_blocks = (total_inodes * INODE_SIZE as u64 + block_size as u64 - 1) / block_size as u64;

        let metadata_blocks = superblock_blocks + block_bitmap_blocks + inode_bitmap_blocks + inode_table_blocks;
        let first_data_block = metadata_blocks;

        // Create superblock
        let sb = Superblock {
            magic: OXIDEFS_MAGIC,
            version: 1,
            block_size: block_size as u32,
            total_blocks,
            free_blocks: total_blocks - metadata_blocks - 1, // -1 for root dir
            total_inodes,
            free_inodes: total_inodes - 1, // -1 for root inode
            block_bitmap_start: superblock_blocks,
            inode_bitmap_start: superblock_blocks + block_bitmap_blocks,
            inode_table_start: superblock_blocks + block_bitmap_blocks + inode_bitmap_blocks,
            first_data_block,
            root_inode: ROOT_INO,
            mount_time: 0,
            write_time: 0,
            mount_count: 0,
            max_mount_count: 20,
            state: 1, // Clean
            checksum: 0,
        };

        // Write superblock
        let mut buf = alloc::vec![0u8; block_size];
        sb.serialize(&mut buf);
        device.write(0, &buf)?;

        // Initialize block bitmap (mark metadata as used)
        let mut block_bitmap = bitmap::Bitmap::new((total_blocks) as usize);
        for i in 0..metadata_blocks {
            block_bitmap.set(i as usize);
        }
        // Mark block for root directory data
        block_bitmap.set(first_data_block as usize);
        block_bitmap.save(&*device, sb.block_bitmap_start)?;

        // Initialize inode bitmap (mark root inode as used)
        let mut inode_bitmap = bitmap::Bitmap::new(total_inodes as usize);
        inode_bitmap.set(0); // Reserved
        inode_bitmap.set(1); // Reserved
        inode_bitmap.set(ROOT_INO as usize); // Root
        inode_bitmap.save(&*device, sb.inode_bitmap_start)?;

        // Create root inode
        let root_inode = InodeData {
            mode: 0o40755, // Directory
            uid: 0,
            gid: 0,
            size: block_size as u64,
            atime: 0,
            mtime: 0,
            ctime: 0,
            links: 2, // . and parent (itself for root)
            blocks: 1,
            flags: 0,
            direct: {
                let mut d = [0u64; 12];
                d[0] = first_data_block;
                d
            },
            indirect: 0,
            double_indirect: 0,
            triple_indirect: 0,
            checksum: 0,
        };

        // Write root inode
        inode::write_inode(&*device, &sb, ROOT_INO, &root_inode)?;

        // Initialize root directory with . and ..
        let mut dir_buf = alloc::vec![0u8; block_size];
        dir::init_directory(&mut dir_buf, ROOT_INO, ROOT_INO);
        device.write(first_data_block, &dir_buf)?;

        // Sync
        device.flush()?;

        Ok(())
    }

    /// Get the root inode
    pub fn root(&self) -> OxidefsResult<Arc<dyn VnodeOps>> {
        let sb = self.superblock.read();
        let inode_data = inode::read_inode(&*self.device, &sb, ROOT_INO)?;

        Ok(Arc::new(OxidefsVnode::new(
            Arc::new(self.clone_ref()),
            ROOT_INO,
            inode_data,
        )))
    }

    /// Clone a reference to self (for vnodes)
    fn clone_ref(&self) -> OxidefsRef {
        OxidefsRef {
            device: Arc::clone(&self.device),
            read_only: self.read_only,
        }
    }

    /// Allocate a block
    pub fn alloc_block(&self) -> OxidefsResult<u64> {
        if self.read_only {
            return Err(OxidefsError::ReadOnly);
        }

        let mut bitmap = self.block_bitmap.lock();
        let sb = self.superblock.read();

        if let Some(block) = bitmap.find_free() {
            bitmap.set(block);
            drop(bitmap);

            // Update superblock
            drop(sb);
            let mut sb = self.superblock.write();
            sb.free_blocks -= 1;

            Ok(block as u64)
        } else {
            Err(OxidefsError::NoSpace)
        }
    }

    /// Free a block
    pub fn free_block(&self, block: u64) -> OxidefsResult<()> {
        if self.read_only {
            return Err(OxidefsError::ReadOnly);
        }

        let mut bitmap = self.block_bitmap.lock();
        bitmap.clear(block as usize);
        drop(bitmap);

        let mut sb = self.superblock.write();
        sb.free_blocks += 1;

        Ok(())
    }

    /// Allocate an inode
    pub fn alloc_inode(&self) -> OxidefsResult<u64> {
        if self.read_only {
            return Err(OxidefsError::ReadOnly);
        }

        let mut bitmap = self.inode_bitmap.lock();
        let sb = self.superblock.read();

        if let Some(ino) = bitmap.find_free() {
            bitmap.set(ino);
            drop(bitmap);

            drop(sb);
            let mut sb = self.superblock.write();
            sb.free_inodes -= 1;

            Ok(ino as u64)
        } else {
            Err(OxidefsError::NoInodes)
        }
    }

    /// Free an inode
    pub fn free_inode(&self, ino: u64) -> OxidefsResult<()> {
        if self.read_only {
            return Err(OxidefsError::ReadOnly);
        }

        let mut bitmap = self.inode_bitmap.lock();
        bitmap.clear(ino as usize);
        drop(bitmap);

        let mut sb = self.superblock.write();
        sb.free_inodes += 1;

        Ok(())
    }
}

/// Reference to filesystem (for vnodes)
struct OxidefsRef {
    device: Arc<dyn BlockDevice>,
    read_only: bool,
}

/// OXIDEFS vnode implementation
pub struct OxidefsVnode {
    /// Reference to filesystem
    fs: Arc<OxidefsRef>,
    /// Inode number
    ino: u64,
    /// Cached inode data
    inode: RwLock<InodeData>,
}

impl OxidefsVnode {
    fn new(fs: Arc<OxidefsRef>, ino: u64, inode: InodeData) -> Self {
        OxidefsVnode {
            fs,
            ino,
            inode: RwLock::new(inode),
        }
    }

    fn inode_type(&self) -> VnodeType {
        let inode = self.inode.read();
        let mode = inode.mode;
        match mode & 0o170000 {
            0o040000 => VnodeType::Directory,
            0o120000 => VnodeType::Symlink,
            0o020000 => VnodeType::CharDevice,
            0o060000 => VnodeType::BlockDevice,
            0o010000 => VnodeType::Fifo,
            0o140000 => VnodeType::Socket,
            _ => VnodeType::File,
        }
    }
}

impl VnodeOps for OxidefsVnode {
    fn vtype(&self) -> VnodeType {
        self.inode_type()
    }

    fn lookup(&self, name: &str) -> VfsResult<Arc<dyn VnodeOps>> {
        if self.vtype() != VnodeType::Directory {
            return Err(VfsError::NotDirectory);
        }

        // Read directory and find entry
        // Stub implementation
        Err(VfsError::NotFound)
    }

    fn create(&self, name: &str, mode: Mode) -> VfsResult<Arc<dyn VnodeOps>> {
        if self.vtype() != VnodeType::Directory {
            return Err(VfsError::NotDirectory);
        }

        if self.fs.read_only {
            return Err(VfsError::ReadOnly);
        }

        // Allocate inode, create file
        // Stub implementation
        Err(VfsError::NotSupported)
    }

    fn read(&self, offset: u64, buf: &mut [u8]) -> VfsResult<usize> {
        if self.vtype() == VnodeType::Directory {
            return Err(VfsError::IsDirectory);
        }

        let inode = self.inode.read();
        if offset >= inode.size {
            return Ok(0);
        }

        // Read file data
        // Stub implementation
        Ok(0)
    }

    fn write(&self, offset: u64, buf: &[u8]) -> VfsResult<usize> {
        if self.vtype() == VnodeType::Directory {
            return Err(VfsError::IsDirectory);
        }

        if self.fs.read_only {
            return Err(VfsError::ReadOnly);
        }

        // Write file data
        // Stub implementation
        Ok(0)
    }

    fn readdir(&self, offset: u64) -> VfsResult<Option<DirEntry>> {
        if self.vtype() != VnodeType::Directory {
            return Err(VfsError::NotDirectory);
        }

        // Read directory entries
        // Stub implementation
        Ok(None)
    }

    fn mkdir(&self, name: &str, mode: Mode) -> VfsResult<Arc<dyn VnodeOps>> {
        if self.vtype() != VnodeType::Directory {
            return Err(VfsError::NotDirectory);
        }

        if self.fs.read_only {
            return Err(VfsError::ReadOnly);
        }

        // Create directory
        // Stub implementation
        Err(VfsError::NotSupported)
    }

    fn rmdir(&self, name: &str) -> VfsResult<()> {
        if self.vtype() != VnodeType::Directory {
            return Err(VfsError::NotDirectory);
        }

        if self.fs.read_only {
            return Err(VfsError::ReadOnly);
        }

        // Remove directory
        // Stub implementation
        Err(VfsError::NotSupported)
    }

    fn unlink(&self, name: &str) -> VfsResult<()> {
        if self.vtype() != VnodeType::Directory {
            return Err(VfsError::NotDirectory);
        }

        if self.fs.read_only {
            return Err(VfsError::ReadOnly);
        }

        // Unlink file
        // Stub implementation
        Err(VfsError::NotSupported)
    }

    fn rename(&self, old_name: &str, new_dir: &dyn VnodeOps, new_name: &str) -> VfsResult<()> {
        if self.fs.read_only {
            return Err(VfsError::ReadOnly);
        }

        // Rename
        // Stub implementation
        Err(VfsError::NotSupported)
    }

    fn stat(&self) -> VfsResult<Stat> {
        let inode = self.inode.read();
        Ok(Stat {
            dev: 0,
            ino: self.ino,
            mode: inode.mode,
            nlink: inode.links as u64,
            uid: inode.uid,
            gid: inode.gid,
            rdev: 0,
            size: inode.size,
            blksize: BLOCK_SIZE as u64,
            blocks: inode.blocks,
            atime: inode.atime,
            mtime: inode.mtime,
            ctime: inode.ctime,
        })
    }

    fn truncate(&self, size: u64) -> VfsResult<()> {
        if self.vtype() == VnodeType::Directory {
            return Err(VfsError::IsDirectory);
        }

        if self.fs.read_only {
            return Err(VfsError::ReadOnly);
        }

        // Truncate file
        // Stub implementation
        Ok(())
    }
}
