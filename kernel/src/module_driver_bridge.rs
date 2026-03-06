//! Module-Driver Bridge
//!
//! Integrates driver-core with the module loading system.
//! When .ko modules are loaded, they can register drivers at runtime.
//! — PatchBay: bridging static and dynamic worlds

use alloc::vec::Vec;

/// Module driver registration callback
///
/// After a module is loaded, this scans for driver registrations
/// and calls probe on matched devices.
pub fn module_drivers_loaded() {
    // After loading a .ko file, the module's init_module() should:
    // 1. Call __kernel_register_pci_driver() for each PciDriver
    // 2. This adds the driver to the runtime registry

    // Then we probe all devices again to see if the new driver matches
    let _ = driver_core::probe_all_devices();
    let _ = driver_core::probe_isa_devices();
}

/// Get statistics on loaded drivers
pub fn get_driver_stats() -> DriverStats {
    DriverStats {
        pci_drivers: driver_core::list_pci_drivers().len(),
        isa_drivers: driver_core::list_isa_drivers().len(),
    }
}

#[derive(Debug, Clone, Copy)]
pub struct DriverStats {
    pub pci_drivers: usize,
    pub isa_drivers: usize,
}

/// Example usage for a dynamically loaded driver module:
///
/// ```ignore
/// // In your driver .ko file:
/// use driver_core::{PciDriver, PciDeviceId, DriverError, DriverBindingData};
///
/// struct MyDriver;
/// impl PciDriver for MyDriver {
///     fn name(&self) -> &'static str { "my-driver" }
///     fn id_table(&self) -> &'static [PciDeviceId] {
///         &[PciDeviceId::new(0x1234, 0x5678)]
///     }
///     fn probe(&self, dev: &pci::PciDevice, id: &PciDeviceId)
///         -> Result<DriverBindingData, DriverError> {
///         // Initialize hardware...
///         Ok(DriverBindingData::new(0))
///     }
///     unsafe fn remove(&self, dev: &pci::PciDevice, binding_data: DriverBindingData) {
///         // Cleanup...
///     }
/// }
///
/// static DRIVER: MyDriver = MyDriver;
///
/// #[no_mangle]
/// pub extern "C" fn init_module() -> i32 {
///     unsafe {
///         __kernel_register_pci_driver(&DRIVER);
///     }
///     0 // Success
/// }
///
/// #[no_mangle]
/// pub extern "C" fn cleanup_module() {
///     // Driver cleanup if needed
/// }
/// ```
///
/// # Module Loading Flow
///
/// 1. Kernel loads .ko file via `module::load_module(elf_data)`
/// 2. Module's `init_module()` calls `__kernel_register_pci_driver()`
/// 3. Driver added to runtime registry
/// 4. Kernel calls `module_driver_bridge::module_drivers_loaded()`
/// 5. New driver probes all PCI devices
/// 6. Matching devices are initialized
/// — PatchBay: Load a .ko driver module, run its init_module(), then
/// probe all devices so the new driver can claim matching hardware.
pub fn load_driver_module(module_data: &[u8]) -> Result<(), ()> {
    use module::ModuleFlags;

    module::load_module(module_data, "", ModuleFlags::NONE)
        .map_err(|_| ())?;

    // — PatchBay: After loading, re-probe devices so the new driver
    // can match against PCI/ISA devices it was compiled for.
    module_drivers_loaded();

    Ok(())
}
