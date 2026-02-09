//! Kernel Symbol Exports for Dynamic Modules
//!
//! Exports kernel functions that dynamically loaded driver modules need.
//! These symbols are resolved by the module loader when loading .ko files.
//! — PatchBay: exposing the kernel's guts for modules to hook into

use alloc::boxed::Box;
use alloc::string::ToString;
use alloc::vec;
use mm_manager::mm;
use mm_traits::FrameAllocator;

// ============================================================================
// Memory Management Exports
// ============================================================================

/// Allocate contiguous physical memory frames
///
/// # Safety
/// Caller must ensure proper use of the returned physical address.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __kernel_mm_alloc_contiguous(num_pages: usize) -> u64 {
    match mm().alloc_contiguous(num_pages) {
        Ok(phys_addr) => phys_addr.as_u64(),
        Err(_) => 0, // Return 0 on failure
    }
}

// Note: free_contiguous not implemented in FrameAllocator trait yet
// TODO: Add when memory management supports it

// ============================================================================
// PCI Configuration Space Access Exports
// ============================================================================

/// Read 32-bit value from PCI configuration space
#[unsafe(no_mangle)]
pub extern "C" fn __kernel_pci_config_read32(bus: u8, device: u8, function: u8, offset: u8) -> u32 {
    let addr = pci::PciAddress::new(bus, device, function);
    pci::config_read32(addr, offset)
}

/// Read 16-bit value from PCI configuration space
#[unsafe(no_mangle)]
pub extern "C" fn __kernel_pci_config_read16(bus: u8, device: u8, function: u8, offset: u8) -> u16 {
    let addr = pci::PciAddress::new(bus, device, function);
    pci::config_read16(addr, offset)
}

/// Read 8-bit value from PCI configuration space
#[unsafe(no_mangle)]
pub extern "C" fn __kernel_pci_config_read8(bus: u8, device: u8, function: u8, offset: u8) -> u8 {
    let addr = pci::PciAddress::new(bus, device, function);
    pci::config_read8(addr, offset)
}

/// Write 32-bit value to PCI configuration space
#[unsafe(no_mangle)]
pub extern "C" fn __kernel_pci_config_write32(bus: u8, device: u8, function: u8, offset: u8, value: u32) {
    let addr = pci::PciAddress::new(bus, device, function);
    pci::config_write32(addr, offset, value);
}

/// Write 16-bit value to PCI configuration space
#[unsafe(no_mangle)]
pub extern "C" fn __kernel_pci_config_write16(bus: u8, device: u8, function: u8, offset: u8, value: u16) {
    let addr = pci::PciAddress::new(bus, device, function);
    pci::config_write16(addr, offset, value);
}

/// Write 8-bit value to PCI configuration space
#[unsafe(no_mangle)]
pub extern "C" fn __kernel_pci_config_write8(bus: u8, device: u8, function: u8, offset: u8, value: u8) {
    let addr = pci::PciAddress::new(bus, device, function);
    pci::config_write8(addr, offset, value);
}

/// Enable PCI bus mastering
#[unsafe(no_mangle)]
pub extern "C" fn __kernel_pci_enable_bus_master(bus: u8, device: u8, function: u8) {
    let addr = pci::PciAddress::new(bus, device, function);
    pci::enable_bus_master(addr);
}

/// Enable PCI I/O space access
#[unsafe(no_mangle)]
pub extern "C" fn __kernel_pci_enable_io_space(bus: u8, device: u8, function: u8) {
    let addr = pci::PciAddress::new(bus, device, function);
    pci::enable_io_space(addr);
}

/// Enable PCI memory space access
#[unsafe(no_mangle)]
pub extern "C" fn __kernel_pci_enable_memory_space(bus: u8, device: u8, function: u8) {
    let addr = pci::PciAddress::new(bus, device, function);
    pci::enable_memory_space(addr);
}

// ============================================================================
// Driver Registration Exports
// ============================================================================

/// Register a PCI driver at runtime (for dynamic modules)
///
/// # Safety
/// Driver pointer must be valid for 'static lifetime.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __kernel_register_pci_driver(driver: &'static dyn driver_core::PciDriver) {
    driver_core::register_pci_driver_runtime(driver);
}

/// Register an ISA driver at runtime (for dynamic modules)
///
/// # Safety
/// Driver pointer must be valid for 'static lifetime.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __kernel_register_isa_driver(driver: &'static dyn driver_core::IsaDriver) {
    driver_core::register_isa_driver_runtime(driver);
}

// ============================================================================
// Block Device Exports
// ============================================================================

/// Register a block device (for dynamic block drivers)
///
/// # Safety
/// Device must implement BlockDevice trait and remain valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __kernel_block_register_device(
    name: *const u8,
    name_len: usize,
    device: *mut dyn block::BlockDevice,
) {
    if !name.is_null() && !device.is_null() {
        let name_slice = core::slice::from_raw_parts(name, name_len);
        if let Ok(name_str) = core::str::from_utf8(name_slice) {
            let device_box = Box::from_raw(device);
            block::register_device(name_str.to_string(), device_box);
        }
    }
}

// ============================================================================
// Symbol Table for Module Loader
// ============================================================================

/// Kernel symbol table entry
pub struct KernelSymbol {
    pub name: &'static str,
    pub addr: usize,
}

/// Get kernel symbol table for module linking
///
/// Returns array of exported symbols that modules can link against.
/// Built at runtime since function pointer casts aren't allowed in const context.
pub fn get_kernel_symbols() -> alloc::vec::Vec<KernelSymbol> {
    alloc::vec![
        // Memory management
        KernelSymbol {
            name: "mm_alloc_contiguous",
            addr: __kernel_mm_alloc_contiguous as usize,
        },
        // PCI config space
        KernelSymbol {
            name: "pci_config_read32",
            addr: __kernel_pci_config_read32 as usize,
        },
        KernelSymbol {
            name: "pci_config_read16",
            addr: __kernel_pci_config_read16 as usize,
        },
        KernelSymbol {
            name: "pci_config_read8",
            addr: __kernel_pci_config_read8 as usize,
        },
        KernelSymbol {
            name: "pci_config_write32",
            addr: __kernel_pci_config_write32 as usize,
        },
        KernelSymbol {
            name: "pci_config_write16",
            addr: __kernel_pci_config_write16 as usize,
        },
        KernelSymbol {
            name: "pci_config_write8",
            addr: __kernel_pci_config_write8 as usize,
        },
        KernelSymbol {
            name: "pci_enable_bus_master",
            addr: __kernel_pci_enable_bus_master as usize,
        },
        KernelSymbol {
            name: "pci_enable_io_space",
            addr: __kernel_pci_enable_io_space as usize,
        },
        KernelSymbol {
            name: "pci_enable_memory_space",
            addr: __kernel_pci_enable_memory_space as usize,
        },
        // Driver registration
        KernelSymbol {
            name: "register_pci_driver",
            addr: __kernel_register_pci_driver as usize,
        },
        KernelSymbol {
            name: "register_isa_driver",
            addr: __kernel_register_isa_driver as usize,
        },
        // Block devices
        KernelSymbol {
            name: "block_register_device",
            addr: __kernel_block_register_device as usize,
        },
    ]
}
