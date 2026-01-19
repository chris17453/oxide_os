//! FAT32 Filesystem Driver
//!
//! Implements FAT32 filesystem with long filename support.

#![no_std]

extern crate alloc;

use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};

use block::BlockDevice;
use vfs::{
    DirEntry as VfsDirEntry, Mode, Stat, VfsError, VfsResult, VnodeOps, VnodeType,
};

mod bpb;
mod dir;
mod fat;

pub use bpb::*;
pub use dir::*;
pub use fat::*;

/// FAT32 filesystem
pub struct Fat32 {
    /// Block device
    device: Arc<dyn BlockDevice>,
    /// BIOS Parameter Block
    bpb: Bpb,
    /// FAT table
    fat: FatTable,
    /// Next inode number
    next_ino: AtomicU64,
    /// Root directory first cluster
    root_cluster: u32,
    /// Bytes per cluster
    cluster_size: u32,
    /// First data sector
    first_data_sector: u64,
}

impl Fat32 {
    /// Mount a FAT32 filesystem
    pub fn mount(device: Arc<dyn BlockDevice>) -> VfsResult<Arc<Self>> {
        // Read boot sector
        let block_size = device.block_size() as usize;
        let mut buf = vec![0u8; block_size];
        device.read(0, &mut buf).map_err(|_| VfsError::IoError)?;

        // Parse BPB
        let bpb = Bpb::parse(&buf)?;

        // Verify FAT32
        if !bpb.is_fat32() {
            return Err(VfsError::InvalidFilesystem);
        }

        // Calculate filesystem parameters
        let root_dir_sectors = ((bpb.root_entry_count as u32 * 32) + (bpb.bytes_per_sector as u32 - 1))
            / bpb.bytes_per_sector as u32;

        let fat_size = if bpb.fat_size_16 != 0 {
            bpb.fat_size_16 as u32
        } else {
            bpb.fat32_size
        };

        let first_data_sector = bpb.reserved_sectors as u64
            + (bpb.num_fats as u64 * fat_size as u64)
            + root_dir_sectors as u64;

        let cluster_size = bpb.bytes_per_sector as u32 * bpb.sectors_per_cluster as u32;
        let root_cluster = bpb.root_cluster;

        // Load FAT table
        let fat = FatTable::load(&*device, &bpb)?;

        Ok(Arc::new(Fat32 {
            device,
            bpb,
            fat,
            next_ino: AtomicU64::new(2), // 1 is root
            root_cluster,
            cluster_size,
            first_data_sector,
        }))
    }

    /// Get the root vnode
    pub fn root(self: &Arc<Self>) -> Arc<dyn VnodeOps> {
        Arc::new(Fat32Vnode {
            ino: 1,
            cluster: self.root_cluster,
            size: 0,
            is_dir: true,
            name: String::from("/"),
            fs: Arc::clone(self),
        })
    }

    /// Read a cluster
    fn read_cluster(&self, cluster: u32, buf: &mut [u8]) -> VfsResult<()> {
        if cluster < 2 {
            return Err(VfsError::InvalidArgument);
        }

        let first_sector = self.first_data_sector
            + ((cluster - 2) as u64 * self.bpb.sectors_per_cluster as u64);

        let sector_size = self.bpb.bytes_per_sector as usize;
        let block_size = self.device.block_size() as usize;
        let sectors_per_block = block_size / sector_size;

        for i in 0..self.bpb.sectors_per_cluster as usize {
            let sector = first_sector + i as u64;
            let block = sector / sectors_per_block as u64;
            let offset_in_block = (sector % sectors_per_block as u64) as usize * sector_size;

            let mut block_buf = vec![0u8; block_size];
            self.device.read(block, &mut block_buf).map_err(|_| VfsError::IoError)?;

            let dest_offset = i * sector_size;
            buf[dest_offset..dest_offset + sector_size]
                .copy_from_slice(&block_buf[offset_in_block..offset_in_block + sector_size]);
        }

        Ok(())
    }

    /// Get cluster chain for a file
    fn get_cluster_chain(&self, start: u32) -> VfsResult<Vec<u32>> {
        let mut chain = Vec::new();
        let mut cluster = start;

        while cluster >= 2 && cluster < 0x0FFFFFF8 {
            chain.push(cluster);
            cluster = self.fat.get_entry(cluster)?;

            // Prevent infinite loops
            if chain.len() > 0x10000000 {
                return Err(VfsError::CorruptedFilesystem);
            }
        }

        Ok(chain)
    }

    /// Generate next inode number
    fn next_ino(&self) -> u64 {
        self.next_ino.fetch_add(1, Ordering::SeqCst)
    }

    /// Sync filesystem
    pub fn sync(&self) -> VfsResult<()> {
        self.fat.sync(&*self.device)?;
        self.device.flush().map_err(|_| VfsError::IoError)
    }
}

/// FAT32 vnode
pub struct Fat32Vnode {
    /// Inode number (synthesized)
    ino: u64,
    /// First cluster
    cluster: u32,
    /// File size
    size: u64,
    /// Is directory
    is_dir: bool,
    /// Name
    name: String,
    /// Filesystem reference
    fs: Arc<Fat32>,
}

impl VnodeOps for Fat32Vnode {
    fn vtype(&self) -> VnodeType {
        if self.is_dir {
            VnodeType::Directory
        } else {
            VnodeType::File
        }
    }

    fn lookup(&self, name: &str) -> VfsResult<Arc<dyn VnodeOps>> {
        if !self.is_dir {
            return Err(VfsError::NotDirectory);
        }

        // Read directory entries
        let chain = self.fs.get_cluster_chain(self.cluster)?;
        let cluster_size = self.fs.cluster_size as usize;

        let mut lfn_buffer: Vec<LfnEntry> = Vec::new();

        for cluster in chain {
            let mut buf = vec![0u8; cluster_size];
            self.fs.read_cluster(cluster, &mut buf)?;

            // Iterate directory entries
            for i in (0..cluster_size).step_by(32) {
                let entry_data = &buf[i..i + 32];

                // End of directory
                if entry_data[0] == 0x00 {
                    return Err(VfsError::NotFound);
                }

                // Deleted entry
                if entry_data[0] == 0xE5 {
                    lfn_buffer.clear();
                    continue;
                }

                // LFN entry
                if entry_data[11] == 0x0F {
                    let lfn = LfnEntry::parse(entry_data);
                    lfn_buffer.push(lfn);
                    continue;
                }

                // Regular entry
                let entry = dir::DirEntry::parse(entry_data);

                // Get name (LFN or 8.3)
                let entry_name = if !lfn_buffer.is_empty() {
                    lfn_buffer.reverse();
                    let long_name = LfnEntry::combine(&lfn_buffer);
                    lfn_buffer.clear();
                    long_name
                } else {
                    entry.name_83()
                };

                if entry_name.eq_ignore_ascii_case(name) {
                    let first_cluster = ((entry.first_cluster_hi as u32) << 16)
                        | entry.first_cluster_lo as u32;

                    return Ok(Arc::new(Fat32Vnode {
                        ino: self.fs.next_ino(),
                        cluster: first_cluster,
                        size: entry.size as u64,
                        is_dir: entry.is_directory(),
                        name: entry_name,
                        fs: Arc::clone(&self.fs),
                    }));
                }

                lfn_buffer.clear();
            }
        }

        Err(VfsError::NotFound)
    }

    fn read(&self, offset: u64, buf: &mut [u8]) -> VfsResult<usize> {
        if self.is_dir {
            return Err(VfsError::IsDirectory);
        }

        if offset >= self.size {
            return Ok(0);
        }

        let chain = self.fs.get_cluster_chain(self.cluster)?;
        let cluster_size = self.fs.cluster_size as usize;

        let mut bytes_read = 0;
        let mut current_offset = offset;
        let max_read = buf.len().min((self.size - offset) as usize);

        while bytes_read < max_read {
            let cluster_index = (current_offset / cluster_size as u64) as usize;
            let cluster_offset = (current_offset % cluster_size as u64) as usize;

            if cluster_index >= chain.len() {
                break;
            }

            let mut cluster_buf = vec![0u8; cluster_size];
            self.fs.read_cluster(chain[cluster_index], &mut cluster_buf)?;

            let to_read = (cluster_size - cluster_offset).min(max_read - bytes_read);
            buf[bytes_read..bytes_read + to_read]
                .copy_from_slice(&cluster_buf[cluster_offset..cluster_offset + to_read]);

            bytes_read += to_read;
            current_offset += to_read as u64;
        }

        Ok(bytes_read)
    }

    fn write(&self, _offset: u64, _buf: &[u8]) -> VfsResult<usize> {
        Err(VfsError::ReadOnly)
    }

    fn stat(&self) -> VfsResult<Stat> {
        let vtype = self.vtype();
        let mode = if self.is_dir { Mode::DEFAULT_DIR } else { Mode::DEFAULT_FILE };
        Ok(Stat::new(vtype, mode, self.size, self.ino))
    }

    fn readdir(&self, offset: u64) -> VfsResult<Option<VfsDirEntry>> {
        if !self.is_dir {
            return Err(VfsError::NotDirectory);
        }

        let chain = self.fs.get_cluster_chain(self.cluster)?;
        let cluster_size = self.fs.cluster_size as usize;

        let mut lfn_buffer: Vec<LfnEntry> = Vec::new();
        let mut entry_index = 0u64;

        for cluster in chain {
            let mut buf = vec![0u8; cluster_size];
            self.fs.read_cluster(cluster, &mut buf)?;

            for i in (0..cluster_size).step_by(32) {
                let entry_data = &buf[i..i + 32];

                // End of directory
                if entry_data[0] == 0x00 {
                    return Ok(None);
                }

                // Deleted entry
                if entry_data[0] == 0xE5 {
                    lfn_buffer.clear();
                    continue;
                }

                // LFN entry
                if entry_data[11] == 0x0F {
                    let lfn = LfnEntry::parse(entry_data);
                    lfn_buffer.push(lfn);
                    continue;
                }

                // Regular entry
                let entry = dir::DirEntry::parse(entry_data);

                // Skip volume label
                if entry.is_volume_label() {
                    lfn_buffer.clear();
                    continue;
                }

                if entry_index == offset {
                    let name = if !lfn_buffer.is_empty() {
                        lfn_buffer.reverse();
                        let long_name = LfnEntry::combine(&lfn_buffer);
                        lfn_buffer.clear();
                        long_name
                    } else {
                        entry.name_83()
                    };

                    let first_cluster = ((entry.first_cluster_hi as u32) << 16)
                        | entry.first_cluster_lo as u32;

                    let file_type = if entry.is_directory() {
                        VnodeType::Directory
                    } else {
                        VnodeType::File
                    };

                    // Synthesize inode from cluster
                    let ino = if first_cluster == 0 {
                        self.fs.next_ino()
                    } else {
                        first_cluster as u64
                    };

                    return Ok(Some(VfsDirEntry {
                        name,
                        ino,
                        file_type,
                    }));
                }

                entry_index += 1;
                lfn_buffer.clear();
            }
        }

        Ok(None)
    }

    fn truncate(&self, _size: u64) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }

    fn create(&self, _name: &str, _mode: Mode) -> VfsResult<Arc<dyn VnodeOps>> {
        Err(VfsError::ReadOnly)
    }

    fn mkdir(&self, _name: &str, _mode: Mode) -> VfsResult<Arc<dyn VnodeOps>> {
        Err(VfsError::ReadOnly)
    }

    fn unlink(&self, _name: &str) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }

    fn rmdir(&self, _name: &str) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }

    fn rename(&self, _old_name: &str, _new_dir: &dyn VnodeOps, _new_name: &str) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }
}
