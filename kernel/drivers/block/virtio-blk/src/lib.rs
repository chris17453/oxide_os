//! VirtIO Block Device Driver for OXIDE OS
//!
//! Implements the virtio-blk specification for virtual block devices.

#![no_std]
#![allow(unused)]

extern crate alloc;

use alloc::boxed::Box;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU16, AtomicU32, AtomicU64, Ordering};
use spin::Mutex;

use block::{BlockDevice, BlockDeviceInfo, BlockError, BlockResult};
use mm_manager::mm;
use mm_traits::FrameAllocator;

// — WireSaint: Shared virtio plumbing. No more copy-paste virtqueue séances.
use virtio_core::status;
use virtio_core::virtqueue::{desc_flags, VirtqDesc};
use virtio_core::{phys_to_virt, virt_to_phys, Virtqueue};

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

// — WireSaint: Queue size constant lives here so DMA buffer math stays sane
const QUEUE_SIZE: usize = virtio_core::virtqueue::MAX_QUEUE_SIZE;

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

            // — TorqueJax: Set queue addresses via the public getter
            let (desc_phys, avail_phys, used_phys) = queue.physical_addresses();
            core::ptr::write_volatile(
                (mmio_base + mmio::QUEUE_DESC_LOW) as *mut u32,
                desc_phys as u32,
            );
            core::ptr::write_volatile(
                (mmio_base + mmio::QUEUE_DESC_HIGH) as *mut u32,
                (desc_phys >> 32) as u32,
            );
            core::ptr::write_volatile(
                (mmio_base + mmio::QUEUE_AVAIL_LOW) as *mut u32,
                avail_phys as u32,
            );
            core::ptr::write_volatile(
                (mmio_base + mmio::QUEUE_AVAIL_HIGH) as *mut u32,
                (avail_phys >> 32) as u32,
            );
            core::ptr::write_volatile(
                (mmio_base + mmio::QUEUE_USED_LOW) as *mut u32,
                used_phys as u32,
            );
            core::ptr::write_volatile(
                (mmio_base + mmio::QUEUE_USED_HIGH) as *mut u32,
                (used_phys >> 32) as u32,
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
            // — WireSaint: PCI legacy I/O port notification via os_core HAL
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

        // — TorqueJax: Build descriptor chain via write_desc. No more raw pointer
        // arithmetic into virtqueue internals — that's virtio-core's problem now.
        unsafe {
            if let (Some(desc_data), Some(data)) = (desc_data, data.as_ref()) {
                // Header → Data → Status chain
                queue.write_desc(
                    desc_header,
                    header_phys,
                    core::mem::size_of::<VirtioBlkReqHeader>() as u32,
                    desc_flags::NEXT,
                    desc_data,
                );

                // For writes (OUT), copy data from caller buffer to bounce buffer
                if req_type == req_type::OUT {
                    let copy_len = data.len().min(SECTOR_SIZE);
                    core::ptr::copy_nonoverlapping(data.as_ptr(), bounce_ptr, copy_len);
                }

                let mut data_flags = desc_flags::NEXT;
                if req_type == req_type::IN {
                    data_flags |= desc_flags::WRITE;
                }
                queue.write_desc(
                    desc_data,
                    bounce_phys,
                    data.len().min(SECTOR_SIZE) as u32,
                    data_flags,
                    desc_status,
                );

                queue.write_desc(desc_status, status_phys, 1, desc_flags::WRITE, 0);
            } else {
                // Header → Status chain (no data, e.g. flush)
                queue.write_desc(
                    desc_header,
                    header_phys,
                    core::mem::size_of::<VirtioBlkReqHeader>() as u32,
                    desc_flags::NEXT,
                    desc_status,
                );

                queue.write_desc(desc_status, status_phys, 1, desc_flags::WRITE, 0);
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
        //
        // — WireSaint: Poll for completion using volatile reads on the
        // used ring. The Acquire fence + read_volatile in has_completed()
        // ensures we see device DMA writes on every iteration — without
        // volatile, the compiler can hoist the read and spin forever.
        //
        // We use spin_loop() (PAUSE) rather than HLT because block I/O
        // runs during early kernel boot BEFORE the APIC timer starts.
        // HLT without pending interrupts would sleep forever. PAUSE is
        // sufficient: QEMU TCG cooperatively yields to its event loop
        // between translation blocks, so VirtIO completions are delivered
        // even in a tight spin. On KVM/real hardware, the device DMA
        // writes directly to the used ring and the volatile read sees
        // the completion immediately.
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
                // — TorqueJax: dump diagnostic state on timeout before giving up.
                if self.is_pci_io() {
                    let isr = inb(self.io_base() + pci_io::ISR_STATUS);
                    serial_debug_timeout(sector, isr, queue.num_free());
                }
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

            // — TorqueJax: Legacy virtio PFN from the shared Virtqueue getter
            let (desc_phys, _, _) = queue.physical_addresses();
            let queue_pfn = (desc_phys / 4096) as u32;
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

// — WireSaint: Port I/O routed through os_core hooks. No more cfg-gated arch
// imports — the HAL owns the instructions, drivers own the logic. Portable.
#[inline]
fn inb(port: u16) -> u8 {
    unsafe { os_core::inb(port) }
}

#[inline]
fn outb(port: u16, value: u8) {
    unsafe { os_core::outb(port, value) }
}

#[inline]
fn inw(port: u16) -> u16 {
    unsafe { os_core::inw(port) }
}

#[inline]
fn outw(port: u16, value: u16) {
    unsafe { os_core::outw(port, value) }
}

#[inline]
fn inl(port: u16) -> u32 {
    unsafe { os_core::inl(port) }
}

#[inline]
fn outl(port: u16, value: u32) {
    unsafe { os_core::outl(port, value) }
}

// — TorqueJax: diagnostic serial output for block I/O timeout debugging.
// Uses raw COM1 port I/O — no locks, no allocations, ISR-safe.
fn serial_debug_timeout(sector: u64, isr: u8, free_descs: u16) {
    // Write "[BLK-TIMEOUT] s=<sector> isr=<isr> free=<free>\n" to COM1
    serial_write_str("[BLK-TIMEOUT] s=");
    serial_write_hex(sector);
    serial_write_str(" isr=");
    serial_write_hex(isr as u64);
    serial_write_str(" free=");
    serial_write_hex(free_descs as u64);
    serial_write_str("\n");
}

fn serial_write_str(s: &str) {
    for &b in s.as_bytes() {
        serial_write_byte(b);
    }
}

fn serial_write_byte(b: u8) {
    // Wait for UART TX ready (bounded)
    for _ in 0..10000 {
        if inb(0x3FD) & 0x20 != 0 {
            break;
        }
    }
    outb(0x3F8, b);
}

fn serial_write_hex(val: u64) {
    let digits = b"0123456789abcdef";
    serial_write_byte(b'0');
    serial_write_byte(b'x');
    // Find first non-zero nibble
    let mut started = false;
    for i in (0..16).rev() {
        let nibble = ((val >> (i * 4)) & 0xF) as usize;
        if nibble != 0 || started || i == 0 {
            serial_write_byte(digits[nibble]);
            started = true;
        }
    }
}

// ============================================================================
// PciDriver Implementation for Dynamic Driver Loading
// ============================================================================
// — GraveShift: the new way, automatic probe, no manual init.rs wiring

use driver_core::{PciDriver, PciDeviceId, DriverError, DriverBindingData};

/// Device ID table for VirtIO block devices
static VIRTIO_BLK_IDS: &[PciDeviceId] = &[
    PciDeviceId::new(pci::vendor::VIRTIO, pci::virtio_device::BLOCK),   // Legacy
    PciDeviceId::new(pci::vendor::VIRTIO, pci::virtio_modern::BLOCK),   // Modern
];

/// VirtIO block driver for driver-core system
struct VirtioBlkDriver;

impl PciDriver for VirtioBlkDriver {
    fn name(&self) -> &'static str {
        "virtio-blk"
    }

    fn id_table(&self) -> &'static [PciDeviceId] {
        VIRTIO_BLK_IDS
    }

    fn probe(&self, dev: &pci::PciDevice, _id: &PciDeviceId) -> Result<DriverBindingData, DriverError> {
        // SAFETY: PCI device is valid and matches our ID table
        let device = unsafe { VirtioBlk::from_pci(dev) }
            .ok_or(DriverError::InitFailed)?;

        // Register with block subsystem
        let name = alloc::format!("vd{}", get_next_drive_letter());
        let device_name = name.clone();
        block::register_device(name, Box::new(device));

        // Store device name for removal
        let name_ptr = alloc::boxed::Box::into_raw(alloc::boxed::Box::new(device_name));
        Ok(DriverBindingData::new(name_ptr as usize))
    }

    unsafe fn remove(&self, _dev: &pci::PciDevice, binding_data: DriverBindingData) {
        // — GraveShift: Rust 2024 wants unsafe blocks inside unsafe fn. Fine.
        let name_ptr = unsafe { binding_data.as_ptr::<alloc::string::String>() };
        if !name_ptr.is_null() {
            let name = unsafe { alloc::boxed::Box::from_raw(name_ptr) };
            let _ = block::unregister_device(&name);
        }
    }
}

/// Static driver instance for registration
static VIRTIO_BLK_DRIVER: VirtioBlkDriver = VirtioBlkDriver;

// Register driver via compile-time linker section
driver_core::register_pci_driver!(VIRTIO_BLK_DRIVER);

/// Drive letter counter for device naming (vda, vdb, vdc, ...)
static DRIVE_COUNTER: core::sync::atomic::AtomicU8 = core::sync::atomic::AtomicU8::new(0);

fn get_next_drive_letter() -> char {
    let n = DRIVE_COUNTER.fetch_add(1, core::sync::atomic::Ordering::SeqCst);
    (b'a' + n) as char
}
