//! RDP Input Handling
//!
//! Translates RDP input events (keyboard scancodes, mouse events)
//! into OXIDE input subsystem events.

#![no_std]

extern crate alloc;

mod scancode;

pub use scancode::{rdp_scancode_to_evdev, ScancodeTranslator};

use alloc::string::String;
use input::{
    keycodes::{BTN_EXTRA, BTN_LEFT, BTN_MIDDLE, BTN_RIGHT, BTN_SIDE, REL_HWHEEL, REL_WHEEL, ABS_X, ABS_Y},
    report_abs, report_key, report_rel, report_sync, register_device_info,
    InputDeviceInfo, InputDeviceType, KeyValue,
};
use rdp_proto::fast_path::FastPathInputEvent;
use rdp_traits::{
    ExtendedMouseFlags, InputInjector, KeyboardFlags, MouseFlags, RdpResult,
};
use spin::Mutex;

/// RDP Input Handler
///
/// Registers virtual input devices and injects events from RDP clients.
pub struct RdpInputHandler {
    /// Virtual keyboard device ID
    keyboard_device: usize,
    /// Virtual mouse device ID
    mouse_device: usize,
    /// Scancode translator
    translator: ScancodeTranslator,
    /// Screen dimensions for absolute coordinate conversion
    screen_width: u32,
    screen_height: u32,
}

impl RdpInputHandler {
    /// Create a new RDP input handler
    pub fn new(screen_width: u32, screen_height: u32) -> Self {
        // Register virtual keyboard
        let keyboard_device = register_device_info(InputDeviceInfo {
            name: String::from("RDP Virtual Keyboard"),
            phys: String::from("rdp/keyboard"),
            uniq: String::from("rdp-kbd-0"),
            device_type: InputDeviceType::Keyboard,
            vendor: 0x0001,
            product: 0x0001,
            version: 0x0100,
        });

        // Register virtual mouse
        let mouse_device = register_device_info(InputDeviceInfo {
            name: String::from("RDP Virtual Mouse"),
            phys: String::from("rdp/mouse"),
            uniq: String::from("rdp-mouse-0"),
            device_type: InputDeviceType::Mouse,
            vendor: 0x0001,
            product: 0x0002,
            version: 0x0100,
        });

        Self {
            keyboard_device,
            mouse_device,
            translator: ScancodeTranslator::new(),
            screen_width,
            screen_height,
        }
    }

    /// Update screen dimensions
    pub fn set_screen_dimensions(&mut self, width: u32, height: u32) {
        self.screen_width = width;
        self.screen_height = height;
    }

    /// Process a fast-path input event
    pub fn process_fast_path_event(&self, event: &FastPathInputEvent) -> RdpResult<()> {
        match event {
            FastPathInputEvent::Keyboard { flags, scancode } => {
                let rdp_flags = if flags.0 & 0x01 != 0 {
                    KeyboardFlags::RELEASE
                } else {
                    0
                } | if flags.0 & 0x02 != 0 {
                    KeyboardFlags::EXTENDED
                } else {
                    0
                } | if flags.0 & 0x04 != 0 {
                    KeyboardFlags::EXTENDED1
                } else {
                    0
                };
                self.inject_keyboard(*scancode as u16, KeyboardFlags::new(rdp_flags))
            }
            FastPathInputEvent::Mouse { flags, x, y } => {
                self.inject_mouse(*x, *y, *flags)
            }
            FastPathInputEvent::ExtendedMouse { flags, x, y } => {
                // Extract wheel delta from lower 9 bits
                let delta = (flags.0 & 0x00FF) as i16;
                let delta = if flags.0 & ExtendedMouseFlags::WHEEL_NEGATIVE != 0 {
                    -delta
                } else {
                    delta
                };
                self.inject_mouse_extended(*x, *y, *flags, delta)
            }
            FastPathInputEvent::Synchronize { flags: _ } => {
                // Sync event - update keyboard LEDs if needed
                Ok(())
            }
            FastPathInputEvent::Unicode { code_point, is_release } => {
                self.inject_unicode(*code_point, *is_release)
            }
        }
    }
}

impl InputInjector for RdpInputHandler {
    fn inject_keyboard(&self, scancode: u16, flags: KeyboardFlags) -> RdpResult<()> {
        // Translate RDP scancode to evdev keycode
        let evdev_code = self.translator.translate(scancode, flags.is_extended(), flags.is_extended1());

        if evdev_code == 0 {
            // Unknown scancode - ignore
            return Ok(());
        }

        // Determine key state
        let value = if flags.is_release() {
            KeyValue::Released
        } else {
            KeyValue::Pressed
        };

        // Report the key event
        report_key(self.keyboard_device, evdev_code, value);
        report_sync(self.keyboard_device);

        Ok(())
    }

    fn inject_mouse(&self, x: u16, y: u16, flags: MouseFlags) -> RdpResult<()> {
        // RDP uses absolute coordinates
        if flags.is_move() {
            report_abs(self.mouse_device, ABS_X, x as i32);
            report_abs(self.mouse_device, ABS_Y, y as i32);
        }

        // Handle button events
        if flags.is_left_down() {
            report_key(self.mouse_device, BTN_LEFT, KeyValue::Pressed);
        }
        if flags.is_left_up() {
            report_key(self.mouse_device, BTN_LEFT, KeyValue::Released);
        }
        if flags.is_right_down() {
            report_key(self.mouse_device, BTN_RIGHT, KeyValue::Pressed);
        }
        if flags.is_right_up() {
            report_key(self.mouse_device, BTN_RIGHT, KeyValue::Released);
        }
        if flags.is_middle_down() {
            report_key(self.mouse_device, BTN_MIDDLE, KeyValue::Pressed);
        }
        if flags.is_middle_up() {
            report_key(self.mouse_device, BTN_MIDDLE, KeyValue::Released);
        }

        report_sync(self.mouse_device);

        Ok(())
    }

    fn inject_mouse_extended(&self, x: u16, y: u16, flags: ExtendedMouseFlags, delta: i16) -> RdpResult<()> {
        // Position update (same as regular mouse)
        report_abs(self.mouse_device, ABS_X, x as i32);
        report_abs(self.mouse_device, ABS_Y, y as i32);

        // Wheel events
        if flags.is_wheel() {
            // Vertical wheel
            report_rel(self.mouse_device, REL_WHEEL, delta as i32);
        }
        if flags.is_hwheel() {
            // Horizontal wheel
            report_rel(self.mouse_device, REL_HWHEEL, delta as i32);
        }

        // Extra button events (X1, X2)
        if flags.0 & ExtendedMouseFlags::XBUTTON1_DOWN != 0 {
            report_key(self.mouse_device, BTN_SIDE, KeyValue::Pressed);
        }
        if flags.0 & ExtendedMouseFlags::XBUTTON1_UP != 0 {
            report_key(self.mouse_device, BTN_SIDE, KeyValue::Released);
        }
        if flags.0 & ExtendedMouseFlags::XBUTTON2_DOWN != 0 {
            report_key(self.mouse_device, BTN_EXTRA, KeyValue::Pressed);
        }
        if flags.0 & ExtendedMouseFlags::XBUTTON2_UP != 0 {
            report_key(self.mouse_device, BTN_EXTRA, KeyValue::Released);
        }

        report_sync(self.mouse_device);

        Ok(())
    }

    fn inject_unicode(&self, code_point: u16, is_release: bool) -> RdpResult<()> {
        // Unicode input requires a different approach - typically we'd use
        // a virtual keyboard that supports Unicode input.
        // For now, try to map common characters to keycodes.

        // Map ASCII characters to keycodes
        let keycode = match code_point as u8 as char {
            'a'..='z' => {
                let idx = (code_point as u8 - b'a') as u16;
                // KEY_A = 30, letters are mostly contiguous
                match idx {
                    0..=5 => 30 + idx, // a-f
                    6 => 34,           // g
                    7 => 35,           // h
                    8 => 23,           // i
                    9 => 36,           // j
                    10 => 37,          // k
                    11 => 38,          // l
                    12 => 50,          // m
                    13 => 49,          // n
                    14 => 24,          // o
                    15 => 25,          // p
                    16 => 16,          // q
                    17 => 19,          // r
                    18 => 31,          // s
                    19 => 20,          // t
                    20 => 22,          // u
                    21 => 47,          // v
                    22 => 17,          // w
                    23 => 45,          // x
                    24 => 21,          // y
                    25 => 44,          // z
                    _ => return Ok(()),
                }
            }
            '0'..='9' => {
                if code_point == b'0' as u16 {
                    11 // KEY_0
                } else {
                    (code_point as u8 - b'0') as u16 + 1 // KEY_1 to KEY_9
                }
            }
            ' ' => 57,  // KEY_SPACE
            '\n' => 28, // KEY_ENTER
            '\t' => 15, // KEY_TAB
            _ => return Ok(()),
        };

        let value = if is_release {
            KeyValue::Released
        } else {
            KeyValue::Pressed
        };

        report_key(self.keyboard_device, keycode, value);
        report_sync(self.keyboard_device);

        Ok(())
    }
}

/// Global shared input handler
static INPUT_HANDLER: Mutex<Option<RdpInputHandler>> = Mutex::new(None);

/// Initialize the global input handler
pub fn init_input_handler(screen_width: u32, screen_height: u32) {
    let mut handler = INPUT_HANDLER.lock();
    *handler = Some(RdpInputHandler::new(screen_width, screen_height));
}

/// Get a reference to the input handler
pub fn input_handler() -> Option<&'static Mutex<Option<RdpInputHandler>>> {
    Some(&INPUT_HANDLER)
}
