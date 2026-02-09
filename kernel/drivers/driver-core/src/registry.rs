//! Driver Registry - Automatic Device-Driver Matching
//!
//! Walks linker-generated driver tables at boot and probes matching devices.
//! The Linux model: hardware tells us what it is, we find the right driver.
//! — NeonRoot: scanning the silicon graveyard for a pulse

use alloc::vec::Vec;
use pci::PciDevice;
use spin::Mutex;

use crate::{PciDriver, IsaDriver, PciDeviceId, DriverError};
use crate::binding::{register_binding, DriverBindingData};

/// Global PCI driver registry
static PCI_DRIVERS: Mutex<Vec<&'static dyn PciDriver>> = Mutex::new(Vec::new());

/// Global ISA driver registry
static ISA_DRIVERS: Mutex<Vec<&'static dyn IsaDriver>> = Mutex::new(Vec::new());

/// Linker-provided symbols for driver sections
extern "C" {
    #[link_name = "__start_pci_drivers"]
    static START_PCI_DRIVERS: &'static dyn PciDriver;
    #[link_name = "__stop_pci_drivers"]
    static STOP_PCI_DRIVERS: &'static dyn PciDriver;

    #[link_name = "__start_isa_drivers"]
    static START_ISA_DRIVERS: &'static dyn IsaDriver;
    #[link_name = "__stop_isa_drivers"]
    static STOP_ISA_DRIVERS: &'static dyn IsaDriver;
}

/// Initialize driver registry from linker sections
///
/// Called once at boot to collect all statically registered drivers.
/// Walks the .pci_drivers and .isa_drivers linker sections which contain
/// fat pointers (16 bytes each on x86_64: data pointer + vtable pointer).
/// — BlackLatch: pulling drivers from the void, one vtable at a time
pub fn init_driver_registry() {
    unsafe {
        // Walk PCI drivers section
        let start = &START_PCI_DRIVERS as *const _ as *const u8;
        let stop = &STOP_PCI_DRIVERS as *const _ as *const u8;
        let size = core::mem::size_of::<&'static dyn PciDriver>();

        if stop as usize > start as usize {
            let count = (stop as usize - start as usize) / size;
            let mut drivers = PCI_DRIVERS.lock();

            for i in 0..count {
                // Each entry in the linker section is a &'static dyn PciDriver (fat pointer)
                let ptr = (start as usize + i * size) as *const &'static dyn PciDriver;
                let driver: &'static dyn PciDriver = *ptr;
                drivers.push(driver);
            }
        }

        // Walk ISA drivers section
        let start = &START_ISA_DRIVERS as *const _ as *const u8;
        let stop = &STOP_ISA_DRIVERS as *const _ as *const u8;
        let size = core::mem::size_of::<&'static dyn IsaDriver>();

        if stop as usize > start as usize {
            let count = (stop as usize - start as usize) / size;
            let mut drivers = ISA_DRIVERS.lock();

            for i in 0..count {
                let ptr = (start as usize + i * size) as *const &'static dyn IsaDriver;
                let driver: &'static dyn IsaDriver = *ptr;
                drivers.push(driver);
            }
        }
    }
}

/// Register a PCI driver at runtime
///
/// Used for dynamic module loading. For static drivers, use register_pci_driver! macro.
pub fn register_pci_driver_runtime(driver: &'static dyn PciDriver) {
    PCI_DRIVERS.lock().push(driver);
}

/// Register an ISA driver at runtime
pub fn register_isa_driver_runtime(driver: &'static dyn IsaDriver) {
    ISA_DRIVERS.lock().push(driver);
}

/// Find a driver for a PCI device
///
/// Walks all registered drivers and checks if their ID table matches.
/// Returns the first matching driver.
fn match_pci_driver(dev: &PciDevice) -> Option<(&'static dyn PciDriver, PciDeviceId)> {
    let drivers = PCI_DRIVERS.lock();

    for driver in drivers.iter() {
        for id in driver.id_table().iter() {
            if id.matches(dev) {
                return Some((*driver, *id));
            }
        }
    }

    None
}

/// Probe all PCI devices
///
/// Called after PCI enumeration to match devices with drivers.
/// — SableWire: the moment of truth, where silicon meets code
pub fn probe_all_devices() -> Result<(), DriverError> {
    let devices = pci::devices();

    for dev in devices.iter() {
        if let Some((driver, id)) = match_pci_driver(dev) {
            // Found a matching driver, probe it
            match driver.probe(dev, &id) {
                Ok(binding_data) => {
                    // Success! Register the binding
                    register_binding(dev.address, driver.name(), binding_data);

                    // Log success (in real kernel, this would use proper logging)
                    #[cfg(debug_assertions)]
                    {
                        // TODO: Use kernel logging once available
                    }
                }
                Err(_e) => {
                    // Probe failed, try next driver
                    #[cfg(debug_assertions)]
                    {
                        // TODO: Log probe failure
                    }
                }
            }
        }
    }

    Ok(())
}

/// Probe ISA devices
///
/// ISA drivers probe for hardware themselves (no enumeration).
pub fn probe_isa_devices() -> Result<(), DriverError> {
    let drivers = ISA_DRIVERS.lock();

    for driver in drivers.iter() {
        match driver.probe() {
            Ok(binding_data) => {
                // ISA devices don't have addresses, use dummy address
                let dummy_addr = pci::PciAddress::new(0xFF, 0xFF, 0xFF);
                register_binding(dummy_addr, driver.name(), binding_data);
            }
            Err(_e) => {
                // Probe failed, driver's hardware not present
                #[cfg(debug_assertions)]
                {
                    // TODO: Log probe failure
                }
            }
        }
    }

    Ok(())
}

/// List all registered PCI drivers
pub fn list_pci_drivers() -> Vec<&'static str> {
    PCI_DRIVERS
        .lock()
        .iter()
        .map(|d| d.name())
        .collect()
}

/// List all registered ISA drivers
pub fn list_isa_drivers() -> Vec<&'static str> {
    ISA_DRIVERS
        .lock()
        .iter()
        .map(|d| d.name())
        .collect()
}
