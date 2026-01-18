//! VirtIO Block Device Driver for EFFLUX OS
//!
//! Implements the virtio-blk specification for virtual block devices.

#![no_std]

extern crate alloc;

use alloc::boxed::Box;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::Mutex;

use efflux_block::{BlockDevice, BlockDeviceInfo, BlockError, BlockResult};

/// VirtIO device status
mod status {
    pub const ACKNOWLEDGE: u8 = 1;
    pub const DRIVER: u8 = 2;
    pub const DRIVER_OK: u8 = 4;
    pub const FEATURES_OK: u8 = 8;
    pub const DEVICE_NEEDS_RESET: u8 = 64;
    pub const FAILED: u8 = 128;
}

/// VirtIO block request types
mod req_type {
    pub const IN: u32 = 0;     // Read
    pub const OUT: u32 = 1;    // Write
    pub const FLUSH: u32 = 4;  // Flush
    pub const DISCARD: u32 = 11;
    pub const WRITE_ZEROES: u32 = 13;
}

/// VirtIO block status values
mod blk_status {
    pub const OK: u8 = 0;
    pub const IOERR: u8 = 1;
    pub const UNSUPP: u8 = 2;
}

/// VirtIO feature bits
mod features {
    pub const SIZE_MAX: u64 = 1 << 1;
    pub const SEG_MAX: u64 = 1 << 2;
    pub const GEOMETRY: u64 = 1 << 4;
    pub const RO: u64 = 1 << 5;
    pub const BLK_SIZE: u64 = 1 << 6;
    pub const FLUSH: u64 = 1 << 9;
    pub const TOPOLOGY: u64 = 1 << 10;
    pub const CONFIG_WCE: u64 = 1 << 11;
    pub const DISCARD: u64 = 1 << 13;
    pub const WRITE_ZEROES: u64 = 1 << 14;
}

/// VirtIO block request header
#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct VirtioBlkReqHeader {
    /// Request type (IN, OUT, FLUSH, etc.)
    req_type: u32,
    /// Reserved
    reserved: u32,
    /// Sector number
    sector: u64,
}

/// VirtIO virtqueue descriptor
#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct VirtqDesc {
    /// Physical address of buffer
    addr: u64,
    /// Length of buffer
    len: u32,
    /// Flags (NEXT, WRITE, INDIRECT)
    flags: u16,
    /// Next descriptor index (if NEXT flag set)
    next: u16,
}

/// Virtqueue descriptor flags
mod desc_flags {
    pub const NEXT: u16 = 1;
    pub const WRITE: u16 = 2;
    pub const INDIRECT: u16 = 4;
}

/// VirtIO available ring
#[repr(C)]
struct VirtqAvail {
    flags: u16,
    idx: u16,
    ring: [u16; 256], // Ring entries
    used_event: u16,
}

/// VirtIO used ring element
#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct VirtqUsedElem {
    id: u32,
    len: u32,
}

/// VirtIO used ring
#[repr(C)]
struct VirtqUsed {
    flags: u16,
    idx: u16,
    ring: [VirtqUsedElem; 256],
    avail_event: u16,
}

/// VirtIO block device configuration
#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct VirtioBlkConfig {
    /// Capacity in 512-byte sectors
    capacity: u64,
    /// Size max
    size_max: u32,
    /// Seg max
    seg_max: u32,
    /// Geometry
    geometry_cylinders: u16,
    geometry_heads: u8,
    geometry_sectors: u8,
    /// Block size
    blk_size: u32,
    /// Topology
    physical_block_exp: u8,
    alignment_offset: u8,
    min_io_size: u16,
    opt_io_size: u32,
    /// Writeback mode
    wce: u8,
    _unused: u8,
    /// Number of queues
    num_queues: u16,
    /// Max discard sectors
    max_discard_sectors: u32,
    /// Max discard seg
    max_discard_seg: u32,
    /// Discard sector alignment
    discard_sector_alignment: u32,
    /// Max write zeroes sectors
    max_write_zeroes_sectors: u32,
    /// Max write zeroes seg
    max_write_zeroes_seg: u32,
    /// Write zeroes may unmap
    write_zeroes_may_unmap: u8,
    _unused2: [u8; 3],
}

/// VirtIO block device
pub struct VirtioBlk {
    /// MMIO base address
    mmio_base: u64,
    /// Device configuration
    capacity: u64,
    /// Block size
    block_size: u32,
    /// Read-only flag
    read_only: bool,
    /// Virtqueue descriptors
    desc: Mutex<Vec<VirtqDesc>>,
    /// Free descriptor indices
    free_desc: Mutex<Vec<u16>>,
    /// Request counter
    request_id: AtomicU64,
}

impl VirtioBlk {
    /// Probe for a virtio-blk device at the given MMIO address
    ///
    /// # Safety
    /// The MMIO address must be valid and mapped.
    pub unsafe fn probe(mmio_base: u64) -> Option<Self> {
        unsafe {
            // Read magic value (offset 0x00)
            let magic = core::ptr::read_volatile((mmio_base) as *const u32);
            if magic != 0x74726976 {
                // "virt" in little-endian
                return None;
            }

            // Read device ID (offset 0x08)
            let device_id = core::ptr::read_volatile((mmio_base + 0x08) as *const u32);
            if device_id != 2 {
                // Block device ID
                return None;
            }

            // Initialize device
            // 1. Reset device
            core::ptr::write_volatile((mmio_base + 0x70) as *mut u32, 0);

            // 2. Set ACKNOWLEDGE
            core::ptr::write_volatile((mmio_base + 0x70) as *mut u32, status::ACKNOWLEDGE as u32);

            // 3. Set DRIVER
            let mut status = status::ACKNOWLEDGE | status::DRIVER;
            core::ptr::write_volatile((mmio_base + 0x70) as *mut u32, status as u32);

            // 4. Read features
            core::ptr::write_volatile((mmio_base + 0x14) as *mut u32, 0); // Select page 0
            let features_lo = core::ptr::read_volatile((mmio_base + 0x10) as *const u32);
            core::ptr::write_volatile((mmio_base + 0x14) as *mut u32, 1); // Select page 1
            let features_hi = core::ptr::read_volatile((mmio_base + 0x10) as *const u32);
            let device_features = (features_hi as u64) << 32 | (features_lo as u64);

            // 5. Negotiate features (accept what we support)
            let accepted = device_features & (features::RO | features::BLK_SIZE | features::FLUSH);
            core::ptr::write_volatile((mmio_base + 0x24) as *mut u32, 0);
            core::ptr::write_volatile((mmio_base + 0x20) as *mut u32, accepted as u32);
            core::ptr::write_volatile((mmio_base + 0x24) as *mut u32, 1);
            core::ptr::write_volatile((mmio_base + 0x20) as *mut u32, (accepted >> 32) as u32);

            // 6. Set FEATURES_OK
            status |= status::FEATURES_OK;
            core::ptr::write_volatile((mmio_base + 0x70) as *mut u32, status as u32);

            // 7. Check FEATURES_OK was accepted
            let status_read = core::ptr::read_volatile((mmio_base + 0x70) as *const u32);
            if status_read & (status::FEATURES_OK as u32) == 0 {
                return None;
            }

            // 8. Read device configuration
            let capacity = core::ptr::read_volatile((mmio_base + 0x100) as *const u64);

            let block_size = if device_features & features::BLK_SIZE != 0 {
                core::ptr::read_volatile((mmio_base + 0x114) as *const u32)
            } else {
                512
            };

            let read_only = device_features & features::RO != 0;

            // 9. Setup virtqueue (queue 0)
            // ... virtqueue setup is complex, simplified here

            // 10. Set DRIVER_OK
            status |= status::DRIVER_OK;
            core::ptr::write_volatile((mmio_base + 0x70) as *mut u32, status as u32);

            // Create descriptor pool
            let desc = Vec::with_capacity(256);
            let mut free_desc = Vec::with_capacity(256);
            for i in (0..256).rev() {
                free_desc.push(i);
            }

            Some(VirtioBlk {
                mmio_base,
                capacity,
                block_size,
                read_only,
                desc: Mutex::new(desc),
                free_desc: Mutex::new(free_desc),
                request_id: AtomicU64::new(0),
            })
        }
    }

    /// Get MMIO base address
    pub fn mmio_base(&self) -> u64 {
        self.mmio_base
    }

    /// Check if flush is supported
    pub fn supports_flush(&self) -> bool {
        // Would check features
        true
    }
}

impl BlockDevice for VirtioBlk {
    fn read(&self, start_block: u64, buf: &mut [u8]) -> BlockResult<usize> {
        let sector_size = 512usize;
        let sectors = buf.len() / sector_size;

        if start_block + sectors as u64 > self.capacity {
            return Err(BlockError::InvalidBlock);
        }

        // In a real implementation, we would:
        // 1. Allocate descriptors
        // 2. Build request header
        // 3. Add to available ring
        // 4. Notify device
        // 5. Wait for completion
        // 6. Check status

        // For now, stub implementation
        buf.fill(0);
        Ok(buf.len())
    }

    fn write(&self, start_block: u64, buf: &[u8]) -> BlockResult<usize> {
        if self.read_only {
            return Err(BlockError::WriteProtected);
        }

        let sector_size = 512usize;
        let sectors = buf.len() / sector_size;

        if start_block + sectors as u64 > self.capacity {
            return Err(BlockError::InvalidBlock);
        }

        // In a real implementation, similar to read but with OUT request type

        Ok(buf.len())
    }

    fn flush(&self) -> BlockResult<()> {
        // Send flush request
        Ok(())
    }

    fn block_size(&self) -> u32 {
        self.block_size
    }

    fn block_count(&self) -> u64 {
        // Capacity is in 512-byte sectors, convert to block_size blocks
        self.capacity * 512 / self.block_size as u64
    }

    fn info(&self) -> BlockDeviceInfo {
        BlockDeviceInfo {
            name: "virtio-blk",
            block_size: self.block_size,
            block_count: self.block_count(),
            read_only: self.read_only,
            removable: false,
            model: "VirtIO Block Device",
        }
    }

    fn is_read_only(&self) -> bool {
        self.read_only
    }
}

/// Probe all virtio-blk devices at standard MMIO addresses
///
/// # Safety
/// The addresses must be valid MMIO regions.
pub unsafe fn probe_all() -> Vec<VirtioBlk> {
    let mut devices = Vec::new();

    // Standard virtio-blk MMIO addresses in QEMU
    let addresses: [u64; 8] = [
        0x10001000, 0x10002000, 0x10003000, 0x10004000,
        0x10005000, 0x10006000, 0x10007000, 0x10008000,
    ];

    for addr in addresses {
        if let Some(dev) = VirtioBlk::probe(addr) {
            devices.push(dev);
        }
    }

    devices
}
