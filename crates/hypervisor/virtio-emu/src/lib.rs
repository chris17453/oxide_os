//! virtio Device Emulation
//!
//! Emulated virtio devices for the hypervisor.

#![no_std]

extern crate alloc;

pub mod console;
pub mod block;
pub mod net;
pub mod transport;

use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use spin::Mutex;

use vmm::device::{Virtqueue, status};
use vmm::device::mmio as mmio_regs;

/// virtio device base implementation
pub struct VirtioDeviceBase {
    /// Device type
    device_type: u32,
    /// Device features
    features: u64,
    /// Acknowledged features
    acked_features: AtomicU64,
    /// Device status
    status: AtomicU32,
    /// Selected queue
    queue_sel: AtomicU32,
    /// Virtqueues
    queues: Mutex<Vec<Virtqueue>>,
    /// Interrupt status
    interrupt_status: AtomicU32,
    /// Configuration generation
    config_generation: AtomicU32,
}

impl VirtioDeviceBase {
    /// Create new device base
    pub fn new(device_type: u32, features: u64, num_queues: u16) -> Self {
        let mut queues = Vec::new();
        for i in 0..num_queues {
            queues.push(Virtqueue::new(i));
        }

        VirtioDeviceBase {
            device_type,
            features,
            acked_features: AtomicU64::new(0),
            status: AtomicU32::new(0),
            queue_sel: AtomicU32::new(0),
            queues: Mutex::new(queues),
            interrupt_status: AtomicU32::new(0),
            config_generation: AtomicU32::new(0),
        }
    }

    /// Get device type
    pub fn device_type(&self) -> u32 {
        self.device_type
    }

    /// Get features
    pub fn features(&self) -> u64 {
        self.features
    }

    /// Acknowledge features
    pub fn ack_features(&self, features: u64) {
        self.acked_features.store(features & self.features, Ordering::SeqCst);
    }

    /// Get acked features
    pub fn acked_features(&self) -> u64 {
        self.acked_features.load(Ordering::SeqCst)
    }

    /// Get status
    pub fn status(&self) -> u32 {
        self.status.load(Ordering::SeqCst)
    }

    /// Set status
    pub fn set_status(&self, status: u32) {
        self.status.store(status, Ordering::SeqCst);
    }

    /// Get selected queue
    pub fn queue_sel(&self) -> u16 {
        self.queue_sel.load(Ordering::SeqCst) as u16
    }

    /// Set queue selector
    pub fn set_queue_sel(&self, sel: u16) {
        self.queue_sel.store(sel as u32, Ordering::SeqCst);
    }

    /// Get queue
    pub fn queue(&self, index: u16) -> Option<Virtqueue> {
        self.queues.lock().get(index as usize).cloned()
    }

    /// Configure queue
    pub fn configure_queue<F: FnOnce(&mut Virtqueue)>(&self, index: u16, f: F) {
        if let Some(queue) = self.queues.lock().get_mut(index as usize) {
            f(queue);
        }
    }

    /// Get interrupt status
    pub fn interrupt_status(&self) -> u32 {
        self.interrupt_status.load(Ordering::SeqCst)
    }

    /// Set interrupt status
    pub fn set_interrupt(&self) {
        self.interrupt_status.fetch_or(1, Ordering::SeqCst);
    }

    /// Acknowledge interrupt
    pub fn ack_interrupt(&self, value: u32) {
        self.interrupt_status.fetch_and(!value, Ordering::SeqCst);
    }

    /// Get config generation
    pub fn config_generation(&self) -> u32 {
        self.config_generation.load(Ordering::SeqCst)
    }

    /// Increment config generation
    pub fn inc_config_generation(&self) {
        self.config_generation.fetch_add(1, Ordering::SeqCst);
    }

    /// Reset device
    pub fn reset(&self) {
        self.status.store(0, Ordering::SeqCst);
        self.acked_features.store(0, Ordering::SeqCst);
        self.interrupt_status.store(0, Ordering::SeqCst);
        self.queue_sel.store(0, Ordering::SeqCst);

        let mut queues = self.queues.lock();
        for queue in queues.iter_mut() {
            queue.ready = false;
            queue.size = 0;
            queue.desc_addr = 0;
            queue.avail_addr = 0;
            queue.used_addr = 0;
        }
    }

    /// Check if device is activated
    pub fn is_activated(&self) -> bool {
        self.status.load(Ordering::SeqCst) & status::DRIVER_OK as u32 != 0
    }

    /// Handle MMIO read
    pub fn mmio_read(&self, offset: u64, data: &mut [u8]) {
        let value = match offset {
            mmio_regs::MAGIC_VALUE => 0x74726976, // "virt"
            mmio_regs::VERSION => 2,
            mmio_regs::DEVICE_ID => self.device_type,
            mmio_regs::VENDOR_ID => 0x554D4551, // "QEMU" for compatibility
            mmio_regs::DEVICE_FEATURES => {
                let sel = self.queue_sel.load(Ordering::SeqCst);
                if sel == 0 {
                    (self.features & 0xFFFF_FFFF) as u32
                } else {
                    ((self.features >> 32) & 0xFFFF_FFFF) as u32
                }
            }
            mmio_regs::QUEUE_NUM_MAX => 256,
            mmio_regs::QUEUE_READY => {
                let sel = self.queue_sel();
                self.queue(sel).map(|q| if q.ready { 1 } else { 0 }).unwrap_or(0)
            }
            mmio_regs::INTERRUPT_STATUS => self.interrupt_status(),
            mmio_regs::STATUS => self.status(),
            mmio_regs::CONFIG_GENERATION => self.config_generation(),
            _ => 0,
        };

        match data.len() {
            1 => data[0] = value as u8,
            2 => data.copy_from_slice(&(value as u16).to_le_bytes()),
            4 => data.copy_from_slice(&value.to_le_bytes()),
            8 => data.copy_from_slice(&(value as u64).to_le_bytes()),
            _ => {}
        }
    }

    /// Handle MMIO write
    pub fn mmio_write(&self, offset: u64, data: &[u8]) {
        let value = match data.len() {
            1 => data[0] as u32,
            2 => u16::from_le_bytes([data[0], data[1]]) as u32,
            4 => u32::from_le_bytes([data[0], data[1], data[2], data[3]]),
            8 => u64::from_le_bytes([data[0], data[1], data[2], data[3], data[4], data[5], data[6], data[7]]) as u32,
            _ => return,
        };

        match offset {
            mmio_regs::DEVICE_FEATURES_SEL => {
                self.queue_sel.store(value, Ordering::SeqCst);
            }
            mmio_regs::DRIVER_FEATURES => {
                let sel = self.queue_sel.load(Ordering::SeqCst);
                let shift = if sel == 0 { 0 } else { 32 };
                let mask = 0xFFFF_FFFF << shift;
                let current = self.acked_features.load(Ordering::SeqCst);
                self.acked_features.store((current & !mask) | ((value as u64) << shift), Ordering::SeqCst);
            }
            mmio_regs::DRIVER_FEATURES_SEL => {
                self.queue_sel.store(value, Ordering::SeqCst);
            }
            mmio_regs::QUEUE_SEL => {
                self.set_queue_sel(value as u16);
            }
            mmio_regs::QUEUE_NUM => {
                let sel = self.queue_sel();
                self.configure_queue(sel, |q| q.size = value as u16);
            }
            mmio_regs::QUEUE_READY => {
                let sel = self.queue_sel();
                self.configure_queue(sel, |q| q.ready = value != 0);
            }
            mmio_regs::QUEUE_DESC_LOW => {
                let sel = self.queue_sel();
                self.configure_queue(sel, |q| {
                    q.desc_addr = (q.desc_addr & 0xFFFF_FFFF_0000_0000) | (value as u64);
                });
            }
            mmio_regs::QUEUE_DESC_HIGH => {
                let sel = self.queue_sel();
                self.configure_queue(sel, |q| {
                    q.desc_addr = (q.desc_addr & 0x0000_0000_FFFF_FFFF) | ((value as u64) << 32);
                });
            }
            mmio_regs::QUEUE_DRIVER_LOW => {
                let sel = self.queue_sel();
                self.configure_queue(sel, |q| {
                    q.avail_addr = (q.avail_addr & 0xFFFF_FFFF_0000_0000) | (value as u64);
                });
            }
            mmio_regs::QUEUE_DRIVER_HIGH => {
                let sel = self.queue_sel();
                self.configure_queue(sel, |q| {
                    q.avail_addr = (q.avail_addr & 0x0000_0000_FFFF_FFFF) | ((value as u64) << 32);
                });
            }
            mmio_regs::QUEUE_DEVICE_LOW => {
                let sel = self.queue_sel();
                self.configure_queue(sel, |q| {
                    q.used_addr = (q.used_addr & 0xFFFF_FFFF_0000_0000) | (value as u64);
                });
            }
            mmio_regs::QUEUE_DEVICE_HIGH => {
                let sel = self.queue_sel();
                self.configure_queue(sel, |q| {
                    q.used_addr = (q.used_addr & 0x0000_0000_FFFF_FFFF) | ((value as u64) << 32);
                });
            }
            mmio_regs::INTERRUPT_ACK => {
                self.ack_interrupt(value);
            }
            mmio_regs::STATUS => {
                if value == 0 {
                    self.reset();
                } else {
                    self.set_status(value);
                }
            }
            _ => {}
        }
    }
}
