//! Network Device Trait

use alloc::string::String;
use alloc::vec::Vec;

use crate::{MacAddress, NetResult, NetStats};

/// Network device information
#[derive(Debug, Clone)]
pub struct NetworkDeviceInfo {
    /// Device name (e.g., "eth0")
    pub name: String,
    /// MAC address
    pub mac: MacAddress,
    /// Maximum transmission unit
    pub mtu: usize,
    /// Link speed in Mbps (0 if unknown)
    pub speed: u32,
    /// Full duplex
    pub full_duplex: bool,
}

/// Network device flags
#[derive(Debug, Clone, Copy, Default)]
pub struct DeviceFlags {
    /// Device is up
    pub up: bool,
    /// Broadcast supported
    pub broadcast: bool,
    /// Loopback device
    pub loopback: bool,
    /// Point-to-point link
    pub point_to_point: bool,
    /// Promiscuous mode
    pub promisc: bool,
    /// Multicast supported
    pub multicast: bool,
}

/// Network device trait
pub trait NetworkDevice: Send + Sync {
    /// Get device name
    fn name(&self) -> &str;

    /// Get MAC address
    fn mac_address(&self) -> MacAddress;

    /// Get maximum transmission unit
    fn mtu(&self) -> usize;

    /// Transmit a packet
    fn transmit(&self, packet: &[u8]) -> NetResult<()>;

    /// Receive a packet (non-blocking)
    /// Returns the number of bytes received, or None if no packet available
    fn receive(&self, buf: &mut [u8]) -> NetResult<Option<usize>>;

    /// Check if link is up
    fn link_up(&self) -> bool;

    /// Get device flags
    fn flags(&self) -> DeviceFlags;

    /// Set device flags
    fn set_flags(&self, flags: DeviceFlags) -> NetResult<()>;

    /// Get device statistics
    fn stats(&self) -> NetStats;

    /// Get device info
    fn info(&self) -> NetworkDeviceInfo {
        NetworkDeviceInfo {
            name: String::from(self.name()),
            mac: self.mac_address(),
            mtu: self.mtu(),
            speed: 0,
            full_duplex: true,
        }
    }
}

/// Loopback network device
pub struct LoopbackDevice {
    /// Receive buffer
    rx_buffer: spin::Mutex<Vec<Vec<u8>>>,
    /// Statistics
    stats: spin::Mutex<NetStats>,
}

impl LoopbackDevice {
    /// Create a new loopback device
    pub fn new() -> Self {
        LoopbackDevice {
            rx_buffer: spin::Mutex::new(Vec::new()),
            stats: spin::Mutex::new(NetStats::default()),
        }
    }
}

impl Default for LoopbackDevice {
    fn default() -> Self {
        Self::new()
    }
}

impl NetworkDevice for LoopbackDevice {
    fn name(&self) -> &str {
        "lo"
    }

    fn mac_address(&self) -> MacAddress {
        MacAddress([0, 0, 0, 0, 0, 0])
    }

    fn mtu(&self) -> usize {
        65536
    }

    fn transmit(&self, packet: &[u8]) -> NetResult<()> {
        // Loopback - just put in receive buffer
        let mut rx = self.rx_buffer.lock();
        rx.push(packet.to_vec());

        let mut stats = self.stats.lock();
        stats.tx_packets += 1;
        stats.tx_bytes += packet.len() as u64;
        stats.rx_packets += 1;
        stats.rx_bytes += packet.len() as u64;

        Ok(())
    }

    fn receive(&self, buf: &mut [u8]) -> NetResult<Option<usize>> {
        let mut rx = self.rx_buffer.lock();
        if let Some(packet) = rx.pop() {
            let len = packet.len().min(buf.len());
            buf[..len].copy_from_slice(&packet[..len]);
            Ok(Some(len))
        } else {
            Ok(None)
        }
    }

    fn link_up(&self) -> bool {
        true
    }

    fn flags(&self) -> DeviceFlags {
        DeviceFlags {
            up: true,
            loopback: true,
            multicast: true,
            ..Default::default()
        }
    }

    fn set_flags(&self, _flags: DeviceFlags) -> NetResult<()> {
        Ok(())
    }

    fn stats(&self) -> NetStats {
        *self.stats.lock()
    }
}
