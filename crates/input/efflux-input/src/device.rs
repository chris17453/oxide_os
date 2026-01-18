//! Input Device Trait

use alloc::string::String;

/// Input device type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputDeviceType {
    /// Keyboard
    Keyboard,
    /// Mouse
    Mouse,
    /// Touchpad
    Touchpad,
    /// Touchscreen
    Touchscreen,
    /// Joystick/Gamepad
    Joystick,
    /// Tablet/Stylus
    Tablet,
    /// Generic pointer
    Pointer,
    /// Unknown
    Unknown,
}

/// Input device information
#[derive(Debug, Clone)]
pub struct InputDeviceInfo {
    /// Device name
    pub name: String,
    /// Physical path
    pub phys: String,
    /// Unique identifier
    pub uniq: String,
    /// Device type
    pub device_type: InputDeviceType,
    /// Vendor ID
    pub vendor: u16,
    /// Product ID
    pub product: u16,
    /// Version
    pub version: u16,
}

impl Default for InputDeviceInfo {
    fn default() -> Self {
        InputDeviceInfo {
            name: String::new(),
            phys: String::new(),
            uniq: String::new(),
            device_type: InputDeviceType::Unknown,
            vendor: 0,
            product: 0,
            version: 0,
        }
    }
}

/// Input device trait
pub trait InputDevice: Send + Sync {
    /// Get device information
    fn info(&self) -> InputDeviceInfo;

    /// Poll for events
    fn poll(&self);

    /// Set LED state
    fn set_led(&self, _led: u16, _on: bool) -> bool {
        false
    }

    /// Set key repeat parameters
    fn set_repeat(&self, _delay: u32, _period: u32) -> bool {
        false
    }

    /// Get key repeat parameters
    fn get_repeat(&self) -> (u32, u32) {
        (250, 33) // Default: 250ms delay, 30Hz repeat
    }
}
