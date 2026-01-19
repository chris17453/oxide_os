//! virtio-console Emulation

use alloc::collections::VecDeque;
use alloc::vec::Vec;
use spin::Mutex;

use vmm::device::VirtioDevice;
use crate::VirtioDeviceBase;

/// virtio-console device type
pub const VIRTIO_CONSOLE_DEVICE_TYPE: u32 = 3;

/// Console features
pub mod features {
    /// Device has console resize feature
    pub const VIRTIO_CONSOLE_F_SIZE: u64 = 1 << 0;
    /// Device has multiple ports
    pub const VIRTIO_CONSOLE_F_MULTIPORT: u64 = 1 << 1;
    /// Device has emergency write
    pub const VIRTIO_CONSOLE_F_EMERG_WRITE: u64 = 1 << 2;
}

/// Console configuration
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct ConsoleConfig {
    /// Console columns
    pub cols: u16,
    /// Console rows
    pub rows: u16,
    /// Maximum number of ports
    pub max_nr_ports: u32,
    /// Emergency write value
    pub emerg_wr: u32,
}

/// virtio-console device
pub struct VirtioConsole {
    /// Base device
    base: VirtioDeviceBase,
    /// Configuration
    config: Mutex<ConsoleConfig>,
    /// Input buffer (from host to guest)
    input_buf: Mutex<VecDeque<u8>>,
    /// Output callback
    output_callback: Mutex<Option<fn(&[u8])>>,
}

impl VirtioConsole {
    /// Create new console
    pub fn new(cols: u16, rows: u16) -> Self {
        VirtioConsole {
            base: VirtioDeviceBase::new(
                VIRTIO_CONSOLE_DEVICE_TYPE,
                features::VIRTIO_CONSOLE_F_SIZE,
                2, // RX and TX queues
            ),
            config: Mutex::new(ConsoleConfig {
                cols,
                rows,
                max_nr_ports: 1,
                emerg_wr: 0,
            }),
            input_buf: Mutex::new(VecDeque::new()),
            output_callback: Mutex::new(None),
        }
    }

    /// Set output callback
    pub fn set_output_callback(&self, callback: fn(&[u8])) {
        *self.output_callback.lock() = Some(callback);
    }

    /// Queue input data (from host to guest)
    pub fn queue_input(&self, data: &[u8]) {
        let mut buf = self.input_buf.lock();
        buf.extend(data);
        self.base.set_interrupt();
    }

    /// Get pending input
    pub fn get_input(&self, max_len: usize) -> Vec<u8> {
        let mut buf = self.input_buf.lock();
        let len = buf.len().min(max_len);
        buf.drain(..len).collect()
    }

    /// Process output from guest
    fn process_output(&self, data: &[u8]) {
        if let Some(callback) = *self.output_callback.lock() {
            callback(data);
        }
    }

    /// Read config
    fn read_config_inner(&self, offset: u64, data: &mut [u8]) {
        let config = self.config.lock();
        let config_bytes = unsafe {
            core::slice::from_raw_parts(
                &*config as *const ConsoleConfig as *const u8,
                core::mem::size_of::<ConsoleConfig>(),
            )
        };

        let start = offset as usize;
        let end = (start + data.len()).min(config_bytes.len());
        if start < config_bytes.len() {
            data[..end - start].copy_from_slice(&config_bytes[start..end]);
        }
    }

    /// Update console size
    pub fn set_size(&self, cols: u16, rows: u16) {
        let mut config = self.config.lock();
        config.cols = cols;
        config.rows = rows;
        self.base.inc_config_generation();
    }
}

impl VirtioDevice for VirtioConsole {
    fn device_type(&self) -> u32 {
        self.base.device_type()
    }

    fn features(&self) -> u64 {
        self.base.features()
    }

    fn ack_features(&mut self, features: u64) {
        self.base.ack_features(features);
    }

    fn read_config(&self, offset: u64, data: &mut [u8]) {
        if offset < 0x100 {
            self.base.mmio_read(offset, data);
        } else {
            self.read_config_inner(offset - 0x100, data);
        }
    }

    fn write_config(&mut self, offset: u64, data: &[u8]) {
        if offset < 0x100 {
            self.base.mmio_write(offset, data);
        }
        // Console config is read-only
    }

    fn reset(&mut self) {
        self.base.reset();
        self.input_buf.lock().clear();
    }

    fn process_queue(&mut self, queue: u16) {
        match queue {
            0 => {
                // RX queue - guest reading from us
                // Get pending input and provide to guest
                let _input = self.get_input(4096);
                // Would write to guest buffers via memory
            }
            1 => {
                // TX queue - guest writing to us
                // Read guest output and process
                // For now just trigger callback with placeholder
            }
            _ => {}
        }
    }

    fn is_activated(&self) -> bool {
        self.base.is_activated()
    }
}
