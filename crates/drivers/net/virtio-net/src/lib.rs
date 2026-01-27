//! VirtIO Network Driver
//!
//! Implements the virtio-net specification for virtual network devices.
//! Supports both MMIO and PCI-based VirtIO devices.

#![no_std]
#![allow(unused)]

extern crate alloc;

use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU16, AtomicU32, Ordering};
use spin::Mutex;

use mm_manager::mm;
use mm_traits::FrameAllocator;
use net::{DeviceFlags, MacAddress, NetError, NetResult, NetStats, NetworkDevice};
use pci::{PciBar, PciDevice};

/// VirtIO network header (prepended to packets)
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct VirtioNetHdr {
    /// Flags
    pub flags: u8,
    /// GSO type
    pub gso_type: u8,
    /// Header length
    pub hdr_len: u16,
    /// GSO size
    pub gso_size: u16,
    /// Checksum start
    pub csum_start: u16,
    /// Checksum offset
    pub csum_offset: u16,
    /// Number of buffers (mergeable only)
    pub num_buffers: u16,
}

/// VirtIO net header size (without MRG_RXBUF feature - no num_buffers field)
/// flags(1) + gso_type(1) + hdr_len(2) + gso_size(2) + csum_start(2) + csum_offset(2) = 10
const VIRTIO_NET_HDR_SIZE: usize = 10;

/// VirtIO net header flags
pub mod hdr_flags {
    pub const NEEDS_CSUM: u8 = 1;
    pub const DATA_VALID: u8 = 2;
    pub const RSC_INFO: u8 = 4;
}

/// VirtIO net GSO types
pub mod gso_type {
    pub const NONE: u8 = 0;
    pub const TCPV4: u8 = 1;
    pub const UDP: u8 = 3;
    pub const TCPV6: u8 = 4;
    pub const ECN: u8 = 0x80;
}

/// VirtIO net feature bits
pub mod features {
    pub const CSUM: u64 = 1 << 0;
    pub const GUEST_CSUM: u64 = 1 << 1;
    pub const CTRL_GUEST_OFFLOADS: u64 = 1 << 2;
    pub const MTU: u64 = 1 << 3;
    pub const MAC: u64 = 1 << 5;
    pub const GSO: u64 = 1 << 6;
    pub const GUEST_TSO4: u64 = 1 << 7;
    pub const GUEST_TSO6: u64 = 1 << 8;
    pub const GUEST_ECN: u64 = 1 << 9;
    pub const GUEST_UFO: u64 = 1 << 10;
    pub const HOST_TSO4: u64 = 1 << 11;
    pub const HOST_TSO6: u64 = 1 << 12;
    pub const HOST_ECN: u64 = 1 << 13;
    pub const HOST_UFO: u64 = 1 << 14;
    pub const MRG_RXBUF: u64 = 1 << 15;
    pub const STATUS: u64 = 1 << 16;
    pub const CTRL_VQ: u64 = 1 << 17;
    pub const CTRL_RX: u64 = 1 << 18;
    pub const CTRL_VLAN: u64 = 1 << 19;
    pub const GUEST_ANNOUNCE: u64 = 1 << 21;
    pub const MQ: u64 = 1 << 22;
    pub const CTRL_MAC_ADDR: u64 = 1 << 23;
}

/// VirtIO net status bits
pub mod status {
    pub const LINK_UP: u16 = 1;
    pub const ANNOUNCE: u16 = 2;
}

/// VirtIO device status
mod dev_status {
    pub const ACKNOWLEDGE: u8 = 1;
    pub const DRIVER: u8 = 2;
    pub const DRIVER_OK: u8 = 4;
    pub const FEATURES_OK: u8 = 8;
    pub const DEVICE_NEEDS_RESET: u8 = 64;
    pub const FAILED: u8 = 128;
}

/// VirtIO net config space
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct VirtioNetConfig {
    /// MAC address
    pub mac: [u8; 6],
    /// Status
    pub status: u16,
    /// Max virtqueue pairs
    pub max_virtqueue_pairs: u16,
    /// MTU
    pub mtu: u16,
}

/// Virtqueue descriptor
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct VirtqDesc {
    /// Physical address
    pub addr: u64,
    /// Length
    pub len: u32,
    /// Flags
    pub flags: u16,
    /// Next descriptor index
    pub next: u16,
}

/// Virtqueue descriptor flags
pub mod desc_flags {
    pub const NEXT: u16 = 1;
    pub const WRITE: u16 = 2;
    pub const INDIRECT: u16 = 4;
}

/// Virtqueue available ring
#[repr(C)]
pub struct VirtqAvail {
    pub flags: u16,
    pub idx: u16,
    pub ring: [u16; 256],
    pub used_event: u16,
}

/// Virtqueue used element
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct VirtqUsedElem {
    pub id: u32,
    pub len: u32,
}

/// Virtqueue used ring
#[repr(C)]
pub struct VirtqUsed {
    pub flags: u16,
    pub idx: u16,
    pub ring: [VirtqUsedElem; 256],
    pub avail_event: u16,
}

/// Queue size (number of descriptors)
const QUEUE_SIZE: usize = 256;

/// Number of RX buffers to pre-post
const RX_BUFFER_COUNT: usize = 64;

/// Size of each RX buffer (MTU + header + some padding)
const RX_BUFFER_SIZE: usize = 2048;

/// Physical memory mapping base
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

/// Virtqueue management
struct Virtqueue {
    /// Descriptor table (physical address)
    desc_phys: u64,
    /// Descriptor table (virtual address)
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
    /// Allocate and initialize a new virtqueue for legacy VirtIO PCI
    ///
    /// # Safety
    /// Caller must ensure proper memory management and that the returned
    /// virtqueue is used correctly with a VirtIO device.
    unsafe fn new_legacy(num: u16) -> Option<Self> {
        let desc_size = (num as usize) * core::mem::size_of::<VirtqDesc>();
        let avail_size = 6 + 2 * (num as usize);
        let used_size = 6 + 8 * (num as usize);

        // Used ring must be at page boundary for legacy
        let avail_end = desc_size + avail_size;
        let used_offset = (avail_end + 4095) & !4095;

        let total_size = used_offset + used_size;
        let num_pages = (total_size + 4095) / 4096;

        // Allocate physical frames
        let phys_addr = mm().alloc_contiguous(num_pages).ok()?;
        let phys_base = phys_addr.as_u64();

        let virt_base = phys_to_virt(phys_base);
        let ptr = virt_base as *mut u8;

        // Zero the memory
        // SAFETY: ptr points to freshly allocated memory of size num_pages * 4096
        unsafe { core::ptr::write_bytes(ptr, 0, num_pages * 4096) };

        let desc = ptr as *mut VirtqDesc;
        // SAFETY: desc_size is within the allocated region
        let avail = unsafe { ptr.add(desc_size) } as *mut VirtqAvail;
        // SAFETY: used_offset is within the allocated region
        let used = unsafe { ptr.add(used_offset) } as *mut VirtqUsed;

        let desc_phys = phys_base;
        let avail_phys = phys_base + desc_size as u64;
        let used_phys = phys_base + used_offset as u64;

        // Initialize free list
        for i in 0..num {
            // SAFETY: i is within bounds of the descriptor array
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

    /// Free a descriptor
    fn free_desc(&mut self, idx: u16) {
        unsafe {
            let desc = &mut *self.desc.add(idx as usize);
            desc.next = self.free_head;
            self.free_head = idx;
            self.num_free += 1;
        }
    }

    /// Add a descriptor to the available ring
    fn add_available(&mut self, head: u16) {
        unsafe {
            let avail = &mut *self.avail;
            let idx = avail.idx as usize % self.num as usize;
            avail.ring[idx] = head;
            core::sync::atomic::fence(Ordering::Release);
            avail.idx = avail.idx.wrapping_add(1);
        }
    }

    /// Check if there are completed items in the used ring
    fn has_used(&self) -> bool {
        unsafe {
            let used = &*self.used;
            core::sync::atomic::fence(Ordering::Acquire);
            used.idx != self.last_used_idx
        }
    }

    /// Pop next used item
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

/// RX buffer management
struct RxBuffers {
    /// Physical address of buffer region
    phys_base: u64,
    /// Virtual address of buffer region
    virt_base: *mut u8,
    /// Which descriptors are in use (true = posted to device)
    in_use: [bool; RX_BUFFER_COUNT],
}

impl RxBuffers {
    /// Allocate RX buffers
    fn new() -> Option<Self> {
        let total_size = RX_BUFFER_COUNT * RX_BUFFER_SIZE;
        let num_pages = (total_size + 4095) / 4096;

        let phys_addr = mm().alloc_contiguous(num_pages).ok()?;
        let phys_base = phys_addr.as_u64();
        let virt_base = phys_to_virt(phys_base) as *mut u8;

        // Zero the memory
        unsafe {
            core::ptr::write_bytes(virt_base, 0, num_pages * 4096);
        }

        Some(RxBuffers {
            phys_base,
            virt_base,
            in_use: [false; RX_BUFFER_COUNT],
        })
    }

    /// Get buffer physical and virtual addresses for a slot
    fn buffer(&self, slot: usize) -> (*mut u8, u64) {
        let offset = slot * RX_BUFFER_SIZE;
        let virt = unsafe { self.virt_base.add(offset) };
        let phys = self.phys_base + offset as u64;
        (virt, phys)
    }
}

/// TX buffer management
struct TxBuffers {
    /// Physical address of buffer region
    phys_base: u64,
    /// Virtual address of buffer region
    virt_base: *mut u8,
}

impl TxBuffers {
    /// Allocate TX buffers (one buffer per possible descriptor)
    fn new() -> Option<Self> {
        let total_size = QUEUE_SIZE * RX_BUFFER_SIZE;
        let num_pages = (total_size + 4095) / 4096;

        let phys_addr = mm().alloc_contiguous(num_pages).ok()?;
        let phys_base = phys_addr.as_u64();
        let virt_base = phys_to_virt(phys_base) as *mut u8;

        unsafe {
            core::ptr::write_bytes(virt_base, 0, num_pages * 4096);
        }

        Some(TxBuffers {
            phys_base,
            virt_base,
        })
    }

    /// Get buffer for a descriptor slot
    fn buffer(&self, slot: usize) -> (*mut u8, u64) {
        let offset = slot * RX_BUFFER_SIZE;
        let virt = unsafe { self.virt_base.add(offset) };
        let phys = self.phys_base + offset as u64;
        (virt, phys)
    }
}

/// VirtIO device mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VirtioMode {
    /// Memory-mapped I/O (VirtIO v2)
    Mmio,
    /// PCI I/O port (legacy transitional)
    PciIo,
    /// PCI memory-mapped (modern)
    PciMem,
}

/// VirtIO network device
pub struct VirtioNet {
    /// Device mode
    mode: VirtioMode,
    /// Base address (MMIO or I/O port)
    base: u64,
    /// MAC address
    mac: MacAddress,
    /// MTU
    mtu: usize,
    /// Device flags
    flags: Mutex<DeviceFlags>,
    /// Statistics
    stats: Mutex<NetStats>,
    /// RX virtqueue
    rx_queue: Mutex<Virtqueue>,
    /// TX virtqueue
    tx_queue: Mutex<Virtqueue>,
    /// RX buffers
    rx_buffers: Mutex<RxBuffers>,
    /// TX buffers
    tx_buffers: Mutex<TxBuffers>,
    /// Negotiated features
    features: u64,
}

impl VirtioNet {
    /// PCI legacy I/O port register offsets
    const PCI_IO_DEVICE_FEATURES: u16 = 0x00;
    const PCI_IO_DRIVER_FEATURES: u16 = 0x04;
    const PCI_IO_QUEUE_ADDRESS: u16 = 0x08;
    const PCI_IO_QUEUE_SIZE: u16 = 0x0C;
    const PCI_IO_QUEUE_SELECT: u16 = 0x0E;
    const PCI_IO_QUEUE_NOTIFY: u16 = 0x10;
    const PCI_IO_DEVICE_STATUS: u16 = 0x12;
    const PCI_IO_ISR_STATUS: u16 = 0x13;
    const PCI_IO_CONFIG: u16 = 0x14;

    /// Create a VirtIO network device from a PCI device
    pub unsafe fn from_pci(pci_dev: &PciDevice) -> Option<Self> {
        if !pci_dev.is_virtio_net() {
            return None;
        }

        // Enable the device
        pci::enable_bus_master(pci_dev.address);
        pci::enable_io_space(pci_dev.address);
        pci::enable_memory_space(pci_dev.address);

        let (mode, base) = match pci_dev.bars[0] {
            PciBar::Io { port, .. } => (VirtioMode::PciIo, port as u64),
            PciBar::Memory { address, .. } => (VirtioMode::PciMem, address),
            PciBar::None => return None,
        };

        if mode == VirtioMode::PciIo {
            let io_base = base as u16;

            // Reset device
            outb(io_base + Self::PCI_IO_DEVICE_STATUS, 0);

            // Set ACKNOWLEDGE
            outb(
                io_base + Self::PCI_IO_DEVICE_STATUS,
                dev_status::ACKNOWLEDGE,
            );

            // Set DRIVER
            outb(
                io_base + Self::PCI_IO_DEVICE_STATUS,
                dev_status::ACKNOWLEDGE | dev_status::DRIVER,
            );

            // Read device features
            let device_features = inl(io_base + Self::PCI_IO_DEVICE_FEATURES) as u64;

            // Negotiate features - we want MAC and STATUS
            let wanted_features = features::MAC | features::STATUS;
            let negotiated = device_features & wanted_features;

            // Write driver features
            outl(io_base + Self::PCI_IO_DRIVER_FEATURES, negotiated as u32);

            // Set FEATURES_OK
            outb(
                io_base + Self::PCI_IO_DEVICE_STATUS,
                dev_status::ACKNOWLEDGE | dev_status::DRIVER | dev_status::FEATURES_OK,
            );

            // Verify FEATURES_OK
            let status = inb(io_base + Self::PCI_IO_DEVICE_STATUS);
            if status & dev_status::FEATURES_OK == 0 {
                return None;
            }

            // Read MAC address
            let mut mac = [0u8; 6];
            for i in 0..6 {
                mac[i] = inb(io_base + Self::PCI_IO_CONFIG + i as u16);
            }

            // Set up RX virtqueue (queue 0)
            outw(io_base + Self::PCI_IO_QUEUE_SELECT, 0);
            let rx_queue_size = inw(io_base + Self::PCI_IO_QUEUE_SIZE);
            if rx_queue_size == 0 {
                return None;
            }
            let rx_queue_size = rx_queue_size.min(QUEUE_SIZE as u16);

            // SAFETY: We've validated queue size and device is in correct state
            let rx_queue = unsafe { Virtqueue::new_legacy(rx_queue_size)? };
            let rx_pfn = (rx_queue.desc_phys / 4096) as u32;
            outl(io_base + Self::PCI_IO_QUEUE_ADDRESS, rx_pfn);

            // Set up TX virtqueue (queue 1)
            outw(io_base + Self::PCI_IO_QUEUE_SELECT, 1);
            let tx_queue_size = inw(io_base + Self::PCI_IO_QUEUE_SIZE);
            if tx_queue_size == 0 {
                return None;
            }
            let tx_queue_size = tx_queue_size.min(QUEUE_SIZE as u16);

            // SAFETY: We've validated queue size and device is in correct state
            let tx_queue = unsafe { Virtqueue::new_legacy(tx_queue_size)? };
            let tx_pfn = (tx_queue.desc_phys / 4096) as u32;
            outl(io_base + Self::PCI_IO_QUEUE_ADDRESS, tx_pfn);

            // Allocate DMA-safe buffers
            let rx_buffers = RxBuffers::new()?;
            let tx_buffers = TxBuffers::new()?;

            // Set DRIVER_OK
            outb(
                io_base + Self::PCI_IO_DEVICE_STATUS,
                dev_status::ACKNOWLEDGE
                    | dev_status::DRIVER
                    | dev_status::FEATURES_OK
                    | dev_status::DRIVER_OK,
            );

            let mut device = VirtioNet {
                mode,
                base,
                mac: MacAddress(mac),
                mtu: 1500,
                flags: Mutex::new(DeviceFlags {
                    up: true,
                    broadcast: true,
                    multicast: true,
                    ..Default::default()
                }),
                stats: Mutex::new(NetStats::default()),
                rx_queue: Mutex::new(rx_queue),
                tx_queue: Mutex::new(tx_queue),
                rx_buffers: Mutex::new(rx_buffers),
                tx_buffers: Mutex::new(tx_buffers),
                features: negotiated,
            };

            // Post initial RX buffers
            device.post_rx_buffers();

            Some(device)
        } else {
            None // Only PCI I/O mode supported for now
        }
    }

    /// Post RX buffers to the device
    /// Uses direct mapping: descriptor index N corresponds to buffer slot N
    fn post_rx_buffers(&self) {
        let mut rx_queue = self.rx_queue.lock();
        let mut rx_buffers = self.rx_buffers.lock();

        let mut posted = 0u32;
        for slot in 0..RX_BUFFER_COUNT {
            if rx_buffers.in_use[slot] {
                continue;
            }

            // Use slot as descriptor index for direct mapping
            let desc_idx = slot as u16;
            let (_, phys) = rx_buffers.buffer(slot);

            // Set up descriptor for device to write into
            unsafe {
                let desc = &mut *rx_queue.desc.add(desc_idx as usize);
                desc.addr = phys;
                desc.len = RX_BUFFER_SIZE as u32;
                desc.flags = desc_flags::WRITE; // Device writes to this buffer
                desc.next = 0;
            }

            rx_queue.add_available(desc_idx);
            rx_buffers.in_use[slot] = true;
            posted += 1;
        }

        // Memory barrier before notifying device
        core::sync::atomic::fence(Ordering::SeqCst);

        // Notify device of new RX buffers
        self.notify(0);
    }

    /// Reclaim completed TX descriptors
    fn reclaim_tx_descriptors(&self) {
        // Read ISR to clear interrupt status (helps with some QEMU versions)
        let _isr = self.read_isr();

        let mut tx_queue = self.tx_queue.lock();
        while tx_queue.has_used() {
            if let Some((id, _len)) = tx_queue.pop_used() {
                tx_queue.free_desc(id);
            }
        }
    }

    /// Notify the device
    fn notify(&self, queue: u16) {
        match self.mode {
            VirtioMode::PciIo => {
                let io_base = self.base as u16;
                outw(io_base + Self::PCI_IO_QUEUE_NOTIFY, queue);
            }
            _ => {}
        }
    }

    /// Read ISR status (and clear interrupt)
    fn read_isr(&self) -> u8 {
        match self.mode {
            VirtioMode::PciIo => {
                let io_base = self.base as u16;
                inb(io_base + Self::PCI_IO_ISR_STATUS)
            }
            _ => 0,
        }
    }

    /// Check if link is up
    fn read_link_status(&self) -> bool {
        if self.features & features::STATUS == 0 {
            return true;
        }

        match self.mode {
            VirtioMode::PciIo => {
                let io_base = self.base as u16;
                let net_status = inw(io_base + Self::PCI_IO_CONFIG + 6);
                net_status & status::LINK_UP != 0
            }
            _ => true,
        }
    }
}

// I/O port access functions
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

impl NetworkDevice for VirtioNet {
    fn name(&self) -> &str {
        "eth0"
    }

    fn mac_address(&self) -> MacAddress {
        self.mac
    }

    fn mtu(&self) -> usize {
        self.mtu
    }

    fn transmit(&self, packet: &[u8]) -> NetResult<()> {
        if packet.len() > self.mtu + 14 {
            return Err(NetError::InvalidArgument);
        }

        // First, reclaim any completed TX descriptors
        self.reclaim_tx_descriptors();

        let mut tx_queue = self.tx_queue.lock();
        let tx_buffers = self.tx_buffers.lock();

        // Allocate a descriptor
        let desc_idx = tx_queue.alloc_desc().ok_or(NetError::NoBuffers)?;
        let slot = desc_idx as usize;

        // Get DMA-safe buffer
        let (virt, phys) = tx_buffers.buffer(slot);

        // Build packet with virtio header
        let total_len = VIRTIO_NET_HDR_SIZE + packet.len();
        unsafe {
            // Zero the header
            core::ptr::write_bytes(virt, 0, VIRTIO_NET_HDR_SIZE);
            // Copy packet data after header
            core::ptr::copy_nonoverlapping(
                packet.as_ptr(),
                virt.add(VIRTIO_NET_HDR_SIZE),
                packet.len(),
            );

            // Set up descriptor
            let desc = &mut *tx_queue.desc.add(desc_idx as usize);
            desc.addr = phys;
            desc.len = total_len as u32;
            desc.flags = 0; // Device reads from this buffer
            desc.next = 0;
        }

        // Add to available ring
        tx_queue.add_available(desc_idx);

        // Update stats
        {
            let mut stats = self.stats.lock();
            stats.tx_packets += 1;
            stats.tx_bytes += packet.len() as u64;
        }

        // Memory barrier before notifying device
        core::sync::atomic::fence(Ordering::SeqCst);

        // Release locks before notify
        drop(tx_buffers);
        drop(tx_queue);

        // Notify device (fire and forget - don't wait for completion)
        self.notify(1);

        Ok(())
    }

    fn receive(&self, buf: &mut [u8]) -> NetResult<Option<usize>> {
        let mut rx_queue = self.rx_queue.lock();
        let mut rx_buffers = self.rx_buffers.lock();

        // Check for completed RX buffers
        if !rx_queue.has_used() {
            return Ok(None);
        }

        let (desc_idx, len) = rx_queue.pop_used().unwrap();

        // With direct mapping, descriptor index IS the buffer slot
        let slot = desc_idx as usize;
        if slot >= RX_BUFFER_COUNT {
            // Invalid descriptor index, skip
            return Ok(None);
        }

        let (virt, _) = rx_buffers.buffer(slot);

        // Skip virtio header, copy packet data
        let data_len = (len as usize).saturating_sub(VIRTIO_NET_HDR_SIZE);
        let copy_len = data_len.min(buf.len());

        if copy_len > 0 {
            unsafe {
                core::ptr::copy_nonoverlapping(
                    virt.add(VIRTIO_NET_HDR_SIZE),
                    buf.as_mut_ptr(),
                    copy_len,
                );
            }
        }

        // Mark buffer as not in use (will be re-posted)
        rx_buffers.in_use[slot] = false;

        // Note: Don't use free_desc for RX - we use direct mapping
        // The descriptor will be re-used when we re-post this buffer

        // Update stats
        {
            let mut stats = self.stats.lock();
            stats.rx_packets += 1;
            stats.rx_bytes += copy_len as u64;
        }

        // Re-post this RX buffer
        drop(rx_buffers);
        drop(rx_queue);
        self.post_rx_buffers();

        Ok(Some(copy_len))
    }

    fn link_up(&self) -> bool {
        self.read_link_status()
    }

    fn flags(&self) -> DeviceFlags {
        *self.flags.lock()
    }

    fn set_flags(&self, flags: DeviceFlags) -> NetResult<()> {
        *self.flags.lock() = flags;
        Ok(())
    }

    fn stats(&self) -> NetStats {
        *self.stats.lock()
    }
}

// SAFETY: VirtioNet uses internal synchronization (Mutex)
unsafe impl Send for VirtioNet {}
unsafe impl Sync for VirtioNet {}
