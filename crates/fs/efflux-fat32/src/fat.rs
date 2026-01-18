//! FAT32 File Allocation Table management

use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};

use efflux_block::BlockDevice;
use efflux_vfs::{VfsError, VfsResult};

use crate::bpb::Bpb;

/// FAT entry constants
pub mod fat_entry {
    /// Free cluster
    pub const FREE: u32 = 0x00000000;
    /// Reserved cluster
    pub const RESERVED: u32 = 0x00000001;
    /// Bad cluster
    pub const BAD: u32 = 0x0FFFFFF7;
    /// End of chain marker (minimum value)
    pub const EOC_MIN: u32 = 0x0FFFFFF8;
    /// End of chain marker (typical value)
    pub const EOC: u32 = 0x0FFFFFFF;
}

/// FAT table
pub struct FatTable {
    /// FAT entries (cached in memory)
    entries: Vec<AtomicU32>,
    /// First FAT sector
    first_sector: u64,
    /// FAT size in sectors
    size_sectors: u32,
    /// Bytes per sector
    bytes_per_sector: u16,
    /// Total data clusters
    total_clusters: u32,
    /// Dirty flag
    dirty: core::sync::atomic::AtomicBool,
}

impl FatTable {
    /// Load FAT from disk
    pub fn load(device: &dyn BlockDevice, bpb: &Bpb) -> VfsResult<Self> {
        let first_sector = bpb.first_fat_sector();
        let size_sectors = bpb.fat_size();
        let bytes_per_sector = bpb.bytes_per_sector;

        // Calculate total clusters
        let root_dir_sectors = ((bpb.root_entry_count as u32 * 32) + (bytes_per_sector as u32 - 1))
            / bytes_per_sector as u32;
        let data_sectors = bpb.total_sectors()
            - (bpb.reserved_sectors as u64
                + (bpb.num_fats as u64 * size_sectors as u64)
                + root_dir_sectors as u64);
        let total_clusters = (data_sectors / bpb.sectors_per_cluster as u64) as u32;

        // Read FAT into memory
        let block_size = device.block_size() as usize;
        let fat_bytes = size_sectors as usize * bytes_per_sector as usize;
        let num_entries = fat_bytes / 4;

        let mut entries = Vec::with_capacity(num_entries);

        let sectors_per_block = block_size / bytes_per_sector as usize;
        let mut fat_data = vec![0u8; fat_bytes];

        // Read FAT sector by sector
        for i in 0..size_sectors as usize {
            let sector = first_sector + i as u64;
            let block = sector / sectors_per_block as u64;
            let offset_in_block = (sector % sectors_per_block as u64) as usize * bytes_per_sector as usize;

            let mut block_buf = vec![0u8; block_size];
            device.read(block, &mut block_buf).map_err(|_| VfsError::IoError)?;

            let dest_offset = i * bytes_per_sector as usize;
            fat_data[dest_offset..dest_offset + bytes_per_sector as usize]
                .copy_from_slice(&block_buf[offset_in_block..offset_in_block + bytes_per_sector as usize]);
        }

        // Parse FAT entries
        for i in 0..num_entries {
            let offset = i * 4;
            let entry = u32::from_le_bytes([
                fat_data[offset],
                fat_data[offset + 1],
                fat_data[offset + 2],
                fat_data[offset + 3],
            ]) & 0x0FFFFFFF; // Mask off reserved bits

            entries.push(AtomicU32::new(entry));
        }

        Ok(FatTable {
            entries,
            first_sector,
            size_sectors,
            bytes_per_sector,
            total_clusters,
            dirty: core::sync::atomic::AtomicBool::new(false),
        })
    }

    /// Get FAT entry for a cluster
    pub fn get_entry(&self, cluster: u32) -> VfsResult<u32> {
        if cluster < 2 || cluster as usize >= self.entries.len() {
            return Err(VfsError::InvalidArgument);
        }

        Ok(self.entries[cluster as usize].load(Ordering::SeqCst))
    }

    /// Set FAT entry for a cluster
    pub fn set_entry(&self, cluster: u32, value: u32) -> VfsResult<()> {
        if cluster < 2 || cluster as usize >= self.entries.len() {
            return Err(VfsError::InvalidArgument);
        }

        self.entries[cluster as usize].store(value & 0x0FFFFFFF, Ordering::SeqCst);
        self.dirty.store(true, Ordering::SeqCst);

        Ok(())
    }

    /// Allocate a free cluster
    pub fn alloc_cluster(&self) -> VfsResult<u32> {
        // Search for a free cluster
        for i in 2..self.entries.len() {
            let entry = self.entries[i].load(Ordering::SeqCst);
            if entry == fat_entry::FREE {
                // Try to allocate (CAS)
                if self.entries[i]
                    .compare_exchange(fat_entry::FREE, fat_entry::EOC, Ordering::SeqCst, Ordering::SeqCst)
                    .is_ok()
                {
                    self.dirty.store(true, Ordering::SeqCst);
                    return Ok(i as u32);
                }
            }
        }

        Err(VfsError::NoSpace)
    }

    /// Allocate a chain of clusters
    pub fn alloc_chain(&self, count: u32) -> VfsResult<u32> {
        if count == 0 {
            return Err(VfsError::InvalidArgument);
        }

        let mut allocated = Vec::with_capacity(count as usize);

        for _ in 0..count {
            match self.alloc_cluster() {
                Ok(cluster) => allocated.push(cluster),
                Err(e) => {
                    // Free already allocated clusters
                    for &c in &allocated {
                        let _ = self.set_entry(c, fat_entry::FREE);
                    }
                    return Err(e);
                }
            }
        }

        // Link the chain
        for i in 0..allocated.len() - 1 {
            self.set_entry(allocated[i], allocated[i + 1])?;
        }
        self.set_entry(allocated[allocated.len() - 1], fat_entry::EOC)?;

        Ok(allocated[0])
    }

    /// Extend a cluster chain
    pub fn extend_chain(&self, last_cluster: u32, count: u32) -> VfsResult<u32> {
        let first_new = self.alloc_chain(count)?;
        self.set_entry(last_cluster, first_new)?;
        Ok(first_new)
    }

    /// Free a cluster chain
    pub fn free_chain(&self, start: u32) -> VfsResult<()> {
        let mut cluster = start;

        while cluster >= 2 && cluster < fat_entry::EOC_MIN {
            let next = self.get_entry(cluster)?;
            self.set_entry(cluster, fat_entry::FREE)?;
            cluster = next;
        }

        Ok(())
    }

    /// Count free clusters
    pub fn free_clusters(&self) -> VfsResult<u32> {
        let mut count = 0;

        for i in 2..self.entries.len().min(self.total_clusters as usize + 2) {
            if self.entries[i].load(Ordering::SeqCst) == fat_entry::FREE {
                count += 1;
            }
        }

        Ok(count)
    }

    /// Get total clusters
    pub fn total_clusters(&self) -> u32 {
        self.total_clusters
    }

    /// Check if FAT is dirty
    pub fn is_dirty(&self) -> bool {
        self.dirty.load(Ordering::SeqCst)
    }

    /// Sync FAT to disk
    pub fn sync(&self, device: &dyn BlockDevice) -> VfsResult<()> {
        if !self.is_dirty() {
            return Ok(());
        }

        let block_size = device.block_size() as usize;
        let sectors_per_block = block_size / self.bytes_per_sector as usize;

        // Serialize FAT entries
        let fat_bytes = self.entries.len() * 4;
        let mut fat_data = vec![0u8; fat_bytes];

        for (i, entry) in self.entries.iter().enumerate() {
            let value = entry.load(Ordering::SeqCst);
            let offset = i * 4;
            fat_data[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        }

        // Write FAT sector by sector
        for i in 0..self.size_sectors as usize {
            let sector = self.first_sector + i as u64;
            let block = sector / sectors_per_block as u64;
            let offset_in_block = (sector % sectors_per_block as u64) as usize * self.bytes_per_sector as usize;

            let mut block_buf = vec![0u8; block_size];

            // Read-modify-write for partial blocks
            if (self.bytes_per_sector as usize) < block_size {
                device.read(block, &mut block_buf).map_err(|_| VfsError::IoError)?;
            }

            let src_offset = i * self.bytes_per_sector as usize;
            block_buf[offset_in_block..offset_in_block + self.bytes_per_sector as usize]
                .copy_from_slice(&fat_data[src_offset..src_offset + self.bytes_per_sector as usize]);

            device.write(block, &block_buf).map_err(|_| VfsError::IoError)?;
        }

        self.dirty.store(false, Ordering::SeqCst);

        Ok(())
    }

    /// Check if a cluster value indicates end of chain
    pub fn is_eoc(value: u32) -> bool {
        value >= fat_entry::EOC_MIN
    }

    /// Check if a cluster is free
    pub fn is_free(value: u32) -> bool {
        value == fat_entry::FREE
    }

    /// Check if a cluster is bad
    pub fn is_bad(value: u32) -> bool {
        value == fat_entry::BAD
    }

    /// Get chain length
    pub fn chain_length(&self, start: u32) -> VfsResult<u32> {
        let mut count = 0;
        let mut cluster = start;

        while cluster >= 2 && cluster < fat_entry::EOC_MIN {
            count += 1;
            cluster = self.get_entry(cluster)?;

            // Prevent infinite loops
            if count > self.total_clusters {
                return Err(VfsError::CorruptedFilesystem);
            }
        }

        Ok(count)
    }

    /// Find last cluster in chain
    pub fn find_last(&self, start: u32) -> VfsResult<u32> {
        let mut cluster = start;
        let mut prev = start;
        let mut count = 0;

        while cluster >= 2 && cluster < fat_entry::EOC_MIN {
            prev = cluster;
            cluster = self.get_entry(cluster)?;
            count += 1;

            if count > self.total_clusters {
                return Err(VfsError::CorruptedFilesystem);
            }
        }

        Ok(prev)
    }

    /// Truncate chain at specified cluster
    pub fn truncate_chain(&self, cluster: u32) -> VfsResult<()> {
        let next = self.get_entry(cluster)?;

        // Mark this as end of chain
        self.set_entry(cluster, fat_entry::EOC)?;

        // Free the rest
        if next >= 2 && next < fat_entry::EOC_MIN {
            self.free_chain(next)?;
        }

        Ok(())
    }
}
