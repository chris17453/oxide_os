//! USB Hub Driver

use alloc::sync::Arc;
use alloc::vec::Vec;
use crate::{UsbDevice, UsbResult, UsbError, SetupPacket};

/// USB hub status
#[repr(C, packed)]
#[derive(Debug, Clone, Copy, Default)]
pub struct HubStatus {
    /// Status bits
    pub status: u16,
    /// Change bits
    pub change: u16,
}

/// Hub port status
#[repr(C, packed)]
#[derive(Debug, Clone, Copy, Default)]
pub struct PortStatusBits {
    /// Status bits
    pub status: u16,
    /// Change bits
    pub change: u16,
}

impl PortStatusBits {
    /// Port is connected
    pub fn connected(&self) -> bool {
        self.status & 0x0001 != 0
    }

    /// Port is enabled
    pub fn enabled(&self) -> bool {
        self.status & 0x0002 != 0
    }

    /// Port is suspended
    pub fn suspended(&self) -> bool {
        self.status & 0x0004 != 0
    }

    /// Overcurrent condition
    pub fn overcurrent(&self) -> bool {
        self.status & 0x0008 != 0
    }

    /// Port is being reset
    pub fn reset(&self) -> bool {
        self.status & 0x0010 != 0
    }

    /// Port power is on
    pub fn powered(&self) -> bool {
        self.status & 0x0100 != 0
    }

    /// Low speed device attached
    pub fn low_speed(&self) -> bool {
        self.status & 0x0200 != 0
    }

    /// High speed device attached
    pub fn high_speed(&self) -> bool {
        self.status & 0x0400 != 0
    }

    /// Connection changed
    pub fn connection_changed(&self) -> bool {
        self.change & 0x0001 != 0
    }

    /// Enable changed
    pub fn enable_changed(&self) -> bool {
        self.change & 0x0002 != 0
    }

    /// Reset completed
    pub fn reset_changed(&self) -> bool {
        self.change & 0x0010 != 0
    }
}

/// Hub descriptor
#[derive(Debug, Clone)]
pub struct HubDescriptor {
    /// Length
    pub length: u8,
    /// Descriptor type (0x29)
    pub descriptor_type: u8,
    /// Number of ports
    pub num_ports: u8,
    /// Hub characteristics
    pub characteristics: u16,
    /// Power on to power good time (2ms units)
    pub pwr_on_to_pwr_good: u8,
    /// Max hub current (mA)
    pub max_current: u8,
    /// Device removable bitmap
    pub device_removable: Vec<u8>,
}

impl HubDescriptor {
    /// Parse from bytes
    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        if data.len() < 7 {
            return None;
        }

        let num_ports = data[2];
        let bitmap_bytes = (num_ports as usize + 8) / 8;

        let device_removable = if data.len() >= 7 + bitmap_bytes {
            data[7..7 + bitmap_bytes].to_vec()
        } else {
            Vec::new()
        };

        Some(HubDescriptor {
            length: data[0],
            descriptor_type: data[1],
            num_ports,
            characteristics: u16::from_le_bytes([data[3], data[4]]),
            pwr_on_to_pwr_good: data[5],
            max_current: data[6],
            device_removable,
        })
    }

    /// Is compound device
    pub fn is_compound(&self) -> bool {
        self.characteristics & 0x0004 != 0
    }

    /// Get power switching mode
    pub fn power_switching(&self) -> PowerSwitching {
        match self.characteristics & 0x0003 {
            0 => PowerSwitching::Ganged,
            1 => PowerSwitching::Individual,
            _ => PowerSwitching::None,
        }
    }
}

/// Power switching mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerSwitching {
    /// All ports powered together
    Ganged,
    /// Individual port power control
    Individual,
    /// No power switching (always on)
    None,
}

/// Hub class requests
pub mod request {
    pub const GET_STATUS: u8 = 0;
    pub const CLEAR_FEATURE: u8 = 1;
    pub const SET_FEATURE: u8 = 3;
    pub const GET_DESCRIPTOR: u8 = 6;
    pub const SET_DESCRIPTOR: u8 = 7;
    pub const CLEAR_TT_BUFFER: u8 = 8;
    pub const RESET_TT: u8 = 9;
    pub const GET_TT_STATE: u8 = 10;
    pub const STOP_TT: u8 = 11;
}

/// Hub features
pub mod feature {
    pub const C_HUB_LOCAL_POWER: u16 = 0;
    pub const C_HUB_OVER_CURRENT: u16 = 1;
    pub const PORT_CONNECTION: u16 = 0;
    pub const PORT_ENABLE: u16 = 1;
    pub const PORT_SUSPEND: u16 = 2;
    pub const PORT_OVER_CURRENT: u16 = 3;
    pub const PORT_RESET: u16 = 4;
    pub const PORT_POWER: u16 = 8;
    pub const PORT_LOW_SPEED: u16 = 9;
    pub const C_PORT_CONNECTION: u16 = 16;
    pub const C_PORT_ENABLE: u16 = 17;
    pub const C_PORT_SUSPEND: u16 = 18;
    pub const C_PORT_OVER_CURRENT: u16 = 19;
    pub const C_PORT_RESET: u16 = 20;
}

/// USB hub driver
pub struct UsbHub {
    /// USB device
    device: Arc<UsbDevice>,
    /// Hub descriptor
    descriptor: HubDescriptor,
    /// Port status cache
    port_status: Vec<PortStatusBits>,
}

impl UsbHub {
    /// Create a new hub driver
    pub fn new(device: Arc<UsbDevice>) -> UsbResult<Self> {
        // Get hub descriptor
        let mut buf = [0u8; 16];
        device.control_transfer(
            0xA0, // Class, device, IN
            request::GET_DESCRIPTOR,
            0x2900, // Hub descriptor
            0,
            Some(&mut buf),
        )?;

        let descriptor = HubDescriptor::from_bytes(&buf)
            .ok_or(UsbError::InvalidDescriptor)?;

        let num_ports = descriptor.num_ports as usize;
        let port_status = alloc::vec![PortStatusBits::default(); num_ports];

        Ok(UsbHub {
            device,
            descriptor,
            port_status,
        })
    }

    /// Get number of ports
    pub fn num_ports(&self) -> u8 {
        self.descriptor.num_ports
    }

    /// Get port status
    pub fn get_port_status(&mut self, port: u8) -> UsbResult<PortStatusBits> {
        if port == 0 || port > self.descriptor.num_ports {
            return Err(UsbError::InvalidEndpoint);
        }

        let mut buf = [0u8; 4];
        self.device.control_transfer(
            0xA3, // Class, other (port), IN
            request::GET_STATUS,
            0,
            port as u16,
            Some(&mut buf),
        )?;

        let status = PortStatusBits {
            status: u16::from_le_bytes([buf[0], buf[1]]),
            change: u16::from_le_bytes([buf[2], buf[3]]),
        };

        self.port_status[port as usize - 1] = status;
        Ok(status)
    }

    /// Set port feature
    pub fn set_port_feature(&self, port: u8, feature: u16) -> UsbResult<()> {
        self.device.control_transfer(
            0x23, // Class, other (port), OUT
            request::SET_FEATURE,
            feature,
            port as u16,
            None,
        )?;
        Ok(())
    }

    /// Clear port feature
    pub fn clear_port_feature(&self, port: u8, feature: u16) -> UsbResult<()> {
        self.device.control_transfer(
            0x23,
            request::CLEAR_FEATURE,
            feature,
            port as u16,
            None,
        )?;
        Ok(())
    }

    /// Power on a port
    pub fn power_on(&self, port: u8) -> UsbResult<()> {
        self.set_port_feature(port, feature::PORT_POWER)
    }

    /// Power off a port
    pub fn power_off(&self, port: u8) -> UsbResult<()> {
        self.clear_port_feature(port, feature::PORT_POWER)
    }

    /// Reset a port
    pub fn reset_port(&self, port: u8) -> UsbResult<()> {
        self.set_port_feature(port, feature::PORT_RESET)
    }

    /// Clear connection change
    pub fn clear_connection_change(&self, port: u8) -> UsbResult<()> {
        self.clear_port_feature(port, feature::C_PORT_CONNECTION)
    }

    /// Clear reset change
    pub fn clear_reset_change(&self, port: u8) -> UsbResult<()> {
        self.clear_port_feature(port, feature::C_PORT_RESET)
    }
}
