//! PCI Bus Support
//!
//! Provides PCI device enumeration and configuration space access.
//!
//! Currently uses x86_64 port I/O via arch_traits::PortIo trait.
//! — TorqueJax

#![no_std]

extern crate alloc;

use alloc::vec::Vec;
use arch_traits::PortIo;
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
/// Base formula: 0x1040 + device_type
/// — TorqueJax: the silicon never lies
pub mod virtio_modern {
    pub const NET: u16 = 0x1041;   // Network card (type 1)
    pub const BLOCK: u16 = 0x1042; // Block device (type 2)
    pub const GPU: u16 = 0x1050;   // GPU device (type 16)
    pub const SOUND: u16 = 0x1059; // Sound device (type 25)
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

    /// Check if this is a VirtIO GPU device (modern only — no legacy transitional ID)
    pub fn is_virtio_gpu(&self) -> bool {
        self.vendor_id == vendor::VIRTIO && self.device_id == virtio_modern::GPU
    }

    /// Check if this is a VirtIO sound device (modern only — no legacy transitional ID)
    pub fn is_virtio_snd(&self) -> bool {
        self.vendor_id == vendor::VIRTIO && self.device_id == virtio_modern::SOUND
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

/// Find all VirtIO GPU devices (modern PCI only)
/// — TorqueJax: pixels need a pipeline
pub fn find_virtio_gpu() -> Vec<PciDevice> {
    DEVICES
        .lock()
        .iter()
        .filter(|d| d.is_virtio_gpu())
        .cloned()
        .collect()
}

/// Find all VirtIO sound devices (modern PCI only)
/// — TorqueJax: sound is just vibrations on the bus
pub fn find_virtio_snd() -> Vec<PciDevice> {
    DEVICES
        .lock()
        .iter()
        .filter(|d| d.is_virtio_snd())
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

// ============================================================================
// VirtIO PCI Capability Walking
// ============================================================================
//
// Modern VirtIO devices (GPU, Sound, etc.) expose config regions through
// PCI capabilities rather than legacy I/O ports. We walk the capability
// linked list to find vendor-specific (0x09) entries with VirtIO structure
// types.
// — TorqueJax: chasing pointers through config space like a bloodhound

/// VirtIO PCI capability structure types (VirtIO spec §4.1.4)
pub mod virtio_pci_cap_type {
    pub const COMMON_CFG: u8 = 1;
    pub const NOTIFY_CFG: u8 = 2;
    pub const ISR_CFG: u8 = 3;
    pub const DEVICE_CFG: u8 = 4;
    pub const PCI_CFG: u8 = 5;
}

/// Parsed VirtIO PCI capability
#[derive(Debug, Clone, Copy)]
pub struct VirtioPciCap {
    /// Capability type (1=common, 2=notify, 3=ISR, 4=device, 5=PCI)
    pub cfg_type: u8,
    /// BAR index (0-5)
    pub bar: u8,
    /// Offset within the BAR
    pub offset: u32,
    /// Length of the region
    pub length: u32,
}

/// Collection of all VirtIO PCI capability regions for a device
#[derive(Debug, Clone)]
pub struct VirtioPciCaps {
    pub common: Option<VirtioPciCap>,
    pub notify: Option<VirtioPciCap>,
    pub isr: Option<VirtioPciCap>,
    pub device_cfg: Option<VirtioPciCap>,
    /// Notify offset multiplier (from notify cap extra dword)
    pub notify_off_multiplier: u32,
}

/// Walk PCI capability list and extract VirtIO PCI capabilities
///
/// VirtIO modern devices encode their register regions as vendor-specific
/// PCI capabilities (cap_id=0x09). Each one specifies a BAR + offset + length
/// for a particular config region.
/// — TorqueJax: follow the breadcrumbs through config space
pub fn find_virtio_caps(dev: &PciDevice) -> VirtioPciCaps {
    let mut caps = VirtioPciCaps {
        common: None,
        notify: None,
        isr: None,
        device_cfg: None,
        notify_off_multiplier: 0,
    };

    // Read status register to check if capabilities list exists
    let status = config_read16(dev.address, 0x06);
    if status & (1 << 4) == 0 {
        // No capabilities list bit set
        return caps;
    }

    // Read capabilities pointer (offset 0x34, low 8 bits)
    let mut cap_ptr = config_read8(dev.address, 0x34) & 0xFC;
    if cap_ptr == 0 {
        return caps;
    }

    // Walk the linked list (bounded to prevent infinite loops)
    let mut seen = 0u32;
    while cap_ptr != 0 && seen < 48 {
        seen += 1;

        let cap_id = config_read8(dev.address, cap_ptr);
        let next_ptr = config_read8(dev.address, cap_ptr + 1);

        // VirtIO vendor-specific capability: cap_id == 0x09
        if cap_id == 0x09 {
            // VirtIO PCI cap layout (VirtIO spec §4.1.4):
            //   offset +2: cfg_type (u8)
            //   offset +3: bar (u8)
            //   offset +4: offset within BAR (u32, LE)
            //   offset +8: length (u32, LE)
            //   For notify cap, offset +12: notify_off_multiplier (u32, LE)
            let type_bar = config_read32(dev.address, cap_ptr + 4);
            let cfg_type = (type_bar & 0xFF) as u8;
            let bar = ((type_bar >> 8) & 0xFF) as u8;
            // padding at +6,+7
            let bar_offset = config_read32(dev.address, cap_ptr + 8);
            let bar_length = config_read32(dev.address, cap_ptr + 12);

            let cap = VirtioPciCap {
                cfg_type,
                bar,
                offset: bar_offset,
                length: bar_length,
            };

            match cfg_type {
                virtio_pci_cap_type::COMMON_CFG => caps.common = Some(cap),
                virtio_pci_cap_type::NOTIFY_CFG => {
                    caps.notify = Some(cap);
                    // Notify cap has an extra dword: the multiplier
                    caps.notify_off_multiplier = config_read32(dev.address, cap_ptr + 16);
                }
                virtio_pci_cap_type::ISR_CFG => caps.isr = Some(cap),
                virtio_pci_cap_type::DEVICE_CFG => caps.device_cfg = Some(cap),
                _ => {} // PCI_CFG or unknown — skip
            }
        }

        cap_ptr = next_ptr & 0xFC;
    }

    caps
}

/// Resolve a VirtIO PCI capability to a kernel virtual address.
///
/// Takes a PCI device and a capability, reads the BAR's physical base,
/// adds the capability offset, and returns the direct-mapped kernel
/// virtual address via PHYS_MAP_BASE.
///
/// Returns None if the referenced BAR is not a memory BAR.
/// — TorqueJax: from silicon pin to kernel pointer in one hop
pub fn resolve_cap_addr(dev: &PciDevice, cap: &VirtioPciCap) -> Option<usize> {
    let bar_idx = cap.bar as usize;
    if bar_idx >= 6 {
        return None;
    }

    match dev.bars[bar_idx] {
        PciBar::Memory { address, .. } => {
            // Physical address of the BAR region + capability offset
            let phys = address + cap.offset as u64;
            // Convert to kernel virtual address via direct physical map
            // PHYS_MAP_BASE = 0xFFFF_8000_0000_0000
            let virt = phys + 0xFFFF_8000_0000_0000;
            Some(virt as usize)
        }
        _ => None,
    }
}

// ============================================================================
// VirtIO PCI Transport
// ============================================================================
//
// Modern VirtIO devices expose their registers through BAR-mapped memory
// regions identified by PCI capabilities. This transport struct provides
// type-safe register access for the common config, notify, ISR, and
// device-specific regions.
// — TorqueJax: the bridge between PCI config space and virtio register I/O

/// VirtIO PCI common configuration layout (VirtIO spec §4.1.4.3)
///
/// All offsets relative to the common config BAR region.
pub mod virtio_pci_common {
    pub const DEVICE_FEATURE_SELECT: usize = 0x00;   // u32
    pub const DEVICE_FEATURE: usize = 0x04;           // u32
    pub const DRIVER_FEATURE_SELECT: usize = 0x08;    // u32
    pub const DRIVER_FEATURE: usize = 0x0C;           // u32
    pub const MSIX_CONFIG: usize = 0x10;              // u16
    pub const NUM_QUEUES: usize = 0x12;               // u16
    pub const DEVICE_STATUS: usize = 0x14;            // u8
    pub const CONFIG_GENERATION: usize = 0x15;        // u8
    pub const QUEUE_SELECT: usize = 0x16;             // u16
    pub const QUEUE_SIZE: usize = 0x18;               // u16
    pub const QUEUE_MSIX_VECTOR: usize = 0x1A;        // u16
    pub const QUEUE_ENABLE: usize = 0x1C;             // u16
    pub const QUEUE_NOTIFY_OFF: usize = 0x1E;         // u16
    pub const QUEUE_DESC_LO: usize = 0x20;            // u32
    pub const QUEUE_DESC_HI: usize = 0x24;            // u32
    pub const QUEUE_AVAIL_LO: usize = 0x28;           // u32
    pub const QUEUE_AVAIL_HI: usize = 0x2C;           // u32
    pub const QUEUE_USED_LO: usize = 0x30;            // u32
    pub const QUEUE_USED_HI: usize = 0x34;            // u32
}

/// VirtIO PCI transport — register-level access via BAR-mapped regions
///
/// This wraps the four key config regions that modern VirtIO PCI devices
/// expose through capability structures: common config, notify, ISR status,
/// and device-specific config.
pub struct VirtioPciTransport {
    /// Common configuration registers (virtqueue setup, feature negotiation)
    pub common: usize,
    /// Notification area (write to kick virtqueues)
    pub notify: usize,
    /// ISR status register (interrupt acknowledge)
    pub isr: usize,
    /// Device-specific configuration (display info, sound config, etc.)
    pub device_cfg: usize,
    /// Notify offset multiplier: actual notify addr = notify_base + queue_notify_off * multiplier
    pub notify_off_multiplier: u32,
}

impl VirtioPciTransport {
    /// Build transport from parsed PCI capabilities
    ///
    /// Returns None if required capability regions are missing.
    pub fn from_caps(dev: &PciDevice, caps: &VirtioPciCaps) -> Option<Self> {
        let common_cap = caps.common.as_ref()?;
        let notify_cap = caps.notify.as_ref()?;
        let isr_cap = caps.isr.as_ref()?;

        let common = resolve_cap_addr(dev, common_cap)?;
        let notify = resolve_cap_addr(dev, notify_cap)?;
        let isr = resolve_cap_addr(dev, isr_cap)?;
        let device_cfg = caps
            .device_cfg
            .as_ref()
            .and_then(|c| resolve_cap_addr(dev, c))
            .unwrap_or(0);

        Some(VirtioPciTransport {
            common,
            notify,
            isr,
            device_cfg,
            notify_off_multiplier: caps.notify_off_multiplier,
        })
    }

    // ---- Status register ----

    /// Read device status byte
    pub fn read_status(&self) -> u8 {
        unsafe {
            core::ptr::read_volatile(
                (self.common + virtio_pci_common::DEVICE_STATUS) as *const u8,
            )
        }
    }

    /// Write device status byte
    pub fn write_status(&self, val: u8) {
        unsafe {
            core::ptr::write_volatile(
                (self.common + virtio_pci_common::DEVICE_STATUS) as *mut u8,
                val,
            );
        }
    }

    // ---- Feature negotiation ----

    /// Read device feature bits for given selector page (0 or 1)
    pub fn read_device_features(&self, sel: u32) -> u32 {
        unsafe {
            core::ptr::write_volatile(
                (self.common + virtio_pci_common::DEVICE_FEATURE_SELECT) as *mut u32,
                sel,
            );
            core::ptr::read_volatile(
                (self.common + virtio_pci_common::DEVICE_FEATURE) as *const u32,
            )
        }
    }

    /// Write driver feature bits for given selector page (0 or 1)
    pub fn write_driver_features(&self, sel: u32, val: u32) {
        unsafe {
            core::ptr::write_volatile(
                (self.common + virtio_pci_common::DRIVER_FEATURE_SELECT) as *mut u32,
                sel,
            );
            core::ptr::write_volatile(
                (self.common + virtio_pci_common::DRIVER_FEATURE) as *mut u32,
                val,
            );
        }
    }

    // ---- Queue setup ----

    /// Select which virtqueue subsequent queue registers refer to
    pub fn select_queue(&self, idx: u16) {
        unsafe {
            core::ptr::write_volatile(
                (self.common + virtio_pci_common::QUEUE_SELECT) as *mut u16,
                idx,
            );
        }
    }

    /// Read maximum queue size for currently selected queue
    pub fn queue_max_size(&self) -> u16 {
        unsafe {
            core::ptr::read_volatile(
                (self.common + virtio_pci_common::QUEUE_SIZE) as *const u16,
            )
        }
    }

    /// Set queue size for currently selected queue
    pub fn set_queue_size(&self, size: u16) {
        unsafe {
            core::ptr::write_volatile(
                (self.common + virtio_pci_common::QUEUE_SIZE) as *mut u16,
                size,
            );
        }
    }

    /// Set descriptor table physical address for currently selected queue
    pub fn set_queue_desc(&self, addr: u64) {
        unsafe {
            core::ptr::write_volatile(
                (self.common + virtio_pci_common::QUEUE_DESC_LO) as *mut u32,
                addr as u32,
            );
            core::ptr::write_volatile(
                (self.common + virtio_pci_common::QUEUE_DESC_HI) as *mut u32,
                (addr >> 32) as u32,
            );
        }
    }

    /// Set available ring physical address for currently selected queue
    pub fn set_queue_avail(&self, addr: u64) {
        unsafe {
            core::ptr::write_volatile(
                (self.common + virtio_pci_common::QUEUE_AVAIL_LO) as *mut u32,
                addr as u32,
            );
            core::ptr::write_volatile(
                (self.common + virtio_pci_common::QUEUE_AVAIL_HI) as *mut u32,
                (addr >> 32) as u32,
            );
        }
    }

    /// Set used ring physical address for currently selected queue
    pub fn set_queue_used(&self, addr: u64) {
        unsafe {
            core::ptr::write_volatile(
                (self.common + virtio_pci_common::QUEUE_USED_LO) as *mut u32,
                addr as u32,
            );
            core::ptr::write_volatile(
                (self.common + virtio_pci_common::QUEUE_USED_HI) as *mut u32,
                (addr >> 32) as u32,
            );
        }
    }

    /// Enable the currently selected queue
    pub fn enable_queue(&self) {
        unsafe {
            core::ptr::write_volatile(
                (self.common + virtio_pci_common::QUEUE_ENABLE) as *mut u16,
                1,
            );
        }
    }

    /// Read queue notify offset for currently selected queue
    pub fn queue_notify_off(&self) -> u16 {
        unsafe {
            core::ptr::read_volatile(
                (self.common + virtio_pci_common::QUEUE_NOTIFY_OFF) as *const u16,
            )
        }
    }

    /// Notify (kick) a virtqueue
    ///
    /// Writes the queue index to the correct notify address computed from
    /// notify_base + queue_notify_off * notify_off_multiplier.
    pub fn notify_queue(&self, queue_idx: u16) {
        let notify_off = self.queue_notify_off();
        let addr = self.notify + (notify_off as usize) * (self.notify_off_multiplier as usize);
        unsafe {
            core::ptr::write_volatile(addr as *mut u16, queue_idx);
        }
    }

    // ---- Device-specific config ----

    /// Read a u32 from device-specific config at given byte offset
    pub fn read_device_config_u32(&self, offset: usize) -> u32 {
        if self.device_cfg == 0 {
            return 0;
        }
        unsafe {
            core::ptr::read_volatile((self.device_cfg + offset) as *const u32)
        }
    }

    /// Read a u8 from device-specific config at given byte offset
    pub fn read_device_config_u8(&self, offset: usize) -> u8 {
        if self.device_cfg == 0 {
            return 0;
        }
        unsafe {
            core::ptr::read_volatile((self.device_cfg + offset) as *const u8)
        }
    }

    // ---- ISR ----

    /// Read and acknowledge ISR status
    pub fn read_isr(&self) -> u8 {
        unsafe {
            core::ptr::read_volatile(self.isr as *const u8)
        }
    }
}

// Architecture-agnostic I/O port access via trait
// Uses the current architecture's PortIo implementation
// — TorqueJax
#[inline]
unsafe fn outl(port: u16, value: u32) {
    unsafe { arch_x86_64::X86_64::outl(port, value) }
}

#[inline]
unsafe fn inl(port: u16) -> u32 {
    unsafe { arch_x86_64::X86_64::inl(port) }
}
