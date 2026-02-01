//! Input Subsystem for OXIDE OS
//!
//! Provides the input event abstraction and device management.

#![no_std]

extern crate alloc;

/// Debug print for input events
/// Enable with: cargo build --features debug-input
/// Note: Currently a no-op in input crate - kernel handles debug output
#[macro_export]
macro_rules! debug_input {
    ($($arg:tt)*) => {
        #[cfg(feature = "debug-input")]
        {
            // Input crate doesn't output directly - kernel logs these events
            // This macro exists so code compiles with debug-input feature
        }
    };
}

pub mod device;
pub mod event;
pub mod keycodes;
pub mod keymap;
pub mod layouts;

pub use device::{InputDevice, InputDeviceInfo, InputDeviceType};
pub use event::{EventType, InputEvent, KeyValue, SynCode};
pub use keycodes::*;
pub use keymap::Keymap;
pub use layouts::{KeyboardLayout, LAYOUTS, default_layout, get_layout};

use alloc::collections::VecDeque;
use alloc::sync::Arc;
use alloc::vec::Vec;
use spin::Mutex;

/// Maximum events in queue per device
const MAX_EVENT_QUEUE: usize = 256;

/// Maximum number of input devices we track blocked readers for
const MAX_DEVICES: usize = 16;

/// PIDs of tasks blocked waiting for input on each device (0 = none)
static BLOCKED_READERS: Mutex<[u32; MAX_DEVICES]> = Mutex::new([0; MAX_DEVICES]);

/// Callback type for waking a blocked task by PID
pub type WakeUpFn = fn(u32);

/// Global wake callback (set by kernel during init)
static mut WAKE_CALLBACK: Option<WakeUpFn> = None;

/// Set the wake-up callback for blocked readers
///
/// # Safety
/// Must be called during single-threaded initialization
pub unsafe fn set_wake_callback(f: WakeUpFn) {
    unsafe {
        WAKE_CALLBACK = Some(f);
    }
}

/// Set the PID of a task blocked waiting for input on a device
pub fn set_blocked_reader(device_id: usize, pid: u32) {
    let mut readers = BLOCKED_READERS.lock();
    if device_id < MAX_DEVICES {
        readers[device_id] = pid;
    }
}

/// Wake up any task blocked waiting for input on a device
fn wake_blocked_reader(device_id: usize) {
    let mut readers = BLOCKED_READERS.lock();
    if device_id < MAX_DEVICES {
        let pid = readers[device_id];
        if pid != 0 {
            readers[device_id] = 0;
            drop(readers); // Release lock before callback
            unsafe {
                if let Some(wake_fn) = WAKE_CALLBACK {
                    wake_fn(pid);
                }
            }
        }
    }
}

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

    /// Clear all queued events
    pub fn clear_events(&self) {
        self.events.lock().clear();
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
        debug_input!("[INPUT] dev{} type={} code={} val={}", device_id, event.type_, event.code, event.value);
        // Wake up any task blocked reading from this device
        wake_blocked_reader(device_id);
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
