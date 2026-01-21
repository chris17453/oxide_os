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

use alloc::string::String;
use alloc::sync::Arc;
use spin::{Mutex, RwLock};

use block::{BlockDevice, BlockError};
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
    pub fn root(self: &Arc<Self>) -> OxidefsResult<Arc<dyn VnodeOps>> {
        let sb = self.superblock.read();
        let inode_data = inode::read_inode(&*self.device, &sb, ROOT_INO)?;

        Ok(Arc::new(OxidefsVnode::new(
            Arc::clone(self),
            ROOT_INO,
            inode_data,
        )))
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

/// OXIDEFS vnode implementation
pub struct OxidefsVnode {
    /// Reference to filesystem
    fs: Arc<Oxidefs>,
    /// Inode number
    ino: u64,
    /// Cached inode data
    inode: RwLock<InodeData>,
}

impl OxidefsVnode {
    fn new(fs: Arc<Oxidefs>, ino: u64, inode: InodeData) -> Self {
        OxidefsVnode {
            fs,
            ino,
            inode: RwLock::new(inode),
        }
    }

    /// Read inode from disk and update cache
    #[allow(dead_code)]
    fn reload_inode(&self) -> OxidefsResult<()> {
        let sb = self.fs.superblock.read();
        let inode_data = inode::read_inode(&*self.fs.device, &sb, self.ino)?;
        *self.inode.write() = inode_data;
        Ok(())
    }

    /// Write inode cache to disk
    fn sync_inode(&self) -> OxidefsResult<()> {
        let sb = self.fs.superblock.read();
        let inode_data = self.inode.read();
        inode::write_inode(&*self.fs.device, &sb, self.ino, &*inode_data)?;
        Ok(())
    }

    /// Add a directory entry
    fn add_dir_entry(&self, name: &str, ino: u64, file_type: u8) -> OxidefsResult<()> {
        let sb = self.fs.superblock.read();
        let mut inode_data = self.inode.write();
        let block_size = sb.block_size as usize;

        // Try to add to existing blocks first
        let num_blocks = ((inode_data.size + block_size as u64 - 1) / block_size as u64) as usize;

        for block_idx in 0..num_blocks {
            let block_num = if block_idx < 12 {
                inode_data.direct[block_idx]
            } else {
                break; // Only support direct blocks for now
            };

            if block_num == 0 {
                continue;
            }

            let mut dir_buf = alloc::vec![0u8; block_size];
            self.fs.device.read(block_num, &mut dir_buf)?;

            if dir::add_entry(&mut dir_buf, ino, name, file_type)? {
                self.fs.device.write(block_num, &dir_buf)?;
                return Ok(());
            }
        }

        // Need to allocate a new block
        let new_block = self.fs.alloc_block()?;

        // Add to direct blocks
        if num_blocks < 12 {
            inode_data.direct[num_blocks] = new_block;
            inode_data.blocks += 1;
            inode_data.size += block_size as u64;

            // Initialize new block with entry
            let mut dir_buf = alloc::vec![0u8; block_size];
            dir::add_entry(&mut dir_buf, ino, name, file_type)?;
            self.fs.device.write(new_block, &dir_buf)?;

            // Write updated inode
            drop(inode_data);
            drop(sb);
            self.sync_inode()?;

            Ok(())
        } else {
            Err(OxidefsError::NoSpace)
        }
    }

    /// Remove a directory entry
    fn remove_dir_entry(&self, name: &str) -> OxidefsResult<u64> {
        let sb = self.fs.superblock.read();
        let inode_data = self.inode.read();
        let block_size = sb.block_size as usize;

        let num_blocks = ((inode_data.size + block_size as u64 - 1) / block_size as u64) as usize;

        for block_idx in 0..num_blocks {
            let block_num = if block_idx < 12 {
                inode_data.direct[block_idx]
            } else {
                break;
            };

            if block_num == 0 {
                continue;
            }

            let mut dir_buf = alloc::vec![0u8; block_size];
            self.fs.device.read(block_num, &mut dir_buf)?;

            // Get the inode number before removing
            if let Some(entry) = dir::find_entry(&dir_buf, name) {
                let found_ino = entry.ino;

                if dir::remove_entry(&mut dir_buf, name)? {
                    self.fs.device.write(block_num, &dir_buf)?;
                    return Ok(found_ino);
                }
            }
        }

        Err(OxidefsError::NotFound)
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

        let sb = self.fs.superblock.read();
        let inode_data = self.inode.read();
        let block_size = sb.block_size as usize;

        // Read directory blocks and search for entry
        let num_blocks = ((inode_data.size + block_size as u64 - 1) / block_size as u64) as usize;

        for block_idx in 0..num_blocks {
            // Get block number directly from inode
            let block_num = if block_idx < 12 {
                inode_data.direct[block_idx]
            } else {
                // For now, only support direct blocks in directories
                break;
            };

            if block_num == 0 {
                continue;
            }

            let mut dir_buf = alloc::vec![0u8; block_size];
            self.fs.device.read(block_num, &mut dir_buf)
                .map_err(|_| VfsError::IoError)?;

            // Search for entry in this block
            if let Some(entry) = dir::find_entry(&dir_buf, name) {
                // Found it! Load the inode
                let found_inode = inode::read_inode(&*self.fs.device, &sb, entry.ino)
                    .map_err(|e| -> VfsError { e.into() })?;

                return Ok(Arc::new(OxidefsVnode::new(
                    Arc::clone(&self.fs),
                    entry.ino,
                    found_inode,
                )));
            }
        }

        Err(VfsError::NotFound)
    }

    fn create(&self, name: &str, mode: Mode) -> VfsResult<Arc<dyn VnodeOps>> {
        if self.vtype() != VnodeType::Directory {
            return Err(VfsError::NotDirectory);
        }

        if self.fs.read_only {
            return Err(VfsError::ReadOnly);
        }

        if name.len() > MAX_NAME_LEN {
            return Err(VfsError::NameTooLong);
        }

        // Check if file already exists
        if self.lookup(name).is_ok() {
            return Err(VfsError::AlreadyExists);
        }

        // Allocate new inode
        let new_ino = self.fs.alloc_inode().map_err(|e| -> VfsError { e.into() })?;

        // Create inode data
        let new_inode = InodeData {
            mode: mode.bits() | 0o100000, // Regular file
            uid: 0, // TODO: Get current uid
            gid: 0, // TODO: Get current gid
            size: 0,
            atime: 0, // TODO: Get current time
            mtime: 0,
            ctime: 0,
            links: 1,
            blocks: 0,
            flags: 0,
            direct: [0; 12],
            indirect: 0,
            double_indirect: 0,
            triple_indirect: 0,
            checksum: 0,
        };

        // Write new inode to disk
        let sb = self.fs.superblock.read();
        inode::write_inode(&*self.fs.device, &sb, new_ino, &new_inode)
            .map_err(|e| -> VfsError { e.into() })?;

        // Add directory entry
        let file_type = dir::file_type::REG_FILE;
        self.add_dir_entry(name, new_ino, file_type)?;

        // Update directory inode link count and timestamps
        {
            let mut inode_data = self.inode.write();
            inode_data.mtime = 0; // TODO: current time
            inode_data.ctime = 0;
        }
        self.sync_inode().map_err(|e| -> VfsError { e.into() })?;

        Ok(Arc::new(OxidefsVnode::new(
            Arc::clone(&self.fs),
            new_ino,
            new_inode,
        )))
    }

    fn read(&self, offset: u64, buf: &mut [u8]) -> VfsResult<usize> {
        if self.vtype() == VnodeType::Directory {
            return Err(VfsError::IsDirectory);
        }

        let sb = self.fs.superblock.read();
        let inode = self.inode.read();

        if offset >= inode.size {
            return Ok(0);
        }

        file::read_file(&*self.fs.device, &sb, &*inode, offset, buf)
            .map_err(|e| -> VfsError { e.into() })
    }

    fn write(&self, offset: u64, buf: &[u8]) -> VfsResult<usize> {
        if self.vtype() == VnodeType::Directory {
            return Err(VfsError::IsDirectory);
        }

        if self.fs.read_only {
            return Err(VfsError::ReadOnly);
        }

        let sb = self.fs.superblock.read();
        let mut inode = self.inode.write();

        let bytes_written = file::write_file(
            &*self.fs.device,
            &sb,
            &mut *inode,
            offset,
            buf,
            || self.fs.alloc_block(),
        )
        .map_err(|e| -> VfsError { e.into() })?;

        // Update timestamps
        inode.mtime = 0; // TODO: current time
        inode.ctime = 0;

        drop(inode);
        drop(sb);

        self.sync_inode().map_err(|e| -> VfsError { e.into() })?;

        Ok(bytes_written)
    }

    fn readdir(&self, offset: u64) -> VfsResult<Option<DirEntry>> {
        if self.vtype() != VnodeType::Directory {
            return Err(VfsError::NotDirectory);
        }

        let sb = self.fs.superblock.read();
        let inode_data = self.inode.read();
        let block_size = sb.block_size as usize;

        let num_blocks = ((inode_data.size + block_size as u64 - 1) / block_size as u64) as usize;
        let mut entry_index = 0u64;

        for block_idx in 0..num_blocks {
            let block_num = if block_idx < 12 {
                inode_data.direct[block_idx]
            } else {
                break;
            };

            if block_num == 0 {
                continue;
            }

            let mut dir_buf = alloc::vec![0u8; block_size];
            self.fs.device.read(block_num, &mut dir_buf)
                .map_err(|_| VfsError::IoError)?;

            for entry in dir::iter_entries(&dir_buf) {
                if entry_index >= offset {
                    // Convert file type
                    let file_type = match entry.file_type {
                        dir::file_type::REG_FILE => VnodeType::File,
                        dir::file_type::DIR => VnodeType::Directory,
                        dir::file_type::SYMLINK => VnodeType::Symlink,
                        dir::file_type::CHRDEV => VnodeType::CharDevice,
                        dir::file_type::BLKDEV => VnodeType::BlockDevice,
                        dir::file_type::FIFO => VnodeType::Fifo,
                        dir::file_type::SOCK => VnodeType::Socket,
                        _ => VnodeType::File,
                    };

                    return Ok(Some(DirEntry {
                        name: String::from(entry.name_str()),
                        ino: entry.ino,
                        file_type,
                    }));
                }
                entry_index += 1;
            }
        }

        Ok(None)
    }

    fn mkdir(&self, name: &str, mode: Mode) -> VfsResult<Arc<dyn VnodeOps>> {
        if self.vtype() != VnodeType::Directory {
            return Err(VfsError::NotDirectory);
        }

        if self.fs.read_only {
            return Err(VfsError::ReadOnly);
        }

        if name.len() > MAX_NAME_LEN {
            return Err(VfsError::NameTooLong);
        }

        // Check if already exists
        if self.lookup(name).is_ok() {
            return Err(VfsError::AlreadyExists);
        }

        // Allocate new inode
        let new_ino = self.fs.alloc_inode().map_err(|e| -> VfsError { e.into() })?;

        // Allocate block for directory data
        let data_block = self.fs.alloc_block().map_err(|e| -> VfsError { e.into() })?;

        // Create directory inode
        let new_inode = InodeData {
            mode: mode.bits() | 0o040000, // Directory
            uid: 0,
            gid: 0,
            size: BLOCK_SIZE as u64,
            atime: 0,
            mtime: 0,
            ctime: 0,
            links: 2, // . and parent reference
            blocks: 1,
            flags: 0,
            direct: {
                let mut d = [0u64; 12];
                d[0] = data_block;
                d
            },
            indirect: 0,
            double_indirect: 0,
            triple_indirect: 0,
            checksum: 0,
        };

        // Initialize directory with . and ..
        let block_size = BLOCK_SIZE as usize;
        let mut dir_buf = alloc::vec![0u8; block_size];
        dir::init_directory(&mut dir_buf, new_ino, self.ino);
        self.fs.device.write(data_block, &dir_buf)
            .map_err(|_| VfsError::IoError)?;

        // Write new inode
        let sb = self.fs.superblock.read();
        inode::write_inode(&*self.fs.device, &sb, new_ino, &new_inode)
            .map_err(|e| -> VfsError { e.into() })?;

        // Add directory entry to parent
        let file_type = dir::file_type::DIR;
        self.add_dir_entry(name, new_ino, file_type)
            .map_err(|e| -> VfsError { e.into() })?;

        // Update parent directory link count (for ".." in new dir)
        {
            let mut inode_data = self.inode.write();
            inode_data.links += 1;
            inode_data.mtime = 0;
            inode_data.ctime = 0;
        }
        self.sync_inode().map_err(|e| -> VfsError { e.into() })?;

        Ok(Arc::new(OxidefsVnode::new(
            Arc::clone(&self.fs),
            new_ino,
            new_inode,
        )))
    }

    fn rmdir(&self, name: &str) -> VfsResult<()> {
        if self.vtype() != VnodeType::Directory {
            return Err(VfsError::NotDirectory);
        }

        if self.fs.read_only {
            return Err(VfsError::ReadOnly);
        }

        // Cannot remove . or ..
        if name == "." || name == ".." {
            return Err(VfsError::InvalidArgument);
        }

        // Lookup the directory
        let target = self.lookup(name)?;
        if target.vtype() != VnodeType::Directory {
            return Err(VfsError::NotDirectory);
        }

        // Check if directory is empty (only has . and ..)
        let mut entry_count = 0;
        let mut offset = 0;
        while let Some(_entry) = target.readdir(offset)? {
            entry_count += 1;
            offset += 1;
            if entry_count > 2 {
                return Err(VfsError::NotEmpty);
            }
        }

        // Remove the directory entry from parent
        let removed_ino = self.remove_dir_entry(name)
            .map_err(|e| -> VfsError { e.into() })?;

        // Free the inode
        self.fs.free_inode(removed_ino)
            .map_err(|e| -> VfsError { e.into() })?;

        // Update parent link count (was pointing to this dir via "..")
        {
            let mut inode_data = self.inode.write();
            inode_data.links -= 1;
            inode_data.mtime = 0;
            inode_data.ctime = 0;
        }
        self.sync_inode().map_err(|e| -> VfsError { e.into() })?;

        Ok(())
    }

    fn unlink(&self, name: &str) -> VfsResult<()> {
        if self.vtype() != VnodeType::Directory {
            return Err(VfsError::NotDirectory);
        }

        if self.fs.read_only {
            return Err(VfsError::ReadOnly);
        }

        // Cannot unlink . or ..
        if name == "." || name == ".." {
            return Err(VfsError::InvalidArgument);
        }

        // Lookup the file
        let target = self.lookup(name)?;
        if target.vtype() == VnodeType::Directory {
            return Err(VfsError::IsDirectory);
        }

        // Remove directory entry
        let removed_ino = self.remove_dir_entry(name)
            .map_err(|e| -> VfsError { e.into() })?;

        // Read the inode to decrement link count
        let sb = self.fs.superblock.read();
        let mut inode_data = inode::read_inode(&*self.fs.device, &sb, removed_ino)
            .map_err(|e| -> VfsError { e.into() })?;

        inode_data.links -= 1;

        // If no more links, free the inode and its blocks
        if inode_data.links == 0 {
            // Free all blocks
            let num_blocks = inode_data.blocks;
            for block_idx in 0..num_blocks {
                if block_idx < 12 && inode_data.direct[block_idx as usize] != 0 {
                    self.fs.free_block(inode_data.direct[block_idx as usize])
                        .map_err(|e| -> VfsError { e.into() })?;
                }
            }

            // Free the inode
            self.fs.free_inode(removed_ino)
                .map_err(|e| -> VfsError { e.into() })?;
        } else {
            // Still has links, just update inode
            inode::write_inode(&*self.fs.device, &sb, removed_ino, &inode_data)
                .map_err(|e| -> VfsError { e.into() })?;
        }

        // Update parent directory timestamps
        {
            let mut parent_inode = self.inode.write();
            parent_inode.mtime = 0;
            parent_inode.ctime = 0;
        }
        self.sync_inode().map_err(|e| -> VfsError { e.into() })?;

        Ok(())
    }

    fn rename(&self, old_name: &str, new_dir: &dyn VnodeOps, new_name: &str) -> VfsResult<()> {
        if self.fs.read_only {
            return Err(VfsError::ReadOnly);
        }

        if old_name == "." || old_name == ".." || new_name == "." || new_name == ".." {
            return Err(VfsError::InvalidArgument);
        }

        if new_name.len() > MAX_NAME_LEN {
            return Err(VfsError::NameTooLong);
        }

        // Lookup the source file
        let target = self.lookup(old_name)?;

        // Get the target inode number before removing
        let removed_ino = self.remove_dir_entry(old_name)
            .map_err(|e| -> VfsError { e.into() })?;

        // Try to add to new directory
        // For simplicity, only support rename within same directory for now
        // Full cross-directory rename is complex
        if !core::ptr::addr_eq(new_dir as *const dyn VnodeOps, self as *const dyn VnodeOps) {
            // Cross-directory rename - would need to handle link counts, "..", etc.
            // Restore the entry we removed and return error
            let file_type = match target.vtype() {
                VnodeType::File => dir::file_type::REG_FILE,
                VnodeType::Directory => dir::file_type::DIR,
                VnodeType::Symlink => dir::file_type::SYMLINK,
                _ => dir::file_type::REG_FILE,
            };
            self.add_dir_entry(old_name, removed_ino, file_type)
                .map_err(|e| -> VfsError { e.into() })?;
            return Err(VfsError::NotSupported);
        }

        // Same directory rename - just add with new name
        let file_type = match target.vtype() {
            VnodeType::File => dir::file_type::REG_FILE,
            VnodeType::Directory => dir::file_type::DIR,
            VnodeType::Symlink => dir::file_type::SYMLINK,
            VnodeType::CharDevice => dir::file_type::CHRDEV,
            VnodeType::BlockDevice => dir::file_type::BLKDEV,
            VnodeType::Fifo => dir::file_type::FIFO,
            VnodeType::Socket => dir::file_type::SOCK,
        };

        self.add_dir_entry(new_name, removed_ino, file_type)
            .map_err(|e| -> VfsError { e.into() })?;

        // Update timestamps
        {
            let mut inode_data = self.inode.write();
            inode_data.mtime = 0;
            inode_data.ctime = 0;
        }
        self.sync_inode().map_err(|e| -> VfsError { e.into() })?;

        Ok(())
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

    fn truncate(&self, new_size: u64) -> VfsResult<()> {
        if self.vtype() == VnodeType::Directory {
            return Err(VfsError::IsDirectory);
        }

        if self.fs.read_only {
            return Err(VfsError::ReadOnly);
        }

        let sb = self.fs.superblock.read();
        let mut inode_data = self.inode.write();

        file::truncate_file(
            &*self.fs.device,
            &sb,
            &mut *inode_data,
            new_size,
            |block| self.fs.free_block(block),
        )
        .map_err(|e| -> VfsError { e.into() })?;

        inode_data.mtime = 0;
        inode_data.ctime = 0;

        drop(inode_data);
        drop(sb);

        self.sync_inode().map_err(|e| -> VfsError { e.into() })?;

        Ok(())
    }

    fn set_times(&self, atime: Option<u64>, mtime: Option<u64>) -> VfsResult<()> {
        if self.fs.read_only {
            return Err(VfsError::ReadOnly);
        }

        let mut inode_data = self.inode.write();

        // Update times if specified
        if let Some(t) = atime {
            inode_data.atime = t;
        }
        if let Some(t) = mtime {
            inode_data.mtime = t;
        }

        // Always update ctime when metadata changes
        inode_data.ctime = 0; // TODO: use actual current time

        drop(inode_data);

        self.sync_inode().map_err(|e| -> VfsError { e.into() })?;

        Ok(())
    }
}
