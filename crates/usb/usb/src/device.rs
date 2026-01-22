//! USB Device

use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU8, Ordering};
use spin::Mutex;

use crate::{
    ConfigDescriptor, DeviceDescriptor, EndpointDescriptor, InterfaceDescriptor, SetupPacket,
    TransferDirection, UsbError, UsbHostController, UsbResult,
};

/// USB device speed
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DeviceSpeed {
    /// Low speed (1.5 Mbps)
    Low,
    /// Full speed (12 Mbps)
    Full,
    /// High speed (480 Mbps)
    #[default]
    High,
    /// SuperSpeed (5 Gbps)
    Super,
    /// SuperSpeed+ (10 Gbps)
    SuperPlus,
}

impl DeviceSpeed {
    /// Get max packet size for control endpoint
    pub fn control_max_packet(&self) -> u16 {
        match self {
            DeviceSpeed::Low => 8,
            DeviceSpeed::Full => 64,
            DeviceSpeed::High => 64,
            DeviceSpeed::Super | DeviceSpeed::SuperPlus => 512,
        }
    }
}

/// USB device information
#[derive(Debug, Clone)]
pub struct UsbDeviceInfo {
    /// Vendor ID
    pub vendor_id: u16,
    /// Product ID
    pub product_id: u16,
    /// Device class
    pub class: u8,
    /// Device subclass
    pub subclass: u8,
    /// Device protocol
    pub protocol: u8,
    /// Manufacturer string
    pub manufacturer: String,
    /// Product string
    pub product: String,
    /// Serial number string
    pub serial: String,
    /// Device speed
    pub speed: DeviceSpeed,
}

/// USB device
pub struct UsbDevice {
    /// Host controller
    controller: Arc<dyn UsbHostController>,
    /// Slot ID
    slot: u8,
    /// USB address
    address: AtomicU8,
    /// Device speed
    speed: DeviceSpeed,
    /// Device descriptor
    device_descriptor: DeviceDescriptor,
    /// Configuration descriptors
    configs: Mutex<Vec<ConfigDescriptor>>,
    /// Active configuration
    active_config: AtomicU8,
    /// Device info cache
    info: Mutex<Option<UsbDeviceInfo>>,
}

impl UsbDevice {
    /// Create a new USB device
    pub fn new(
        controller: Arc<dyn UsbHostController>,
        slot: u8,
        address: u8,
        speed: DeviceSpeed,
        device_descriptor: DeviceDescriptor,
    ) -> Self {
        UsbDevice {
            controller,
            slot,
            address: AtomicU8::new(address),
            speed,
            device_descriptor,
            configs: Mutex::new(Vec::new()),
            active_config: AtomicU8::new(0),
            info: Mutex::new(None),
        }
    }

    /// Get slot ID
    pub fn slot(&self) -> u8 {
        self.slot
    }

    /// Get USB address
    pub fn address(&self) -> u8 {
        self.address.load(Ordering::SeqCst)
    }

    /// Get device speed
    pub fn speed(&self) -> DeviceSpeed {
        self.speed
    }

    /// Get device descriptor
    pub fn device_descriptor(&self) -> &DeviceDescriptor {
        &self.device_descriptor
    }

    /// Get vendor ID
    pub fn vendor_id(&self) -> u16 {
        self.device_descriptor.vendor_id
    }

    /// Get product ID
    pub fn product_id(&self) -> u16 {
        self.device_descriptor.product_id
    }

    /// Get device class
    pub fn class(&self) -> u8 {
        self.device_descriptor.device_class
    }

    /// Get device subclass
    pub fn subclass(&self) -> u8 {
        self.device_descriptor.device_subclass
    }

    /// Get device protocol
    pub fn protocol(&self) -> u8 {
        self.device_descriptor.device_protocol
    }

    /// Check if device matches class/subclass/protocol
    pub fn matches(&self, class: u8, subclass: u8, protocol: u8) -> bool {
        (class == 0xFF || self.class() == class)
            && (subclass == 0xFF || self.subclass() == subclass)
            && (protocol == 0xFF || self.protocol() == protocol)
    }

    /// Perform control transfer
    pub fn control_transfer(
        &self,
        request_type: u8,
        request: u8,
        value: u16,
        index: u16,
        data: Option<&mut [u8]>,
    ) -> UsbResult<usize> {
        let length = data.as_ref().map(|d| d.len() as u16).unwrap_or(0);

        let setup = SetupPacket {
            request_type,
            request,
            value,
            index,
            length,
        };

        self.controller.control_transfer(self.slot, setup, data)
    }

    /// Get string descriptor
    pub fn get_string(&self, index: u8) -> UsbResult<String> {
        if index == 0 {
            return Ok(String::new());
        }

        // First get string length
        let mut buf = [0u8; 4];
        self.control_transfer(
            0x80,
            6,                       // GET_DESCRIPTOR
            (3 << 8) | index as u16, // String descriptor
            0x0409,                  // English
            Some(&mut buf),
        )?;

        let length = buf[0] as usize;
        if length < 2 {
            return Ok(String::new());
        }

        // Get full string
        let mut buf = [0u8; 256];
        self.control_transfer(
            0x80,
            6,
            (3 << 8) | index as u16,
            0x0409,
            Some(&mut buf[..length]),
        )?;

        // Convert UTF-16LE to string
        let mut s = String::new();
        for i in (2..length).step_by(2) {
            if i + 1 < length {
                let c = buf[i] as u16 | ((buf[i + 1] as u16) << 8);
                if c == 0 {
                    break;
                }
                if let Some(ch) = char::from_u32(c as u32) {
                    s.push(ch);
                }
            }
        }

        Ok(s)
    }

    /// Get configuration descriptor
    pub fn get_configuration(&self, index: u8) -> UsbResult<ConfigDescriptor> {
        // Get first 9 bytes for total length
        let mut buf = [0u8; 9];
        self.control_transfer(0x80, 6, (2 << 8) | index as u16, 0, Some(&mut buf))?;

        let total_length = u16::from_le_bytes([buf[2], buf[3]]) as usize;

        // Get full configuration
        let mut full_buf = alloc::vec![0u8; total_length];
        self.control_transfer(0x80, 6, (2 << 8) | index as u16, 0, Some(&mut full_buf))?;

        ConfigDescriptor::from_bytes(&full_buf).ok_or(UsbError::InvalidDescriptor)
    }

    /// Set configuration
    pub fn set_configuration(&self, config_value: u8) -> UsbResult<()> {
        self.control_transfer(
            0x00,
            9, // SET_CONFIGURATION
            config_value as u16,
            0,
            None,
        )?;

        self.active_config.store(config_value, Ordering::SeqCst);
        Ok(())
    }

    /// Get active configuration descriptor
    pub fn configuration(&self) -> Option<ConfigDescriptor> {
        let active = self.active_config.load(Ordering::SeqCst);
        if active == 0 {
            // Try to get first config if not configured
            self.get_configuration(0).ok()
        } else {
            // Get the active configuration
            let configs = self.configs.lock();
            configs
                .iter()
                .find(|c| c.configuration_value == active)
                .cloned()
                .or_else(|| self.get_configuration(active.saturating_sub(1)).ok())
        }
    }

    /// Bulk transfer
    pub fn bulk_transfer(
        &self,
        endpoint: u8,
        data: &mut [u8],
        direction: TransferDirection,
    ) -> UsbResult<usize> {
        self.controller
            .bulk_transfer(self.slot, endpoint, data, direction)
    }

    /// Interrupt transfer
    pub fn interrupt_transfer(
        &self,
        endpoint: u8,
        data: &mut [u8],
        direction: TransferDirection,
    ) -> UsbResult<usize> {
        self.controller
            .interrupt_transfer(self.slot, endpoint, data, direction)
    }

    /// Get device info
    pub fn info(&self) -> UsbDeviceInfo {
        let mut info_lock = self.info.lock();
        if let Some(ref info) = *info_lock {
            return info.clone();
        }

        let manufacturer = self
            .get_string(self.device_descriptor.manufacturer)
            .unwrap_or_default();
        let product = self
            .get_string(self.device_descriptor.product)
            .unwrap_or_default();
        let serial = self
            .get_string(self.device_descriptor.serial_number)
            .unwrap_or_default();

        let info = UsbDeviceInfo {
            vendor_id: self.vendor_id(),
            product_id: self.product_id(),
            class: self.class(),
            subclass: self.subclass(),
            protocol: self.protocol(),
            manufacturer,
            product,
            serial,
            speed: self.speed,
        };

        *info_lock = Some(info.clone());
        info
    }
}

unsafe impl Send for UsbDevice {}
unsafe impl Sync for UsbDevice {}
