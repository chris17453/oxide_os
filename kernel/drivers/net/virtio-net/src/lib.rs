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

// — NeonRoot: Shared VirtIO plumbing. The ring belongs to virtio-core now.
use virtio_core::status as dev_status;
use virtio_core::virtqueue::desc_flags;
use virtio_core::{phys_to_virt, virt_to_phys, Virtqueue};

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

// — NeonRoot: dev_status constants imported from virtio_core::status above

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

// — NeonRoot: Queue size from virtio-core, buffer constants stay local
const QUEUE_SIZE: usize = virtio_core::virtqueue::MAX_QUEUE_SIZE;

/// Number of RX buffers to pre-post
const RX_BUFFER_COUNT: usize = 64;

/// Size of each RX buffer (MTU + header + some padding)
const RX_BUFFER_SIZE: usize = 2048;

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
            let (rx_desc_phys, _, _) = rx_queue.physical_addresses();
            let rx_pfn = (rx_desc_phys / 4096) as u32;
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
            let (tx_desc_phys, _, _) = tx_queue.physical_addresses();
            let tx_pfn = (tx_desc_phys / 4096) as u32;
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

            // — NeonRoot: Set up descriptor for device to write into
            unsafe {
                rx_queue.write_desc(desc_idx, phys, RX_BUFFER_SIZE as u32, desc_flags::WRITE, 0);
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
        while tx_queue.has_completed() {
            if let Some((id, _len)) = tx_queue.pop_used() {
                // — NeonRoot: free_chain handles single-desc chains just fine
                tx_queue.free_chain(id);
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

            // — NeonRoot: Descriptor setup through virtio-core's safe accessor
            tx_queue.write_desc(desc_idx, phys, total_len as u32, 0, 0);
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
        if !rx_queue.has_completed() {
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

// ============================================================================
// PciDriver Implementation for Dynamic Driver Loading
// ============================================================================
// — NeonRoot: packet pusher, auto-probed

use driver_core::{PciDriver, PciDeviceId, DriverError, DriverBindingData};

/// Device ID table for VirtIO network devices
static VIRTIO_NET_IDS: &[PciDeviceId] = &[
    PciDeviceId::new(pci::vendor::VIRTIO, pci::virtio_device::NET),   // Legacy
    PciDeviceId::new(pci::vendor::VIRTIO, pci::virtio_modern::NET),   // Modern
];

/// VirtIO network driver for driver-core system
struct VirtioNetDriver;

impl PciDriver for VirtioNetDriver {
    fn name(&self) -> &'static str {
        "virtio-net"
    }

    fn id_table(&self) -> &'static [PciDeviceId] {
        VIRTIO_NET_IDS
    }

    fn probe(&self, dev: &pci::PciDevice, _id: &PciDeviceId) -> Result<DriverBindingData, DriverError> {
        // SAFETY: PCI device is valid and matches our ID table
        let device = unsafe { VirtioNet::from_pci(dev) }
            .ok_or(DriverError::InitFailed)?;

        // Register with network subsystem
        let device = alloc::sync::Arc::new(device);
        net::register_device(device.clone());

        // Store Arc pointer for cleanup
        let binding_data = alloc::sync::Arc::into_raw(device) as usize;
        Ok(DriverBindingData::new(binding_data))
    }

    unsafe fn remove(&self, _dev: &pci::PciDevice, binding_data: DriverBindingData) {
        // — WireSaint: Rust 2024 needs explicit unsafe blocks inside unsafe fn
        let _device = unsafe { alloc::sync::Arc::from_raw(
            unsafe { binding_data.as_ptr::<VirtioNet>() }
        ) };
    }
}

/// Static driver instance for registration
static VIRTIO_NET_DRIVER: VirtioNetDriver = VirtioNetDriver;

// Register driver via compile-time linker section
driver_core::register_pci_driver!(VIRTIO_NET_DRIVER);
