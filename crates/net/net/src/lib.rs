//! EFFLUX Network Device Layer
//!
//! Provides the network device abstraction and management.

#![no_std]

extern crate alloc;

pub mod device;
pub mod socket;
pub mod interface;
pub mod addr;

pub use device::{NetworkDevice, NetworkDeviceInfo, DeviceFlags, LoopbackDevice};
pub use socket::{Socket, SocketType, SocketDomain, SocketProtocol, SocketState};
pub use interface::{NetworkInterface, InterfaceConfig};
pub use addr::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, MacAddress};

use alloc::sync::Arc;
use alloc::vec::Vec;
use spin::Mutex;

/// Network subsystem errors
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetError {
    /// Device not found
    NotFound,
    /// Device is down
    DeviceDown,
    /// No route to host
    NoRoute,
    /// Host unreachable
    HostUnreachable,
    /// Connection refused
    ConnectionRefused,
    /// Connection reset
    ConnectionReset,
    /// Connection timed out
    TimedOut,
    /// Timeout
    Timeout,
    /// Address already in use
    AddrInUse,
    /// Address not available
    AddrNotAvailable,
    /// Address family not supported
    AddressFamilyNotSupported,
    /// Network is unreachable
    NetworkUnreachable,
    /// Operation would block
    WouldBlock,
    /// Invalid argument
    InvalidArgument,
    /// Not connected
    NotConnected,
    /// Already connected
    AlreadyConnected,
    /// Buffer too small
    BufferTooSmall,
    /// Protocol not supported
    ProtocolNotSupported,
    /// Socket type not supported
    SocketTypeNotSupported,
    /// I/O error
    IoError,
    /// Permission denied
    PermissionDenied,
}

/// Result type for network operations
pub type NetResult<T> = Result<T, NetError>;

/// Global network device registry
static DEVICES: Mutex<Vec<Arc<dyn NetworkDevice>>> = Mutex::new(Vec::new());

/// Register a network device
pub fn register_device(device: Arc<dyn NetworkDevice>) {
    DEVICES.lock().push(device);
}

/// Get device by index
pub fn get_device(index: usize) -> Option<Arc<dyn NetworkDevice>> {
    DEVICES.lock().get(index).cloned()
}

/// Get device by name
pub fn get_device_by_name(name: &str) -> Option<Arc<dyn NetworkDevice>> {
    DEVICES.lock().iter().find(|d| d.name() == name).cloned()
}

/// Get all devices
pub fn devices() -> Vec<Arc<dyn NetworkDevice>> {
    DEVICES.lock().clone()
}

/// Network statistics
#[derive(Debug, Default, Clone, Copy)]
pub struct NetStats {
    /// Packets received
    pub rx_packets: u64,
    /// Packets transmitted
    pub tx_packets: u64,
    /// Bytes received
    pub rx_bytes: u64,
    /// Bytes transmitted
    pub tx_bytes: u64,
    /// Receive errors
    pub rx_errors: u64,
    /// Transmit errors
    pub tx_errors: u64,
    /// Receive drops
    pub rx_dropped: u64,
    /// Transmit drops
    pub tx_dropped: u64,
}
