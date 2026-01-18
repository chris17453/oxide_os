//! USB Class Driver Framework

use alloc::sync::Arc;
use crate::{UsbDevice, UsbResult};

/// USB class driver trait
pub trait UsbClassDriver: Send + Sync {
    /// Get driver name
    fn name(&self) -> &str;

    /// Check if driver supports device
    fn probe(&self, device: &UsbDevice) -> bool;

    /// Attach to device
    fn attach(&self, device: &Arc<UsbDevice>) -> UsbResult<()>;

    /// Detach from device
    fn detach(&self, device: &Arc<UsbDevice>) -> UsbResult<()>;
}
