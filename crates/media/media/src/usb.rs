//! USB device handling

use alloc::string::String;
use alloc::vec::Vec;
use crate::{Partition, UsbId};

/// USB device class codes
pub mod class {
    /// Mass storage device
    pub const MASS_STORAGE: u8 = 0x08;
    /// Hub
    pub const HUB: u8 = 0x09;
    /// CDC (Communications)
    pub const CDC: u8 = 0x02;
    /// HID (Human Interface Device)
    pub const HID: u8 = 0x03;
    /// Audio
    pub const AUDIO: u8 = 0x01;
    /// Video
    pub const VIDEO: u8 = 0x0E;
    /// Vendor specific
    pub const VENDOR_SPECIFIC: u8 = 0xFF;
}

/// USB event type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsbEventType {
    /// Device connected
    Connected,
    /// Device disconnected
    Disconnected,
    /// Device configuration changed
    ConfigChanged,
}

/// USB device event
#[derive(Debug, Clone)]
pub struct UsbEvent {
    /// Event type
    pub event_type: UsbEventType,
    /// Device information
    pub device: UsbDevice,
    /// Timestamp
    pub timestamp: u64,
}

impl UsbEvent {
    /// Create new event
    pub fn new(event_type: UsbEventType, device: UsbDevice, timestamp: u64) -> Self {
        UsbEvent {
            event_type,
            device,
            timestamp,
        }
    }
}

/// USB device information
#[derive(Debug, Clone)]
pub struct UsbDevice {
    /// Vendor ID
    pub vendor_id: u16,
    /// Product ID
    pub product_id: u16,
    /// Serial number
    pub serial: Option<String>,
    /// Manufacturer string
    pub manufacturer: Option<String>,
    /// Product string
    pub product: Option<String>,
    /// Device class
    pub device_class: u8,
    /// Device subclass
    pub device_subclass: u8,
    /// Device protocol
    pub device_protocol: u8,
    /// USB version (BCD)
    pub usb_version: u16,
    /// Device path in sysfs-like structure
    pub path: String,
    /// Partitions (for mass storage)
    pub partitions: Vec<Partition>,
    /// Bus number
    pub bus: u8,
    /// Device address
    pub address: u8,
    /// Speed
    pub speed: UsbSpeed,
}

impl UsbDevice {
    /// Create new device
    pub fn new(vendor_id: u16, product_id: u16) -> Self {
        UsbDevice {
            vendor_id,
            product_id,
            serial: None,
            manufacturer: None,
            product: None,
            device_class: 0,
            device_subclass: 0,
            device_protocol: 0,
            usb_version: 0x0200,
            path: String::new(),
            partitions: Vec::new(),
            bus: 0,
            address: 0,
            speed: UsbSpeed::Full,
        }
    }

    /// Get USB ID for this device
    pub fn id(&self) -> UsbId {
        UsbId {
            vendor_id: self.vendor_id,
            product_id: self.product_id,
            serial: self.serial.clone(),
        }
    }

    /// Check if this is a mass storage device
    pub fn is_mass_storage(&self) -> bool {
        self.device_class == class::MASS_STORAGE
    }

    /// Get display name
    pub fn display_name(&self) -> String {
        if let Some(ref product) = self.product {
            if let Some(ref manufacturer) = self.manufacturer {
                return alloc::format!("{} {}", manufacturer, product);
            }
            return product.clone();
        }
        alloc::format!("USB Device {:04x}:{:04x}", self.vendor_id, self.product_id)
    }
}

/// USB speed
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsbSpeed {
    /// Low speed (1.5 Mbps)
    Low,
    /// Full speed (12 Mbps)
    Full,
    /// High speed (480 Mbps)
    High,
    /// Super speed (5 Gbps)
    Super,
    /// Super speed plus (10 Gbps)
    SuperPlus,
}

impl UsbSpeed {
    /// Get speed in bits per second
    pub fn bits_per_second(&self) -> u64 {
        match self {
            Self::Low => 1_500_000,
            Self::Full => 12_000_000,
            Self::High => 480_000_000,
            Self::Super => 5_000_000_000,
            Self::SuperPlus => 10_000_000_000,
        }
    }
}

/// USB device filter for matching
#[derive(Debug, Clone, Default)]
pub struct UsbFilter {
    /// Match vendor ID
    pub vendor_id: Option<u16>,
    /// Match product ID
    pub product_id: Option<u16>,
    /// Match device class
    pub device_class: Option<u8>,
    /// Match serial pattern
    pub serial_pattern: Option<String>,
}

impl UsbFilter {
    /// Create new filter
    pub fn new() -> Self {
        Self::default()
    }

    /// Filter by vendor ID
    pub fn vendor(mut self, id: u16) -> Self {
        self.vendor_id = Some(id);
        self
    }

    /// Filter by product ID
    pub fn product(mut self, id: u16) -> Self {
        self.product_id = Some(id);
        self
    }

    /// Filter by device class
    pub fn class(mut self, class: u8) -> Self {
        self.device_class = Some(class);
        self
    }

    /// Check if device matches filter
    pub fn matches(&self, device: &UsbDevice) -> bool {
        if let Some(vid) = self.vendor_id {
            if device.vendor_id != vid {
                return false;
            }
        }
        if let Some(pid) = self.product_id {
            if device.product_id != pid {
                return false;
            }
        }
        if let Some(class) = self.device_class {
            if device.device_class != class {
                return false;
            }
        }
        if let Some(ref pattern) = self.serial_pattern {
            if let Some(ref serial) = device.serial {
                if !serial.contains(pattern.as_str()) {
                    return false;
                }
            } else {
                return false;
            }
        }
        true
    }
}

/// USB device monitor callback trait
pub trait UsbMonitor: Send + Sync {
    /// Called when a device is connected
    fn on_device_connected(&self, device: &UsbDevice);

    /// Called when a device is disconnected
    fn on_device_disconnected(&self, device: &UsbDevice);
}
