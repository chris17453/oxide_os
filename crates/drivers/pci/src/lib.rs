//! PCI Bus Support
//!
//! Provides PCI device enumeration and configuration space access for x86_64.

#![no_std]

extern crate alloc;

use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, Ordering};
use spin::Mutex;

/// PCI configuration space I/O ports
const CONFIG_ADDRESS: u16 = 0xCF8;
const CONFIG_DATA: u16 = 0xCFC;

/// PCI class codes for common device types
pub mod class {
    pub const NETWORK_CONTROLLER: u8 = 0x02;
    pub const ETHERNET: u8 = 0x00;
}

/// PCI vendor IDs
pub mod vendor {
    pub const VIRTIO: u16 = 0x1AF4;
    pub const INTEL: u16 = 0x8086;
    pub const AMD: u16 = 0x1022;
}

/// VirtIO device IDs (transitional)
pub mod virtio_device {
    pub const NET: u16 = 0x1000; // Network card
    pub const BLOCK: u16 = 0x1001; // Block device
    pub const CONSOLE: u16 = 0x1003; // Console
    pub const ENTROPY: u16 = 0x1005; // Entropy source
    pub const BALLOON: u16 = 0x1002; // Memory balloon
}

/// VirtIO device IDs (modern, non-transitional)
pub mod virtio_modern {
    pub const NET: u16 = 0x1041; // Network card
    pub const BLOCK: u16 = 0x1042; // Block device
}

/// PCI device address
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PciAddress {
    pub bus: u8,
    pub device: u8,
    pub function: u8,
}

impl PciAddress {
    pub fn new(bus: u8, device: u8, function: u8) -> Self {
        PciAddress {
            bus,
            device,
            function,
        }
    }

    /// Create config address for given register offset
    fn config_address(&self, offset: u8) -> u32 {
        (1 << 31)
            | ((self.bus as u32) << 16)
            | ((self.device as u32) << 11)
            | ((self.function as u32) << 8)
            | ((offset as u32) & 0xFC)
    }
}

/// PCI Base Address Register (BAR)
#[derive(Debug, Clone, Copy)]
pub enum PciBar {
    /// Memory-mapped I/O
    Memory {
        address: u64,
        size: u64,
        prefetchable: bool,
        is_64bit: bool,
    },
    /// Port I/O
    Io { port: u32, size: u32 },
    /// Not present or invalid
    None,
}

/// PCI device information
#[derive(Debug, Clone)]
pub struct PciDevice {
    pub address: PciAddress,
    pub vendor_id: u16,
    pub device_id: u16,
    pub class_code: u8,
    pub subclass: u8,
    pub prog_if: u8,
    pub revision: u8,
    pub header_type: u8,
    pub interrupt_line: u8,
    pub interrupt_pin: u8,
    pub bars: [PciBar; 6],
}

impl PciDevice {
    /// Check if this is a VirtIO network device
    pub fn is_virtio_net(&self) -> bool {
        self.vendor_id == vendor::VIRTIO
            && (self.device_id == virtio_device::NET || self.device_id == virtio_modern::NET)
    }

    /// Check if this is a VirtIO block device
    pub fn is_virtio_blk(&self) -> bool {
        self.vendor_id == vendor::VIRTIO
            && (self.device_id == virtio_device::BLOCK || self.device_id == virtio_modern::BLOCK)
    }

    /// Check if this is a network device
    pub fn is_network(&self) -> bool {
        self.class_code == class::NETWORK_CONTROLLER
    }

    /// Get the first memory BAR address
    pub fn bar0_address(&self) -> Option<u64> {
        match self.bars[0] {
            PciBar::Memory { address, .. } => Some(address),
            _ => None,
        }
    }
}

/// Read a 32-bit value from PCI configuration space
pub fn config_read32(addr: PciAddress, offset: u8) -> u32 {
    unsafe {
        let address = addr.config_address(offset);
        outl(CONFIG_ADDRESS, address);
        inl(CONFIG_DATA)
    }
}

/// Write a 32-bit value to PCI configuration space
pub fn config_write32(addr: PciAddress, offset: u8, value: u32) {
    unsafe {
        let address = addr.config_address(offset);
        outl(CONFIG_ADDRESS, address);
        outl(CONFIG_DATA, value);
    }
}

/// Read a 16-bit value from PCI configuration space
pub fn config_read16(addr: PciAddress, offset: u8) -> u16 {
    let val32 = config_read32(addr, offset & 0xFC);
    let shift = ((offset & 2) * 8) as u32;
    ((val32 >> shift) & 0xFFFF) as u16
}

/// Read an 8-bit value from PCI configuration space
pub fn config_read8(addr: PciAddress, offset: u8) -> u8 {
    let val32 = config_read32(addr, offset & 0xFC);
    let shift = ((offset & 3) * 8) as u32;
    ((val32 >> shift) & 0xFF) as u8
}

/// Probe a PCI device at given address
fn probe_device(bus: u8, device: u8, function: u8) -> Option<PciDevice> {
    let addr = PciAddress::new(bus, device, function);

    let vendor_device = config_read32(addr, 0x00);
    let vendor_id = (vendor_device & 0xFFFF) as u16;
    let device_id = ((vendor_device >> 16) & 0xFFFF) as u16;

    // Check if device exists
    if vendor_id == 0xFFFF {
        return None;
    }

    let class_revision = config_read32(addr, 0x08);
    let revision = (class_revision & 0xFF) as u8;
    let prog_if = ((class_revision >> 8) & 0xFF) as u8;
    let subclass = ((class_revision >> 16) & 0xFF) as u8;
    let class_code = ((class_revision >> 24) & 0xFF) as u8;

    let header_type = config_read8(addr, 0x0E) & 0x7F;

    let int_line_pin = config_read32(addr, 0x3C);
    let interrupt_line = (int_line_pin & 0xFF) as u8;
    let interrupt_pin = ((int_line_pin >> 8) & 0xFF) as u8;

    // Read BARs (only for header type 0)
    let mut bars = [PciBar::None; 6];
    if header_type == 0 {
        let mut i = 0;
        while i < 6 {
            let bar_offset = (0x10 + i * 4) as u8;
            let bar = config_read32(addr, bar_offset);

            if bar == 0 {
                i += 1;
                continue;
            }

            if bar & 1 == 0 {
                // Memory BAR
                let is_64bit = (bar >> 1) & 3 == 2;
                let prefetchable = (bar >> 3) & 1 == 1;

                let address = if is_64bit && i < 5 {
                    let bar_high = config_read32(addr, bar_offset + 4);
                    ((bar_high as u64) << 32) | ((bar as u64) & !0xF)
                } else {
                    (bar as u64) & !0xF
                };

                // Get BAR size by writing all 1s and reading back
                config_write32(addr, bar_offset, 0xFFFFFFFF);
                let size_mask = config_read32(addr, bar_offset);
                config_write32(addr, bar_offset, bar); // Restore

                let size = if size_mask == 0 {
                    0
                } else {
                    (!(size_mask & !0xF) + 1) as u64
                };

                bars[i] = PciBar::Memory {
                    address,
                    size,
                    prefetchable,
                    is_64bit,
                };

                if is_64bit {
                    i += 1; // Skip next BAR (upper 32 bits)
                }
            } else {
                // I/O BAR
                let port = bar & !3;

                config_write32(addr, bar_offset, 0xFFFFFFFF);
                let size_mask = config_read32(addr, bar_offset);
                config_write32(addr, bar_offset, bar);

                let size = if size_mask == 0 {
                    0
                } else {
                    !(size_mask & !3) + 1
                };

                bars[i] = PciBar::Io { port, size };
            }

            i += 1;
        }
    }

    Some(PciDevice {
        address: addr,
        vendor_id,
        device_id,
        class_code,
        subclass,
        prog_if,
        revision,
        header_type,
        interrupt_line,
        interrupt_pin,
        bars,
    })
}

/// Check if device has multiple functions
fn is_multifunction(bus: u8, device: u8) -> bool {
    let addr = PciAddress::new(bus, device, 0);
    let header = config_read8(addr, 0x0E);
    header & 0x80 != 0
}

/// Global device list
static DEVICES: Mutex<Vec<PciDevice>> = Mutex::new(Vec::new());
static INITIALIZED: AtomicBool = AtomicBool::new(false);

/// Enumerate all PCI devices
pub fn enumerate() {
    if INITIALIZED.swap(true, Ordering::SeqCst) {
        return; // Already initialized
    }

    let mut devices = Vec::new();

    // Scan all buses, devices, and functions
    for bus in 0..=255u8 {
        for device in 0..32u8 {
            // Check function 0
            if let Some(dev) = probe_device(bus, device, 0) {
                devices.push(dev);

                // Check for multi-function device
                if is_multifunction(bus, device) {
                    for function in 1..8u8 {
                        if let Some(dev) = probe_device(bus, device, function) {
                            devices.push(dev);
                        }
                    }
                }
            }
        }
    }

    *DEVICES.lock() = devices;
}

/// Get all enumerated devices
pub fn devices() -> Vec<PciDevice> {
    DEVICES.lock().clone()
}

/// Find devices by vendor and device ID
pub fn find_device(vendor_id: u16, device_id: u16) -> Vec<PciDevice> {
    DEVICES
        .lock()
        .iter()
        .filter(|d| d.vendor_id == vendor_id && d.device_id == device_id)
        .cloned()
        .collect()
}

/// Find devices by class code
pub fn find_by_class(class_code: u8, subclass: u8) -> Vec<PciDevice> {
    DEVICES
        .lock()
        .iter()
        .filter(|d| d.class_code == class_code && d.subclass == subclass)
        .cloned()
        .collect()
}

/// Find VirtIO network devices
pub fn find_virtio_net() -> Vec<PciDevice> {
    DEVICES
        .lock()
        .iter()
        .filter(|d| d.is_virtio_net())
        .cloned()
        .collect()
}

/// Find all VirtIO block devices
pub fn find_virtio_blk() -> Vec<PciDevice> {
    DEVICES
        .lock()
        .iter()
        .filter(|d| d.is_virtio_blk())
        .cloned()
        .collect()
}

/// Enable bus mastering for a device
pub fn enable_bus_master(addr: PciAddress) {
    let cmd = config_read16(addr, 0x04);
    config_write32(addr, 0x04, (cmd | 0x04) as u32); // Set bit 2 (bus master)
}

/// Enable memory space access for a device
pub fn enable_memory_space(addr: PciAddress) {
    let cmd = config_read16(addr, 0x04);
    config_write32(addr, 0x04, (cmd | 0x02) as u32); // Set bit 1 (memory space)
}

/// Enable I/O space access for a device
pub fn enable_io_space(addr: PciAddress) {
    let cmd = config_read16(addr, 0x04);
    config_write32(addr, 0x04, (cmd | 0x01) as u32); // Set bit 0 (I/O space)
}

// x86_64 I/O port access
#[inline]
unsafe fn outl(port: u16, value: u32) {
    core::arch::asm!(
        "out dx, eax",
        in("dx") port,
        in("eax") value,
        options(nomem, nostack, preserves_flags)
    );
}

#[inline]
unsafe fn inl(port: u16) -> u32 {
    let value: u32;
    core::arch::asm!(
        "in eax, dx",
        out("eax") value,
        in("dx") port,
        options(nomem, nostack, preserves_flags)
    );
    value
}
