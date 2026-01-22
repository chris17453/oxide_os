//! VirtIO Input Device Driver
//!
//! Implements the VirtIO input device specification for keyboard, mouse,
//! and other input devices in virtualized environments.

#![no_std]

extern crate alloc;

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use core::ptr::{read_volatile, write_volatile};
use input::{EventType, InputDeviceInfo, InputDeviceType, InputEvent, KeyValue};
use spin::Mutex;

/// VirtIO input device configuration select values
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VirtioInputConfigSelect {
    /// Returns the name of the device
    IdName = 0x01,
    /// Returns a serial number
    IdSerial = 0x02,
    /// Device type identifier
    IdDevids = 0x03,
    /// Input property bits
    PropBits = 0x10,
    /// Event type bits
    EvBits = 0x11,
    /// Absolute axis info
    AbsInfo = 0x12,
}

/// VirtIO input device IDs
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct VirtioInputDevids {
    pub bustype: u16,
    pub vendor: u16,
    pub product: u16,
    pub version: u16,
}

/// VirtIO input absolute axis info
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct VirtioInputAbsinfo {
    pub min: u32,
    pub max: u32,
    pub fuzz: u32,
    pub flat: u32,
    pub res: u32,
}

/// VirtIO input configuration space
#[repr(C)]
pub struct VirtioInputConfig {
    pub select: u8,
    pub subsel: u8,
    pub size: u8,
    _reserved: [u8; 5],
    pub data: [u8; 128],
}

/// VirtIO input event structure
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct VirtioInputEvent {
    pub event_type: u16,
    pub code: u16,
    pub value: u32,
}

/// VirtIO input device type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VirtioInputType {
    /// Keyboard device
    Keyboard,
    /// Mouse device
    Mouse,
    /// Tablet (absolute pointing device)
    Tablet,
    /// Generic input device
    Generic,
}

/// VirtIO input device driver
pub struct VirtioInput {
    /// MMIO base address
    mmio_base: usize,
    /// Configuration space pointer
    config: *mut VirtioInputConfig,
    /// Event virtqueue
    event_queue: VirtQueue,
    /// Status virtqueue
    status_queue: VirtQueue,
    /// Device name
    name: String,
    /// Device type
    device_type: VirtioInputType,
    /// Device IDs
    devids: VirtioInputDevids,
    /// Input device handle
    device_id: Option<usize>,
}

/// Virtqueue structure for input events
struct VirtQueue {
    /// Descriptor table
    descriptors: *mut VirtqDesc,
    /// Available ring
    available: *mut VirtqAvail,
    /// Used ring
    used: *mut VirtqUsed,
    /// Queue size
    size: u16,
    /// Next descriptor index
    next_desc: u16,
    /// Last seen used index
    last_used: u16,
    /// Event buffers
    buffers: Vec<Box<VirtioInputEvent>>,
}

#[repr(C)]
struct VirtqDesc {
    addr: u64,
    len: u32,
    flags: u16,
    next: u16,
}

#[repr(C)]
struct VirtqAvail {
    flags: u16,
    idx: u16,
    ring: [u16; 256],
}

#[repr(C)]
struct VirtqUsed {
    flags: u16,
    idx: u16,
    ring: [VirtqUsedElem; 256],
}

#[repr(C)]
#[derive(Clone, Copy)]
struct VirtqUsedElem {
    id: u32,
    len: u32,
}

/// VirtIO MMIO register offsets
const VIRTIO_MMIO_MAGIC: usize = 0x000;
const VIRTIO_MMIO_VERSION: usize = 0x004;
const VIRTIO_MMIO_DEVICE_ID: usize = 0x008;
const VIRTIO_MMIO_VENDOR_ID: usize = 0x00c;
const VIRTIO_MMIO_DEVICE_FEATURES: usize = 0x010;
const VIRTIO_MMIO_DEVICE_FEATURES_SEL: usize = 0x014;
const VIRTIO_MMIO_DRIVER_FEATURES: usize = 0x020;
const VIRTIO_MMIO_DRIVER_FEATURES_SEL: usize = 0x024;
const VIRTIO_MMIO_QUEUE_SEL: usize = 0x030;
const VIRTIO_MMIO_QUEUE_NUM_MAX: usize = 0x034;
const VIRTIO_MMIO_QUEUE_NUM: usize = 0x038;
const VIRTIO_MMIO_QUEUE_READY: usize = 0x044;
const VIRTIO_MMIO_QUEUE_NOTIFY: usize = 0x050;
const VIRTIO_MMIO_INTERRUPT_STATUS: usize = 0x060;
const VIRTIO_MMIO_INTERRUPT_ACK: usize = 0x064;
const VIRTIO_MMIO_STATUS: usize = 0x070;
const VIRTIO_MMIO_QUEUE_DESC_LOW: usize = 0x080;
const VIRTIO_MMIO_QUEUE_DESC_HIGH: usize = 0x084;
const VIRTIO_MMIO_QUEUE_AVAIL_LOW: usize = 0x090;
const VIRTIO_MMIO_QUEUE_AVAIL_HIGH: usize = 0x094;
const VIRTIO_MMIO_QUEUE_USED_LOW: usize = 0x0a0;
const VIRTIO_MMIO_QUEUE_USED_HIGH: usize = 0x0a4;
const VIRTIO_MMIO_CONFIG: usize = 0x100;

/// VirtIO status bits
const VIRTIO_STATUS_ACKNOWLEDGE: u32 = 1;
const VIRTIO_STATUS_DRIVER: u32 = 2;
const VIRTIO_STATUS_FEATURES_OK: u32 = 8;
const VIRTIO_STATUS_DRIVER_OK: u32 = 4;
const VIRTIO_STATUS_FAILED: u32 = 128;

/// VirtIO descriptor flags
const VIRTQ_DESC_F_WRITE: u16 = 2;

/// Event queue index
const VIRTIO_INPUT_EVENT_QUEUE: u32 = 0;
/// Status queue index
const VIRTIO_INPUT_STATUS_QUEUE: u32 = 1;

impl VirtioInput {
    /// Probe for a VirtIO input device at the given MMIO address
    pub fn probe(mmio_base: usize) -> Option<Self> {
        let magic = unsafe { read_volatile((mmio_base + VIRTIO_MMIO_MAGIC) as *const u32) };
        if magic != 0x74726976 {
            return None;
        }

        let version = unsafe { read_volatile((mmio_base + VIRTIO_MMIO_VERSION) as *const u32) };
        if version != 2 {
            return None;
        }

        let device_id = unsafe { read_volatile((mmio_base + VIRTIO_MMIO_DEVICE_ID) as *const u32) };
        if device_id != 18 {
            // Not an input device
            return None;
        }

        Some(Self::new(mmio_base))
    }

    /// Create a new VirtIO input device driver
    fn new(mmio_base: usize) -> Self {
        Self {
            mmio_base,
            config: (mmio_base + VIRTIO_MMIO_CONFIG) as *mut VirtioInputConfig,
            event_queue: VirtQueue::empty(),
            status_queue: VirtQueue::empty(),
            name: String::new(),
            device_type: VirtioInputType::Generic,
            devids: VirtioInputDevids::default(),
            device_id: None,
        }
    }

    /// Initialize the VirtIO input device
    pub fn init(&mut self) -> Result<(), &'static str> {
        // Reset device
        self.write_reg(VIRTIO_MMIO_STATUS, 0);

        // Acknowledge device
        self.write_reg(VIRTIO_MMIO_STATUS, VIRTIO_STATUS_ACKNOWLEDGE);

        // Driver loaded
        self.write_reg(
            VIRTIO_MMIO_STATUS,
            VIRTIO_STATUS_ACKNOWLEDGE | VIRTIO_STATUS_DRIVER,
        );

        // Read device features
        self.write_reg(VIRTIO_MMIO_DEVICE_FEATURES_SEL, 0);
        let _features = self.read_reg(VIRTIO_MMIO_DEVICE_FEATURES);

        // Write driver features (accept all for now)
        self.write_reg(VIRTIO_MMIO_DRIVER_FEATURES_SEL, 0);
        self.write_reg(VIRTIO_MMIO_DRIVER_FEATURES, 0);

        // Features OK
        self.write_reg(
            VIRTIO_MMIO_STATUS,
            VIRTIO_STATUS_ACKNOWLEDGE | VIRTIO_STATUS_DRIVER | VIRTIO_STATUS_FEATURES_OK,
        );

        // Verify features accepted
        let status = self.read_reg(VIRTIO_MMIO_STATUS);
        if status & VIRTIO_STATUS_FEATURES_OK == 0 {
            self.write_reg(VIRTIO_MMIO_STATUS, VIRTIO_STATUS_FAILED);
            return Err("Features not accepted");
        }

        // Read device configuration
        self.read_device_config();

        // Initialize virtqueues
        self.init_event_queue()?;
        self.init_status_queue()?;

        // Driver ready
        self.write_reg(
            VIRTIO_MMIO_STATUS,
            VIRTIO_STATUS_ACKNOWLEDGE
                | VIRTIO_STATUS_DRIVER
                | VIRTIO_STATUS_FEATURES_OK
                | VIRTIO_STATUS_DRIVER_OK,
        );

        // Register with input subsystem
        let device_type = match self.device_type {
            VirtioInputType::Keyboard => InputDeviceType::Keyboard,
            VirtioInputType::Mouse => InputDeviceType::Mouse,
            VirtioInputType::Tablet => InputDeviceType::Tablet,
            VirtioInputType::Generic => InputDeviceType::Unknown,
        };

        let info = InputDeviceInfo {
            name: self.name.clone(),
            phys: String::from("virtio"),
            uniq: String::new(),
            device_type,
            vendor: self.devids.vendor,
            product: self.devids.product,
            version: self.devids.version,
        };

        self.device_id = Some(input::register_device_info(info));

        Ok(())
    }

    /// Read device configuration
    fn read_device_config(&mut self) {
        // Read device name
        unsafe {
            (*self.config).select = VirtioInputConfigSelect::IdName as u8;
            (*self.config).subsel = 0;

            core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);

            let size = read_volatile(&(*self.config).size) as usize;
            if size > 0 && size <= 128 {
                let mut name_bytes = [0u8; 128];
                for i in 0..size {
                    name_bytes[i] = read_volatile(&(*self.config).data[i]);
                }
                if let Ok(name) = core::str::from_utf8(&name_bytes[..size]) {
                    self.name = String::from(name.trim_end_matches('\0'));
                }
            }
        }

        // Read device IDs
        unsafe {
            (*self.config).select = VirtioInputConfigSelect::IdDevids as u8;
            (*self.config).subsel = 0;

            core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);

            let size = read_volatile(&(*self.config).size) as usize;
            if size >= core::mem::size_of::<VirtioInputDevids>() {
                self.devids.bustype =
                    read_volatile(&(*self.config).data[0] as *const u8 as *const u16);
                self.devids.vendor =
                    read_volatile(&(*self.config).data[2] as *const u8 as *const u16);
                self.devids.product =
                    read_volatile(&(*self.config).data[4] as *const u8 as *const u16);
                self.devids.version =
                    read_volatile(&(*self.config).data[6] as *const u8 as *const u16);
            }
        }

        // Determine device type from event bits
        unsafe {
            // Check for keyboard (EV_KEY with keyboard keys)
            (*self.config).select = VirtioInputConfigSelect::EvBits as u8;
            (*self.config).subsel = 1; // EV_KEY

            core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);

            let size = read_volatile(&(*self.config).size) as usize;
            if size > 0 {
                // Has key events, check if it has letter keys
                if size > 4 {
                    let byte4 = read_volatile(&(*self.config).data[4]);
                    if byte4 != 0 {
                        self.device_type = VirtioInputType::Keyboard;
                        return;
                    }
                }
            }

            // Check for mouse/tablet (EV_REL or EV_ABS)
            (*self.config).select = VirtioInputConfigSelect::EvBits as u8;
            (*self.config).subsel = 2; // EV_REL

            core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);

            let size = read_volatile(&(*self.config).size) as usize;
            if size > 0 {
                self.device_type = VirtioInputType::Mouse;
                return;
            }

            (*self.config).select = VirtioInputConfigSelect::EvBits as u8;
            (*self.config).subsel = 3; // EV_ABS

            core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);

            let size = read_volatile(&(*self.config).size) as usize;
            if size > 0 {
                self.device_type = VirtioInputType::Tablet;
            }
        }
    }

    /// Initialize the event virtqueue
    fn init_event_queue(&mut self) -> Result<(), &'static str> {
        self.write_reg(VIRTIO_MMIO_QUEUE_SEL, VIRTIO_INPUT_EVENT_QUEUE);

        let max_size = self.read_reg(VIRTIO_MMIO_QUEUE_NUM_MAX) as u16;
        if max_size == 0 {
            return Err("Event queue not available");
        }

        let size = max_size.min(64);
        self.write_reg(VIRTIO_MMIO_QUEUE_NUM, size as u32);

        self.event_queue = VirtQueue::new(size)?;

        // Set queue addresses
        let desc_addr = self.event_queue.descriptors as u64;
        let avail_addr = self.event_queue.available as u64;
        let used_addr = self.event_queue.used as u64;

        self.write_reg(VIRTIO_MMIO_QUEUE_DESC_LOW, desc_addr as u32);
        self.write_reg(VIRTIO_MMIO_QUEUE_DESC_HIGH, (desc_addr >> 32) as u32);
        self.write_reg(VIRTIO_MMIO_QUEUE_AVAIL_LOW, avail_addr as u32);
        self.write_reg(VIRTIO_MMIO_QUEUE_AVAIL_HIGH, (avail_addr >> 32) as u32);
        self.write_reg(VIRTIO_MMIO_QUEUE_USED_LOW, used_addr as u32);
        self.write_reg(VIRTIO_MMIO_QUEUE_USED_HIGH, (used_addr >> 32) as u32);

        // Enable queue
        self.write_reg(VIRTIO_MMIO_QUEUE_READY, 1);

        // Populate event queue with buffers
        for i in 0..size {
            let event = Box::new(VirtioInputEvent::default());
            let addr = &*event as *const VirtioInputEvent as u64;

            unsafe {
                let desc = &mut *self.event_queue.descriptors.add(i as usize);
                desc.addr = addr;
                desc.len = core::mem::size_of::<VirtioInputEvent>() as u32;
                desc.flags = VIRTQ_DESC_F_WRITE;
                desc.next = 0;

                let avail = &mut *self.event_queue.available;
                avail.ring[i as usize] = i;
            }

            self.event_queue.buffers.push(event);
        }

        // Update available index
        unsafe {
            let avail = &mut *self.event_queue.available;
            avail.idx = size;
        }

        self.event_queue.next_desc = size;

        // Notify device
        self.write_reg(VIRTIO_MMIO_QUEUE_NOTIFY, VIRTIO_INPUT_EVENT_QUEUE);

        Ok(())
    }

    /// Initialize the status virtqueue
    fn init_status_queue(&mut self) -> Result<(), &'static str> {
        self.write_reg(VIRTIO_MMIO_QUEUE_SEL, VIRTIO_INPUT_STATUS_QUEUE);

        let max_size = self.read_reg(VIRTIO_MMIO_QUEUE_NUM_MAX) as u16;
        if max_size == 0 {
            return Err("Status queue not available");
        }

        let size = max_size.min(16);
        self.write_reg(VIRTIO_MMIO_QUEUE_NUM, size as u32);

        self.status_queue = VirtQueue::new(size)?;

        // Set queue addresses
        let desc_addr = self.status_queue.descriptors as u64;
        let avail_addr = self.status_queue.available as u64;
        let used_addr = self.status_queue.used as u64;

        self.write_reg(VIRTIO_MMIO_QUEUE_DESC_LOW, desc_addr as u32);
        self.write_reg(VIRTIO_MMIO_QUEUE_DESC_HIGH, (desc_addr >> 32) as u32);
        self.write_reg(VIRTIO_MMIO_QUEUE_AVAIL_LOW, avail_addr as u32);
        self.write_reg(VIRTIO_MMIO_QUEUE_AVAIL_HIGH, (avail_addr >> 32) as u32);
        self.write_reg(VIRTIO_MMIO_QUEUE_USED_LOW, used_addr as u32);
        self.write_reg(VIRTIO_MMIO_QUEUE_USED_HIGH, (used_addr >> 32) as u32);

        // Enable queue
        self.write_reg(VIRTIO_MMIO_QUEUE_READY, 1);

        Ok(())
    }

    /// Handle interrupt from device
    pub fn handle_interrupt(&mut self) {
        let status = self.read_reg(VIRTIO_MMIO_INTERRUPT_STATUS);
        self.write_reg(VIRTIO_MMIO_INTERRUPT_ACK, status);

        if status & 1 != 0 {
            // Used buffer notification
            self.process_events();
        }
    }

    /// Process pending input events
    fn process_events(&mut self) {
        loop {
            let used_idx = unsafe { read_volatile(&(*self.event_queue.used).idx) };

            if self.event_queue.last_used == used_idx {
                break;
            }

            let idx = (self.event_queue.last_used % self.event_queue.size) as usize;
            let used_elem = unsafe { read_volatile(&(*self.event_queue.used).ring[idx]) };

            let desc_idx = used_elem.id as usize;
            if desc_idx < self.event_queue.buffers.len() {
                let event = &self.event_queue.buffers[desc_idx];
                self.dispatch_event(event);
            }

            // Re-add buffer to available ring
            let avail_idx = unsafe { read_volatile(&(*self.event_queue.available).idx) };
            unsafe {
                let avail = &mut *self.event_queue.available;
                avail.ring[(avail_idx % self.event_queue.size) as usize] = desc_idx as u16;
                core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
                write_volatile(&mut avail.idx, avail_idx.wrapping_add(1));
            }

            self.event_queue.last_used = self.event_queue.last_used.wrapping_add(1);
        }

        // Notify device of new available buffers
        self.write_reg(VIRTIO_MMIO_QUEUE_NOTIFY, VIRTIO_INPUT_EVENT_QUEUE);
    }

    /// Dispatch an input event to the input subsystem
    fn dispatch_event(&self, event: &VirtioInputEvent) {
        if let Some(device_id) = self.device_id {
            match event.event_type {
                0 => {
                    // EV_SYN
                    input::report_sync(device_id);
                }
                1 => {
                    // EV_KEY
                    let value = match event.value {
                        0 => KeyValue::Released,
                        1 => KeyValue::Pressed,
                        2 => KeyValue::Repeat,
                        _ => return,
                    };
                    input::report_key(device_id, event.code, value);
                }
                2 => {
                    // EV_REL
                    input::report_rel(device_id, event.code, event.value as i32);
                }
                3 => {
                    // EV_ABS
                    input::report_abs(device_id, event.code, event.value as i32);
                }
                _ => {}
            }
        }
    }

    /// Send LED status to device
    pub fn set_led(&mut self, led: u16, on: bool) {
        if self.status_queue.size == 0 {
            return;
        }

        let event = VirtioInputEvent {
            event_type: 0x11, // EV_LED
            code: led,
            value: if on { 1 } else { 0 },
        };

        // Find a free descriptor
        let avail_idx = unsafe { read_volatile(&(*self.status_queue.available).idx) };
        let desc_idx = (avail_idx % self.status_queue.size) as usize;

        // Set up descriptor
        unsafe {
            let desc = &mut *self.status_queue.descriptors.add(desc_idx);

            // Use stack-allocated event copied to a stable location
            let event_ptr = Box::into_raw(Box::new(event));
            desc.addr = event_ptr as u64;
            desc.len = core::mem::size_of::<VirtioInputEvent>() as u32;
            desc.flags = 0; // Device reads from us
            desc.next = 0;

            let avail = &mut *self.status_queue.available;
            avail.ring[desc_idx] = desc_idx as u16;
            core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
            write_volatile(&mut avail.idx, avail_idx.wrapping_add(1));
        }

        // Notify device
        self.write_reg(VIRTIO_MMIO_QUEUE_SEL, VIRTIO_INPUT_STATUS_QUEUE);
        self.write_reg(VIRTIO_MMIO_QUEUE_NOTIFY, VIRTIO_INPUT_STATUS_QUEUE);
    }

    /// Get device type
    pub fn device_type(&self) -> VirtioInputType {
        self.device_type
    }

    /// Get device name
    pub fn name(&self) -> &str {
        &self.name
    }

    fn read_reg(&self, offset: usize) -> u32 {
        unsafe { read_volatile((self.mmio_base + offset) as *const u32) }
    }

    fn write_reg(&self, offset: usize, value: u32) {
        unsafe { write_volatile((self.mmio_base + offset) as *mut u32, value) }
    }
}

impl VirtQueue {
    fn empty() -> Self {
        Self {
            descriptors: core::ptr::null_mut(),
            available: core::ptr::null_mut(),
            used: core::ptr::null_mut(),
            size: 0,
            next_desc: 0,
            last_used: 0,
            buffers: Vec::new(),
        }
    }

    fn new(size: u16) -> Result<Self, &'static str> {
        use alloc::alloc::{Layout, alloc_zeroed};

        // Allocate descriptor table
        let desc_size = size as usize * core::mem::size_of::<VirtqDesc>();
        let desc_layout =
            Layout::from_size_align(desc_size, 16).map_err(|_| "Invalid descriptor layout")?;
        let descriptors = unsafe { alloc_zeroed(desc_layout) } as *mut VirtqDesc;
        if descriptors.is_null() {
            return Err("Failed to allocate descriptors");
        }

        // Allocate available ring
        let avail_size = core::mem::size_of::<VirtqAvail>();
        let avail_layout =
            Layout::from_size_align(avail_size, 2).map_err(|_| "Invalid available ring layout")?;
        let available = unsafe { alloc_zeroed(avail_layout) } as *mut VirtqAvail;
        if available.is_null() {
            return Err("Failed to allocate available ring");
        }

        // Allocate used ring
        let used_size = core::mem::size_of::<VirtqUsed>();
        let used_layout =
            Layout::from_size_align(used_size, 4).map_err(|_| "Invalid used ring layout")?;
        let used = unsafe { alloc_zeroed(used_layout) } as *mut VirtqUsed;
        if used.is_null() {
            return Err("Failed to allocate used ring");
        }

        Ok(Self {
            descriptors,
            available,
            used,
            size,
            next_desc: 0,
            last_used: 0,
            buffers: Vec::with_capacity(size as usize),
        })
    }
}

unsafe impl Send for VirtioInput {}
unsafe impl Sync for VirtioInput {}

/// Global VirtIO input devices
static VIRTIO_INPUT_DEVICES: Mutex<Vec<VirtioInput>> = Mutex::new(Vec::new());

/// Initialize VirtIO input devices from device tree
pub fn init_from_mmio(base_addresses: &[usize]) {
    let mut devices = VIRTIO_INPUT_DEVICES.lock();

    for &base in base_addresses {
        if let Some(mut device) = VirtioInput::probe(base) {
            if device.init().is_ok() {
                devices.push(device);
            }
        }
    }
}

/// Handle interrupt for all VirtIO input devices
pub fn handle_interrupt() {
    let mut devices = VIRTIO_INPUT_DEVICES.lock();
    for device in devices.iter_mut() {
        device.handle_interrupt();
    }
}

/// Get the number of initialized VirtIO input devices
pub fn device_count() -> usize {
    VIRTIO_INPUT_DEVICES.lock().len()
}
