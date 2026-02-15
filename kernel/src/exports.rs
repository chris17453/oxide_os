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

/// Free contiguous physical memory frames
///
/// # Safety
/// Physical address must have been allocated via __kernel_mm_alloc_contiguous.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __kernel_mm_free_contiguous(phys_addr: u64, num_pages: usize) -> i32 {
    if phys_addr == 0 {
        return -1; // Invalid address
    }
    let addr = os_core::PhysAddr::new(phys_addr);
    match mm().free_contiguous(addr, num_pages) {
        Ok(()) => 0,
        Err(_) => -1,
    }
}

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
/// `data` and `vtable` must form a valid fat pointer to a `&'static dyn PciDriver`.
/// — PatchBay: smuggling trait objects past the FFI bouncer
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __kernel_register_pci_driver(data: *const (), vtable: *const ()) {
    let driver: &'static dyn driver_core::PciDriver = unsafe {
        core::mem::transmute::<(*const (), *const ()), &'static dyn driver_core::PciDriver>((data, vtable))
    };
    driver_core::register_pci_driver_runtime(driver);
}

/// Register an ISA driver at runtime (for dynamic modules)
///
/// # Safety
/// `data` and `vtable` must form a valid fat pointer to a `&'static dyn IsaDriver`.
/// — PatchBay: same trick, different driver flavor
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __kernel_register_isa_driver(data: *const (), vtable: *const ()) {
    let driver: &'static dyn driver_core::IsaDriver = unsafe {
        core::mem::transmute::<(*const (), *const ()), &'static dyn driver_core::IsaDriver>((data, vtable))
    };
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
    device_data: *mut (),
    device_vtable: *const (),
) {
    // — PatchBay: reconstruct fat pointer from split data+vtable components
    if !name.is_null() && !device_data.is_null() {
        let name_slice = unsafe { core::slice::from_raw_parts(name, name_len) };
        if let Ok(name_str) = core::str::from_utf8(name_slice) {
            let device: *mut dyn block::BlockDevice = unsafe {
                core::mem::transmute::<(*mut (), *const ()), *mut dyn block::BlockDevice>((device_data, device_vtable))
            };
            let device_box = unsafe { Box::from_raw(device) };
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
            addr: __kernel_mm_alloc_contiguous as *const () as usize,
        },
        KernelSymbol {
            name: "mm_free_contiguous",
            addr: __kernel_mm_free_contiguous as *const () as usize,
        },
        // PCI config space
        KernelSymbol {
            name: "pci_config_read32",
            addr: __kernel_pci_config_read32 as *const () as usize,
        },
        KernelSymbol {
            name: "pci_config_read16",
            addr: __kernel_pci_config_read16 as *const () as usize,
        },
        KernelSymbol {
            name: "pci_config_read8",
            addr: __kernel_pci_config_read8 as *const () as usize,
        },
        KernelSymbol {
            name: "pci_config_write32",
            addr: __kernel_pci_config_write32 as *const () as usize,
        },
        KernelSymbol {
            name: "pci_config_write16",
            addr: __kernel_pci_config_write16 as *const () as usize,
        },
        KernelSymbol {
            name: "pci_config_write8",
            addr: __kernel_pci_config_write8 as *const () as usize,
        },
        KernelSymbol {
            name: "pci_enable_bus_master",
            addr: __kernel_pci_enable_bus_master as *const () as usize,
        },
        KernelSymbol {
            name: "pci_enable_io_space",
            addr: __kernel_pci_enable_io_space as *const () as usize,
        },
        KernelSymbol {
            name: "pci_enable_memory_space",
            addr: __kernel_pci_enable_memory_space as *const () as usize,
        },
        // Driver registration
        KernelSymbol {
            name: "register_pci_driver",
            addr: __kernel_register_pci_driver as *const () as usize,
        },
        KernelSymbol {
            name: "register_isa_driver",
            addr: __kernel_register_isa_driver as *const () as usize,
        },
        // Block devices
        KernelSymbol {
            name: "block_register_device",
            addr: __kernel_block_register_device as *const () as usize,
        },
    ]
}
