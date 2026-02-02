//! Partition abstraction
//!
//! A partition is a view into a portion of a block device.

use alloc::sync::Arc;

use crate::{BlockDevice, BlockDeviceInfo, BlockError, BlockResult};

/// A partition - a view into a region of a block device
pub struct Partition {
    /// The underlying device
    device: Arc<dyn BlockDevice>,
    /// Starting block of the partition
    start_block: u64,
    /// Number of blocks in the partition
    block_count: u64,
    /// Partition number (1-indexed)
    number: u8,
    /// Partition name
    name: &'static str,
}

impl Partition {
    /// Create a new partition view
    pub fn new(
        device: Arc<dyn BlockDevice>,
        start_block: u64,
        block_count: u64,
        number: u8,
        name: &'static str,
    ) -> Self {
        Partition {
            device,
            start_block,
            block_count,
            number,
            name,
        }
    }

    /// Get the partition number
    pub fn number(&self) -> u8 {
        self.number
    }

    /// Get the starting block on the underlying device
    pub fn start(&self) -> u64 {
        self.start_block
    }

    /// Get the underlying device
    pub fn device(&self) -> &Arc<dyn BlockDevice> {
        &self.device
    }
}

impl BlockDevice for Partition {
    fn read(&self, start_block: u64, buf: &mut [u8]) -> BlockResult<usize> {
        // Check bounds
        let blocks_to_read = buf.len() / self.block_size() as usize;
        if start_block + blocks_to_read as u64 > self.block_count {
            return Err(BlockError::InvalidBlock);
        }

        // Translate to device block
        let device_block = self.start_block + start_block;
        self.device.read(device_block, buf)
    }

    fn write(&self, start_block: u64, buf: &[u8]) -> BlockResult<usize> {
        // Check bounds
        let blocks_to_write = buf.len() / self.block_size() as usize;
        if start_block + blocks_to_write as u64 > self.block_count {
            return Err(BlockError::InvalidBlock);
        }

        // Translate to device block
        let device_block = self.start_block + start_block;
        self.device.write(device_block, buf)
    }

    fn flush(&self) -> BlockResult<()> {
        self.device.flush()
    }

    fn block_size(&self) -> u32 {
        self.device.block_size()
    }

    fn block_count(&self) -> u64 {
        self.block_count
    }

    fn info(&self) -> BlockDeviceInfo {
        BlockDeviceInfo {
            name: self.name,
            block_size: self.block_size(),
            block_count: self.block_count,
            read_only: self.device.is_read_only(),
            removable: false,
            model: "Partition",
        }
    }

    fn is_read_only(&self) -> bool {
        self.device.is_read_only()
    }
}
