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
use mm_manager::mm;
use mm_traits::FrameAllocator;

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
    /// Allocate and initialize a new virtqueue for modern VirtIO (MMIO)
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

    /// Allocate and initialize a new virtqueue for legacy VirtIO PCI
    ///
    /// Legacy VirtIO requires all three rings in a single contiguous region with
    /// the used ring starting at a page boundary.
    ///
    /// Layout:
    /// - Descriptor table: at offset 0, size = 16 * num
    /// - Available ring: immediately after descriptors, size = 6 + 2 * num
    /// - Padding to next page boundary
    /// - Used ring: at page-aligned offset, size = 6 + 8 * num
    ///
    /// # Safety
    /// Requires a working memory manager (call mm_manager::init_global first).
    unsafe fn new_legacy(num: u16) -> Option<Self> {
        // Calculate sizes
        let desc_size = (num as usize) * core::mem::size_of::<VirtqDesc>();
        let avail_size = 6 + 2 * (num as usize);
        let used_size = 6 + 8 * (num as usize);

        // For legacy, used ring must be at page boundary
        // Calculate offset: round up (desc_size + avail_size) to page boundary
        let avail_end = desc_size + avail_size;
        let used_offset = (avail_end + 4095) & !4095; // Round up to 4096

        let total_size = used_offset + used_size;
        let num_pages = (total_size + 4095) / 4096;

        // Allocate physical frames using the frame allocator
        // This gives us a known physical address that we can use for DMA
        let phys_addr = mm().alloc_contiguous(num_pages).ok()?;
        let phys_base = phys_addr.as_u64();

        // Access the physical memory through the physical map
        let virt_base = phys_to_virt(phys_base);
        let ptr = virt_base as *mut u8;

        // Zero the memory
        unsafe {
            core::ptr::write_bytes(ptr, 0, num_pages * 4096);
        }

        // Calculate addresses within the allocation
        let desc = ptr as *mut VirtqDesc;
        let avail = unsafe { ptr.add(desc_size) } as *mut VirtqAvail;
        let used = unsafe { ptr.add(used_offset) } as *mut VirtqUsed;

        // Physical addresses are straightforward since we allocated physical frames
        let desc_phys = phys_base;
        let avail_phys = phys_base + desc_size as u64;
        let used_phys = phys_base + used_offset as u64;

        // Initialize free list - chain all descriptors together
        for i in 0..num {
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

/// Size of the data bounce buffer per slot (one sector)
const SECTOR_SIZE: usize = 512;

/// Request buffer management
///
/// These buffers are allocated from physical frames for DMA access.
struct RequestBuffers {
    /// Physical address of headers array
    headers_phys: u64,
    /// Virtual address of headers array
    headers: *mut VirtioBlkReqHeader,
    /// Physical address of status array
    status_phys: u64,
    /// Virtual address of status array
    status: *mut u8,
    /// Physical address of data bounce buffer
    data_phys: u64,
    /// Virtual address of data bounce buffer (QUEUE_SIZE * SECTOR_SIZE bytes)
    data: *mut u8,
}

impl RequestBuffers {
    /// Allocate request buffers from physical frames
    fn new_dma() -> Option<Self> {
        // Calculate sizes
        let headers_size = QUEUE_SIZE * core::mem::size_of::<VirtioBlkReqHeader>();
        let status_size = QUEUE_SIZE;
        let data_size = QUEUE_SIZE * SECTOR_SIZE; // Bounce buffer for data
        let total_size = headers_size + status_size + data_size;
        let num_pages = (total_size + 4095) / 4096;

        // Allocate physical frames
        let phys_addr = mm().alloc_contiguous(num_pages).ok()?;
        let phys_base = phys_addr.as_u64();

        // Access through physical map
        let virt_base = phys_to_virt(phys_base);
        let ptr = virt_base as *mut u8;

        // Zero the memory
        unsafe {
            core::ptr::write_bytes(ptr, 0, num_pages * 4096);
        }

        let headers = ptr as *mut VirtioBlkReqHeader;
        let status = unsafe { ptr.add(headers_size) };
        let data = unsafe { ptr.add(headers_size + status_size) };

        Some(RequestBuffers {
            headers_phys: phys_base,
            headers,
            status_phys: phys_base + headers_size as u64,
            status,
            data_phys: phys_base + (headers_size + status_size) as u64,
            data,
        })
    }

    /// Get header at slot index (returns virtual and physical addresses)
    fn header(&self, slot: usize) -> (*mut VirtioBlkReqHeader, u64) {
        let offset = slot * core::mem::size_of::<VirtioBlkReqHeader>();
        let virt = unsafe { self.headers.add(slot) };
        let phys = self.headers_phys + offset as u64;
        (virt, phys)
    }

    /// Get status byte at slot index (returns virtual and physical addresses)
    fn status(&self, slot: usize) -> (*mut u8, u64) {
        let virt = unsafe { self.status.add(slot) };
        let phys = self.status_phys + slot as u64;
        (virt, phys)
    }

    /// Get data bounce buffer at slot index (returns virtual and physical addresses)
    fn data_buffer(&self, slot: usize) -> (*mut u8, u64) {
        let offset = slot * SECTOR_SIZE;
        let virt = unsafe { self.data.add(offset) };
        let phys = self.data_phys + offset as u64;
        (virt, phys)
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
            core::ptr::write_volatile(
                (mmio_base + mmio::STATUS) as *mut u32,
                status::ACKNOWLEDGE as u32,
            );

            // 3. Set DRIVER
            let mut dev_status = status::ACKNOWLEDGE | status::DRIVER;
            core::ptr::write_volatile((mmio_base + mmio::STATUS) as *mut u32, dev_status as u32);

            // 4. Read device features
            core::ptr::write_volatile((mmio_base + mmio::DEVICE_FEATURES_SEL) as *mut u32, 0);
            let features_lo =
                core::ptr::read_volatile((mmio_base + mmio::DEVICE_FEATURES) as *const u32);
            core::ptr::write_volatile((mmio_base + mmio::DEVICE_FEATURES_SEL) as *mut u32, 1);
            let features_hi =
                core::ptr::read_volatile((mmio_base + mmio::DEVICE_FEATURES) as *const u32);
            let device_features = (features_hi as u64) << 32 | (features_lo as u64);

            // 5. Negotiate features (accept RO, BLK_SIZE, FLUSH)
            let accepted = device_features & (features::RO | features::BLK_SIZE | features::FLUSH);
            core::ptr::write_volatile((mmio_base + mmio::DRIVER_FEATURES_SEL) as *mut u32, 0);
            core::ptr::write_volatile(
                (mmio_base + mmio::DRIVER_FEATURES) as *mut u32,
                accepted as u32,
            );
            core::ptr::write_volatile((mmio_base + mmio::DRIVER_FEATURES_SEL) as *mut u32, 1);
            core::ptr::write_volatile(
                (mmio_base + mmio::DRIVER_FEATURES) as *mut u32,
                (accepted >> 32) as u32,
            );

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
            let queue_max =
                core::ptr::read_volatile((mmio_base + mmio::QUEUE_NUM_MAX) as *const u32);
            if queue_max == 0 {
                return None;
            }
            let queue_size = (queue_max as u16).min(QUEUE_SIZE as u16);

            // Allocate virtqueue
            let queue = Virtqueue::new(queue_size)?;

            // Set queue size
            core::ptr::write_volatile((mmio_base + mmio::QUEUE_NUM) as *mut u32, queue_size as u32);

            // Set queue addresses
            core::ptr::write_volatile(
                (mmio_base + mmio::QUEUE_DESC_LOW) as *mut u32,
                queue.desc_phys as u32,
            );
            core::ptr::write_volatile(
                (mmio_base + mmio::QUEUE_DESC_HIGH) as *mut u32,
                (queue.desc_phys >> 32) as u32,
            );
            core::ptr::write_volatile(
                (mmio_base + mmio::QUEUE_AVAIL_LOW) as *mut u32,
                queue.avail_phys as u32,
            );
            core::ptr::write_volatile(
                (mmio_base + mmio::QUEUE_AVAIL_HIGH) as *mut u32,
                (queue.avail_phys >> 32) as u32,
            );
            core::ptr::write_volatile(
                (mmio_base + mmio::QUEUE_USED_LOW) as *mut u32,
                queue.used_phys as u32,
            );
            core::ptr::write_volatile(
                (mmio_base + mmio::QUEUE_USED_HIGH) as *mut u32,
                (queue.used_phys >> 32) as u32,
            );

            // Enable queue
            core::ptr::write_volatile((mmio_base + mmio::QUEUE_READY) as *mut u32, 1);

            // 10. Set DRIVER_OK
            dev_status |= status::DRIVER_OK;
            core::ptr::write_volatile((mmio_base + mmio::STATUS) as *mut u32, dev_status as u32);

            // Allocate DMA-safe request buffers
            let req_buffers = RequestBuffers::new_dma()?;

            Some(VirtioBlk {
                mmio_base,
                capacity,
                block_size,
                read_only,
                supports_flush,
                queue: Mutex::new(queue),
                req_buffers: Mutex::new(req_buffers),
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
        if self.is_pci_io() {
            // PCI I/O port mode
            outw(self.io_base() + pci_io::QUEUE_NOTIFY, 0);
        } else {
            // MMIO mode
            unsafe {
                core::ptr::write_volatile((self.mmio_base + mmio::QUEUE_NOTIFY) as *mut u32, 0);
            }
        }
    }

    /// Perform a block I/O request (internal)
    ///
    /// Holds both queue and buffer locks for the entire duration of the request
    /// to prevent concurrent requests from stealing each other's completions.
    /// The used ring returns completions in arbitrary order, so without holding
    /// the lock, Thread A could pop Thread B's completion and read stale data
    /// from its own (not-yet-completed) bounce buffer slot.
    fn do_request(&self, req_type: u32, sector: u64, data: Option<&mut [u8]>) -> BlockResult<()> {
        let mut queue = self.queue.lock();
        let buffers = self.req_buffers.lock();

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

        // Get DMA-safe header and status buffers with their physical addresses
        let (header_ptr, header_phys) = buffers.header(slot);
        let (status_ptr, status_phys) = buffers.status(slot);

        // Set up request header
        unsafe {
            *header_ptr = VirtioBlkReqHeader {
                req_type,
                reserved: 0,
                sector,
            };
            *status_ptr = 0xFF; // Invalid status to detect completion
        }

        // Get DMA-safe data bounce buffer
        let (bounce_ptr, bounce_phys) = buffers.data_buffer(slot);

        // Build descriptor chain
        unsafe {
            // Header descriptor (device-readable)
            let hdr = &mut *queue.desc.add(desc_header as usize);
            hdr.addr = header_phys;
            hdr.len = core::mem::size_of::<VirtioBlkReqHeader>() as u32;

            if let (Some(desc_data), Some(data)) = (desc_data, data.as_ref()) {
                // Has data buffer - use bounce buffer for DMA safety
                hdr.flags = desc_flags::NEXT;
                hdr.next = desc_data;

                // For writes (OUT), copy data from caller buffer to bounce buffer
                if req_type == req_type::OUT {
                    let copy_len = data.len().min(SECTOR_SIZE);
                    core::ptr::copy_nonoverlapping(data.as_ptr(), bounce_ptr, copy_len);
                }

                // Data descriptor - use bounce buffer physical address
                let dat = &mut *queue.desc.add(desc_data as usize);
                dat.addr = bounce_phys;
                dat.len = data.len().min(SECTOR_SIZE as usize) as u32;
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

        // Drop buffers lock before polling - the queue lock serializes access
        // to the used ring so completions can't be stolen by another thread.
        drop(buffers);

        // Notify device
        self.notify();

        // Poll for completion while holding the queue lock.
        // This ensures only one thread polls the used ring at a time,
        // preventing completion stealing between concurrent requests.
        let mut timeout = 10_000_000u32;
        loop {
            if queue.has_completed() {
                let (id, _len) = queue.pop_used().unwrap();

                // Verify we got our own completion (should always match since
                // we hold the lock, but check for safety)
                debug_assert_eq!(id, desc_header, "virtio-blk: wrong completion");

                // Check status and copy data back for reads
                let buffers = self.req_buffers.lock();
                let (status_ptr, _) = buffers.status(slot);
                let status = unsafe { *status_ptr };

                // For reads (IN), copy data from bounce buffer back to caller's buffer
                if req_type == req_type::IN && status == blk_status::OK {
                    if let Some(data) = data {
                        let (bounce_ptr, _) = buffers.data_buffer(slot);
                        let copy_len = data.len().min(SECTOR_SIZE);
                        unsafe {
                            core::ptr::copy_nonoverlapping(bounce_ptr, data.as_mut_ptr(), copy_len);
                        }
                    }
                }
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

            timeout -= 1;
            if timeout == 0 {
                // Timeout - free descriptors
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
        0x10001000, 0x10002000, 0x10003000, 0x10004000, 0x10005000, 0x10006000, 0x10007000,
        0x10008000,
    ];

    for addr in addresses {
        // SAFETY: Caller guarantees these addresses are valid MMIO regions
        if let Some(dev) = unsafe { VirtioBlk::probe(addr) } {
            devices.push(dev);
        }
    }

    devices
}

/// PCI legacy I/O port register offsets
mod pci_io {
    pub const DEVICE_FEATURES: u16 = 0x00;
    pub const DRIVER_FEATURES: u16 = 0x04;
    pub const QUEUE_ADDRESS: u16 = 0x08; // Legacy: queue PFN
    pub const QUEUE_SIZE: u16 = 0x0C;
    pub const QUEUE_SELECT: u16 = 0x0E;
    pub const QUEUE_NOTIFY: u16 = 0x10;
    pub const DEVICE_STATUS: u16 = 0x12;
    pub const ISR_STATUS: u16 = 0x13;
    pub const CONFIG: u16 = 0x14; // Device-specific config starts here
}

/// Probe all virtio-blk devices on the PCI bus
///
/// This function must be called after PCI enumeration.
pub fn probe_all_pci() -> Vec<VirtioBlk> {
    let mut devices = Vec::new();

    // Find all VirtIO block devices on PCI bus
    let pci_devices = pci::find_virtio_blk();

    for pci_dev in pci_devices {
        // SAFETY: The PCI device is a valid VirtIO block device
        if let Some(dev) = unsafe { VirtioBlk::from_pci(&pci_dev) } {
            devices.push(dev);
        }
    }

    devices
}

impl VirtioBlk {
    /// Create a VirtIO block device from a PCI device
    ///
    /// # Safety
    /// The PCI device must be a valid VirtIO block device.
    pub unsafe fn from_pci(pci_dev: &pci::PciDevice) -> Option<Self> {
        // Verify this is a VirtIO block device
        if !pci_dev.is_virtio_blk() {
            return None;
        }

        // Enable the device
        pci::enable_bus_master(pci_dev.address);
        pci::enable_io_space(pci_dev.address);
        pci::enable_memory_space(pci_dev.address);

        // Get BAR0
        let (is_io, base) = match pci_dev.bars[0] {
            pci::PciBar::Io { port, .. } => (true, port as u64),
            pci::PciBar::Memory { address, .. } => (false, address),
            pci::PciBar::None => return None,
        };

        if is_io {
            // Legacy I/O port based initialization
            let io_base = base as u16;

            // Reset device
            outb(io_base + pci_io::DEVICE_STATUS, 0);

            // Set ACKNOWLEDGE
            outb(io_base + pci_io::DEVICE_STATUS, status::ACKNOWLEDGE);

            // Set DRIVER
            outb(
                io_base + pci_io::DEVICE_STATUS,
                status::ACKNOWLEDGE | status::DRIVER,
            );

            // Read device features
            let device_features = inl(io_base + pci_io::DEVICE_FEATURES) as u64;

            // Negotiate features (accept RO, BLK_SIZE, FLUSH)
            let accepted = device_features & (features::RO | features::BLK_SIZE | features::FLUSH);
            outl(io_base + pci_io::DRIVER_FEATURES, accepted as u32);

            // Set FEATURES_OK
            outb(
                io_base + pci_io::DEVICE_STATUS,
                status::ACKNOWLEDGE | status::DRIVER | status::FEATURES_OK,
            );

            // Verify FEATURES_OK was accepted
            let status_read = inb(io_base + pci_io::DEVICE_STATUS);
            if status_read & status::FEATURES_OK == 0 {
                return None;
            }

            // Read device configuration (capacity is 8 bytes at CONFIG offset)
            let capacity_lo = inl(io_base + pci_io::CONFIG) as u64;
            let capacity_hi = inl(io_base + pci_io::CONFIG + 4) as u64;
            let capacity = (capacity_hi << 32) | capacity_lo;

            let block_size = if device_features & features::BLK_SIZE != 0 {
                // Block size is at offset 0x14 from config start
                inl(io_base + pci_io::CONFIG + 0x14)
            } else {
                512
            };

            let read_only = device_features & features::RO != 0;
            let supports_flush = device_features & features::FLUSH != 0;

            // Set up virtqueue 0
            outw(io_base + pci_io::QUEUE_SELECT, 0);
            let queue_max = inw(io_base + pci_io::QUEUE_SIZE);
            if queue_max == 0 {
                return None;
            }
            let queue_size = queue_max.min(QUEUE_SIZE as u16);

            // Allocate virtqueue with legacy layout (used ring page-aligned)
            // SAFETY: Requires working heap allocator
            let queue = unsafe { Virtqueue::new_legacy(queue_size)? };

            // Legacy virtio uses page-aligned queue at single physical address
            // The queue PFN (Page Frame Number) is written to the device
            // For legacy: all three rings must be contiguous with used at page boundary
            let queue_pfn = (queue.desc_phys / 4096) as u32;
            outl(io_base + pci_io::QUEUE_ADDRESS, queue_pfn);

            // Set DRIVER_OK
            outb(
                io_base + pci_io::DEVICE_STATUS,
                status::ACKNOWLEDGE | status::DRIVER | status::FEATURES_OK | status::DRIVER_OK,
            );

            // Allocate DMA-safe request buffers
            let req_buffers = RequestBuffers::new_dma()?;

            Some(VirtioBlk {
                mmio_base: io_base as u64 | 0x8000_0000_0000_0000, // Mark as PCI I/O mode
                capacity,
                block_size,
                read_only,
                supports_flush,
                queue: Mutex::new(queue),
                req_buffers: Mutex::new(req_buffers),
            })
        } else {
            // Memory-mapped BAR - try MMIO-style probing
            // Map the physical address to virtual
            let virt_base = phys_to_virt(base);
            // SAFETY: base is a valid PCI BAR address
            unsafe { Self::probe(virt_base) }
        }
    }

    /// Check if device uses PCI I/O port mode (vs MMIO)
    fn is_pci_io(&self) -> bool {
        self.mmio_base & 0x8000_0000_0000_0000 != 0
    }

    /// Get I/O base port (only valid if is_pci_io() returns true)
    fn io_base(&self) -> u16 {
        (self.mmio_base & 0xFFFF) as u16
    }
}

// x86_64 I/O port access functions
#[inline]
fn inb(port: u16) -> u8 {
    let value: u8;
    unsafe {
        core::arch::asm!(
            "in al, dx",
            out("al") value,
            in("dx") port,
            options(nomem, nostack, preserves_flags)
        );
    }
    value
}

#[inline]
fn outb(port: u16, value: u8) {
    unsafe {
        core::arch::asm!(
            "out dx, al",
            in("dx") port,
            in("al") value,
            options(nomem, nostack, preserves_flags)
        );
    }
}

#[inline]
fn inw(port: u16) -> u16 {
    let value: u16;
    unsafe {
        core::arch::asm!(
            "in ax, dx",
            out("ax") value,
            in("dx") port,
            options(nomem, nostack, preserves_flags)
        );
    }
    value
}

#[inline]
fn outw(port: u16, value: u16) {
    unsafe {
        core::arch::asm!(
            "out dx, ax",
            in("dx") port,
            in("ax") value,
            options(nomem, nostack, preserves_flags)
        );
    }
}

#[inline]
fn inl(port: u16) -> u32 {
    let value: u32;
    unsafe {
        core::arch::asm!(
            "in eax, dx",
            out("eax") value,
            in("dx") port,
            options(nomem, nostack, preserves_flags)
        );
    }
    value
}

#[inline]
fn outl(port: u16, value: u32) {
    unsafe {
        core::arch::asm!(
            "out dx, eax",
            in("dx") port,
            in("eax") value,
            options(nomem, nostack, preserves_flags)
        );
    }
}
