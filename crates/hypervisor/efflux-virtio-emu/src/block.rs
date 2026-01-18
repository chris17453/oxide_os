//! virtio-blk Emulation

use alloc::boxed::Box;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::Mutex;

use efflux_vmm::device::VirtioDevice;
use crate::VirtioDeviceBase;

/// virtio-blk device type
pub const VIRTIO_BLK_DEVICE_TYPE: u32 = 2;

/// Block features
pub mod features {
    /// Maximum size of any single segment is in size_max
    pub const VIRTIO_BLK_F_SIZE_MAX: u64 = 1 << 1;
    /// Maximum number of segments in a request is in seg_max
    pub const VIRTIO_BLK_F_SEG_MAX: u64 = 1 << 2;
    /// Disk-style geometry specified in geometry
    pub const VIRTIO_BLK_F_GEOMETRY: u64 = 1 << 4;
    /// Device is read-only
    pub const VIRTIO_BLK_F_RO: u64 = 1 << 5;
    /// Block size of disk is in blk_size
    pub const VIRTIO_BLK_F_BLK_SIZE: u64 = 1 << 6;
    /// Flush command supported
    pub const VIRTIO_BLK_F_FLUSH: u64 = 1 << 9;
    /// Topology info available
    pub const VIRTIO_BLK_F_TOPOLOGY: u64 = 1 << 10;
    /// Cache writeback mode
    pub const VIRTIO_BLK_F_CONFIG_WCE: u64 = 1 << 11;
    /// Discard command supported
    pub const VIRTIO_BLK_F_DISCARD: u64 = 1 << 13;
    /// Write zeros command supported
    pub const VIRTIO_BLK_F_WRITE_ZEROES: u64 = 1 << 14;
}

/// Block request types
pub mod request_type {
    pub const VIRTIO_BLK_T_IN: u32 = 0;
    pub const VIRTIO_BLK_T_OUT: u32 = 1;
    pub const VIRTIO_BLK_T_FLUSH: u32 = 4;
    pub const VIRTIO_BLK_T_DISCARD: u32 = 11;
    pub const VIRTIO_BLK_T_WRITE_ZEROES: u32 = 13;
}

/// Block status
pub mod status {
    pub const VIRTIO_BLK_S_OK: u8 = 0;
    pub const VIRTIO_BLK_S_IOERR: u8 = 1;
    pub const VIRTIO_BLK_S_UNSUPP: u8 = 2;
}

/// Block configuration
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct BlockConfig {
    /// Capacity in 512-byte sectors
    pub capacity: u64,
    /// Maximum segment size
    pub size_max: u32,
    /// Maximum number of segments
    pub seg_max: u32,
    /// Geometry - cylinders
    pub cylinders: u16,
    /// Geometry - heads
    pub heads: u8,
    /// Geometry - sectors
    pub sectors: u8,
    /// Block size
    pub blk_size: u32,
    /// Topology - physical block exponent
    pub physical_block_exp: u8,
    /// Topology - alignment offset
    pub alignment_offset: u8,
    /// Topology - minimum I/O size
    pub min_io_size: u16,
    /// Topology - optimal I/O size
    pub opt_io_size: u32,
    /// Writeback mode
    pub writeback: u8,
    /// Unused
    pub unused0: u8,
    /// Number of queues
    pub num_queues: u16,
    /// Max discard sectors
    pub max_discard_sectors: u32,
    /// Max discard segment count
    pub max_discard_seg: u32,
    /// Discard sector alignment
    pub discard_sector_alignment: u32,
    /// Max write zeros sectors
    pub max_write_zeroes_sectors: u32,
    /// Max write zeros segments
    pub max_write_zeroes_seg: u32,
    /// Write zeros may unmap
    pub write_zeroes_may_unmap: u8,
    /// Unused
    pub unused1: [u8; 3],
}

/// Block request header
#[repr(C)]
#[derive(Clone, Copy)]
pub struct BlockRequestHeader {
    /// Request type
    pub request_type: u32,
    /// Reserved
    pub reserved: u32,
    /// Sector number
    pub sector: u64,
}

/// Backend trait for block device storage
pub trait BlockBackend: Send + Sync {
    /// Read sectors
    fn read(&self, sector: u64, buf: &mut [u8]) -> Result<(), ()>;
    /// Write sectors
    fn write(&self, sector: u64, buf: &[u8]) -> Result<(), ()>;
    /// Flush
    fn flush(&self) -> Result<(), ()>;
    /// Get capacity in sectors
    fn capacity(&self) -> u64;
    /// Check if read-only
    fn is_read_only(&self) -> bool;
}

/// Memory-backed block device
pub struct MemoryBlockBackend {
    /// Data
    data: Mutex<Vec<u8>>,
    /// Read-only flag
    read_only: bool,
}

impl MemoryBlockBackend {
    /// Create new memory backend with given size in bytes
    pub fn new(size: usize, read_only: bool) -> Self {
        MemoryBlockBackend {
            data: Mutex::new(alloc::vec![0u8; size]),
            read_only,
        }
    }

    /// Create from existing data
    pub fn from_data(data: Vec<u8>, read_only: bool) -> Self {
        MemoryBlockBackend {
            data: Mutex::new(data),
            read_only,
        }
    }
}

impl BlockBackend for MemoryBlockBackend {
    fn read(&self, sector: u64, buf: &mut [u8]) -> Result<(), ()> {
        let data = self.data.lock();
        let offset = (sector * 512) as usize;
        if offset + buf.len() > data.len() {
            return Err(());
        }
        buf.copy_from_slice(&data[offset..offset + buf.len()]);
        Ok(())
    }

    fn write(&self, sector: u64, buf: &[u8]) -> Result<(), ()> {
        if self.read_only {
            return Err(());
        }
        let mut data = self.data.lock();
        let offset = (sector * 512) as usize;
        if offset + buf.len() > data.len() {
            return Err(());
        }
        data[offset..offset + buf.len()].copy_from_slice(buf);
        Ok(())
    }

    fn flush(&self) -> Result<(), ()> {
        Ok(())
    }

    fn capacity(&self) -> u64 {
        self.data.lock().len() as u64 / 512
    }

    fn is_read_only(&self) -> bool {
        self.read_only
    }
}

/// virtio-blk device
pub struct VirtioBlock {
    /// Base device
    base: VirtioDeviceBase,
    /// Configuration
    config: Mutex<BlockConfig>,
    /// Backend storage
    backend: Box<dyn BlockBackend>,
    /// Statistics - sectors read
    sectors_read: AtomicU64,
    /// Statistics - sectors written
    sectors_written: AtomicU64,
}

impl VirtioBlock {
    /// Create new block device
    pub fn new(backend: Box<dyn BlockBackend>) -> Self {
        let capacity = backend.capacity();
        let read_only = backend.is_read_only();

        let mut features = features::VIRTIO_BLK_F_SIZE_MAX
            | features::VIRTIO_BLK_F_SEG_MAX
            | features::VIRTIO_BLK_F_BLK_SIZE
            | features::VIRTIO_BLK_F_FLUSH;

        if read_only {
            features |= features::VIRTIO_BLK_F_RO;
        }

        let config = BlockConfig {
            capacity,
            size_max: 1 << 20, // 1MB
            seg_max: 128,
            blk_size: 512,
            ..Default::default()
        };

        VirtioBlock {
            base: VirtioDeviceBase::new(VIRTIO_BLK_DEVICE_TYPE, features, 1),
            config: Mutex::new(config),
            backend,
            sectors_read: AtomicU64::new(0),
            sectors_written: AtomicU64::new(0),
        }
    }

    /// Read config
    fn read_config_inner(&self, offset: u64, data: &mut [u8]) {
        let config = self.config.lock();
        let config_bytes = unsafe {
            core::slice::from_raw_parts(
                &*config as *const BlockConfig as *const u8,
                core::mem::size_of::<BlockConfig>(),
            )
        };

        let start = offset as usize;
        let end = (start + data.len()).min(config_bytes.len());
        if start < config_bytes.len() {
            data[..end - start].copy_from_slice(&config_bytes[start..end]);
        }
    }

    /// Process a block request
    fn process_request(&self, header: &BlockRequestHeader, data: &mut [u8]) -> u8 {
        match header.request_type {
            request_type::VIRTIO_BLK_T_IN => {
                // Read
                match self.backend.read(header.sector, data) {
                    Ok(()) => {
                        self.sectors_read.fetch_add((data.len() / 512) as u64, Ordering::Relaxed);
                        status::VIRTIO_BLK_S_OK
                    }
                    Err(()) => status::VIRTIO_BLK_S_IOERR,
                }
            }
            request_type::VIRTIO_BLK_T_OUT => {
                // Write
                match self.backend.write(header.sector, data) {
                    Ok(()) => {
                        self.sectors_written.fetch_add((data.len() / 512) as u64, Ordering::Relaxed);
                        status::VIRTIO_BLK_S_OK
                    }
                    Err(()) => status::VIRTIO_BLK_S_IOERR,
                }
            }
            request_type::VIRTIO_BLK_T_FLUSH => {
                match self.backend.flush() {
                    Ok(()) => status::VIRTIO_BLK_S_OK,
                    Err(()) => status::VIRTIO_BLK_S_IOERR,
                }
            }
            _ => status::VIRTIO_BLK_S_UNSUPP,
        }
    }

    /// Get statistics
    pub fn stats(&self) -> (u64, u64) {
        (
            self.sectors_read.load(Ordering::Relaxed),
            self.sectors_written.load(Ordering::Relaxed),
        )
    }
}

impl VirtioDevice for VirtioBlock {
    fn device_type(&self) -> u32 {
        self.base.device_type()
    }

    fn features(&self) -> u64 {
        self.base.features()
    }

    fn ack_features(&mut self, features: u64) {
        self.base.ack_features(features);
    }

    fn read_config(&self, offset: u64, data: &mut [u8]) {
        if offset < 0x100 {
            self.base.mmio_read(offset, data);
        } else {
            self.read_config_inner(offset - 0x100, data);
        }
    }

    fn write_config(&mut self, offset: u64, data: &[u8]) {
        if offset < 0x100 {
            self.base.mmio_write(offset, data);
        }
        // Block config is mostly read-only
    }

    fn reset(&mut self) {
        self.base.reset();
    }

    fn process_queue(&mut self, _queue: u16) {
        // Process requests from virtqueue
        // Would read descriptor chain, process request, update used ring
    }

    fn is_activated(&self) -> bool {
        self.base.is_activated()
    }
}
