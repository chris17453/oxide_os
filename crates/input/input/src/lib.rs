//! Input Subsystem for OXIDE OS
//!
//! Provides the input event abstraction and device management.

#![no_std]

extern crate alloc;

pub mod event;
pub mod device;
pub mod keymap;
pub mod keycodes;
pub mod layouts;

pub use event::{InputEvent, EventType, SynCode, KeyValue};
pub use device::{InputDevice, InputDeviceInfo, InputDeviceType};
pub use keymap::Keymap;
pub use keycodes::*;
pub use layouts::{KeyboardLayout, LAYOUTS, get_layout, default_layout};

use alloc::collections::VecDeque;
use alloc::sync::Arc;
use alloc::vec::Vec;
use spin::Mutex;

/// Maximum events in queue per device
const MAX_EVENT_QUEUE: usize = 256;

/// Input device handle
pub struct InputDeviceHandle {
    /// Device information
    pub info: InputDeviceInfo,
    /// Event queue
    events: Mutex<VecDeque<InputEvent>>,
    /// Device reference
    device: Arc<dyn InputDevice>,
}

impl InputDeviceHandle {
    /// Create a new input device handle
    pub fn new(device: Arc<dyn InputDevice>) -> Self {
        InputDeviceHandle {
            info: device.info(),
            events: Mutex::new(VecDeque::with_capacity(MAX_EVENT_QUEUE)),
            device,
        }
    }

    /// Push an event to the queue
    pub fn push_event(&self, event: InputEvent) {
        let mut queue = self.events.lock();
        if queue.len() >= MAX_EVENT_QUEUE {
            queue.pop_front();
        }
        queue.push_back(event);
    }

    /// Pop an event from the queue
    pub fn pop_event(&self) -> Option<InputEvent> {
        self.events.lock().pop_front()
    }

    /// Check if events are available
    pub fn has_events(&self) -> bool {
        !self.events.lock().is_empty()
    }

    /// Get device reference
    pub fn device(&self) -> &Arc<dyn InputDevice> {
        &self.device
    }
}

/// Global input device registry
static DEVICES: Mutex<Vec<Arc<InputDeviceHandle>>> = Mutex::new(Vec::new());

/// Register an input device with trait implementation
pub fn register_device(device: Arc<dyn InputDevice>) -> usize {
    let handle = Arc::new(InputDeviceHandle::new(device));
    let mut devices = DEVICES.lock();
    let id = devices.len();
    devices.push(handle);
    id
}

/// Simple device wrapper for device info only registration
struct SimpleDevice {
    info: InputDeviceInfo,
}

impl InputDevice for SimpleDevice {
    fn info(&self) -> InputDeviceInfo {
        self.info.clone()
    }
    fn poll(&self) {}
}

/// Register a device by info only (for hardware drivers that push events directly)
pub fn register_device_info(info: InputDeviceInfo) -> usize {
    let device = Arc::new(SimpleDevice { info });
    register_device(device)
}

/// Get device by index
pub fn get_device(index: usize) -> Option<Arc<InputDeviceHandle>> {
    DEVICES.lock().get(index).cloned()
}

/// Get all devices
pub fn devices() -> Vec<Arc<InputDeviceHandle>> {
    DEVICES.lock().clone()
}

/// Report an input event
pub fn report_event(device_id: usize, event: InputEvent) {
    if let Some(handle) = get_device(device_id) {
        handle.push_event(event);
    }
}

/// Report key event helper
pub fn report_key(device_id: usize, code: u16, value: KeyValue) {
    let event = InputEvent::key(code, value);
    report_event(device_id, event);
}

/// Report relative movement helper
pub fn report_rel(device_id: usize, code: u16, value: i32) {
    let event = InputEvent::rel(code, value);
    report_event(device_id, event);
}

/// Report absolute movement helper
pub fn report_abs(device_id: usize, code: u16, value: i32) {
    let event = InputEvent::abs(code, value);
    report_event(device_id, event);
}

/// Report sync event (end of event batch)
pub fn report_sync(device_id: usize) {
    let event = InputEvent::sync();
    report_event(device_id, event);
}
