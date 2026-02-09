//! Device-Driver Binding Management
//!
//! Tracks which driver is bound to which device for cleanup.
//! — WireSaint: keeping the chains intact when the system crumbles

use alloc::vec::Vec;
use pci::PciAddress;
use spin::Mutex;

/// Opaque driver-specific binding data
///
/// Drivers return this from probe() and receive it back in remove().
/// Typically stores a pointer to the device instance (e.g., Arc<VirtioNet>).
#[derive(Debug, Clone, Copy)]
pub struct DriverBindingData {
    /// Opaque pointer value
    data: usize,
}

impl DriverBindingData {
    /// Create binding data from a raw pointer
    ///
    /// # Safety
    /// Pointer must remain valid until remove() is called.
    pub fn new(data: usize) -> Self {
        Self { data }
    }

    /// Get the raw pointer value
    pub fn as_usize(&self) -> usize {
        self.data
    }

    /// Convert to typed pointer
    ///
    /// # Safety
    /// Type must match what was originally stored.
    pub unsafe fn as_ptr<T>(&self) -> *mut T {
        self.data as *mut T
    }
}

/// Device binding record
#[derive(Debug, Clone)]
pub struct DeviceBinding {
    /// PCI device address
    pub address: PciAddress,
    /// Driver name
    pub driver_name: &'static str,
    /// Opaque binding data for cleanup
    pub binding_data: DriverBindingData,
}

/// Global binding registry
static BINDINGS: Mutex<Vec<DeviceBinding>> = Mutex::new(Vec::new());

/// Register a device binding
pub fn register_binding(address: PciAddress, driver_name: &'static str, binding_data: DriverBindingData) {
    let binding = DeviceBinding {
        address,
        driver_name,
        binding_data,
    };
    BINDINGS.lock().push(binding);
}

/// Find binding for a device
pub fn find_binding(address: PciAddress) -> Option<DeviceBinding> {
    BINDINGS
        .lock()
        .iter()
        .find(|b| b.address == address)
        .cloned()
}

/// Remove a device binding
pub fn remove_binding(address: PciAddress) -> Option<DeviceBinding> {
    let mut bindings = BINDINGS.lock();
    if let Some(pos) = bindings.iter().position(|b| b.address == address) {
        Some(bindings.remove(pos))
    } else {
        None
    }
}

/// List all active bindings
pub fn list_bindings() -> Vec<DeviceBinding> {
    BINDINGS.lock().clone()
}
