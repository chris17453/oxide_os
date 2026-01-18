//! Virtual Device Emulation

/// virtio device types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceType {
    /// Network device
    Net = 1,
    /// Block device
    Block = 2,
    /// Console device
    Console = 3,
    /// Entropy source
    Rng = 4,
    /// Memory balloon
    Balloon = 5,
    /// SCSI host
    Scsi = 8,
    /// 9P transport
    P9 = 9,
    /// GPU device
    Gpu = 16,
    /// Input device
    Input = 18,
    /// Socket device
    Vsock = 19,
    /// Filesystem device
    Fs = 26,
}

/// virtio device trait
pub trait VirtioDevice: Send + Sync {
    /// Get device type
    fn device_type(&self) -> u32;

    /// Get device features
    fn features(&self) -> u64 {
        0
    }

    /// Acknowledge features
    fn ack_features(&mut self, _features: u64) {}

    /// Read configuration space
    fn read_config(&self, offset: u64, data: &mut [u8]);

    /// Write configuration space
    fn write_config(&mut self, offset: u64, data: &[u8]);

    /// Activate device
    fn activate(&mut self) -> bool {
        true
    }

    /// Reset device
    fn reset(&mut self);

    /// Process virtqueue
    fn process_queue(&mut self, queue: u16);

    /// Check if device is activated
    fn is_activated(&self) -> bool {
        false
    }
}

/// virtio MMIO device state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VirtioMmioState {
    /// Device reset
    Reset,
    /// Device acknowledged
    Acknowledge,
    /// Driver loaded
    Driver,
    /// Features OK
    FeaturesOk,
    /// Driver OK (device ready)
    DriverOk,
    /// Device needs reset
    NeedsReset,
    /// Device failed
    Failed,
}

/// Common virtio MMIO registers
pub mod mmio {
    pub const MAGIC_VALUE: u64 = 0x000;
    pub const VERSION: u64 = 0x004;
    pub const DEVICE_ID: u64 = 0x008;
    pub const VENDOR_ID: u64 = 0x00C;
    pub const DEVICE_FEATURES: u64 = 0x010;
    pub const DEVICE_FEATURES_SEL: u64 = 0x014;
    pub const DRIVER_FEATURES: u64 = 0x020;
    pub const DRIVER_FEATURES_SEL: u64 = 0x024;
    pub const QUEUE_SEL: u64 = 0x030;
    pub const QUEUE_NUM_MAX: u64 = 0x034;
    pub const QUEUE_NUM: u64 = 0x038;
    pub const QUEUE_READY: u64 = 0x044;
    pub const QUEUE_NOTIFY: u64 = 0x050;
    pub const INTERRUPT_STATUS: u64 = 0x060;
    pub const INTERRUPT_ACK: u64 = 0x064;
    pub const STATUS: u64 = 0x070;
    pub const QUEUE_DESC_LOW: u64 = 0x080;
    pub const QUEUE_DESC_HIGH: u64 = 0x084;
    pub const QUEUE_DRIVER_LOW: u64 = 0x090;
    pub const QUEUE_DRIVER_HIGH: u64 = 0x094;
    pub const QUEUE_DEVICE_LOW: u64 = 0x0A0;
    pub const QUEUE_DEVICE_HIGH: u64 = 0x0A4;
    pub const CONFIG_GENERATION: u64 = 0x0FC;
    pub const CONFIG_SPACE: u64 = 0x100;
}

/// virtio status bits
pub mod status {
    pub const ACKNOWLEDGE: u8 = 1;
    pub const DRIVER: u8 = 2;
    pub const DRIVER_OK: u8 = 4;
    pub const FEATURES_OK: u8 = 8;
    pub const DEVICE_NEEDS_RESET: u8 = 64;
    pub const FAILED: u8 = 128;
}

/// Virtqueue
#[derive(Debug, Clone)]
pub struct Virtqueue {
    /// Queue index
    pub index: u16,
    /// Queue size
    pub size: u16,
    /// Descriptor table address
    pub desc_addr: u64,
    /// Available ring address
    pub avail_addr: u64,
    /// Used ring address
    pub used_addr: u64,
    /// Queue ready
    pub ready: bool,
}

impl Virtqueue {
    pub fn new(index: u16) -> Self {
        Virtqueue {
            index,
            size: 0,
            desc_addr: 0,
            avail_addr: 0,
            used_addr: 0,
            ready: false,
        }
    }
}

/// Virtqueue descriptor
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct VirtqDesc {
    /// Buffer address
    pub addr: u64,
    /// Buffer length
    pub len: u32,
    /// Flags
    pub flags: u16,
    /// Next descriptor (if VIRTQ_DESC_F_NEXT)
    pub next: u16,
}

/// Virtqueue descriptor flags
pub mod desc_flags {
    /// Buffer continues via next field
    pub const NEXT: u16 = 1;
    /// Buffer is device write-only (otherwise read-only)
    pub const WRITE: u16 = 2;
    /// Buffer contains indirect descriptor table
    pub const INDIRECT: u16 = 4;
}

/// Virtqueue available ring
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct VirtqAvail {
    /// Flags
    pub flags: u16,
    /// Index
    pub idx: u16,
    // ring[]: u16 array follows
}

/// Virtqueue used ring
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct VirtqUsed {
    /// Flags
    pub flags: u16,
    /// Index
    pub idx: u16,
    // ring[]: VirtqUsedElem array follows
}

/// Virtqueue used element
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct VirtqUsedElem {
    /// Descriptor chain head index
    pub id: u32,
    /// Total bytes written
    pub len: u32,
}
