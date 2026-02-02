//! USB Descriptors

use alloc::vec::Vec;

/// Setup packet for control transfers
#[repr(C, packed)]
#[derive(Debug, Clone, Copy, Default)]
pub struct SetupPacket {
    /// Request type
    pub request_type: u8,
    /// Request
    pub request: u8,
    /// Value
    pub value: u16,
    /// Index
    pub index: u16,
    /// Length
    pub length: u16,
}

impl SetupPacket {
    /// Create a GET_DESCRIPTOR request
    pub fn get_descriptor(desc_type: u8, desc_index: u8, length: u16) -> Self {
        SetupPacket {
            request_type: 0x80, // Device-to-host, standard, device
            request: 6,         // GET_DESCRIPTOR
            value: ((desc_type as u16) << 8) | desc_index as u16,
            index: 0,
            length,
        }
    }

    /// Create a SET_CONFIGURATION request
    pub fn set_configuration(config_value: u8) -> Self {
        SetupPacket {
            request_type: 0x00, // Host-to-device, standard, device
            request: 9,         // SET_CONFIGURATION
            value: config_value as u16,
            index: 0,
            length: 0,
        }
    }

    /// Create a SET_INTERFACE request
    pub fn set_interface(interface: u16, alt_setting: u16) -> Self {
        SetupPacket {
            request_type: 0x01, // Host-to-device, standard, interface
            request: 11,        // SET_INTERFACE
            value: alt_setting,
            index: interface,
            length: 0,
        }
    }

    /// Create a class-specific request
    pub fn class_request(
        recipient: u8,
        direction: u8,
        request: u8,
        value: u16,
        index: u16,
        length: u16,
    ) -> Self {
        SetupPacket {
            request_type: (direction << 7) | (1 << 5) | recipient, // Class
            request,
            value,
            index,
            length,
        }
    }
}

/// Device descriptor
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct DeviceDescriptor {
    /// Length (18)
    pub length: u8,
    /// Descriptor type (1)
    pub descriptor_type: u8,
    /// USB version (BCD)
    pub bcd_usb: u16,
    /// Device class
    pub device_class: u8,
    /// Device subclass
    pub device_subclass: u8,
    /// Device protocol
    pub device_protocol: u8,
    /// Max packet size for EP0
    pub max_packet_size0: u8,
    /// Vendor ID
    pub vendor_id: u16,
    /// Product ID
    pub product_id: u16,
    /// Device version (BCD)
    pub bcd_device: u16,
    /// Manufacturer string index
    pub manufacturer: u8,
    /// Product string index
    pub product: u8,
    /// Serial number string index
    pub serial_number: u8,
    /// Number of configurations
    pub num_configurations: u8,
}

impl DeviceDescriptor {
    /// Parse from bytes
    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        if data.len() < 18 {
            return None;
        }

        Some(DeviceDescriptor {
            length: data[0],
            descriptor_type: data[1],
            bcd_usb: u16::from_le_bytes([data[2], data[3]]),
            device_class: data[4],
            device_subclass: data[5],
            device_protocol: data[6],
            max_packet_size0: data[7],
            vendor_id: u16::from_le_bytes([data[8], data[9]]),
            product_id: u16::from_le_bytes([data[10], data[11]]),
            bcd_device: u16::from_le_bytes([data[12], data[13]]),
            manufacturer: data[14],
            product: data[15],
            serial_number: data[16],
            num_configurations: data[17],
        })
    }
}

impl Default for DeviceDescriptor {
    fn default() -> Self {
        DeviceDescriptor {
            length: 18,
            descriptor_type: 1,
            bcd_usb: 0x0200,
            device_class: 0,
            device_subclass: 0,
            device_protocol: 0,
            max_packet_size0: 64,
            vendor_id: 0,
            product_id: 0,
            bcd_device: 0,
            manufacturer: 0,
            product: 0,
            serial_number: 0,
            num_configurations: 1,
        }
    }
}

/// Configuration descriptor
#[derive(Debug, Clone)]
pub struct ConfigDescriptor {
    /// Length (9)
    pub length: u8,
    /// Descriptor type (2)
    pub descriptor_type: u8,
    /// Total length
    pub total_length: u16,
    /// Number of interfaces
    pub num_interfaces: u8,
    /// Configuration value
    pub configuration_value: u8,
    /// Configuration string index
    pub configuration: u8,
    /// Attributes
    pub attributes: u8,
    /// Max power (2mA units)
    pub max_power: u8,
    /// Interfaces
    pub interfaces: Vec<InterfaceDescriptor>,
}

impl ConfigDescriptor {
    /// Parse from bytes
    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        if data.len() < 9 {
            return None;
        }

        let total_length = u16::from_le_bytes([data[2], data[3]]) as usize;
        if data.len() < total_length {
            return None;
        }

        let mut config = ConfigDescriptor {
            length: data[0],
            descriptor_type: data[1],
            total_length: total_length as u16,
            num_interfaces: data[4],
            configuration_value: data[5],
            configuration: data[6],
            attributes: data[7],
            max_power: data[8],
            interfaces: Vec::new(),
        };

        // Parse interfaces
        let mut offset = 9;
        let mut current_interface: Option<InterfaceDescriptor> = None;

        while offset < total_length {
            if offset + 2 > total_length {
                break;
            }

            let len = data[offset] as usize;
            let desc_type = data[offset + 1];

            if len == 0 || offset + len > total_length {
                break;
            }

            match desc_type {
                4 => {
                    // Interface descriptor
                    if let Some(iface) = current_interface.take() {
                        config.interfaces.push(iface);
                    }

                    if len >= 9 {
                        current_interface = Some(InterfaceDescriptor {
                            length: data[offset],
                            descriptor_type: data[offset + 1],
                            interface_number: data[offset + 2],
                            alternate_setting: data[offset + 3],
                            num_endpoints: data[offset + 4],
                            interface_class: data[offset + 5],
                            interface_subclass: data[offset + 6],
                            interface_protocol: data[offset + 7],
                            interface: data[offset + 8],
                            endpoints: Vec::new(),
                        });
                    }
                }
                5 => {
                    // Endpoint descriptor
                    if len >= 7 {
                        if let Some(ref mut iface) = current_interface {
                            iface.endpoints.push(EndpointDescriptor {
                                length: data[offset],
                                descriptor_type: data[offset + 1],
                                endpoint_address: data[offset + 2],
                                attributes: data[offset + 3],
                                max_packet_size: u16::from_le_bytes([
                                    data[offset + 4],
                                    data[offset + 5],
                                ]),
                                interval: data[offset + 6],
                            });
                        }
                    }
                }
                _ => {
                    // Unknown descriptor, skip
                }
            }

            offset += len;
        }

        if let Some(iface) = current_interface {
            config.interfaces.push(iface);
        }

        Some(config)
    }

    /// Check if self-powered
    pub fn is_self_powered(&self) -> bool {
        self.attributes & 0x40 != 0
    }

    /// Check if remote wakeup enabled
    pub fn remote_wakeup(&self) -> bool {
        self.attributes & 0x20 != 0
    }

    /// Get max power in mA
    pub fn max_power_ma(&self) -> u16 {
        self.max_power as u16 * 2
    }
}

/// Interface descriptor
#[derive(Debug, Clone)]
pub struct InterfaceDescriptor {
    /// Length (9)
    pub length: u8,
    /// Descriptor type (4)
    pub descriptor_type: u8,
    /// Interface number
    pub interface_number: u8,
    /// Alternate setting
    pub alternate_setting: u8,
    /// Number of endpoints
    pub num_endpoints: u8,
    /// Interface class
    pub interface_class: u8,
    /// Interface subclass
    pub interface_subclass: u8,
    /// Interface protocol
    pub interface_protocol: u8,
    /// Interface string index
    pub interface: u8,
    /// Endpoints
    pub endpoints: Vec<EndpointDescriptor>,
}

impl InterfaceDescriptor {
    /// Check if this interface matches class/subclass/protocol
    pub fn matches(&self, class: u8, subclass: u8, protocol: u8) -> bool {
        (class == 0xFF || self.interface_class == class)
            && (subclass == 0xFF || self.interface_subclass == subclass)
            && (protocol == 0xFF || self.interface_protocol == protocol)
    }
}

/// Endpoint descriptor
#[derive(Debug, Clone, Copy)]
pub struct EndpointDescriptor {
    /// Length (7)
    pub length: u8,
    /// Descriptor type (5)
    pub descriptor_type: u8,
    /// Endpoint address
    pub endpoint_address: u8,
    /// Attributes
    pub attributes: u8,
    /// Max packet size
    pub max_packet_size: u16,
    /// Polling interval
    pub interval: u8,
}

impl EndpointDescriptor {
    /// Get endpoint number (0-15)
    pub fn number(&self) -> u8 {
        self.endpoint_address & 0x0F
    }

    /// Check if IN endpoint
    pub fn is_in(&self) -> bool {
        self.endpoint_address & 0x80 != 0
    }

    /// Check if OUT endpoint
    pub fn is_out(&self) -> bool {
        self.endpoint_address & 0x80 == 0
    }

    /// Get transfer type
    pub fn transfer_type(&self) -> TransferType {
        match self.attributes & 0x03 {
            0 => TransferType::Control,
            1 => TransferType::Isochronous,
            2 => TransferType::Bulk,
            3 => TransferType::Interrupt,
            _ => TransferType::Control,
        }
    }
}

/// Transfer type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferType {
    Control,
    Isochronous,
    Bulk,
    Interrupt,
}

/// USB class codes
pub mod class {
    pub const INTERFACE: u8 = 0x00;
    pub const AUDIO: u8 = 0x01;
    pub const CDC: u8 = 0x02;
    pub const HID: u8 = 0x03;
    pub const PHYSICAL: u8 = 0x05;
    pub const IMAGE: u8 = 0x06;
    pub const PRINTER: u8 = 0x07;
    pub const MASS_STORAGE: u8 = 0x08;
    pub const HUB: u8 = 0x09;
    pub const CDC_DATA: u8 = 0x0A;
    pub const SMART_CARD: u8 = 0x0B;
    pub const VIDEO: u8 = 0x0E;
    pub const WIRELESS: u8 = 0xE0;
    pub const MISC: u8 = 0xEF;
    pub const APPLICATION: u8 = 0xFE;
    pub const VENDOR: u8 = 0xFF;
}
