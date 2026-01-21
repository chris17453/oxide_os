//! OXIDEFS Bitmap for block/inode allocation

use alloc::vec::Vec;

use crate::OxidefsResult;
use block::BlockDevice;

/// Bitmap for tracking allocation
pub struct Bitmap {
    /// Bitmap data
    data: Vec<u8>,
    /// Total number of bits
    size: usize,
    /// First potentially free bit (hint)
    hint: usize,
}

impl Bitmap {
    /// Create a new empty bitmap
    pub fn new(size: usize) -> Self {
        let bytes = (size + 7) / 8;
        Bitmap {
            data: alloc::vec![0u8; bytes],
            size,
            hint: 0,
        }
    }

    /// Load bitmap from disk
    pub fn load(
        device: &dyn BlockDevice,
        start_block: u64,
        num_blocks: usize,
        size: usize,
    ) -> OxidefsResult<Self> {
        let block_size = device.block_size() as usize;
        let mut data = alloc::vec![0u8; num_blocks * block_size];

        for i in 0..num_blocks {
            device.read(start_block + i as u64, &mut data[i * block_size..(i + 1) * block_size])?;
        }

        // Trim to actual size
        let bytes = (size + 7) / 8;
        data.truncate(bytes);

        Ok(Bitmap {
            data,
            size,
            hint: 0,
        })
    }

    /// Save bitmap to disk
    pub fn save(&self, device: &dyn BlockDevice, start_block: u64) -> OxidefsResult<()> {
        let block_size = device.block_size() as usize;
        let num_blocks = (self.data.len() + block_size - 1) / block_size;

        let mut buf = alloc::vec![0u8; num_blocks * block_size];
        buf[..self.data.len()].copy_from_slice(&self.data);

        for i in 0..num_blocks {
            device.write(start_block + i as u64, &buf[i * block_size..(i + 1) * block_size])?;
        }

        Ok(())
    }

    /// Check if a bit is set
    pub fn is_set(&self, index: usize) -> bool {
        if index >= self.size {
            return false;
        }
        let byte = index / 8;
        let bit = index % 8;
        (self.data[byte] & (1 << bit)) != 0
    }

    /// Set a bit
    pub fn set(&mut self, index: usize) {
        if index >= self.size {
            return;
        }
        let byte = index / 8;
        let bit = index % 8;
        self.data[byte] |= 1 << bit;
    }

    /// Clear a bit
    pub fn clear(&mut self, index: usize) {
        if index >= self.size {
            return;
        }
        let byte = index / 8;
        let bit = index % 8;
        self.data[byte] &= !(1 << bit);

        // Update hint
        if index < self.hint {
            self.hint = index;
        }
    }

    /// Find a free bit (returns index)
    pub fn find_free(&mut self) -> Option<usize> {
        // Start from hint
        for i in self.hint..self.size {
            if !self.is_set(i) {
                self.hint = i + 1;
                return Some(i);
            }
        }

        // Wrap around to beginning
        for i in 0..self.hint {
            if !self.is_set(i) {
                self.hint = i + 1;
                return Some(i);
            }
        }

        None
    }

    /// Find N contiguous free bits
    pub fn find_contiguous(&mut self, count: usize) -> Option<usize> {
        let mut start = 0;
        let mut found = 0;

        for i in 0..self.size {
            if !self.is_set(i) {
                if found == 0 {
                    start = i;
                }
                found += 1;
                if found >= count {
                    self.hint = start + count;
                    return Some(start);
                }
            } else {
                found = 0;
            }
        }

        None
    }

    /// Count free bits
    pub fn count_free(&self) -> usize {
        let mut count = 0;
        for i in 0..self.size {
            if !self.is_set(i) {
                count += 1;
            }
        }
        count
    }

    /// Count used bits
    pub fn count_used(&self) -> usize {
        self.size - self.count_free()
    }
}
