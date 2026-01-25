//! VirtIO Block Device Driver for OXIDE OS
//!
//! Implements the virtio-blk specification for virtual block devices.

#![no_std]
#![allow(unused)]

extern crate alloc;

use alloc::boxed::Box;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU16, AtomicU64, Ordering};
use spin::Mutex;

use block::{BlockDevice, BlockDeviceInfo, BlockError, BlockResult};

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
    pub const IN: u32 = 0; // Read
    pub const OUT: u32 = 1; // Write
    pub const FLUSH: u32 = 4; // Flush
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

/// MMIO register offsets
mod mmio {
    pub const MAGIC: u64 = 0x00;
    pub const VERSION: u64 = 0x04;
    pub const DEVICE_ID: u64 = 0x08;
    pub const VENDOR_ID: u64 = 0x0C;
    pub const DEVICE_FEATURES: u64 = 0x10;
    pub const DEVICE_FEATURES_SEL: u64 = 0x14;
    pub const DRIVER_FEATURES: u64 = 0x20;
    pub const DRIVER_FEATURES_SEL: u64 = 0x24;
    pub const QUEUE_SEL: u64 = 0x30;
    pub const QUEUE_NUM_MAX: u64 = 0x34;
    pub const QUEUE_NUM: u64 = 0x38;
    pub const QUEUE_READY: u64 = 0x44;
    pub const QUEUE_NOTIFY: u64 = 0x50;
    pub const INTERRUPT_STATUS: u64 = 0x60;
    pub const INTERRUPT_ACK: u64 = 0x64;
    pub const STATUS: u64 = 0x70;
    pub const QUEUE_DESC_LOW: u64 = 0x80;
    pub const QUEUE_DESC_HIGH: u64 = 0x84;
    pub const QUEUE_AVAIL_LOW: u64 = 0x90;
    pub const QUEUE_AVAIL_HIGH: u64 = 0x94;
    pub const QUEUE_USED_LOW: u64 = 0xA0;
    pub const QUEUE_USED_HIGH: u64 = 0xA4;
    pub const CONFIG: u64 = 0x100;
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
#[derive(Debug, Clone, Copy, Default)]
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
    ring: [u16; 256],
    used_event: u16,
}

/// VirtIO used ring element
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
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

/// Queue size (number of descriptors)
const QUEUE_SIZE: usize = 256;

/// Physical memory mapping base (same as mm-paging PHYS_MAP_BASE)
const PHYS_MAP_BASE: u64 = 0xFFFF_8000_0000_0000;

/// Convert physical address to virtual address
#[inline]
fn phys_to_virt(phys: u64) -> u64 {
    phys + PHYS_MAP_BASE
}

/// Convert virtual address to physical address
#[inline]
fn virt_to_phys(virt: u64) -> u64 {
    virt - PHYS_MAP_BASE
}

/// Virtqueue management structure
struct Virtqueue {
    /// Descriptor table (physical address)
    desc_phys: u64,
    /// Descriptor table (virtual address for kernel access)
    desc: *mut VirtqDesc,
    /// Available ring (physical address)
    avail_phys: u64,
    /// Available ring (virtual address)
    avail: *mut VirtqAvail,
    /// Used ring (physical address)
    used_phys: u64,
    /// Used ring (virtual address)
    used: *mut VirtqUsed,
    /// Number of descriptors
    num: u16,
    /// Free descriptor list head
    free_head: u16,
    /// Number of free descriptors
    num_free: u16,
    /// Last used index we processed
    last_used_idx: u16,
}

impl Virtqueue {
    /// Allocate and initialize a new virtqueue
    ///
    /// # Safety
    /// Requires a working heap allocator.
    unsafe fn new(num: u16) -> Option<Self> {
        // Calculate sizes
        // Descriptors: 16 bytes each
        let desc_size = (num as usize) * core::mem::size_of::<VirtqDesc>();
        // Available ring: 2 + 2 + 2*num + 2 (flags, idx, ring, used_event)
        let avail_size = 6 + 2 * (num as usize);
        // Used ring: 2 + 2 + 8*num + 2 (flags, idx, ring, avail_event)
        let used_size = 6 + 8 * (num as usize);

        // Allocate memory with proper alignment
        // VirtIO requires specific alignment:
        // - Descriptor table: 16 bytes
        // - Available ring: 2 bytes
        // - Used ring: 4 bytes
        // We'll allocate a contiguous block with proper alignment
        let total_size = desc_size + avail_size + used_size + 4096; // Extra for alignment
        let layout = alloc::alloc::Layout::from_size_align(total_size, 4096).ok()?;
        // SAFETY: Layout is valid and we check for null
        let ptr = unsafe { alloc::alloc::alloc_zeroed(layout) };
        if ptr.is_null() {
            return None;
        }

        // Calculate addresses within the allocation
        let desc = ptr as *mut VirtqDesc;
        // SAFETY: ptr is valid and desc_size is within our allocation
        let avail = unsafe { ptr.add(desc_size) } as *mut VirtqAvail;
        // Used ring needs 4-byte alignment, round up
        let used_offset = ((desc_size + avail_size) + 3) & !3;
        // SAFETY: ptr is valid and used_offset is within our allocation
        let used = unsafe { ptr.add(used_offset) } as *mut VirtqUsed;

        // Calculate physical addresses
        let desc_phys = virt_to_phys(desc as u64);
        let avail_phys = virt_to_phys(avail as u64);
        let used_phys = virt_to_phys(used as u64);

        // Initialize free list - chain all descriptors together
        for i in 0..num {
            // SAFETY: desc is valid and i is within bounds
            unsafe { (*desc.add(i as usize)).next = i + 1 };
        }

        Some(Virtqueue {
            desc_phys,
            desc,
            avail_phys,
            avail,
            used_phys,
            used,
            num,
            free_head: 0,
            num_free: num,
            last_used_idx: 0,
        })
    }

    /// Allocate a descriptor from the free list
    fn alloc_desc(&mut self) -> Option<u16> {
        if self.num_free == 0 {
            return None;
        }
        let idx = self.free_head;
        unsafe {
            self.free_head = (*self.desc.add(idx as usize)).next;
        }
        self.num_free -= 1;
        Some(idx)
    }

    /// Free a descriptor chain starting at idx
    fn free_chain(&mut self, mut idx: u16) {
        loop {
            unsafe {
                let desc = &mut *self.desc.add(idx as usize);
                let next = desc.next;
                let has_next = desc.flags & desc_flags::NEXT != 0;

                // Add to free list
                desc.next = self.free_head;
                self.free_head = idx;
                self.num_free += 1;

                if !has_next {
                    break;
                }
                idx = next;
            }
        }
    }

    /// Add a descriptor chain to the available ring
    fn add_available(&mut self, head: u16) {
        unsafe {
            let avail = &mut *self.avail;
            let idx = avail.idx as usize % self.num as usize;
            avail.ring[idx] = head;
            // Memory barrier to ensure descriptor is visible before idx update
            core::sync::atomic::fence(Ordering::Release);
            avail.idx = avail.idx.wrapping_add(1);
        }
    }

    /// Check if there are completed requests in the used ring
    fn has_completed(&self) -> bool {
        unsafe {
            let used = &*self.used;
            // Memory barrier to ensure we see the latest idx
            core::sync::atomic::fence(Ordering::Acquire);
            used.idx != self.last_used_idx
        }
    }

    /// Get next completed request (descriptor chain head)
    fn pop_used(&mut self) -> Option<(u16, u32)> {
        unsafe {
            let used = &*self.used;
            core::sync::atomic::fence(Ordering::Acquire);
            if used.idx == self.last_used_idx {
                return None;
            }

            let idx = self.last_used_idx as usize % self.num as usize;
            let elem = used.ring[idx];
            self.last_used_idx = self.last_used_idx.wrapping_add(1);

            Some((elem.id as u16, elem.len))
        }
    }
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
    /// Supports flush
    supports_flush: bool,
    /// Virtqueue
    queue: Mutex<Virtqueue>,
    /// Request buffer pool (header + status for each possible in-flight request)
    req_buffers: Mutex<RequestBuffers>,
}

/// Request buffer management
struct RequestBuffers {
    /// Headers (one per possible descriptor)
    headers: Box<[VirtioBlkReqHeader; QUEUE_SIZE]>,
    /// Status bytes (one per possible descriptor)
    status: Box<[u8; QUEUE_SIZE]>,
}

impl RequestBuffers {
    fn new() -> Self {
        RequestBuffers {
            headers: Box::new([VirtioBlkReqHeader {
                req_type: 0,
                reserved: 0,
                sector: 0,
            }; QUEUE_SIZE]),
            status: Box::new([0u8; QUEUE_SIZE]),
        }
    }
}

impl VirtioBlk {
    /// Probe for a virtio-blk device at the given MMIO address
    ///
    /// # Safety
    /// The MMIO address must be valid and mapped.
    pub unsafe fn probe(mmio_base: u64) -> Option<Self> {
        // SAFETY: All volatile reads/writes are to the valid MMIO region
        unsafe {
            // Read magic value (should be "virt" = 0x74726976)
            let magic = core::ptr::read_volatile(mmio_base as *const u32);
            if magic != 0x74726976 {
                return None;
            }

            // Read version (should be 2 for modern virtio)
            let version = core::ptr::read_volatile((mmio_base + mmio::VERSION) as *const u32);
            if version != 2 {
                return None;
            }

            // Read device ID (2 = block device)
            let device_id = core::ptr::read_volatile((mmio_base + mmio::DEVICE_ID) as *const u32);
            if device_id != 2 {
                return None;
            }

            // 1. Reset device
            core::ptr::write_volatile((mmio_base + mmio::STATUS) as *mut u32, 0);

            // 2. Set ACKNOWLEDGE
            core::ptr::write_volatile((mmio_base + mmio::STATUS) as *mut u32, status::ACKNOWLEDGE as u32);

            // 3. Set DRIVER
            let mut dev_status = status::ACKNOWLEDGE | status::DRIVER;
            core::ptr::write_volatile((mmio_base + mmio::STATUS) as *mut u32, dev_status as u32);

            // 4. Read device features
            core::ptr::write_volatile((mmio_base + mmio::DEVICE_FEATURES_SEL) as *mut u32, 0);
            let features_lo = core::ptr::read_volatile((mmio_base + mmio::DEVICE_FEATURES) as *const u32);
            core::ptr::write_volatile((mmio_base + mmio::DEVICE_FEATURES_SEL) as *mut u32, 1);
            let features_hi = core::ptr::read_volatile((mmio_base + mmio::DEVICE_FEATURES) as *const u32);
            let device_features = (features_hi as u64) << 32 | (features_lo as u64);

            // 5. Negotiate features (accept RO, BLK_SIZE, FLUSH)
            let accepted = device_features & (features::RO | features::BLK_SIZE | features::FLUSH);
            core::ptr::write_volatile((mmio_base + mmio::DRIVER_FEATURES_SEL) as *mut u32, 0);
            core::ptr::write_volatile((mmio_base + mmio::DRIVER_FEATURES) as *mut u32, accepted as u32);
            core::ptr::write_volatile((mmio_base + mmio::DRIVER_FEATURES_SEL) as *mut u32, 1);
            core::ptr::write_volatile((mmio_base + mmio::DRIVER_FEATURES) as *mut u32, (accepted >> 32) as u32);

            // 6. Set FEATURES_OK
            dev_status |= status::FEATURES_OK;
            core::ptr::write_volatile((mmio_base + mmio::STATUS) as *mut u32, dev_status as u32);

            // 7. Verify FEATURES_OK was accepted
            let status_read = core::ptr::read_volatile((mmio_base + mmio::STATUS) as *const u32);
            if status_read & (status::FEATURES_OK as u32) == 0 {
                return None;
            }

            // 8. Read device configuration
            let capacity = core::ptr::read_volatile((mmio_base + mmio::CONFIG) as *const u64);
            let block_size = if device_features & features::BLK_SIZE != 0 {
                core::ptr::read_volatile((mmio_base + mmio::CONFIG + 0x14) as *const u32)
            } else {
                512
            };
            let read_only = device_features & features::RO != 0;
            let supports_flush = device_features & features::FLUSH != 0;

            // 9. Set up virtqueue 0
            core::ptr::write_volatile((mmio_base + mmio::QUEUE_SEL) as *mut u32, 0);
            let queue_max = core::ptr::read_volatile((mmio_base + mmio::QUEUE_NUM_MAX) as *const u32);
            if queue_max == 0 {
                return None;
            }
            let queue_size = (queue_max as u16).min(QUEUE_SIZE as u16);

            // Allocate virtqueue
            let queue = Virtqueue::new(queue_size)?;

            // Set queue size
            core::ptr::write_volatile((mmio_base + mmio::QUEUE_NUM) as *mut u32, queue_size as u32);

            // Set queue addresses
            core::ptr::write_volatile((mmio_base + mmio::QUEUE_DESC_LOW) as *mut u32, queue.desc_phys as u32);
            core::ptr::write_volatile((mmio_base + mmio::QUEUE_DESC_HIGH) as *mut u32, (queue.desc_phys >> 32) as u32);
            core::ptr::write_volatile((mmio_base + mmio::QUEUE_AVAIL_LOW) as *mut u32, queue.avail_phys as u32);
            core::ptr::write_volatile((mmio_base + mmio::QUEUE_AVAIL_HIGH) as *mut u32, (queue.avail_phys >> 32) as u32);
            core::ptr::write_volatile((mmio_base + mmio::QUEUE_USED_LOW) as *mut u32, queue.used_phys as u32);
            core::ptr::write_volatile((mmio_base + mmio::QUEUE_USED_HIGH) as *mut u32, (queue.used_phys >> 32) as u32);

            // Enable queue
            core::ptr::write_volatile((mmio_base + mmio::QUEUE_READY) as *mut u32, 1);

            // 10. Set DRIVER_OK
            dev_status |= status::DRIVER_OK;
            core::ptr::write_volatile((mmio_base + mmio::STATUS) as *mut u32, dev_status as u32);

            Some(VirtioBlk {
                mmio_base,
                capacity,
                block_size,
                read_only,
                supports_flush,
                queue: Mutex::new(queue),
                req_buffers: Mutex::new(RequestBuffers::new()),
            })
        }
    }

    /// Get MMIO base address
    pub fn mmio_base(&self) -> u64 {
        self.mmio_base
    }

    /// Check if flush is supported
    pub fn supports_flush(&self) -> bool {
        self.supports_flush
    }

    /// Notify the device that there are buffers available
    fn notify(&self) {
        unsafe {
            core::ptr::write_volatile((self.mmio_base + mmio::QUEUE_NOTIFY) as *mut u32, 0);
        }
    }

    /// Perform a block I/O request (internal)
    fn do_request(&self, req_type: u32, sector: u64, data: Option<&mut [u8]>) -> BlockResult<()> {
        let mut queue = self.queue.lock();
        let mut buffers = self.req_buffers.lock();

        // Allocate descriptors (we need 2 or 3)
        // Descriptor 0: Request header (device-readable)
        // Descriptor 1: Data buffer (device-readable for write, device-writable for read)
        // Descriptor 2: Status byte (device-writable)
        let desc_header = queue.alloc_desc().ok_or(BlockError::Busy)?;
        let desc_status = queue.alloc_desc().ok_or(BlockError::Busy)?;
        let desc_data = if data.is_some() {
            Some(queue.alloc_desc().ok_or(BlockError::Busy)?)
        } else {
            None
        };

        // Use descriptor index for buffer slot
        let slot = desc_header as usize;

        // Set up request header
        buffers.headers[slot] = VirtioBlkReqHeader {
            req_type,
            reserved: 0,
            sector,
        };
        buffers.status[slot] = 0xFF; // Invalid status to detect completion

        // Get physical addresses
        let header_phys = virt_to_phys(&buffers.headers[slot] as *const _ as u64);
        let status_phys = virt_to_phys(&buffers.status[slot] as *const _ as u64);

        // Build descriptor chain
        unsafe {
            // Header descriptor (device-readable)
            let hdr = &mut *queue.desc.add(desc_header as usize);
            hdr.addr = header_phys;
            hdr.len = core::mem::size_of::<VirtioBlkReqHeader>() as u32;

            if let (Some(desc_data), Some(data)) = (desc_data, data.as_ref()) {
                // Has data buffer
                hdr.flags = desc_flags::NEXT;
                hdr.next = desc_data;

                // Data descriptor
                let dat = &mut *queue.desc.add(desc_data as usize);
                let data_phys = virt_to_phys(data.as_ptr() as u64);
                dat.addr = data_phys;
                dat.len = data.len() as u32;
                dat.flags = desc_flags::NEXT;
                if req_type == req_type::IN {
                    dat.flags |= desc_flags::WRITE; // Device writes to this buffer
                }
                dat.next = desc_status;

                // Status descriptor (device-writable)
                let stat = &mut *queue.desc.add(desc_status as usize);
                stat.addr = status_phys;
                stat.len = 1;
                stat.flags = desc_flags::WRITE;
                stat.next = 0;
            } else {
                // No data buffer (e.g., flush)
                hdr.flags = desc_flags::NEXT;
                hdr.next = desc_status;

                // Status descriptor (device-writable)
                let stat = &mut *queue.desc.add(desc_status as usize);
                stat.addr = status_phys;
                stat.len = 1;
                stat.flags = desc_flags::WRITE;
                stat.next = 0;
            }
        }

        // Add to available ring and notify
        queue.add_available(desc_header);

        // Release locks before waiting
        drop(buffers);
        drop(queue);

        // Notify device
        self.notify();

        // Poll for completion
        let mut timeout = 10_000_000u32;
        loop {
            let mut queue = self.queue.lock();
            if queue.has_completed() {
                let (_id, _len) = queue.pop_used().unwrap();

                // Check status
                let buffers = self.req_buffers.lock();
                let status = buffers.status[slot];
                drop(buffers);

                // Free descriptor chain
                queue.free_chain(desc_header);

                return match status {
                    blk_status::OK => Ok(()),
                    blk_status::IOERR => Err(BlockError::IoError),
                    blk_status::UNSUPP => Err(BlockError::InvalidOp),
                    _ => Err(BlockError::IoError),
                };
            }
            drop(queue);

            timeout -= 1;
            if timeout == 0 {
                // Timeout - try to free descriptors anyway
                let mut queue = self.queue.lock();
                queue.free_chain(desc_header);
                return Err(BlockError::Timeout);
            }

            core::hint::spin_loop();
        }
    }
}

impl BlockDevice for VirtioBlk {
    fn read(&self, start_block: u64, buf: &mut [u8]) -> BlockResult<usize> {
        let sector_size = 512usize;
        let num_sectors = buf.len() / sector_size;

        if buf.len() % sector_size != 0 {
            return Err(BlockError::BufferTooSmall);
        }

        if start_block + num_sectors as u64 > self.capacity {
            return Err(BlockError::InvalidBlock);
        }

        // Process sectors one at a time for simplicity
        // A production driver would batch these
        for i in 0..num_sectors {
            let sector = start_block + i as u64;
            let offset = i * sector_size;
            let slice = &mut buf[offset..offset + sector_size];
            self.do_request(req_type::IN, sector, Some(slice))?;
        }

        Ok(buf.len())
    }

    fn write(&self, start_block: u64, buf: &[u8]) -> BlockResult<usize> {
        if self.read_only {
            return Err(BlockError::WriteProtected);
        }

        let sector_size = 512usize;
        let num_sectors = buf.len() / sector_size;

        if buf.len() % sector_size != 0 {
            return Err(BlockError::BufferTooSmall);
        }

        if start_block + num_sectors as u64 > self.capacity {
            return Err(BlockError::InvalidBlock);
        }

        // Process sectors one at a time for simplicity
        for i in 0..num_sectors {
            let sector = start_block + i as u64;
            let offset = i * sector_size;
            // Need to cast away const for the internal function (data is only read for OUT)
            let slice = unsafe {
                core::slice::from_raw_parts_mut(
                    buf[offset..offset + sector_size].as_ptr() as *mut u8,
                    sector_size,
                )
            };
            self.do_request(req_type::OUT, sector, Some(slice))?;
        }

        Ok(buf.len())
    }

    fn flush(&self) -> BlockResult<()> {
        if !self.supports_flush {
            return Ok(()); // No-op if flush not supported
        }
        self.do_request(req_type::FLUSH, 0, None)
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

// SAFETY: VirtioBlk uses internal synchronization (Mutex)
unsafe impl Send for VirtioBlk {}
unsafe impl Sync for VirtioBlk {}

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
        // SAFETY: Caller guarantees these addresses are valid MMIO regions
        if let Some(dev) = unsafe { VirtioBlk::probe(addr) } {
            devices.push(dev);
        }
    }

    devices
}
