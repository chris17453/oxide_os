//! virtio-net Emulation

use alloc::collections::VecDeque;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::Mutex;

use efflux_vmm::device::VirtioDevice;
use crate::VirtioDeviceBase;

/// virtio-net device type
pub const VIRTIO_NET_DEVICE_TYPE: u32 = 1;

/// Network features
pub mod features {
    /// Device handles packets with partial checksum
    pub const VIRTIO_NET_F_CSUM: u64 = 1 << 0;
    /// Guest handles packets with partial checksum
    pub const VIRTIO_NET_F_GUEST_CSUM: u64 = 1 << 1;
    /// Control channel available
    pub const VIRTIO_NET_F_CTRL_VQ: u64 = 1 << 17;
    /// Control channel RX mode support
    pub const VIRTIO_NET_F_CTRL_RX: u64 = 1 << 18;
    /// Control channel VLAN filtering
    pub const VIRTIO_NET_F_CTRL_VLAN: u64 = 1 << 19;
    /// Guest can receive TSOv4
    pub const VIRTIO_NET_F_GUEST_TSO4: u64 = 1 << 7;
    /// Guest can receive TSOv6
    pub const VIRTIO_NET_F_GUEST_TSO6: u64 = 1 << 8;
    /// Guest can receive TSO with ECN
    pub const VIRTIO_NET_F_GUEST_ECN: u64 = 1 << 9;
    /// Guest can receive UFO
    pub const VIRTIO_NET_F_GUEST_UFO: u64 = 1 << 10;
    /// Device can receive TSOv4
    pub const VIRTIO_NET_F_HOST_TSO4: u64 = 1 << 11;
    /// Device can receive TSOv6
    pub const VIRTIO_NET_F_HOST_TSO6: u64 = 1 << 12;
    /// Device can receive TSO with ECN
    pub const VIRTIO_NET_F_HOST_ECN: u64 = 1 << 13;
    /// Device can receive UFO
    pub const VIRTIO_NET_F_HOST_UFO: u64 = 1 << 14;
    /// Device can merge receive buffers
    pub const VIRTIO_NET_F_MRG_RXBUF: u64 = 1 << 15;
    /// Configuration status field available
    pub const VIRTIO_NET_F_STATUS: u64 = 1 << 16;
    /// MAC address available in config
    pub const VIRTIO_NET_F_MAC: u64 = 1 << 5;
    /// Multiple TX/RX queue pairs
    pub const VIRTIO_NET_F_MQ: u64 = 1 << 22;
}

/// Network configuration
#[repr(C)]
#[derive(Clone, Copy)]
pub struct NetConfig {
    /// MAC address
    pub mac: [u8; 6],
    /// Status
    pub status: u16,
    /// Maximum TX/RX queue pairs
    pub max_virtqueue_pairs: u16,
    /// MTU
    pub mtu: u16,
}

impl Default for NetConfig {
    fn default() -> Self {
        NetConfig {
            mac: [0x52, 0x54, 0x00, 0x12, 0x34, 0x56], // QEMU-style MAC
            status: 1, // Link up
            max_virtqueue_pairs: 1,
            mtu: 1500,
        }
    }
}

/// Network status flags
pub mod status_flags {
    pub const VIRTIO_NET_S_LINK_UP: u16 = 1;
    pub const VIRTIO_NET_S_ANNOUNCE: u16 = 2;
}

/// Packet header
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct NetHeader {
    pub flags: u8,
    pub gso_type: u8,
    pub hdr_len: u16,
    pub gso_size: u16,
    pub csum_start: u16,
    pub csum_offset: u16,
    pub num_buffers: u16,
}

/// Header flags
pub mod header_flags {
    pub const VIRTIO_NET_HDR_F_NEEDS_CSUM: u8 = 1;
    pub const VIRTIO_NET_HDR_F_DATA_VALID: u8 = 2;
}

/// GSO types
pub mod gso_type {
    pub const VIRTIO_NET_HDR_GSO_NONE: u8 = 0;
    pub const VIRTIO_NET_HDR_GSO_TCPV4: u8 = 1;
    pub const VIRTIO_NET_HDR_GSO_UDP: u8 = 3;
    pub const VIRTIO_NET_HDR_GSO_TCPV6: u8 = 4;
    pub const VIRTIO_NET_HDR_GSO_ECN: u8 = 0x80;
}

/// virtio-net device
pub struct VirtioNet {
    /// Base device
    base: VirtioDeviceBase,
    /// Configuration
    config: Mutex<NetConfig>,
    /// RX buffer (packets from host to guest)
    rx_buffer: Mutex<VecDeque<Vec<u8>>>,
    /// TX callback (packets from guest to host)
    tx_callback: Mutex<Option<fn(&[u8])>>,
    /// Statistics - packets received
    rx_packets: AtomicU64,
    /// Statistics - packets transmitted
    tx_packets: AtomicU64,
    /// Statistics - bytes received
    rx_bytes: AtomicU64,
    /// Statistics - bytes transmitted
    tx_bytes: AtomicU64,
}

impl VirtioNet {
    /// Create new network device
    pub fn new(mac: [u8; 6]) -> Self {
        let features = features::VIRTIO_NET_F_MAC
            | features::VIRTIO_NET_F_STATUS
            | features::VIRTIO_NET_F_CSUM
            | features::VIRTIO_NET_F_GUEST_CSUM;

        let config = NetConfig {
            mac,
            ..Default::default()
        };

        VirtioNet {
            base: VirtioDeviceBase::new(VIRTIO_NET_DEVICE_TYPE, features, 2), // RX and TX queues
            config: Mutex::new(config),
            rx_buffer: Mutex::new(VecDeque::new()),
            tx_callback: Mutex::new(None),
            rx_packets: AtomicU64::new(0),
            tx_packets: AtomicU64::new(0),
            rx_bytes: AtomicU64::new(0),
            tx_bytes: AtomicU64::new(0),
        }
    }

    /// Set TX callback
    pub fn set_tx_callback(&self, callback: fn(&[u8])) {
        *self.tx_callback.lock() = Some(callback);
    }

    /// Queue packet for RX (from host to guest)
    pub fn queue_rx(&self, packet: Vec<u8>) {
        let len = packet.len() as u64;
        self.rx_buffer.lock().push_back(packet);
        self.rx_packets.fetch_add(1, Ordering::Relaxed);
        self.rx_bytes.fetch_add(len, Ordering::Relaxed);
        self.base.set_interrupt();
    }

    /// Get next RX packet
    pub fn get_rx(&self) -> Option<Vec<u8>> {
        self.rx_buffer.lock().pop_front()
    }

    /// Transmit packet (from guest to host)
    fn transmit(&self, packet: &[u8]) {
        self.tx_packets.fetch_add(1, Ordering::Relaxed);
        self.tx_bytes.fetch_add(packet.len() as u64, Ordering::Relaxed);

        if let Some(callback) = *self.tx_callback.lock() {
            callback(packet);
        }
    }

    /// Read config
    fn read_config_inner(&self, offset: u64, data: &mut [u8]) {
        let config = self.config.lock();
        let config_bytes = unsafe {
            core::slice::from_raw_parts(
                &*config as *const NetConfig as *const u8,
                core::mem::size_of::<NetConfig>(),
            )
        };

        let start = offset as usize;
        let end = (start + data.len()).min(config_bytes.len());
        if start < config_bytes.len() {
            data[..end - start].copy_from_slice(&config_bytes[start..end]);
        }
    }

    /// Set link status
    pub fn set_link_up(&self, up: bool) {
        let mut config = self.config.lock();
        if up {
            config.status |= status_flags::VIRTIO_NET_S_LINK_UP;
        } else {
            config.status &= !status_flags::VIRTIO_NET_S_LINK_UP;
        }
        self.base.inc_config_generation();
    }

    /// Get statistics
    pub fn stats(&self) -> NetStats {
        NetStats {
            rx_packets: self.rx_packets.load(Ordering::Relaxed),
            tx_packets: self.tx_packets.load(Ordering::Relaxed),
            rx_bytes: self.rx_bytes.load(Ordering::Relaxed),
            tx_bytes: self.tx_bytes.load(Ordering::Relaxed),
        }
    }
}

/// Network statistics
#[derive(Clone, Copy, Default)]
pub struct NetStats {
    pub rx_packets: u64,
    pub tx_packets: u64,
    pub rx_bytes: u64,
    pub tx_bytes: u64,
}

impl VirtioDevice for VirtioNet {
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
        // Net config is read-only
    }

    fn reset(&mut self) {
        self.base.reset();
        self.rx_buffer.lock().clear();
    }

    fn process_queue(&mut self, queue: u16) {
        match queue {
            0 => {
                // RX queue - provide packets to guest
                // Would read from rx_buffer and write to guest memory
            }
            1 => {
                // TX queue - receive packets from guest
                // Would read from guest memory and call transmit()
            }
            _ => {}
        }
    }

    fn is_activated(&self) -> bool {
        self.base.is_activated()
    }
}
