//! virtio MMIO Transport

use alloc::boxed::Box;
use alloc::vec::Vec;
use spin::Mutex;

use vmm::device::VirtioDevice;

/// MMIO region size for each device
pub const MMIO_REGION_SIZE: u64 = 0x1000;

/// MMIO device manager
pub struct MmioDeviceManager {
    /// Base address for MMIO regions
    base_addr: u64,
    /// Registered devices
    devices: Mutex<Vec<MmioDevice>>,
}

/// MMIO device wrapper
pub struct MmioDevice {
    /// Device
    device: Box<dyn VirtioDevice>,
    /// MMIO base address
    base_addr: u64,
    /// Interrupt number
    irq: u32,
}

impl MmioDeviceManager {
    /// Create new MMIO device manager
    pub fn new(base_addr: u64) -> Self {
        MmioDeviceManager {
            base_addr,
            devices: Mutex::new(Vec::new()),
        }
    }

    /// Register a virtio device
    pub fn register(&self, device: Box<dyn VirtioDevice>, irq: u32) -> u64 {
        let mut devices = self.devices.lock();
        let index = devices.len();
        let addr = self.base_addr + (index as u64) * MMIO_REGION_SIZE;

        devices.push(MmioDevice {
            device,
            base_addr: addr,
            irq,
        });

        addr
    }

    /// Handle MMIO read
    pub fn handle_read(&self, addr: u64, data: &mut [u8]) -> bool {
        let devices = self.devices.lock();

        for dev in devices.iter() {
            if addr >= dev.base_addr && addr < dev.base_addr + MMIO_REGION_SIZE {
                let offset = addr - dev.base_addr;
                dev.device.read_config(offset, data);
                return true;
            }
        }

        false
    }

    /// Handle MMIO write
    pub fn handle_write(&self, addr: u64, data: &[u8]) -> bool {
        let mut devices = self.devices.lock();

        for dev in devices.iter_mut() {
            if addr >= dev.base_addr && addr < dev.base_addr + MMIO_REGION_SIZE {
                let offset = addr - dev.base_addr;
                dev.device.write_config(offset, data);
                return true;
            }
        }

        false
    }

    /// Get device by address
    pub fn get_device(&self, addr: u64) -> Option<usize> {
        let devices = self.devices.lock();

        for (i, dev) in devices.iter().enumerate() {
            if addr >= dev.base_addr && addr < dev.base_addr + MMIO_REGION_SIZE {
                return Some(i);
            }
        }

        None
    }

    /// Process queue notification
    pub fn notify_queue(&self, addr: u64, queue: u16) {
        let mut devices = self.devices.lock();

        for dev in devices.iter_mut() {
            if addr >= dev.base_addr && addr < dev.base_addr + MMIO_REGION_SIZE {
                dev.device.process_queue(queue);
                return;
            }
        }
    }

    /// Get device interrupt
    pub fn get_device_irq(&self, addr: u64) -> Option<u32> {
        let devices = self.devices.lock();

        for dev in devices.iter() {
            if addr >= dev.base_addr && addr < dev.base_addr + MMIO_REGION_SIZE {
                return Some(dev.irq);
            }
        }

        None
    }

    /// Reset all devices
    pub fn reset_all(&self) {
        let mut devices = self.devices.lock();
        for dev in devices.iter_mut() {
            dev.device.reset();
        }
    }

    /// Get device count
    pub fn device_count(&self) -> usize {
        self.devices.lock().len()
    }
}

/// Device tree helper for virtio MMIO
pub struct DeviceTreeHelper;

impl DeviceTreeHelper {
    /// Generate FDT node for virtio MMIO device
    pub fn generate_fdt_node(base_addr: u64, irq: u32) -> FdtNode {
        FdtNode {
            compatible: "virtio,mmio",
            reg: (base_addr, MMIO_REGION_SIZE),
            interrupts: irq,
        }
    }
}

/// Simple FDT node representation
pub struct FdtNode {
    pub compatible: &'static str,
    pub reg: (u64, u64),
    pub interrupts: u32,
}
