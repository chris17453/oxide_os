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
pub mod kbd;
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

/// PIDs of tasks blocked waiting for input on each device
/// 🔥 NOW SUPPORTS MULTIPLE READERS (Priority #14 Fix) 🔥
/// Before: Only one PID per device → second reader never wakes up
/// After: Vec<u32> per device → all readers wake on input
static BLOCKED_READERS: Mutex<[Vec<u32>; MAX_DEVICES]> =
    Mutex::new([const { Vec::new() }; MAX_DEVICES]);

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

/// Add a PID to the list of tasks blocked waiting for input on a device
pub fn set_blocked_reader(device_id: usize, pid: u32) {
    let mut readers = BLOCKED_READERS.lock();
    if device_id < MAX_DEVICES && !readers[device_id].contains(&pid) {
        readers[device_id].push(pid);
    }
}

/// Wake up all tasks blocked waiting for input on a device
///
/// — GraveShift: Called from keyboard/mouse ISR context via report_event().
/// Uses try_lock() on BLOCKED_READERS because the blocking .lock() would
/// deadlock if the interrupted code (e.g., set_blocked_reader in a syscall)
/// holds the same spin lock. If contended, skip — the next IRQ will retry.
fn wake_blocked_reader(device_id: usize) {
    let pids = {
        let mut readers = match BLOCKED_READERS.try_lock() {
            Some(guard) => guard,
            None => return, // Lock contended — retry on next input event
        };
        if device_id < MAX_DEVICES {
            let pids = readers[device_id].clone();
            readers[device_id].clear();
            pids
        } else {
            Vec::new()
        }
    }; // Release lock!

    // Wake all waiting readers (callback is ISR-safe try_wake_up)
    unsafe {
        if let Some(wake_fn) = WAKE_CALLBACK {
            for pid in pids {
                wake_fn(pid);
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

    /// Push an event to the queue (blocking — syscall context only)
    pub fn push_event(&self, event: InputEvent) {
        let mut queue = self.events.lock();
        if queue.len() >= MAX_EVENT_QUEUE {
            queue.pop_front();
        }
        queue.push_back(event);
    }

    /// Push an event to the queue (non-blocking — ISR-safe).
    /// — GraveShift: Returns false if the event queue lock is contended.
    /// Dropping a single input event is acceptable; deadlocking the CPU is not.
    pub fn try_push_event(&self, event: InputEvent) -> bool {
        if let Some(mut queue) = self.events.try_lock() {
            if queue.len() >= MAX_EVENT_QUEUE {
                queue.pop_front();
            }
            queue.push_back(event);
            true
        } else {
            false
        }
    }

    /// Pop an event from the queue
    pub fn pop_event(&self) -> Option<InputEvent> {
        self.events.lock().pop_front()
    }

    /// Try to pop an event without blocking (ISR-safe)
    pub fn try_pop_event(&self) -> Option<InputEvent> {
        self.events.try_lock()?.pop_front()
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

/// Get device by index (blocking — syscall context only)
pub fn get_device(index: usize) -> Option<Arc<InputDeviceHandle>> {
    DEVICES.lock().get(index).cloned()
}

/// Get device by index (non-blocking — ISR-safe).
/// — GraveShift: Returns None if the device registry lock is contended.
pub fn try_get_device(index: usize) -> Option<Arc<InputDeviceHandle>> {
    DEVICES.try_lock()?.get(index).cloned()
}

/// Get number of registered devices (ISR-safe).
/// — InputShade: Used by terminal_tick to iterate all devices for mouse events.
pub fn device_count() -> usize {
    DEVICES.try_lock().map_or(0, |d| d.len())
}

/// Get all devices
pub fn devices() -> Vec<Arc<InputDeviceHandle>> {
    DEVICES.lock().clone()
}

/// Report an input event.
/// — GraveShift: Called from keyboard/mouse ISR context. Every lock on this
/// path MUST be non-blocking (try_lock). If any lock is contended, drop the
/// event — the next keystroke will succeed. A dropped event beats a dead CPU.
pub fn report_event(device_id: usize, event: InputEvent) {
    if let Some(handle) = try_get_device(device_id) {
        handle.try_push_event(event);
        debug_input!(
            "[INPUT] dev{} type={} code={} val={}",
            device_id,
            event.type_,
            event.code,
            event.value
        );
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
