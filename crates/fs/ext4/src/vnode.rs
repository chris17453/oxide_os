//! ext4 VnodeOps implementation

use alloc::string::String;
use alloc::sync::Arc;
use spin::RwLock;

use block::BlockDevice;
use vfs::{DirEntry, Mode, Stat, VfsError, VfsResult, VnodeOps, VnodeType};

use crate::dir;
use crate::error::Ext4Error;
use crate::file;
use crate::group_desc::BlockGroupTable;
use crate::inode::{self, read_inode, Ext4Inode};
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
}

impl Ext4Fs {
    /// Get the block device
    pub fn device(&self) -> &dyn BlockDevice {
        &*self.device
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

    fn lookup(&self, name: &str) -> VfsResult<Arc<dyn VnodeOps>> {
        let inode = self.inode.read();
        if !inode.is_dir() {
            return Err(VfsError::NotDirectory);
        }

        let fs = self.fs.read();

        // Look up the name in the directory
        let child_ino = dir::lookup(fs.device(), &fs.sb, &fs.group_table, &inode, name)
            .map_err(|e| VfsError::from(e))?
            .ok_or(VfsError::NotFound)?;

        // Read the child inode
        let child_inode = read_inode(fs.device(), &fs.sb, &fs.group_table, child_ino)
            .map_err(|e| VfsError::from(e))?;

        drop(fs);

        Ok(Arc::new(Ext4Vnode::new(self.fs.clone(), child_ino, child_inode)))
    }

    fn create(&self, _name: &str, _mode: Mode) -> VfsResult<Arc<dyn VnodeOps>> {
        let fs = self.fs.read();
        if fs.read_only {
            return Err(VfsError::ReadOnly);
        }
        // Write support not yet implemented
        Err(VfsError::NotSupported)
    }

    fn read(&self, offset: u64, buf: &mut [u8]) -> VfsResult<usize> {
        let inode = self.inode.read();
        if inode.is_dir() {
            return Err(VfsError::IsDirectory);
        }

        let fs = self.fs.read();
        file::read_file(fs.device(), &fs.sb, &inode, offset, buf)
            .map_err(|e| VfsError::from(e))
    }

    fn write(&self, _offset: u64, _buf: &[u8]) -> VfsResult<usize> {
        let fs = self.fs.read();
        if fs.read_only {
            return Err(VfsError::ReadOnly);
        }
        // Write support not yet implemented
        Err(VfsError::NotSupported)
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

    fn mkdir(&self, _name: &str, _mode: Mode) -> VfsResult<Arc<dyn VnodeOps>> {
        let fs = self.fs.read();
        if fs.read_only {
            return Err(VfsError::ReadOnly);
        }
        Err(VfsError::NotSupported)
    }

    fn rmdir(&self, _name: &str) -> VfsResult<()> {
        let fs = self.fs.read();
        if fs.read_only {
            return Err(VfsError::ReadOnly);
        }
        Err(VfsError::NotSupported)
    }

    fn unlink(&self, _name: &str) -> VfsResult<()> {
        let fs = self.fs.read();
        if fs.read_only {
            return Err(VfsError::ReadOnly);
        }
        Err(VfsError::NotSupported)
    }

    fn rename(&self, _old_name: &str, _new_dir: &dyn VnodeOps, _new_name: &str) -> VfsResult<()> {
        let fs = self.fs.read();
        if fs.read_only {
            return Err(VfsError::ReadOnly);
        }
        Err(VfsError::NotSupported)
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

    fn truncate(&self, _size: u64) -> VfsResult<()> {
        let fs = self.fs.read();
        if fs.read_only {
            return Err(VfsError::ReadOnly);
        }
        Err(VfsError::NotSupported)
    }

    fn readlink(&self) -> VfsResult<String> {
        let inode = self.inode.read();
        if !inode.is_symlink() {
            return Err(VfsError::InvalidArgument);
        }

        let fs = self.fs.read();
        file::read_symlink(fs.device(), &fs.sb, &inode)
            .map_err(|e| VfsError::from(e))
    }
}
