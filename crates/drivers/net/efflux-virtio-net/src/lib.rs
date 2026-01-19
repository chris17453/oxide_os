//! VirtIO Network Driver
//!
//! Implements the virtio-net specification for virtual network devices.
//! Supports both MMIO and PCI-based VirtIO devices.

#![no_std]

extern crate alloc;

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::Mutex;

use efflux_net::{DeviceFlags, MacAddress, NetError, NetResult, NetStats, NetworkDevice};
use pci::{PciAddress, PciBar, PciDevice};

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
    /// RX buffer
    rx_buffer: Mutex<Vec<Vec<u8>>>,
    /// TX buffer
    tx_buffer: Mutex<Vec<Vec<u8>>>,
    /// RX virtqueue index
    rx_idx: AtomicU32,
    /// TX virtqueue index
    tx_idx: AtomicU32,
    /// Negotiated features
    features: u64,
}

impl VirtioNet {
    /// VirtIO MMIO register offsets (v2)
    const MMIO_MAGIC_VALUE: usize = 0x000;
    const MMIO_VERSION: usize = 0x004;
    const MMIO_DEVICE_ID: usize = 0x008;
    const MMIO_VENDOR_ID: usize = 0x00C;
    const MMIO_DEVICE_FEATURES: usize = 0x010;
    const MMIO_DEVICE_FEATURES_SEL: usize = 0x014;
    const MMIO_DRIVER_FEATURES: usize = 0x020;
    const MMIO_DRIVER_FEATURES_SEL: usize = 0x024;
    const MMIO_QUEUE_SEL: usize = 0x030;
    const MMIO_QUEUE_NUM_MAX: usize = 0x034;
    const MMIO_QUEUE_NUM: usize = 0x038;
    const MMIO_QUEUE_READY: usize = 0x044;
    const MMIO_QUEUE_NOTIFY: usize = 0x050;
    const MMIO_INTERRUPT_STATUS: usize = 0x060;
    const MMIO_INTERRUPT_ACK: usize = 0x064;
    const MMIO_STATUS: usize = 0x070;
    const MMIO_QUEUE_DESC_LOW: usize = 0x080;
    const MMIO_QUEUE_DESC_HIGH: usize = 0x084;
    const MMIO_QUEUE_AVAIL_LOW: usize = 0x090;
    const MMIO_QUEUE_AVAIL_HIGH: usize = 0x094;
    const MMIO_QUEUE_USED_LOW: usize = 0x0A0;
    const MMIO_QUEUE_USED_HIGH: usize = 0x0A4;
    const MMIO_CONFIG: usize = 0x100;

    /// PCI legacy I/O port register offsets
    const PCI_IO_DEVICE_FEATURES: u16 = 0x00;
    const PCI_IO_DRIVER_FEATURES: u16 = 0x04;
    const PCI_IO_QUEUE_ADDRESS: u16 = 0x08;
    const PCI_IO_QUEUE_SIZE: u16 = 0x0C;
    const PCI_IO_QUEUE_SELECT: u16 = 0x0E;
    const PCI_IO_QUEUE_NOTIFY: u16 = 0x10;
    const PCI_IO_DEVICE_STATUS: u16 = 0x12;
    const PCI_IO_ISR_STATUS: u16 = 0x13;
    const PCI_IO_CONFIG: u16 = 0x14;  // Device-specific config starts here

    /// VirtIO device status bits
    const STATUS_ACKNOWLEDGE: u8 = 1;
    const STATUS_DRIVER: u8 = 2;
    const STATUS_DRIVER_OK: u8 = 4;
    const STATUS_FEATURES_OK: u8 = 8;

    /// Device ID for network
    const DEVICE_ID_NET: u32 = 1;

    /// Probe for a virtio-net device at MMIO address
    ///
    /// # Safety
    /// The MMIO address must be valid and mapped.
    pub unsafe fn probe_mmio(mmio_base: u64) -> Option<Self> {
        unsafe {
            let base = mmio_base as *mut u8;

            // Check magic value
            let magic_ptr = base.add(Self::MMIO_MAGIC_VALUE) as *const u32;
            let magic = core::ptr::read_volatile(magic_ptr);
            if magic != 0x74726976 {
                // "virt"
                return None;
            }

            // Check version
            let version_ptr = base.add(Self::MMIO_VERSION) as *const u32;
            let version = core::ptr::read_volatile(version_ptr);
            if version != 2 {
                return None;
            }

            // Check device ID
            let device_id_ptr = base.add(Self::MMIO_DEVICE_ID) as *const u32;
            let device_id = core::ptr::read_volatile(device_id_ptr);
            if device_id != Self::DEVICE_ID_NET {
                return None;
            }

            // Read device features
            let features_sel_ptr = base.add(Self::MMIO_DEVICE_FEATURES_SEL) as *mut u32;
            let features_ptr = base.add(Self::MMIO_DEVICE_FEATURES) as *const u32;

            core::ptr::write_volatile(features_sel_ptr, 0);
            let features_low = core::ptr::read_volatile(features_ptr) as u64;
            core::ptr::write_volatile(features_sel_ptr, 1);
            let features_high = core::ptr::read_volatile(features_ptr) as u64;
            let device_features = features_low | (features_high << 32);

            // Negotiate features
            let wanted_features = features::MAC | features::STATUS | features::MTU;
            let negotiated = device_features & wanted_features;

            // Write driver features
            let driver_features_sel_ptr = base.add(Self::MMIO_DRIVER_FEATURES_SEL) as *mut u32;
            let driver_features_ptr = base.add(Self::MMIO_DRIVER_FEATURES) as *mut u32;

            core::ptr::write_volatile(driver_features_sel_ptr, 0);
            core::ptr::write_volatile(driver_features_ptr, negotiated as u32);
            core::ptr::write_volatile(driver_features_sel_ptr, 1);
            core::ptr::write_volatile(driver_features_ptr, (negotiated >> 32) as u32);

            // Read config
            let config_ptr = base.add(Self::MMIO_CONFIG) as *const VirtioNetConfig;
            let config = core::ptr::read_volatile(config_ptr);

            let mac = MacAddress(config.mac);
            let mtu = if negotiated & features::MTU != 0 {
                config.mtu as usize
            } else {
                1500
            };

            // Set status to DRIVER_OK
            let status_ptr = base.add(Self::MMIO_STATUS) as *mut u32;
            core::ptr::write_volatile(status_ptr, 0x0F); // ACKNOWLEDGE | DRIVER | FEATURES_OK | DRIVER_OK

            Some(VirtioNet {
                mode: VirtioMode::Mmio,
                base: mmio_base,
                mac,
                mtu,
                flags: Mutex::new(DeviceFlags {
                    up: true,
                    broadcast: true,
                    multicast: true,
                    ..Default::default()
                }),
                stats: Mutex::new(NetStats::default()),
                rx_buffer: Mutex::new(Vec::new()),
                tx_buffer: Mutex::new(Vec::new()),
                rx_idx: AtomicU32::new(0),
                tx_idx: AtomicU32::new(0),
                features: negotiated,
            })
        }
    }

    /// Create a VirtIO network device from a PCI device
    ///
    /// # Safety
    /// The PCI device must be a valid VirtIO network device.
    pub unsafe fn from_pci(pci_dev: &PciDevice) -> Option<Self> {
        // Verify this is a VirtIO network device
        if !pci_dev.is_virtio_net() {
            return None;
        }

        // Enable the device
        pci::enable_bus_master(pci_dev.address);
        pci::enable_io_space(pci_dev.address);
        pci::enable_memory_space(pci_dev.address);

        // Get BAR0 - for legacy/transitional devices, this is I/O ports
        let (mode, base) = match pci_dev.bars[0] {
            PciBar::Io { port, .. } => (VirtioMode::PciIo, port as u64),
            PciBar::Memory { address, .. } => (VirtioMode::PciMem, address),
            PciBar::None => return None,
        };

        if mode == VirtioMode::PciIo {
            // Legacy I/O port based initialization
            let io_base = base as u16;

            // Reset device
            outb(io_base + Self::PCI_IO_DEVICE_STATUS, 0);

            // Set ACKNOWLEDGE
            outb(io_base + Self::PCI_IO_DEVICE_STATUS, Self::STATUS_ACKNOWLEDGE);

            // Set DRIVER
            outb(io_base + Self::PCI_IO_DEVICE_STATUS,
                 Self::STATUS_ACKNOWLEDGE | Self::STATUS_DRIVER);

            // Read device features
            let device_features = inl(io_base + Self::PCI_IO_DEVICE_FEATURES) as u64;

            // Negotiate features
            let wanted_features = features::MAC | features::STATUS;
            let negotiated = device_features & wanted_features;

            // Write driver features
            outl(io_base + Self::PCI_IO_DRIVER_FEATURES, negotiated as u32);

            // Set FEATURES_OK
            outb(io_base + Self::PCI_IO_DEVICE_STATUS,
                 Self::STATUS_ACKNOWLEDGE | Self::STATUS_DRIVER | Self::STATUS_FEATURES_OK);

            // Verify FEATURES_OK
            let status = inb(io_base + Self::PCI_IO_DEVICE_STATUS);
            if status & Self::STATUS_FEATURES_OK == 0 {
                return None; // Feature negotiation failed
            }

            // Read MAC address from device config
            let mut mac = [0u8; 6];
            for i in 0..6 {
                mac[i] = inb(io_base + Self::PCI_IO_CONFIG + i as u16);
            }

            // Set DRIVER_OK
            outb(io_base + Self::PCI_IO_DEVICE_STATUS,
                 Self::STATUS_ACKNOWLEDGE | Self::STATUS_DRIVER |
                 Self::STATUS_FEATURES_OK | Self::STATUS_DRIVER_OK);

            Some(VirtioNet {
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
                rx_buffer: Mutex::new(Vec::new()),
                tx_buffer: Mutex::new(Vec::new()),
                rx_idx: AtomicU32::new(0),
                tx_idx: AtomicU32::new(0),
                features: negotiated,
            })
        } else {
            // Memory-mapped PCI - treat like MMIO
            // Note: Modern PCI VirtIO uses capability structures
            // For now, try MMIO-style access on BAR0
            // Safety: The base address was obtained from PCI BAR0
            unsafe { Self::probe_mmio(base) }
        }
    }

    /// Legacy probe function (alias for probe_mmio)
    #[deprecated(note = "Use probe_mmio instead")]
    pub unsafe fn probe(mmio_base: u64) -> Option<Self> {
        // Safety: Caller guarantees mmio_base is valid
        unsafe { Self::probe_mmio(mmio_base) }
    }

    /// Notify the device of available buffers
    unsafe fn notify(&self, queue: u32) {
        match self.mode {
            VirtioMode::Mmio | VirtioMode::PciMem => {
                let base = self.base as *mut u8;
                // Safety: base is a valid MMIO address from device initialization
                let notify_ptr = unsafe { base.add(Self::MMIO_QUEUE_NOTIFY) as *mut u32 };
                unsafe { core::ptr::write_volatile(notify_ptr, queue) };
            }
            VirtioMode::PciIo => {
                let io_base = self.base as u16;
                outw(io_base + Self::PCI_IO_QUEUE_NOTIFY, queue as u16);
            }
        }
    }

    /// Check if link is up from status register
    fn read_link_status(&self) -> bool {
        if self.features & features::STATUS == 0 {
            return true; // Assume up if status feature not negotiated
        }

        match self.mode {
            VirtioMode::Mmio | VirtioMode::PciMem => {
                unsafe {
                    let base = self.base as *const u8;
                    let config_ptr = base.add(Self::MMIO_CONFIG) as *const VirtioNetConfig;
                    let config = core::ptr::read_volatile(config_ptr);
                    config.status & status::LINK_UP != 0
                }
            }
            VirtioMode::PciIo => {
                // Status is at offset 6 in the config space for net devices
                let io_base = self.base as u16;
                let status = inw(io_base + Self::PCI_IO_CONFIG + 6);
                status & status::LINK_UP != 0
            }
        }
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

impl NetworkDevice for VirtioNet {
    fn name(&self) -> &str {
        "virtio-net0"
    }

    fn mac_address(&self) -> MacAddress {
        self.mac
    }

    fn mtu(&self) -> usize {
        self.mtu
    }

    fn transmit(&self, packet: &[u8]) -> NetResult<()> {
        if packet.len() > self.mtu + 14 {
            // MTU + Ethernet header
            return Err(NetError::InvalidArgument);
        }

        // Create packet with virtio header
        let mut tx_packet = vec![0u8; core::mem::size_of::<VirtioNetHdr>() + packet.len()];
        tx_packet[core::mem::size_of::<VirtioNetHdr>()..].copy_from_slice(packet);

        // Queue the packet
        self.tx_buffer.lock().push(tx_packet);

        // Update stats
        let mut stats = self.stats.lock();
        stats.tx_packets += 1;
        stats.tx_bytes += packet.len() as u64;

        // Notify device (queue 1 = TX)
        unsafe {
            self.notify(1);
        }

        Ok(())
    }

    fn receive(&self, buf: &mut [u8]) -> NetResult<Option<usize>> {
        let mut rx = self.rx_buffer.lock();

        if let Some(packet) = rx.pop() {
            // Skip virtio header
            let hdr_size = core::mem::size_of::<VirtioNetHdr>();
            if packet.len() <= hdr_size {
                return Ok(None);
            }

            let data = &packet[hdr_size..];
            let len = data.len().min(buf.len());
            buf[..len].copy_from_slice(&data[..len]);

            let mut stats = self.stats.lock();
            stats.rx_packets += 1;
            stats.rx_bytes += len as u64;

            Ok(Some(len))
        } else {
            Ok(None)
        }
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
