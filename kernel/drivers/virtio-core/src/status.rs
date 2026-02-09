//! VirtIO Device Status Flags
//!
//! Status register bit definitions from VirtIO spec §2.1.
//! — BlackLatch: the device lifecycle, codified in silicon

/// Device has been detected by the driver
pub const ACKNOWLEDGE: u8 = 1;

/// Driver has acknowledged the device
pub const DRIVER: u8 = 2;

/// Driver is ready to drive the device
pub const DRIVER_OK: u8 = 4;

/// Driver has completed feature negotiation
pub const FEATURES_OK: u8 = 8;

/// Device needs a reset (unrecoverable error)
pub const DEVICE_NEEDS_RESET: u8 = 64;

/// Device initialization failed (driver gave up)
pub const FAILED: u8 = 128;
