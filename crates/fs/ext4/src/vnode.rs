//! ext4 VnodeOps implementation

use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use spin::RwLock;

use block::BlockDevice;
use vfs::{DirEntry, Mode, Stat, VfsError, VfsResult, VnodeOps, VnodeType};

use crate::bitmap;
use crate::dir;
use crate::error::Ext4Error;
use crate::file;
use crate::group_desc::BlockGroupTable;
use crate::inode::{self, Ext4Inode, file_type, read_inode, write_inode};
use crate::journal::{Journal, SharedJournal};
use crate::superblock::Ext4Superblock;

/// Shared ext4 filesystem state
pub struct Ext4Fs {
    /// Block device
    pub device: Arc<dyn BlockDevice>,
    /// Superblock
    pub sb: Ext4Superblock,
    /// Block group table
    pub group_table: BlockGroupTable,
    /// Read-only mode
    pub read_only: bool,
    /// Journal (if available and not read-only)
    pub journal: Option<Arc<SharedJournal>>,
}

impl Ext4Fs {
    /// Get the block device
    pub fn device(&self) -> &dyn BlockDevice {
        &*self.device
    }

    /// Check if journaling is enabled
    pub fn has_journal(&self) -> bool {
        self.journal.is_some()
    }

    /// Get journal reference
    pub fn journal(&self) -> Option<&Arc<SharedJournal>> {
        self.journal.as_ref()
    }

    /// Journal a metadata block write
    pub fn journal_block(&self, blocknr: u64, data: Vec<u8>) {
        if let Some(ref journal) = self.journal {
            let mut j = journal.0.lock();
            // Add to current transaction if one is active
            let _ = j.journal_block(blocknr, data, true);
        }
    }

    /// Start a journal transaction
    pub fn begin_transaction(&self) -> bool {
        if let Some(ref journal) = self.journal {
            let mut j = journal.0.lock();
            j.start_transaction().is_ok()
        } else {
            false
        }
    }

    /// Commit current journal transaction
    pub fn commit_transaction(&self) -> bool {
        if let Some(ref journal) = self.journal {
            let mut j = journal.0.lock();
            j.commit_transaction().is_ok()
        } else {
            true // No journal means nothing to commit
        }
    }

    /// Abort current journal transaction
    pub fn abort_transaction(&self) {
        if let Some(ref journal) = self.journal {
            let mut j = journal.0.lock();
            j.abort_transaction();
        }
    }
}

/// ext4 vnode (file/directory/etc)
pub struct Ext4Vnode {
    /// Shared filesystem state
    fs: Arc<RwLock<Ext4Fs>>,
    /// Inode number
    ino: u32,
    /// Cached inode data
    inode: RwLock<Ext4Inode>,
}

impl Ext4Vnode {
    /// Create a new vnode
    pub fn new(fs: Arc<RwLock<Ext4Fs>>, ino: u32, inode: Ext4Inode) -> Self {
        Ext4Vnode {
            fs,
            ino,
            inode: RwLock::new(inode),
        }
    }

    /// Get inode number
    pub fn ino(&self) -> u32 {
        self.ino
    }

    /// Reload inode from disk
    fn reload_inode(&self) -> Result<(), Ext4Error> {
        let fs = self.fs.read();
        let new_inode = read_inode(fs.device(), &fs.sb, &fs.group_table, self.ino)?;
        *self.inode.write() = new_inode;
        Ok(())
    }

    /// Get vnode type from inode
    fn get_vtype(&self) -> VnodeType {
        let inode = self.inode.read();
        if inode.is_dir() {
            VnodeType::Directory
        } else if inode.is_file() {
            VnodeType::File
        } else if inode.is_symlink() {
            VnodeType::Symlink
        } else if inode.is_char_device() {
            VnodeType::CharDevice
        } else if inode.is_block_device() {
            VnodeType::BlockDevice
        } else if inode.is_fifo() {
            VnodeType::Fifo
        } else if inode.is_socket() {
            VnodeType::Socket
        } else {
            VnodeType::File
        }
    }
}

impl VnodeOps for Ext4Vnode {
    fn vtype(&self) -> VnodeType {
        self.get_vtype()
    }

    fn lookup(&self, path: &str) -> VfsResult<Arc<dyn VnodeOps>> {
        // Handle paths with multiple components (e.g., "sbin/init" or "/sbin/init")
        let path = path.trim_start_matches('/');

        // Split path into components
        let mut components = path.split('/').filter(|s| !s.is_empty()).peekable();

        // Get the first component
        let first = match components.next() {
            Some(name) => name,
            None => return Err(VfsError::NotFound), // Empty path
        };

        // Look up the first component in this directory
        let inode = self.inode.read();
        if !inode.is_dir() {
            return Err(VfsError::NotDirectory);
        }

        let fs = self.fs.read();

        let child_ino = dir::lookup(fs.device(), &fs.sb, &fs.group_table, &inode, first)
            .map_err(|e| VfsError::from(e))?
            .ok_or(VfsError::NotFound)?;

        let child_inode = read_inode(fs.device(), &fs.sb, &fs.group_table, child_ino)
            .map_err(|e| VfsError::from(e))?;

        drop(fs);
        drop(inode);

        let mut current: Arc<dyn VnodeOps> =
            Arc::new(Ext4Vnode::new(self.fs.clone(), child_ino, child_inode));

        // Recursively look up remaining path components
        for component in components {
            current = current.lookup(component)?;
        }

        Ok(current)
    }

    fn create(&self, name: &str, mode: Mode) -> VfsResult<Arc<dyn VnodeOps>> {
        let mut inode = self.inode.write();
        if !inode.is_dir() {
            return Err(VfsError::NotDirectory);
        }

        let fs = self.fs.read();
        if fs.read_only {
            return Err(VfsError::ReadOnly);
        }

        // Check if file already exists
        if dir::lookup(fs.device(), &fs.sb, &fs.group_table, &inode, name)
            .map_err(|e| VfsError::from(e))?
            .is_some()
        {
            return Err(VfsError::AlreadyExists);
        }

        // Start journal transaction
        let has_journal = fs.begin_transaction();

        // Determine group for allocation (same as parent directory)
        let parent_group = (self.ino - 1) / fs.sb.s_inodes_per_group;

        // Allocate inode
        let new_ino = match bitmap::alloc_inode(
            fs.device(),
            &fs.sb,
            &fs.group_table,
            Some(parent_group),
            false, // not a directory
        ) {
            Ok(Some(ino)) => ino,
            Ok(None) => {
                if has_journal {
                    fs.abort_transaction();
                }
                return Err(VfsError::NoSpace);
            }
            Err(e) => {
                if has_journal {
                    fs.abort_transaction();
                }
                return Err(VfsError::from(e));
            }
        };

        // Create new inode structure
        let inode_mode = file_type::S_IFREG | (mode.bits() as u16 & 0o7777);
        let mut new_inode = inode::new_inode(inode_mode, 0, 0); // TODO: use real uid/gid

        // Initialize extent header for new file
        inode::init_extent_header(&mut new_inode);

        // Write the new inode to disk (and journal if available)
        match inode::write_inode_data(fs.device(), &fs.sb, &fs.group_table, new_ino, &new_inode) {
            Ok((blocknr, data)) => {
                fs.journal_block(blocknr, data);
            }
            Err(e) => {
                if has_journal {
                    fs.abort_transaction();
                }
                return Err(VfsError::from(e));
            }
        }

        // Add directory entry
        let entry_file_type = dir::mode_to_file_type(inode_mode);
        if let Err(e) = dir::add_entry(
            fs.device(),
            &fs.sb,
            &fs.group_table,
            &mut inode,
            name,
            new_ino,
            entry_file_type,
        ) {
            if has_journal {
                fs.abort_transaction();
            }
            return Err(VfsError::from(e));
        }

        // Write updated parent directory inode (and journal if available)
        match inode::write_inode_data(fs.device(), &fs.sb, &fs.group_table, self.ino, &inode) {
            Ok((blocknr, data)) => {
                fs.journal_block(blocknr, data);
            }
            Err(e) => {
                if has_journal {
                    fs.abort_transaction();
                }
                return Err(VfsError::from(e));
            }
        }

        // Commit journal transaction
        if has_journal {
            if !fs.commit_transaction() {
                // Journal commit failed - filesystem may be inconsistent
                return Err(VfsError::IoError);
            }
        }

        drop(fs);

        Ok(Arc::new(Ext4Vnode::new(
            self.fs.clone(),
            new_ino,
            new_inode,
        )))
    }

    fn read(&self, offset: u64, buf: &mut [u8]) -> VfsResult<usize> {
        let inode = self.inode.read();
        if inode.is_dir() {
            return Err(VfsError::IsDirectory);
        }

        let fs = self.fs.read();
        file::read_file(fs.device(), &fs.sb, &inode, offset, buf).map_err(|e| VfsError::from(e))
    }

    fn write(&self, offset: u64, buf: &[u8]) -> VfsResult<usize> {
        let mut inode = self.inode.write();
        if inode.is_dir() {
            return Err(VfsError::IsDirectory);
        }

        let fs = self.fs.read();
        if fs.read_only {
            return Err(VfsError::ReadOnly);
        }

        // Start journal transaction (for metadata - data writes use ordered mode)
        let has_journal = fs.begin_transaction();

        // Write the data
        let bytes_written = match file::write_file(
            fs.device(),
            &fs.sb,
            &fs.group_table,
            &mut inode,
            offset,
            buf,
        ) {
            Ok(n) => n,
            Err(e) => {
                if has_journal {
                    fs.abort_transaction();
                }
                return Err(VfsError::from(e));
            }
        };

        // Update inode on disk (and journal the metadata)
        match inode::write_inode_data(fs.device(), &fs.sb, &fs.group_table, self.ino, &inode) {
            Ok((blocknr, data)) => fs.journal_block(blocknr, data),
            Err(e) => {
                if has_journal {
                    fs.abort_transaction();
                }
                return Err(VfsError::from(e));
            }
        }

        // Commit journal transaction
        if has_journal && !fs.commit_transaction() {
            return Err(VfsError::IoError);
        }

        Ok(bytes_written)
    }

    fn readdir(&self, offset: u64) -> VfsResult<Option<DirEntry>> {
        let inode = self.inode.read();
        if !inode.is_dir() {
            return Err(VfsError::NotDirectory);
        }

        let fs = self.fs.read();

        match dir::readdir_at(fs.device(), &fs.sb, &fs.group_table, &inode, offset)
            .map_err(|e| VfsError::from(e))?
        {
            Some((entry, _next_offset)) => Ok(Some(DirEntry {
                name: entry.name,
                ino: entry.inode as u64,
                file_type: dir::file_type_to_vnode_type(entry.file_type),
            })),
            None => Ok(None),
        }
    }

    fn mkdir(&self, name: &str, mode: Mode) -> VfsResult<Arc<dyn VnodeOps>> {
        let mut inode = self.inode.write();
        if !inode.is_dir() {
            return Err(VfsError::NotDirectory);
        }

        let fs = self.fs.read();
        if fs.read_only {
            return Err(VfsError::ReadOnly);
        }

        // Check if already exists
        if dir::lookup(fs.device(), &fs.sb, &fs.group_table, &inode, name)
            .map_err(|e| VfsError::from(e))?
            .is_some()
        {
            return Err(VfsError::AlreadyExists);
        }

        // Start journal transaction
        let has_journal = fs.begin_transaction();

        // Allocate inode for new directory
        let parent_group = (self.ino - 1) / fs.sb.s_inodes_per_group;
        let new_ino = match bitmap::alloc_inode(
            fs.device(),
            &fs.sb,
            &fs.group_table,
            Some(parent_group),
            true, // is a directory
        ) {
            Ok(Some(ino)) => ino,
            Ok(None) => {
                if has_journal {
                    fs.abort_transaction();
                }
                return Err(VfsError::NoSpace);
            }
            Err(e) => {
                if has_journal {
                    fs.abort_transaction();
                }
                return Err(VfsError::from(e));
            }
        };

        // Allocate a block for the new directory
        let new_block =
            match bitmap::alloc_block(fs.device(), &fs.sb, &fs.group_table, Some(parent_group)) {
                Ok(Some(blk)) => blk,
                Ok(None) => {
                    if has_journal {
                        fs.abort_transaction();
                    }
                    return Err(VfsError::NoSpace);
                }
                Err(e) => {
                    if has_journal {
                        fs.abort_transaction();
                    }
                    return Err(VfsError::from(e));
                }
            };

        // Create new directory inode
        let inode_mode = file_type::S_IFDIR | (mode.bits() as u16 & 0o7777);
        let mut new_inode = inode::new_inode(inode_mode, 0, 0);

        // Initialize extent header and add extent for the directory block
        inode::init_extent_header(&mut new_inode);
        if let Err(e) = crate::extent::insert_extent(&mut new_inode, 0, new_block, 1) {
            if has_journal {
                fs.abort_transaction();
            }
            return Err(VfsError::from(e));
        }

        // Set directory size and block count
        new_inode.set_size(fs.sb.block_size());
        new_inode.set_blocks(fs.sb.block_size() / 512);
        new_inode.i_links_count = 2; // . and parent's link

        // Initialize the directory block with . and ..
        if let Err(e) = dir::init_directory(fs.device(), &fs.sb, new_block, new_ino, self.ino) {
            if has_journal {
                fs.abort_transaction();
            }
            return Err(VfsError::from(e));
        }

        // Write the new directory inode (and journal)
        match inode::write_inode_data(fs.device(), &fs.sb, &fs.group_table, new_ino, &new_inode) {
            Ok((blocknr, data)) => fs.journal_block(blocknr, data),
            Err(e) => {
                if has_journal {
                    fs.abort_transaction();
                }
                return Err(VfsError::from(e));
            }
        }

        // Add entry to parent directory
        if let Err(e) = dir::add_entry(
            fs.device(),
            &fs.sb,
            &fs.group_table,
            &mut inode,
            name,
            new_ino,
            dir::file_type::DIR,
        ) {
            if has_journal {
                fs.abort_transaction();
            }
            return Err(VfsError::from(e));
        }

        // Update parent directory link count (for ..)
        inode.inc_links();

        // Write updated parent inode (and journal)
        match inode::write_inode_data(fs.device(), &fs.sb, &fs.group_table, self.ino, &inode) {
            Ok((blocknr, data)) => fs.journal_block(blocknr, data),
            Err(e) => {
                if has_journal {
                    fs.abort_transaction();
                }
                return Err(VfsError::from(e));
            }
        }

        // Commit journal transaction
        if has_journal && !fs.commit_transaction() {
            return Err(VfsError::IoError);
        }

        drop(fs);

        Ok(Arc::new(Ext4Vnode::new(
            self.fs.clone(),
            new_ino,
            new_inode,
        )))
    }

    fn rmdir(&self, name: &str) -> VfsResult<()> {
        let mut parent_inode = self.inode.write();
        if !parent_inode.is_dir() {
            return Err(VfsError::NotDirectory);
        }

        let fs = self.fs.read();
        if fs.read_only {
            return Err(VfsError::ReadOnly);
        }

        // Can't remove . or ..
        if name == "." || name == ".." {
            return Err(VfsError::InvalidArgument);
        }

        // Look up the target directory
        let target_ino = dir::lookup(fs.device(), &fs.sb, &fs.group_table, &parent_inode, name)
            .map_err(|e| VfsError::from(e))?
            .ok_or(VfsError::NotFound)?;

        // Read the target inode
        let target_inode = read_inode(fs.device(), &fs.sb, &fs.group_table, target_ino)
            .map_err(|e| VfsError::from(e))?;

        // Must be a directory
        if !target_inode.is_dir() {
            return Err(VfsError::NotDirectory);
        }

        // Directory must be empty
        if !dir::is_empty(fs.device(), &fs.sb, &fs.group_table, &target_inode)
            .map_err(|e| VfsError::from(e))?
        {
            return Err(VfsError::NotEmpty);
        }

        // Start journal transaction
        let has_journal = fs.begin_transaction();

        // Remove the directory entry from parent
        if let Err(e) = dir::remove_entry(fs.device(), &fs.sb, &parent_inode, name) {
            if has_journal {
                fs.abort_transaction();
            }
            return Err(VfsError::from(e));
        }

        // Free the directory's blocks
        let block_size = fs.sb.block_size();
        let num_blocks = (target_inode.size() + block_size - 1) / block_size;
        for logical in 0..num_blocks {
            if let Some(phys) =
                crate::extent::map_block(fs.device(), &fs.sb, &target_inode, logical)
                    .map_err(|e| VfsError::from(e))?
            {
                if let Err(e) = bitmap::free_block(fs.device(), &fs.sb, &fs.group_table, phys) {
                    if has_journal {
                        fs.abort_transaction();
                    }
                    return Err(VfsError::from(e));
                }
            }
        }

        // Free the inode
        if let Err(e) = bitmap::free_inode(fs.device(), &fs.sb, &fs.group_table, target_ino) {
            if has_journal {
                fs.abort_transaction();
            }
            return Err(VfsError::from(e));
        }

        // Decrement parent link count (for the removed ..)
        parent_inode.dec_links();

        // Write updated parent inode (and journal)
        match inode::write_inode_data(
            fs.device(),
            &fs.sb,
            &fs.group_table,
            self.ino,
            &parent_inode,
        ) {
            Ok((blocknr, data)) => fs.journal_block(blocknr, data),
            Err(e) => {
                if has_journal {
                    fs.abort_transaction();
                }
                return Err(VfsError::from(e));
            }
        }

        // Commit journal transaction
        if has_journal && !fs.commit_transaction() {
            return Err(VfsError::IoError);
        }

        Ok(())
    }

    fn unlink(&self, name: &str) -> VfsResult<()> {
        let parent_inode = self.inode.read();
        if !parent_inode.is_dir() {
            return Err(VfsError::NotDirectory);
        }

        let fs = self.fs.read();
        if fs.read_only {
            return Err(VfsError::ReadOnly);
        }

        // Can't unlink . or ..
        if name == "." || name == ".." {
            return Err(VfsError::InvalidArgument);
        }

        // Look up the target
        let target_ino = dir::lookup(fs.device(), &fs.sb, &fs.group_table, &parent_inode, name)
            .map_err(|e| VfsError::from(e))?
            .ok_or(VfsError::NotFound)?;

        // Read the target inode
        let mut target_inode = read_inode(fs.device(), &fs.sb, &fs.group_table, target_ino)
            .map_err(|e| VfsError::from(e))?;

        // Can't unlink directories (use rmdir)
        if target_inode.is_dir() {
            return Err(VfsError::IsDirectory);
        }

        // Start journal transaction
        let has_journal = fs.begin_transaction();

        // Remove the directory entry
        if let Err(e) = dir::remove_entry(fs.device(), &fs.sb, &parent_inode, name) {
            if has_journal {
                fs.abort_transaction();
            }
            return Err(VfsError::from(e));
        }

        // Decrement link count
        target_inode.dec_links();

        if target_inode.i_links_count == 0 {
            // No more links - free the file's blocks and inode
            let block_size = fs.sb.block_size();
            let num_blocks = (target_inode.size() + block_size - 1) / block_size;

            for logical in 0..num_blocks {
                if let Some(phys) =
                    crate::extent::map_block(fs.device(), &fs.sb, &target_inode, logical)
                        .map_err(|e| VfsError::from(e))?
                {
                    if let Err(e) = bitmap::free_block(fs.device(), &fs.sb, &fs.group_table, phys) {
                        if has_journal {
                            fs.abort_transaction();
                        }
                        return Err(VfsError::from(e));
                    }
                }
            }

            // Mark inode as deleted (set dtime)
            target_inode.set_dtime(0); // TODO: use real time

            // Free the inode
            if let Err(e) = bitmap::free_inode(fs.device(), &fs.sb, &fs.group_table, target_ino) {
                if has_journal {
                    fs.abort_transaction();
                }
                return Err(VfsError::from(e));
            }
        }

        // Write the updated target inode (and journal)
        match inode::write_inode_data(
            fs.device(),
            &fs.sb,
            &fs.group_table,
            target_ino,
            &target_inode,
        ) {
            Ok((blocknr, data)) => fs.journal_block(blocknr, data),
            Err(e) => {
                if has_journal {
                    fs.abort_transaction();
                }
                return Err(VfsError::from(e));
            }
        }

        // Commit journal transaction
        if has_journal && !fs.commit_transaction() {
            return Err(VfsError::IoError);
        }

        Ok(())
    }

    fn rename(&self, old_name: &str, _new_dir: &dyn VnodeOps, new_name: &str) -> VfsResult<()> {
        // For now, only support rename within the same directory
        // Cross-directory rename would require downcasting new_dir to Ext4Vnode
        let mut inode = self.inode.write();
        if !inode.is_dir() {
            return Err(VfsError::NotDirectory);
        }

        let fs = self.fs.read();
        if fs.read_only {
            return Err(VfsError::ReadOnly);
        }

        // Look up the source
        let source_ino = dir::lookup(fs.device(), &fs.sb, &fs.group_table, &inode, old_name)
            .map_err(|e| VfsError::from(e))?
            .ok_or(VfsError::NotFound)?;

        let source_inode = read_inode(fs.device(), &fs.sb, &fs.group_table, source_ino)
            .map_err(|e| VfsError::from(e))?;

        // Start journal transaction
        let has_journal = fs.begin_transaction();

        // Check if destination exists
        if let Some(dest_ino) = dir::lookup(fs.device(), &fs.sb, &fs.group_table, &inode, new_name)
            .map_err(|e| VfsError::from(e))?
        {
            // Destination exists - need to unlink it first
            let dest_inode = read_inode(fs.device(), &fs.sb, &fs.group_table, dest_ino)
                .map_err(|e| VfsError::from(e))?;

            // Can't overwrite directory with file or vice versa
            if source_inode.is_dir() != dest_inode.is_dir() {
                if has_journal {
                    fs.abort_transaction();
                }
                if dest_inode.is_dir() {
                    return Err(VfsError::IsDirectory);
                } else {
                    return Err(VfsError::NotDirectory);
                }
            }

            // Remove the destination entry
            if let Err(e) = dir::remove_entry(fs.device(), &fs.sb, &inode, new_name) {
                if has_journal {
                    fs.abort_transaction();
                }
                return Err(VfsError::from(e));
            }

            // Free destination inode if no more links
            if let Err(e) = bitmap::free_inode(fs.device(), &fs.sb, &fs.group_table, dest_ino) {
                if has_journal {
                    fs.abort_transaction();
                }
                return Err(VfsError::from(e));
            }
        }

        // Remove old entry
        if let Err(e) = dir::remove_entry(fs.device(), &fs.sb, &inode, old_name) {
            if has_journal {
                fs.abort_transaction();
            }
            return Err(VfsError::from(e));
        }

        // Add new entry
        let entry_file_type = dir::mode_to_file_type(source_inode.i_mode);
        if let Err(e) = dir::add_entry(
            fs.device(),
            &fs.sb,
            &fs.group_table,
            &mut inode,
            new_name,
            source_ino,
            entry_file_type,
        ) {
            if has_journal {
                fs.abort_transaction();
            }
            return Err(VfsError::from(e));
        }

        // Write updated directory inode (and journal)
        match inode::write_inode_data(fs.device(), &fs.sb, &fs.group_table, self.ino, &inode) {
            Ok((blocknr, data)) => fs.journal_block(blocknr, data),
            Err(e) => {
                if has_journal {
                    fs.abort_transaction();
                }
                return Err(VfsError::from(e));
            }
        }

        // Commit journal transaction
        if has_journal && !fs.commit_transaction() {
            return Err(VfsError::IoError);
        }

        Ok(())
    }

    fn stat(&self) -> VfsResult<Stat> {
        let inode = self.inode.read();

        let vtype = self.get_vtype();
        let mode = Mode::new(inode.permissions() as u32);
        let size = inode.size();

        let mut stat = Stat::new(vtype, mode, size, self.ino as u64);
        stat.uid = inode.uid();
        stat.gid = inode.gid();
        stat.nlink = inode.i_links_count as u64;
        stat.blocks = inode.blocks();
        stat.atime = inode.i_atime as u64;
        stat.mtime = inode.i_mtime as u64;
        stat.ctime = inode.i_ctime as u64;

        // Get block size from filesystem
        let fs = self.fs.read();
        stat.blksize = fs.sb.block_size();

        Ok(stat)
    }

    fn truncate(&self, size: u64) -> VfsResult<()> {
        let mut inode = self.inode.write();
        if inode.is_dir() {
            return Err(VfsError::IsDirectory);
        }

        let fs = self.fs.read();
        if fs.read_only {
            return Err(VfsError::ReadOnly);
        }

        // Start journal transaction
        let has_journal = fs.begin_transaction();

        // Truncate the file
        if let Err(e) = file::truncate_file(fs.device(), &fs.sb, &fs.group_table, &mut inode, size)
        {
            if has_journal {
                fs.abort_transaction();
            }
            return Err(VfsError::from(e));
        }

        // Write updated inode (and journal)
        match inode::write_inode_data(fs.device(), &fs.sb, &fs.group_table, self.ino, &inode) {
            Ok((blocknr, data)) => fs.journal_block(blocknr, data),
            Err(e) => {
                if has_journal {
                    fs.abort_transaction();
                }
                return Err(VfsError::from(e));
            }
        }

        // Commit journal transaction
        if has_journal && !fs.commit_transaction() {
            return Err(VfsError::IoError);
        }

        Ok(())
    }

    fn readlink(&self) -> VfsResult<String> {
        let inode = self.inode.read();
        if !inode.is_symlink() {
            return Err(VfsError::InvalidArgument);
        }

        let fs = self.fs.read();
        file::read_symlink(fs.device(), &fs.sb, &inode).map_err(|e| VfsError::from(e))
    }
}
