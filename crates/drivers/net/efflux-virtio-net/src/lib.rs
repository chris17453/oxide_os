//! VirtIO Network Driver
//!
//! Implements the virtio-net specification for virtual network devices.

#![no_std]

extern crate alloc;

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::Mutex;

use efflux_net::{DeviceFlags, MacAddress, NetError, NetResult, NetStats, NetworkDevice};

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

/// VirtIO network device
pub struct VirtioNet {
    /// MMIO base address
    mmio_base: u64,
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
    /// VirtIO MMIO register offsets
    const MAGIC_VALUE: usize = 0x000;
    const VERSION: usize = 0x004;
    const DEVICE_ID: usize = 0x008;
    const VENDOR_ID: usize = 0x00C;
    const DEVICE_FEATURES: usize = 0x010;
    const DEVICE_FEATURES_SEL: usize = 0x014;
    const DRIVER_FEATURES: usize = 0x020;
    const DRIVER_FEATURES_SEL: usize = 0x024;
    const QUEUE_SEL: usize = 0x030;
    const QUEUE_NUM_MAX: usize = 0x034;
    const QUEUE_NUM: usize = 0x038;
    const QUEUE_READY: usize = 0x044;
    const QUEUE_NOTIFY: usize = 0x050;
    const INTERRUPT_STATUS: usize = 0x060;
    const INTERRUPT_ACK: usize = 0x064;
    const STATUS: usize = 0x070;
    const QUEUE_DESC_LOW: usize = 0x080;
    const QUEUE_DESC_HIGH: usize = 0x084;
    const QUEUE_AVAIL_LOW: usize = 0x090;
    const QUEUE_AVAIL_HIGH: usize = 0x094;
    const QUEUE_USED_LOW: usize = 0x0A0;
    const QUEUE_USED_HIGH: usize = 0x0A4;
    const CONFIG: usize = 0x100;

    /// Device ID for network
    const DEVICE_ID_NET: u32 = 1;

    /// Probe for a virtio-net device
    ///
    /// # Safety
    /// The MMIO address must be valid and mapped.
    pub unsafe fn probe(mmio_base: u64) -> Option<Self> {
        unsafe {
            let base = mmio_base as *mut u8;

            // Check magic value
            let magic_ptr = base.add(Self::MAGIC_VALUE) as *const u32;
            let magic = core::ptr::read_volatile(magic_ptr);
            if magic != 0x74726976 {
                // "virt"
                return None;
            }

            // Check version
            let version_ptr = base.add(Self::VERSION) as *const u32;
            let version = core::ptr::read_volatile(version_ptr);
            if version != 2 {
                return None;
            }

            // Check device ID
            let device_id_ptr = base.add(Self::DEVICE_ID) as *const u32;
            let device_id = core::ptr::read_volatile(device_id_ptr);
            if device_id != Self::DEVICE_ID_NET {
                return None;
            }

            // Read device features
            let features_sel_ptr = base.add(Self::DEVICE_FEATURES_SEL) as *mut u32;
            let features_ptr = base.add(Self::DEVICE_FEATURES) as *const u32;

            core::ptr::write_volatile(features_sel_ptr, 0);
            let features_low = core::ptr::read_volatile(features_ptr) as u64;
            core::ptr::write_volatile(features_sel_ptr, 1);
            let features_high = core::ptr::read_volatile(features_ptr) as u64;
            let device_features = features_low | (features_high << 32);

            // Negotiate features
            let wanted_features = features::MAC | features::STATUS | features::MTU;
            let negotiated = device_features & wanted_features;

            // Write driver features
            let driver_features_sel_ptr = base.add(Self::DRIVER_FEATURES_SEL) as *mut u32;
            let driver_features_ptr = base.add(Self::DRIVER_FEATURES) as *mut u32;

            core::ptr::write_volatile(driver_features_sel_ptr, 0);
            core::ptr::write_volatile(driver_features_ptr, negotiated as u32);
            core::ptr::write_volatile(driver_features_sel_ptr, 1);
            core::ptr::write_volatile(driver_features_ptr, (negotiated >> 32) as u32);

            // Read config
            let config_ptr = base.add(Self::CONFIG) as *const VirtioNetConfig;
            let config = core::ptr::read_volatile(config_ptr);

            let mac = MacAddress(config.mac);
            let mtu = if negotiated & features::MTU != 0 {
                config.mtu as usize
            } else {
                1500
            };

            // Set status to DRIVER_OK
            let status_ptr = base.add(Self::STATUS) as *mut u32;
            core::ptr::write_volatile(status_ptr, 0x0F); // ACKNOWLEDGE | DRIVER | FEATURES_OK | DRIVER_OK

            Some(VirtioNet {
                mmio_base,
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

    /// Notify the device of available buffers
    unsafe fn notify(&self, queue: u32) {
        unsafe {
            let base = self.mmio_base as *mut u8;
            let notify_ptr = base.add(Self::QUEUE_NOTIFY) as *mut u32;
            core::ptr::write_volatile(notify_ptr, queue);
        }
    }

    /// Check if link is up from status register
    fn read_link_status(&self) -> bool {
        if self.features & features::STATUS == 0 {
            return true; // Assume up if status feature not negotiated
        }

        unsafe {
            let base = self.mmio_base as *const u8;
            let config_ptr = base.add(Self::CONFIG) as *const VirtioNetConfig;
            let config = core::ptr::read_volatile(config_ptr);
            config.status & status::LINK_UP != 0
        }
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
