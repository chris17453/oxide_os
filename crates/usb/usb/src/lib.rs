//! USB Core Framework for EFFLUX OS
//!
//! Provides USB device abstraction, enumeration, and class driver framework.

#![no_std]

extern crate alloc;

pub mod device;
pub mod descriptor;
pub mod transfer;
pub mod hub;
pub mod class;

pub use device::{UsbDevice, UsbDeviceInfo, DeviceSpeed};
pub use descriptor::*;
pub use transfer::{UsbTransfer, TransferType, TransferDirection, EndpointDescriptor};
pub use class::UsbClassDriver;

use alloc::sync::Arc;
use alloc::vec::Vec;
use spin::Mutex;

/// USB error type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsbError {
    /// Device not found
    DeviceNotFound,
    /// Transfer error
    TransferError,
    /// Timeout
    Timeout,
    /// Stall
    Stall,
    /// CRC error
    CrcError,
    /// Babble
    Babble,
    /// Data buffer error
    DataBuffer,
    /// Not supported
    NotSupported,
    /// No device
    NoDevice,
    /// Invalid endpoint
    InvalidEndpoint,
    /// Invalid descriptor
    InvalidDescriptor,
    /// No resources
    NoResources,
    /// Device not configured
    NotConfigured,
    /// Protocol error
    ProtocolError,
    /// Device error
    DeviceError,
}

/// USB result type
pub type UsbResult<T> = Result<T, UsbError>;

/// USB host controller trait
pub trait UsbHostController: Send + Sync {
    /// Get controller name
    fn name(&self) -> &str;

    /// Reset a port
    fn reset_port(&self, port: u8) -> UsbResult<()>;

    /// Get port status
    fn port_status(&self, port: u8) -> UsbResult<PortStatus>;

    /// Enable slot for device
    fn enable_slot(&self) -> UsbResult<u8>;

    /// Disable slot
    fn disable_slot(&self, slot: u8) -> UsbResult<()>;

    /// Address device
    fn address_device(&self, slot: u8, port: u8, speed: DeviceSpeed) -> UsbResult<u8>;

    /// Configure endpoint
    fn configure_endpoint(&self, slot: u8, endpoint: &EndpointDescriptor) -> UsbResult<()>;

    /// Submit control transfer
    fn control_transfer(
        &self,
        slot: u8,
        request: SetupPacket,
        data: Option<&mut [u8]>,
    ) -> UsbResult<usize>;

    /// Submit bulk transfer
    fn bulk_transfer(
        &self,
        slot: u8,
        endpoint: u8,
        data: &mut [u8],
        direction: TransferDirection,
    ) -> UsbResult<usize>;

    /// Submit interrupt transfer
    fn interrupt_transfer(
        &self,
        slot: u8,
        endpoint: u8,
        data: &mut [u8],
        direction: TransferDirection,
    ) -> UsbResult<usize>;

    /// Get maximum device slots
    fn max_slots(&self) -> u8;

    /// Get number of ports
    fn num_ports(&self) -> u8;
}

/// Port status
#[derive(Debug, Clone, Copy, Default)]
pub struct PortStatus {
    /// Port connected
    pub connected: bool,
    /// Port enabled
    pub enabled: bool,
    /// Port reset
    pub reset: bool,
    /// Device speed
    pub speed: DeviceSpeed,
    /// Power on
    pub power: bool,
}

/// Global USB bus
static USB_BUS: Mutex<UsbBus> = Mutex::new(UsbBus::new());

/// USB bus manager
pub struct UsbBus {
    /// Host controllers
    controllers: Vec<Arc<dyn UsbHostController>>,
    /// Connected devices
    devices: Vec<Arc<UsbDevice>>,
    /// Class drivers
    class_drivers: Vec<Arc<dyn UsbClassDriver>>,
}

impl UsbBus {
    /// Create a new USB bus
    const fn new() -> Self {
        UsbBus {
            controllers: Vec::new(),
            devices: Vec::new(),
            class_drivers: Vec::new(),
        }
    }

    /// Register a host controller
    pub fn register_controller(&mut self, controller: Arc<dyn UsbHostController>) {
        self.controllers.push(controller);
    }

    /// Register a class driver
    pub fn register_class_driver(&mut self, driver: Arc<dyn UsbClassDriver>) {
        self.class_drivers.push(driver);
    }

    /// Add a device
    pub fn add_device(&mut self, device: Arc<UsbDevice>) {
        self.devices.push(device);
    }

    /// Get all devices
    pub fn devices(&self) -> &[Arc<UsbDevice>] {
        &self.devices
    }

    /// Get all controllers
    pub fn controllers(&self) -> &[Arc<dyn UsbHostController>] {
        &self.controllers
    }

    /// Find matching class driver for device
    pub fn find_driver(&self, device: &UsbDevice) -> Option<Arc<dyn UsbClassDriver>> {
        for driver in &self.class_drivers {
            if driver.probe(device) {
                return Some(driver.clone());
            }
        }
        None
    }
}

/// Initialize USB subsystem
pub fn init() {
    // USB bus is already initialized statically
}

/// Register a host controller
pub fn register_controller(controller: Arc<dyn UsbHostController>) {
    USB_BUS.lock().register_controller(controller);
}

/// Register a class driver
pub fn register_class_driver(driver: Arc<dyn UsbClassDriver>) {
    USB_BUS.lock().register_class_driver(driver);
}

/// Get all devices
pub fn devices() -> Vec<Arc<UsbDevice>> {
    USB_BUS.lock().devices.clone()
}

/// Enumerate devices on all controllers
pub fn enumerate_devices() {
    let bus = USB_BUS.lock();
    for controller in &bus.controllers {
        for port in 0..controller.num_ports() {
            if let Ok(status) = controller.port_status(port) {
                if status.connected && !status.enabled {
                    // New device, enumerate it
                    let _ = enumerate_port(controller.clone(), port);
                }
            }
        }
    }
}

/// Enumerate a single port
fn enumerate_port(controller: Arc<dyn UsbHostController>, port: u8) -> UsbResult<()> {
    // Reset port
    controller.reset_port(port)?;

    // Get port status
    let status = controller.port_status(port)?;
    if !status.connected {
        return Err(UsbError::NoDevice);
    }

    // Enable slot
    let slot = controller.enable_slot()?;

    // Address device
    let address = controller.address_device(slot, port, status.speed)?;

    // Get device descriptor
    let mut desc_buf = [0u8; 18];
    let setup = SetupPacket {
        request_type: 0x80, // Device-to-host, standard, device
        request: 6,         // GET_DESCRIPTOR
        value: 0x0100,      // Device descriptor
        index: 0,
        length: 18,
    };

    controller.control_transfer(slot, setup, Some(&mut desc_buf))?;

    let device_desc = DeviceDescriptor::from_bytes(&desc_buf)
        .ok_or(UsbError::InvalidDescriptor)?;

    // Create device
    let device = Arc::new(UsbDevice::new(
        controller.clone(),
        slot,
        address,
        status.speed,
        device_desc,
    ));

    // Add to bus
    USB_BUS.lock().add_device(device.clone());

    // Find and attach class driver
    if let Some(driver) = USB_BUS.lock().find_driver(&device) {
        let _ = driver.attach(&device);
    }

    Ok(())
}
