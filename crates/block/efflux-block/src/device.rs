//! Block device trait and info structures

use crate::BlockResult;

/// Block device information
#[derive(Debug, Clone)]
pub struct BlockDeviceInfo {
    /// Device name
    pub name: &'static str,
    /// Block size in bytes (typically 512 or 4096)
    pub block_size: u32,
    /// Total number of blocks
    pub block_count: u64,
    /// Is device read-only?
    pub read_only: bool,
    /// Is device removable?
    pub removable: bool,
    /// Model/description string
    pub model: &'static str,
}

impl BlockDeviceInfo {
    /// Get total size in bytes
    pub fn size_bytes(&self) -> u64 {
        self.block_count * self.block_size as u64
    }
}

/// Block device trait
///
/// All block devices (disks, partitions, etc.) implement this trait.
pub trait BlockDevice: Send + Sync {
    /// Read blocks from device
    ///
    /// # Arguments
    /// * `start_block` - First block to read
    /// * `buf` - Buffer to read into (must be multiple of block_size)
    ///
    /// # Returns
    /// Number of bytes read, or error
    fn read(&self, start_block: u64, buf: &mut [u8]) -> BlockResult<usize>;

    /// Write blocks to device
    ///
    /// # Arguments
    /// * `start_block` - First block to write
    /// * `buf` - Data to write (must be multiple of block_size)
    ///
    /// # Returns
    /// Number of bytes written, or error
    fn write(&self, start_block: u64, buf: &[u8]) -> BlockResult<usize>;

    /// Flush pending writes to device
    fn flush(&self) -> BlockResult<()>;

    /// Get block size in bytes
    fn block_size(&self) -> u32;

    /// Get total number of blocks
    fn block_count(&self) -> u64;

    /// Get device info
    fn info(&self) -> BlockDeviceInfo;

    /// Check if device is read-only
    fn is_read_only(&self) -> bool {
        false
    }

    /// Read a single block
    fn read_block(&self, block: u64, buf: &mut [u8]) -> BlockResult<()> {
        let block_size = self.block_size() as usize;
        if buf.len() < block_size {
            return Err(crate::BlockError::BufferTooSmall);
        }
        self.read(block, &mut buf[..block_size])?;
        Ok(())
    }

    /// Write a single block
    fn write_block(&self, block: u64, buf: &[u8]) -> BlockResult<()> {
        let block_size = self.block_size() as usize;
        if buf.len() < block_size {
            return Err(crate::BlockError::BufferTooSmall);
        }
        self.write(block, &buf[..block_size])?;
        Ok(())
    }
}

/// A RAM-backed block device for testing
pub struct RamDisk {
    /// Block size
    block_size: u32,
    /// Number of blocks
    block_count: u64,
    /// Storage
    data: spin::Mutex<alloc::vec::Vec<u8>>,
}

impl RamDisk {
    /// Create a new RAM disk
    pub fn new(block_size: u32, block_count: u64) -> Self {
        let size = (block_size as u64 * block_count) as usize;
        RamDisk {
            block_size,
            block_count,
            data: spin::Mutex::new(alloc::vec![0u8; size]),
        }
    }
}

impl BlockDevice for RamDisk {
    fn read(&self, start_block: u64, buf: &mut [u8]) -> BlockResult<usize> {
        let bs = self.block_size as usize;
        let offset = start_block as usize * bs;
        let data = self.data.lock();

        if offset + buf.len() > data.len() {
            return Err(crate::BlockError::InvalidBlock);
        }

        buf.copy_from_slice(&data[offset..offset + buf.len()]);
        Ok(buf.len())
    }

    fn write(&self, start_block: u64, buf: &[u8]) -> BlockResult<usize> {
        let bs = self.block_size as usize;
        let offset = start_block as usize * bs;
        let mut data = self.data.lock();

        if offset + buf.len() > data.len() {
            return Err(crate::BlockError::InvalidBlock);
        }

        data[offset..offset + buf.len()].copy_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&self) -> BlockResult<()> {
        Ok(())
    }

    fn block_size(&self) -> u32 {
        self.block_size
    }

    fn block_count(&self) -> u64 {
        self.block_count
    }

    fn info(&self) -> BlockDeviceInfo {
        BlockDeviceInfo {
            name: "ramdisk",
            block_size: self.block_size,
            block_count: self.block_count,
            read_only: false,
            removable: false,
            model: "RAM Disk",
        }
    }
}
