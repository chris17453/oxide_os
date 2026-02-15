//! OXIDE Driver Traits
//!
//! Zero-dependency trait definitions for the driver subsystem.
//! Lives here instead of driver-core to break circular deps
//! (ps2 → driver-core → pci → arch-x86_64 → ps2).
//! — SableWire: the trait crate has no friends, no enemies, no dependencies

#![no_std]

use core::fmt;

/// Serial port driver trait
pub trait SerialDriver: Send {
    /// Initialize the driver
    fn init(&mut self);

    /// Write a byte
    fn write_byte(&mut self, byte: u8);

    /// Read a byte (non-blocking)
    fn read_byte(&mut self) -> Option<u8>;

    /// Check if transmit buffer is empty
    fn tx_empty(&self) -> bool;
}

// ============================================================================
// ISA Driver Infrastructure (PCI-free)
// ============================================================================
// — BlackLatch: ISA drivers can't depend on driver-core because that pulls in
// the entire PCI stack. These traits live here with zero deps.

/// Opaque driver-specific binding data
///
/// Drivers return this from probe() and receive it back in remove().
/// — WireSaint: a pointer dressed up in a suit so the type system doesn't complain
#[derive(Debug, Clone, Copy)]
pub struct DriverBindingData {
    data: usize,
}

impl DriverBindingData {
    pub fn new(data: usize) -> Self {
        Self { data }
    }

    pub fn as_usize(&self) -> usize {
        self.data
    }

    /// # Safety
    /// Type must match what was originally stored.
    pub unsafe fn as_ptr<T>(&self) -> *mut T {
        self.data as *mut T
    }
}

/// Driver errors
/// — GraveShift: when hardware says no and means it
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriverError {
    NotSupported,
    InitFailed,
    ResourceAllocation,
    DmaError,
    InvalidConfig,
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

/// ISA driver interface (for legacy devices like PS/2)
///
/// ISA devices don't sit on a PCI bus, so they can't use PciDriver.
/// This trait lets them register with the driver-core framework
/// without pulling in the PCI dependency chain.
/// — InputShade: for the old guard that still speaks port I/O
pub trait IsaDriver: Send + Sync {
    /// Driver name
    fn name(&self) -> &'static str;

    /// Probe for ISA devices (ISA has no enumeration — poke and pray)
    fn probe(&self) -> Result<DriverBindingData, DriverError>;

    /// Remove the device
    unsafe fn remove(&self, binding_data: DriverBindingData);
}

/// ISA driver registration macro
///
/// Places a driver reference in the `.isa_drivers` linker section for
/// automatic discovery at boot. No runtime registration needed.
/// — SableWire: compile-time registration, zero overhead, maximum swagger
#[macro_export]
macro_rules! register_isa_driver {
    ($driver:expr) => {
        #[used]
        #[unsafe(link_section = ".isa_drivers")]
        static DRIVER_REGISTRATION: &'static dyn $crate::IsaDriver = &$driver;
    };
}
