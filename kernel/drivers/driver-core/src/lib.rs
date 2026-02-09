//! Driver Core - Dynamic Driver Loading Infrastructure
//!
//! Provides automatic device-driver matching via compile-time registration.
//! Drivers implement PciDriver/IsaDriver traits and register via linker sections.
//! — GraveShift: the silicon knows who it belongs to

#![no_std]

extern crate alloc;

use core::fmt;
use pci::PciDevice;

pub mod registry;
pub mod binding;

pub use registry::{init_driver_registry, probe_all_devices, register_pci_driver_runtime, register_isa_driver_runtime};
pub use binding::{DriverBindingData, DeviceBinding};

/// PCI device ID for driver matching
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PciDeviceId {
    /// Vendor ID (0xFFFF = match any)
    pub vendor: u16,
    /// Device ID (0xFFFF = match any)
    pub device: u16,
    /// Subvendor ID (0xFFFF = match any)
    pub subvendor: u16,
    /// Subdevice ID (0xFFFF = match any)
    pub subdevice: u16,
    /// Class code (0xFFFFFF = match any)
    pub class: u32,
    /// Class mask (bits to match in class)
    pub class_mask: u32,
}

impl PciDeviceId {
    /// Match specific vendor:device pair
    pub const fn new(vendor: u16, device: u16) -> Self {
        Self {
            vendor,
            device,
            subvendor: 0xFFFF,
            subdevice: 0xFFFF,
            class: 0xFFFFFF,
            class_mask: 0,
        }
    }

    /// Match by PCI class (e.g., all network controllers)
    pub const fn by_class(class: u32, mask: u32) -> Self {
        Self {
            vendor: 0xFFFF,
            device: 0xFFFF,
            subvendor: 0xFFFF,
            subdevice: 0xFFFF,
            class,
            class_mask: mask,
        }
    }

    /// Check if this ID matches a PCI device
    pub fn matches(&self, dev: &PciDevice) -> bool {
        // Vendor/device matching
        if self.vendor != 0xFFFF && self.vendor != dev.vendor_id {
            return false;
        }
        if self.device != 0xFFFF && self.device != dev.device_id {
            return false;
        }

        // Note: PciDevice doesn't currently have subsystem vendor/device fields
        // Those checks are skipped for now

        // Class matching (if mask is non-zero)
        if self.class_mask != 0 {
            let dev_class = ((dev.class_code as u32) << 16)
                | ((dev.subclass as u32) << 8)
                | (dev.prog_if as u32);
            if (self.class ^ dev_class) & self.class_mask != 0 {
                return false;
            }
        }

        true
    }
}

/// Driver errors
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriverError {
    /// Device not supported by this driver
    NotSupported,
    /// Hardware initialization failed
    InitFailed,
    /// Resource allocation failed
    ResourceAllocation,
    /// DMA setup failed
    DmaError,
    /// Invalid device configuration
    InvalidConfig,
    /// Already bound to another driver
    AlreadyBound,
}

impl fmt::Display for DriverError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotSupported => write!(f, "device not supported"),
            Self::InitFailed => write!(f, "hardware init failed"),
            Self::ResourceAllocation => write!(f, "resource allocation failed"),
            Self::DmaError => write!(f, "DMA setup failed"),
            Self::InvalidConfig => write!(f, "invalid device config"),
            Self::AlreadyBound => write!(f, "device already bound"),
        }
    }
}

/// PCI driver interface
///
/// Drivers implement this trait and register via `register_pci_driver!` macro.
/// The driver core walks all registered drivers at boot and calls probe()
/// for matching devices.
/// — BlackLatch: it's probe or perish in this neon hellscape
pub trait PciDriver: Send + Sync {
    /// Driver name (for logging)
    fn name(&self) -> &'static str;

    /// Device ID table (vendor:device pairs this driver supports)
    fn id_table(&self) -> &'static [PciDeviceId];

    /// Probe a device
    ///
    /// Called when a matching device is found. The driver should:
    /// 1. Initialize hardware
    /// 2. Allocate resources
    /// 3. Register with subsystem (block, net, etc.)
    /// 4. Return binding data for later cleanup
    ///
    /// # Safety
    /// PCI device pointer is valid for the duration of the call.
    fn probe(&self, dev: &PciDevice, id: &PciDeviceId) -> Result<DriverBindingData, DriverError>;

    /// Remove a device
    ///
    /// Called when the device is being removed or the driver is being unloaded.
    /// Must unregister from subsystems and free all resources.
    ///
    /// # Safety
    /// Must be called with the same binding_data returned from probe().
    unsafe fn remove(&self, dev: &PciDevice, binding_data: DriverBindingData);
}

/// ISA driver interface (for legacy devices)
pub trait IsaDriver: Send + Sync {
    /// Driver name
    fn name(&self) -> &'static str;

    /// Probe for ISA devices
    ///
    /// ISA doesn't have enumeration, so driver must probe for hardware presence.
    fn probe(&self) -> Result<DriverBindingData, DriverError>;

    /// Remove the device
    unsafe fn remove(&self, binding_data: DriverBindingData);
}

/// Compile-time driver registration
///
/// Drivers use this macro to register with the driver core:
/// ```
/// static DRIVER: MyDriver = MyDriver;
/// register_pci_driver!(DRIVER);
/// ```
///
/// The linker collects these into .pci_drivers section.
/// — SableWire: compile-time registration means zero runtime overhead
#[macro_export]
macro_rules! register_pci_driver {
    ($driver:expr) => {
        #[used]
        #[link_section = ".pci_drivers"]
        static DRIVER_REGISTRATION: &'static dyn $crate::PciDriver = &$driver;
    };
}

/// ISA driver registration macro
#[macro_export]
macro_rules! register_isa_driver {
    ($driver:expr) => {
        #[used]
        #[link_section = ".isa_drivers"]
        static DRIVER_REGISTRATION: &'static dyn $crate::IsaDriver = &$driver;
    };
}
