//! USB HID (Human Interface Device) Class Driver
//!
//! Supports USB keyboards, mice, and other HID devices.

#![no_std]

extern crate alloc;

use alloc::sync::Arc;
use alloc::vec::Vec;
use alloc::vec;
use alloc::string::String;
use spin::Mutex;
use usb::{UsbDevice, UsbResult, UsbError, UsbClassDriver, TransferDirection};
use usb::descriptor::TransferType;
use input::{
    InputEvent, EventType, report_event, register_device_info,
    InputDeviceInfo, InputDeviceType,
    KEY_A, KEY_B, KEY_C, KEY_D, KEY_E, KEY_F, KEY_G, KEY_H, KEY_I, KEY_J,
    KEY_K, KEY_L, KEY_M, KEY_N, KEY_O, KEY_P, KEY_Q, KEY_R, KEY_S, KEY_T,
    KEY_U, KEY_V, KEY_W, KEY_X, KEY_Y, KEY_Z,
    KEY_1, KEY_2, KEY_3, KEY_4, KEY_5, KEY_6, KEY_7, KEY_8, KEY_9, KEY_0,
    KEY_ENTER, KEY_ESC, KEY_BACKSPACE, KEY_TAB, KEY_SPACE,
    KEY_MINUS, KEY_EQUAL, KEY_LEFTBRACE, KEY_RIGHTBRACE, KEY_BACKSLASH,
    KEY_SEMICOLON, KEY_APOSTROPHE, KEY_GRAVE, KEY_COMMA, KEY_DOT, KEY_SLASH,
    KEY_CAPSLOCK, KEY_F1, KEY_F2, KEY_F3, KEY_F4, KEY_F5, KEY_F6,
    KEY_F7, KEY_F8, KEY_F9, KEY_F10, KEY_F11, KEY_F12,
    KEY_SYSRQ, KEY_SCROLLLOCK, KEY_PAUSE, KEY_INSERT, KEY_HOME,
    KEY_PAGEUP, KEY_DELETE, KEY_END, KEY_PAGEDOWN,
    KEY_RIGHT, KEY_LEFT, KEY_DOWN, KEY_UP,
    KEY_NUMLOCK, KEY_KPSLASH, KEY_KPASTERISK, KEY_KPMINUS, KEY_KPPLUS,
    KEY_KPENTER, KEY_KP1, KEY_KP2, KEY_KP3, KEY_KP4, KEY_KP5,
    KEY_KP6, KEY_KP7, KEY_KP8, KEY_KP9, KEY_KP0, KEY_KPDOT,
    KEY_LEFTCTRL, KEY_LEFTSHIFT, KEY_LEFTALT, KEY_LEFTMETA,
    KEY_RIGHTCTRL, KEY_RIGHTSHIFT, KEY_RIGHTALT, KEY_RIGHTMETA,
    BTN_LEFT, BTN_RIGHT, BTN_MIDDLE,
    REL_X, REL_Y,
};

/// HID class code
pub const USB_CLASS_HID: u8 = 0x03;

/// HID subclass codes
pub mod subclass {
    /// No subclass
    pub const NONE: u8 = 0x00;
    /// Boot interface subclass
    pub const BOOT: u8 = 0x01;
}

/// HID protocol codes
pub mod protocol {
    /// None
    pub const NONE: u8 = 0x00;
    /// Keyboard
    pub const KEYBOARD: u8 = 0x01;
    /// Mouse
    pub const MOUSE: u8 = 0x02;
}

/// HID class requests
pub mod request {
    pub const GET_REPORT: u8 = 0x01;
    pub const GET_IDLE: u8 = 0x02;
    pub const GET_PROTOCOL: u8 = 0x03;
    pub const SET_REPORT: u8 = 0x09;
    pub const SET_IDLE: u8 = 0x0A;
    pub const SET_PROTOCOL: u8 = 0x0B;
}

/// HID report types
pub mod report_type {
    pub const INPUT: u8 = 0x01;
    pub const OUTPUT: u8 = 0x02;
    pub const FEATURE: u8 = 0x03;
}

/// HID descriptor type
pub const HID_DESCRIPTOR_TYPE: u8 = 0x21;
/// Report descriptor type
pub const REPORT_DESCRIPTOR_TYPE: u8 = 0x22;

/// HID descriptor
#[derive(Debug, Clone)]
pub struct HidDescriptor {
    /// Descriptor length
    pub length: u8,
    /// Descriptor type (0x21)
    pub descriptor_type: u8,
    /// HID spec release number (BCD)
    pub hid_version: u16,
    /// Country code
    pub country_code: u8,
    /// Number of descriptors
    pub num_descriptors: u8,
    /// Report descriptor type
    pub report_descriptor_type: u8,
    /// Report descriptor length
    pub report_descriptor_length: u16,
}

impl HidDescriptor {
    /// Parse from bytes
    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        if data.len() < 9 {
            return None;
        }

        Some(HidDescriptor {
            length: data[0],
            descriptor_type: data[1],
            hid_version: u16::from_le_bytes([data[2], data[3]]),
            country_code: data[4],
            num_descriptors: data[5],
            report_descriptor_type: data[6],
            report_descriptor_length: u16::from_le_bytes([data[7], data[8]]),
        })
    }
}

/// Boot keyboard report (8 bytes)
#[repr(C, packed)]
#[derive(Debug, Clone, Copy, Default)]
pub struct BootKeyboardReport {
    /// Modifier keys
    pub modifiers: u8,
    /// Reserved byte
    pub reserved: u8,
    /// Key codes (up to 6 simultaneous keys)
    pub keys: [u8; 6],
}

impl BootKeyboardReport {
    /// Parse from bytes
    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        if data.len() < 8 {
            return None;
        }

        let mut keys = [0u8; 6];
        keys.copy_from_slice(&data[2..8]);

        Some(BootKeyboardReport {
            modifiers: data[0],
            reserved: data[1],
            keys,
        })
    }

    /// Check if left control is pressed
    pub fn left_ctrl(&self) -> bool {
        self.modifiers & 0x01 != 0
    }

    /// Check if left shift is pressed
    pub fn left_shift(&self) -> bool {
        self.modifiers & 0x02 != 0
    }

    /// Check if left alt is pressed
    pub fn left_alt(&self) -> bool {
        self.modifiers & 0x04 != 0
    }

    /// Check if left GUI (Win/Super) is pressed
    pub fn left_gui(&self) -> bool {
        self.modifiers & 0x08 != 0
    }

    /// Check if right control is pressed
    pub fn right_ctrl(&self) -> bool {
        self.modifiers & 0x10 != 0
    }

    /// Check if right shift is pressed
    pub fn right_shift(&self) -> bool {
        self.modifiers & 0x20 != 0
    }

    /// Check if right alt is pressed
    pub fn right_alt(&self) -> bool {
        self.modifiers & 0x40 != 0
    }

    /// Check if right GUI is pressed
    pub fn right_gui(&self) -> bool {
        self.modifiers & 0x80 != 0
    }
}

/// Boot mouse report (3+ bytes)
#[repr(C, packed)]
#[derive(Debug, Clone, Copy, Default)]
pub struct BootMouseReport {
    /// Button states
    pub buttons: u8,
    /// X movement (signed)
    pub x: i8,
    /// Y movement (signed)
    pub y: i8,
}

impl BootMouseReport {
    /// Parse from bytes
    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        if data.len() < 3 {
            return None;
        }

        Some(BootMouseReport {
            buttons: data[0],
            x: data[1] as i8,
            y: data[2] as i8,
        })
    }

    /// Check if left button is pressed
    pub fn left_button(&self) -> bool {
        self.buttons & 0x01 != 0
    }

    /// Check if right button is pressed
    pub fn right_button(&self) -> bool {
        self.buttons & 0x02 != 0
    }

    /// Check if middle button is pressed
    pub fn middle_button(&self) -> bool {
        self.buttons & 0x04 != 0
    }
}

/// USB HID keyboard state
pub struct HidKeyboardState {
    /// Previous key states
    prev_keys: [u8; 6],
    /// Previous modifiers
    prev_modifiers: u8,
}

impl HidKeyboardState {
    /// Create new keyboard state
    pub fn new() -> Self {
        HidKeyboardState {
            prev_keys: [0; 6],
            prev_modifiers: 0,
        }
    }

    /// Process keyboard report and generate input events
    pub fn process_report(&mut self, report: &BootKeyboardReport, device_id: usize) {
        // Check modifier changes
        self.process_modifier_change(0x01, report.modifiers, KEY_LEFTCTRL, device_id);
        self.process_modifier_change(0x02, report.modifiers, KEY_LEFTSHIFT, device_id);
        self.process_modifier_change(0x04, report.modifiers, KEY_LEFTALT, device_id);
        self.process_modifier_change(0x08, report.modifiers, KEY_LEFTMETA, device_id);
        self.process_modifier_change(0x10, report.modifiers, KEY_RIGHTCTRL, device_id);
        self.process_modifier_change(0x20, report.modifiers, KEY_RIGHTSHIFT, device_id);
        self.process_modifier_change(0x40, report.modifiers, KEY_RIGHTALT, device_id);
        self.process_modifier_change(0x80, report.modifiers, KEY_RIGHTMETA, device_id);

        // Check for released keys
        for &prev_key in &self.prev_keys {
            if prev_key != 0 && !report.keys.contains(&prev_key) {
                if let Some(keycode) = hid_to_keycode(prev_key) {
                    let event = InputEvent::new(EventType::Key, keycode, 0);
                    report_event(device_id, event);
                }
            }
        }

        // Check for pressed keys
        for &key in &report.keys {
            if key != 0 && !self.prev_keys.contains(&key) {
                if let Some(keycode) = hid_to_keycode(key) {
                    let event = InputEvent::new(EventType::Key, keycode, 1);
                    report_event(device_id, event);
                }
            }
        }

        // Update state
        self.prev_keys = report.keys;
        self.prev_modifiers = report.modifiers;
    }

    fn process_modifier_change(&mut self, mask: u8, modifiers: u8, keycode: u16, device_id: usize) {
        let prev_pressed = self.prev_modifiers & mask != 0;
        let curr_pressed = modifiers & mask != 0;

        if prev_pressed != curr_pressed {
            let value = if curr_pressed { 1 } else { 0 };
            let event = InputEvent::new(EventType::Key, keycode, value);
            report_event(device_id, event);
        }
    }
}

/// USB HID mouse state
pub struct HidMouseState {
    /// Previous button state
    prev_buttons: u8,
}

impl HidMouseState {
    /// Create new mouse state
    pub fn new() -> Self {
        HidMouseState { prev_buttons: 0 }
    }

    /// Process mouse report and generate input events
    pub fn process_report(&mut self, report: &BootMouseReport, device_id: usize) {
        // Report movement
        if report.x != 0 {
            let event = InputEvent::new(EventType::Rel, REL_X, report.x as i32);
            report_event(device_id, event);
        }

        if report.y != 0 {
            let event = InputEvent::new(EventType::Rel, REL_Y, report.y as i32);
            report_event(device_id, event);
        }

        // Report button changes
        self.process_button_change(0x01, report.buttons, BTN_LEFT, device_id);
        self.process_button_change(0x02, report.buttons, BTN_RIGHT, device_id);
        self.process_button_change(0x04, report.buttons, BTN_MIDDLE, device_id);

        self.prev_buttons = report.buttons;
    }

    fn process_button_change(&self, mask: u8, buttons: u8, code: u16, device_id: usize) {
        let prev_pressed = self.prev_buttons & mask != 0;
        let curr_pressed = buttons & mask != 0;

        if prev_pressed != curr_pressed {
            let value = if curr_pressed { 1 } else { 0 };
            let event = InputEvent::new(EventType::Key, code, value);
            report_event(device_id, event);
        }
    }
}

/// USB HID device
pub struct UsbHid {
    /// USB device
    device: Arc<UsbDevice>,
    /// Interrupt IN endpoint
    interrupt_in: u8,
    /// Interface number
    interface: u8,
    /// Protocol (keyboard/mouse)
    hid_protocol: u8,
    /// Input device ID
    device_id: usize,
    /// Keyboard state (if keyboard)
    keyboard_state: Option<Mutex<HidKeyboardState>>,
    /// Mouse state (if mouse)
    mouse_state: Option<Mutex<HidMouseState>>,
}

impl UsbHid {
    /// Create a new HID device
    pub fn new(device: Arc<UsbDevice>) -> UsbResult<Self> {
        let config = device.configuration().ok_or(UsbError::NotConfigured)?;

        let mut interrupt_in = None;
        let mut interface_num = 0u8;
        let mut hid_protocol = protocol::NONE;

        for interface in &config.interfaces {
            if interface.interface_class == USB_CLASS_HID {
                interface_num = interface.interface_number;
                hid_protocol = interface.interface_protocol;

                for endpoint in &interface.endpoints {
                    if endpoint.transfer_type() == TransferType::Interrupt
                        && endpoint.is_in()
                    {
                        interrupt_in = Some(endpoint.endpoint_address);
                        break;
                    }
                }
                break;
            }
        }

        let interrupt_in = interrupt_in.ok_or(UsbError::InvalidEndpoint)?;

        // Register with input subsystem
        let device_type = match hid_protocol {
            protocol::KEYBOARD => InputDeviceType::Keyboard,
            protocol::MOUSE => InputDeviceType::Mouse,
            _ => InputDeviceType::Keyboard,
        };

        let info = InputDeviceInfo {
            name: String::from("USB HID Device"),
            phys: String::new(),
            uniq: String::new(),
            device_type,
            vendor: device.vendor_id(),
            product: device.product_id(),
            version: 0,
        };

        let device_id = register_device_info(info);

        let keyboard_state = if hid_protocol == protocol::KEYBOARD {
            Some(Mutex::new(HidKeyboardState::new()))
        } else {
            None
        };

        let mouse_state = if hid_protocol == protocol::MOUSE {
            Some(Mutex::new(HidMouseState::new()))
        } else {
            None
        };

        Ok(UsbHid {
            device,
            interrupt_in,
            interface: interface_num,
            hid_protocol,
            device_id,
            keyboard_state,
            mouse_state,
        })
    }

    /// Set idle rate
    pub fn set_idle(&self, duration: u8, report_id: u8) -> UsbResult<()> {
        self.device.control_transfer(
            0x21, // Class, interface, OUT
            request::SET_IDLE,
            ((duration as u16) << 8) | (report_id as u16),
            self.interface as u16,
            None,
        )?;
        Ok(())
    }

    /// Set protocol (boot or report)
    pub fn set_protocol(&self, boot_protocol: bool) -> UsbResult<()> {
        self.device.control_transfer(
            0x21,
            request::SET_PROTOCOL,
            if boot_protocol { 0 } else { 1 },
            self.interface as u16,
            None,
        )?;
        Ok(())
    }

    /// Get report
    pub fn get_report(&self, report_type: u8, report_id: u8, buffer: &mut [u8]) -> UsbResult<()> {
        self.device.control_transfer(
            0xA1, // Class, interface, IN
            request::GET_REPORT,
            ((report_type as u16) << 8) | (report_id as u16),
            self.interface as u16,
            Some(buffer),
        )?;
        Ok(())
    }

    /// Get report descriptor
    pub fn get_report_descriptor(&self, length: u16) -> UsbResult<Vec<u8>> {
        let mut buffer = vec![0u8; length as usize];
        self.device.control_transfer(
            0x81, // Standard, interface, IN
            0x06, // GET_DESCRIPTOR
            (REPORT_DESCRIPTOR_TYPE as u16) << 8,
            self.interface as u16,
            Some(&mut buffer),
        )?;
        Ok(buffer)
    }

    /// Poll for input (interrupt transfer)
    pub fn poll(&self) -> UsbResult<()> {
        let mut buffer = [0u8; 8];

        match self.device.interrupt_transfer(self.interrupt_in, &mut buffer, TransferDirection::In) {
            Ok(_) => {
                self.process_input(&buffer);
                Ok(())
            }
            Err(UsbError::Timeout) => Ok(()), // No data available
            Err(e) => Err(e),
        }
    }

    /// Process input data
    fn process_input(&self, data: &[u8]) {
        match self.hid_protocol {
            protocol::KEYBOARD => {
                if let Some(ref state) = self.keyboard_state {
                    if let Some(report) = BootKeyboardReport::from_bytes(data) {
                        state.lock().process_report(&report, self.device_id);
                    }
                }
            }
            protocol::MOUSE => {
                if let Some(ref state) = self.mouse_state {
                    if let Some(report) = BootMouseReport::from_bytes(data) {
                        state.lock().process_report(&report, self.device_id);
                    }
                }
            }
            _ => {}
        }
    }

    /// Check if this is a keyboard
    pub fn is_keyboard(&self) -> bool {
        self.hid_protocol == protocol::KEYBOARD
    }

    /// Check if this is a mouse
    pub fn is_mouse(&self) -> bool {
        self.hid_protocol == protocol::MOUSE
    }

    /// Get device ID
    pub fn device_id(&self) -> usize {
        self.device_id
    }
}

/// Convert HID usage code to evdev keycode
fn hid_to_keycode(hid_code: u8) -> Option<u16> {
    match hid_code {
        0x04 => Some(KEY_A),
        0x05 => Some(KEY_B),
        0x06 => Some(KEY_C),
        0x07 => Some(KEY_D),
        0x08 => Some(KEY_E),
        0x09 => Some(KEY_F),
        0x0A => Some(KEY_G),
        0x0B => Some(KEY_H),
        0x0C => Some(KEY_I),
        0x0D => Some(KEY_J),
        0x0E => Some(KEY_K),
        0x0F => Some(KEY_L),
        0x10 => Some(KEY_M),
        0x11 => Some(KEY_N),
        0x12 => Some(KEY_O),
        0x13 => Some(KEY_P),
        0x14 => Some(KEY_Q),
        0x15 => Some(KEY_R),
        0x16 => Some(KEY_S),
        0x17 => Some(KEY_T),
        0x18 => Some(KEY_U),
        0x19 => Some(KEY_V),
        0x1A => Some(KEY_W),
        0x1B => Some(KEY_X),
        0x1C => Some(KEY_Y),
        0x1D => Some(KEY_Z),
        0x1E => Some(KEY_1),
        0x1F => Some(KEY_2),
        0x20 => Some(KEY_3),
        0x21 => Some(KEY_4),
        0x22 => Some(KEY_5),
        0x23 => Some(KEY_6),
        0x24 => Some(KEY_7),
        0x25 => Some(KEY_8),
        0x26 => Some(KEY_9),
        0x27 => Some(KEY_0),
        0x28 => Some(KEY_ENTER),
        0x29 => Some(KEY_ESC),
        0x2A => Some(KEY_BACKSPACE),
        0x2B => Some(KEY_TAB),
        0x2C => Some(KEY_SPACE),
        0x2D => Some(KEY_MINUS),
        0x2E => Some(KEY_EQUAL),
        0x2F => Some(KEY_LEFTBRACE),
        0x30 => Some(KEY_RIGHTBRACE),
        0x31 => Some(KEY_BACKSLASH),
        0x33 => Some(KEY_SEMICOLON),
        0x34 => Some(KEY_APOSTROPHE),
        0x35 => Some(KEY_GRAVE),
        0x36 => Some(KEY_COMMA),
        0x37 => Some(KEY_DOT),
        0x38 => Some(KEY_SLASH),
        0x39 => Some(KEY_CAPSLOCK),
        0x3A => Some(KEY_F1),
        0x3B => Some(KEY_F2),
        0x3C => Some(KEY_F3),
        0x3D => Some(KEY_F4),
        0x3E => Some(KEY_F5),
        0x3F => Some(KEY_F6),
        0x40 => Some(KEY_F7),
        0x41 => Some(KEY_F8),
        0x42 => Some(KEY_F9),
        0x43 => Some(KEY_F10),
        0x44 => Some(KEY_F11),
        0x45 => Some(KEY_F12),
        0x46 => Some(KEY_SYSRQ),
        0x47 => Some(KEY_SCROLLLOCK),
        0x48 => Some(KEY_PAUSE),
        0x49 => Some(KEY_INSERT),
        0x4A => Some(KEY_HOME),
        0x4B => Some(KEY_PAGEUP),
        0x4C => Some(KEY_DELETE),
        0x4D => Some(KEY_END),
        0x4E => Some(KEY_PAGEDOWN),
        0x4F => Some(KEY_RIGHT),
        0x50 => Some(KEY_LEFT),
        0x51 => Some(KEY_DOWN),
        0x52 => Some(KEY_UP),
        0x53 => Some(KEY_NUMLOCK),
        0x54 => Some(KEY_KPSLASH),
        0x55 => Some(KEY_KPASTERISK),
        0x56 => Some(KEY_KPMINUS),
        0x57 => Some(KEY_KPPLUS),
        0x58 => Some(KEY_KPENTER),
        0x59 => Some(KEY_KP1),
        0x5A => Some(KEY_KP2),
        0x5B => Some(KEY_KP3),
        0x5C => Some(KEY_KP4),
        0x5D => Some(KEY_KP5),
        0x5E => Some(KEY_KP6),
        0x5F => Some(KEY_KP7),
        0x60 => Some(KEY_KP8),
        0x61 => Some(KEY_KP9),
        0x62 => Some(KEY_KP0),
        0x63 => Some(KEY_KPDOT),
        _ => None,
    }
}

/// HID class driver
pub struct HidDriver;

impl UsbClassDriver for HidDriver {
    fn name(&self) -> &str {
        "usb-hid"
    }

    fn probe(&self, device: &UsbDevice) -> bool {
        if let Some(config) = device.configuration() {
            for interface in &config.interfaces {
                if interface.interface_class == USB_CLASS_HID
                    && interface.interface_subclass == subclass::BOOT
                {
                    return true;
                }
            }
        }
        false
    }

    fn attach(&self, device: &Arc<UsbDevice>) -> UsbResult<()> {
        let hid = UsbHid::new(device.clone())?;

        // Set boot protocol for simpler handling
        hid.set_protocol(true)?;

        // Set idle to 0 (only report on change)
        hid.set_idle(0, 0)?;

        // Device is now ready for polling
        Ok(())
    }

    fn detach(&self, _device: &Arc<UsbDevice>) -> UsbResult<()> {
        Ok(())
    }
}
