//! PS/2 Controller and Device Drivers
//!
//! Implements 8042 PS/2 controller for keyboard and mouse on x86.

#![no_std]

extern crate alloc;

use alloc::string::String;
use alloc::sync::Arc;
use core::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use spin::Mutex;

use input::{
    InputDevice, InputDeviceInfo, InputDeviceType, KeyValue, Keymap,
    REL_X, REL_Y, REL_WHEEL, BTN_LEFT, BTN_RIGHT, BTN_MIDDLE,
};

/// 8042 controller data port
const DATA_PORT: u16 = 0x60;

/// 8042 controller status/command port
const STATUS_PORT: u16 = 0x64;
const COMMAND_PORT: u16 = 0x64;

/// Status register bits
mod status {
    pub const OUTPUT_FULL: u8 = 0x01;
    pub const INPUT_FULL: u8 = 0x02;
    pub const SYSTEM_FLAG: u8 = 0x04;
    pub const COMMAND_DATA: u8 = 0x08;
    pub const KEYBOARD_LOCK: u8 = 0x10;
    pub const MOUSE_DATA: u8 = 0x20;
    pub const TIMEOUT_ERROR: u8 = 0x40;
    pub const PARITY_ERROR: u8 = 0x80;
}

/// Controller commands
mod cmd {
    pub const READ_CONFIG: u8 = 0x20;
    pub const WRITE_CONFIG: u8 = 0x60;
    pub const DISABLE_PORT2: u8 = 0xA7;
    pub const ENABLE_PORT2: u8 = 0xA8;
    pub const TEST_PORT2: u8 = 0xA9;
    pub const SELF_TEST: u8 = 0xAA;
    pub const TEST_PORT1: u8 = 0xAB;
    pub const DISABLE_PORT1: u8 = 0xAD;
    pub const ENABLE_PORT1: u8 = 0xAE;
    pub const WRITE_PORT2: u8 = 0xD4;
}

/// Keyboard commands
mod kbd_cmd {
    pub const SET_LEDS: u8 = 0xED;
    pub const ECHO: u8 = 0xEE;
    pub const GET_SET_SCANCODE: u8 = 0xF0;
    pub const IDENTIFY: u8 = 0xF2;
    pub const SET_TYPEMATIC: u8 = 0xF3;
    pub const ENABLE_SCANNING: u8 = 0xF4;
    pub const DISABLE_SCANNING: u8 = 0xF5;
    pub const SET_DEFAULTS: u8 = 0xF6;
    pub const RESEND: u8 = 0xFE;
    pub const RESET: u8 = 0xFF;
}

/// Mouse commands
mod mouse_cmd {
    pub const SET_SCALING_1_1: u8 = 0xE6;
    pub const SET_SCALING_2_1: u8 = 0xE7;
    pub const SET_RESOLUTION: u8 = 0xE8;
    pub const STATUS_REQUEST: u8 = 0xE9;
    pub const SET_STREAM_MODE: u8 = 0xEA;
    pub const READ_DATA: u8 = 0xEB;
    pub const SET_REMOTE_MODE: u8 = 0xF0;
    pub const GET_ID: u8 = 0xF2;
    pub const SET_SAMPLE_RATE: u8 = 0xF3;
    pub const ENABLE_DATA: u8 = 0xF4;
    pub const DISABLE_DATA: u8 = 0xF5;
    pub const SET_DEFAULTS: u8 = 0xF6;
    pub const RESEND: u8 = 0xFE;
    pub const RESET: u8 = 0xFF;
}

/// Read from I/O port
///
/// # Safety
/// Requires access to I/O port.
#[inline]
unsafe fn inb(port: u16) -> u8 {
    let value: u8;
    unsafe {
        core::arch::asm!("in al, dx", out("al") value, in("dx") port, options(nomem, nostack, preserves_flags));
    }
    value
}

/// Write to I/O port
///
/// # Safety
/// Requires access to I/O port.
#[inline]
unsafe fn outb(port: u16, value: u8) {
    unsafe {
        core::arch::asm!("out dx, al", in("dx") port, in("al") value, options(nomem, nostack, preserves_flags));
    }
}

/// Wait for controller input buffer to be empty
fn wait_input() -> bool {
    for _ in 0..10000 {
        if unsafe { inb(STATUS_PORT) } & status::INPUT_FULL == 0 {
            return true;
        }
    }
    false
}

/// Wait for controller output buffer to be full
fn wait_output() -> bool {
    for _ in 0..10000 {
        if unsafe { inb(STATUS_PORT) } & status::OUTPUT_FULL != 0 {
            return true;
        }
    }
    false
}

/// Send command to controller
fn send_command(cmd: u8) {
    wait_input();
    unsafe { outb(COMMAND_PORT, cmd) };
}

/// Send data to controller
fn send_data(data: u8) {
    wait_input();
    unsafe { outb(DATA_PORT, data) };
}

/// Read data from controller
fn read_data() -> Option<u8> {
    if wait_output() {
        Some(unsafe { inb(DATA_PORT) })
    } else {
        None
    }
}

/// PS/2 keyboard driver
pub struct Ps2Keyboard {
    /// Device ID for input subsystem
    device_id: AtomicU8,
    /// Keymap
    keymap: Mutex<Keymap>,
    /// LED state (Scroll, Num, Caps)
    leds: AtomicU8,
    /// Shift pressed
    shift: AtomicBool,
    /// Ctrl pressed
    ctrl: AtomicBool,
    /// Alt pressed
    alt: AtomicBool,
}

impl Ps2Keyboard {
    /// Create a new PS/2 keyboard driver
    pub fn new() -> Self {
        Ps2Keyboard {
            device_id: AtomicU8::new(255),
            keymap: Mutex::new(Keymap::new()),
            leds: AtomicU8::new(0),
            shift: AtomicBool::new(false),
            ctrl: AtomicBool::new(false),
            alt: AtomicBool::new(false),
        }
    }

    /// Initialize the keyboard
    pub fn init(&self) -> bool {
        // Reset keyboard
        send_data(kbd_cmd::RESET);
        if read_data() != Some(0xAA) {
            return false;
        }

        // Enable scanning
        send_data(kbd_cmd::ENABLE_SCANNING);
        if read_data() != Some(0xFA) {
            return false;
        }

        true
    }

    /// Set device ID
    pub fn set_device_id(&self, id: u8) {
        self.device_id.store(id, Ordering::SeqCst);
    }

    /// Get device ID
    pub fn device_id(&self) -> u8 {
        self.device_id.load(Ordering::SeqCst)
    }

    /// Handle a scancode
    pub fn handle_scancode(&self, scancode: u8) {
        let mut keymap = self.keymap.lock();

        if let Some((keycode, pressed)) = keymap.process_scancode(scancode) {
            // Update modifier state
            match keycode {
                input::KEY_LEFTSHIFT | input::KEY_RIGHTSHIFT => {
                    self.shift.store(pressed, Ordering::SeqCst);
                }
                input::KEY_LEFTCTRL | input::KEY_RIGHTCTRL => {
                    self.ctrl.store(pressed, Ordering::SeqCst);
                }
                input::KEY_LEFTALT | input::KEY_RIGHTALT => {
                    self.alt.store(pressed, Ordering::SeqCst);
                }
                _ => {}
            }

            // Report to input subsystem
            let value = if pressed { KeyValue::Pressed } else { KeyValue::Released };
            input::report_key(self.device_id() as usize, keycode, value);
            input::report_sync(self.device_id() as usize);
        }
    }

    /// Update LED state
    fn update_leds(&self) {
        let leds = self.leds.load(Ordering::SeqCst);
        send_data(kbd_cmd::SET_LEDS);
        let _ = read_data(); // ACK
        send_data(leds);
        let _ = read_data(); // ACK
    }

    /// Check if shift is pressed
    pub fn is_shift(&self) -> bool {
        self.shift.load(Ordering::SeqCst)
    }

    /// Check if ctrl is pressed
    pub fn is_ctrl(&self) -> bool {
        self.ctrl.load(Ordering::SeqCst)
    }

    /// Check if alt is pressed
    pub fn is_alt(&self) -> bool {
        self.alt.load(Ordering::SeqCst)
    }
}

impl Default for Ps2Keyboard {
    fn default() -> Self {
        Self::new()
    }
}

impl InputDevice for Ps2Keyboard {
    fn info(&self) -> InputDeviceInfo {
        InputDeviceInfo {
            name: String::from("AT Translated Set 2 keyboard"),
            phys: String::from("isa0060/serio0/input0"),
            uniq: String::new(),
            device_type: InputDeviceType::Keyboard,
            vendor: 0x0001,
            product: 0x0001,
            version: 0xab41,
        }
    }

    fn poll(&self) {
        // Keyboard is interrupt-driven, no polling needed
    }

    fn set_led(&self, led: u16, on: bool) -> bool {
        let bit = match led {
            0 => 0x04, // Caps Lock
            1 => 0x02, // Num Lock
            2 => 0x01, // Scroll Lock
            _ => return false,
        };

        let mut leds = self.leds.load(Ordering::SeqCst);
        if on {
            leds |= bit;
        } else {
            leds &= !bit;
        }
        self.leds.store(leds, Ordering::SeqCst);
        self.update_leds();
        true
    }
}

/// PS/2 mouse driver
pub struct Ps2Mouse {
    /// Device ID for input subsystem
    device_id: AtomicU8,
    /// Packet buffer
    packet: Mutex<[u8; 4]>,
    /// Current byte index
    byte_index: AtomicU8,
    /// Has scroll wheel
    has_wheel: AtomicBool,
    /// Button state
    buttons: AtomicU8,
}

impl Ps2Mouse {
    /// Create a new PS/2 mouse driver
    pub fn new() -> Self {
        Ps2Mouse {
            device_id: AtomicU8::new(255),
            packet: Mutex::new([0; 4]),
            byte_index: AtomicU8::new(0),
            has_wheel: AtomicBool::new(false),
            buttons: AtomicU8::new(0),
        }
    }

    /// Initialize the mouse
    pub fn init(&self) -> bool {
        // Enable port 2
        send_command(cmd::ENABLE_PORT2);

        // Reset mouse
        send_command(cmd::WRITE_PORT2);
        send_data(mouse_cmd::RESET);

        // Wait for ACK and self-test result
        let _ = read_data(); // ACK (0xFA)
        let _ = read_data(); // Self-test (0xAA)
        let _ = read_data(); // Mouse ID (0x00)

        // Try to enable scroll wheel (IntelliMouse)
        // Magic sequence: sample rates 200, 100, 80
        self.set_sample_rate(200);
        self.set_sample_rate(100);
        self.set_sample_rate(80);

        // Get device ID
        send_command(cmd::WRITE_PORT2);
        send_data(mouse_cmd::GET_ID);
        let _ = read_data(); // ACK
        if let Some(id) = read_data() {
            if id == 0x03 {
                self.has_wheel.store(true, Ordering::SeqCst);
            }
        }

        // Set sample rate to 100
        self.set_sample_rate(100);

        // Enable data reporting
        send_command(cmd::WRITE_PORT2);
        send_data(mouse_cmd::ENABLE_DATA);
        let _ = read_data(); // ACK

        true
    }

    /// Set sample rate
    fn set_sample_rate(&self, rate: u8) {
        send_command(cmd::WRITE_PORT2);
        send_data(mouse_cmd::SET_SAMPLE_RATE);
        let _ = read_data(); // ACK
        send_command(cmd::WRITE_PORT2);
        send_data(rate);
        let _ = read_data(); // ACK
    }

    /// Set device ID
    pub fn set_device_id(&self, id: u8) {
        self.device_id.store(id, Ordering::SeqCst);
    }

    /// Get device ID
    pub fn device_id(&self) -> u8 {
        self.device_id.load(Ordering::SeqCst)
    }

    /// Handle a mouse byte
    pub fn handle_byte(&self, byte: u8) {
        let mut packet = self.packet.lock();
        let idx = self.byte_index.load(Ordering::SeqCst);

        // First byte must have bit 3 set (always 1)
        if idx == 0 && byte & 0x08 == 0 {
            return; // Sync error, skip
        }

        packet[idx as usize] = byte;
        let next_idx = idx + 1;

        let packet_size = if self.has_wheel.load(Ordering::SeqCst) { 4 } else { 3 };

        if next_idx >= packet_size {
            // Complete packet
            self.byte_index.store(0, Ordering::SeqCst);
            self.process_packet(&packet);
        } else {
            self.byte_index.store(next_idx, Ordering::SeqCst);
        }
    }

    /// Process a complete mouse packet
    fn process_packet(&self, packet: &[u8; 4]) {
        let status = packet[0];
        let dx = packet[1] as i8;
        let dy = packet[2] as i8;

        // Report button changes
        let old_buttons = self.buttons.load(Ordering::SeqCst);
        let new_buttons = status & 0x07;
        self.buttons.store(new_buttons, Ordering::SeqCst);

        let device_id = self.device_id() as usize;

        // Left button
        if (old_buttons ^ new_buttons) & 0x01 != 0 {
            let value = if new_buttons & 0x01 != 0 { KeyValue::Pressed } else { KeyValue::Released };
            input::report_key(device_id, BTN_LEFT, value);
        }

        // Right button
        if (old_buttons ^ new_buttons) & 0x02 != 0 {
            let value = if new_buttons & 0x02 != 0 { KeyValue::Pressed } else { KeyValue::Released };
            input::report_key(device_id, BTN_RIGHT, value);
        }

        // Middle button
        if (old_buttons ^ new_buttons) & 0x04 != 0 {
            let value = if new_buttons & 0x04 != 0 { KeyValue::Pressed } else { KeyValue::Released };
            input::report_key(device_id, BTN_MIDDLE, value);
        }

        // Movement
        if dx != 0 {
            input::report_rel(device_id, REL_X, dx as i32);
        }
        if dy != 0 {
            // PS/2 mouse Y axis is inverted
            input::report_rel(device_id, REL_Y, -(dy as i32));
        }

        // Scroll wheel
        if self.has_wheel.load(Ordering::SeqCst) {
            let dz = packet[3] as i8;
            if dz != 0 {
                input::report_rel(device_id, REL_WHEEL, -(dz as i32));
            }
        }

        input::report_sync(device_id);
    }
}

impl Default for Ps2Mouse {
    fn default() -> Self {
        Self::new()
    }
}

impl InputDevice for Ps2Mouse {
    fn info(&self) -> InputDeviceInfo {
        let name = if self.has_wheel.load(Ordering::SeqCst) {
            "ImExPS/2 Generic Explorer Mouse"
        } else {
            "ImPS/2 Generic Wheel Mouse"
        };

        InputDeviceInfo {
            name: String::from(name),
            phys: String::from("isa0060/serio1/input0"),
            uniq: String::new(),
            device_type: InputDeviceType::Mouse,
            vendor: 0x0002,
            product: 0x0001,
            version: 0x0000,
        }
    }

    fn poll(&self) {
        // Mouse is interrupt-driven, no polling needed
    }
}

/// Initialize PS/2 controller
pub fn init_controller() -> bool {
    // Disable both ports
    send_command(cmd::DISABLE_PORT1);
    send_command(cmd::DISABLE_PORT2);

    // Flush output buffer
    while unsafe { inb(STATUS_PORT) } & status::OUTPUT_FULL != 0 {
        let _ = unsafe { inb(DATA_PORT) };
    }

    // Read config
    send_command(cmd::READ_CONFIG);
    let config = read_data().unwrap_or(0);

    // Disable interrupts and translation
    let config = config & !0x43; // Clear bits 0, 1, 6
    send_command(cmd::WRITE_CONFIG);
    send_data(config);

    // Self-test
    send_command(cmd::SELF_TEST);
    if read_data() != Some(0x55) {
        return false;
    }

    // Restore config after self-test
    send_command(cmd::WRITE_CONFIG);
    send_data(config);

    // Test port 1
    send_command(cmd::TEST_PORT1);
    if read_data() != Some(0x00) {
        return false;
    }

    // Enable port 1 and interrupts
    send_command(cmd::ENABLE_PORT1);
    send_command(cmd::READ_CONFIG);
    let config = read_data().unwrap_or(0);
    send_command(cmd::WRITE_CONFIG);
    send_data(config | 0x01); // Enable port 1 interrupt

    // Check for port 2
    send_command(cmd::TEST_PORT2);
    let port2_ok = read_data() == Some(0x00);

    if port2_ok {
        send_command(cmd::ENABLE_PORT2);
        send_command(cmd::READ_CONFIG);
        let config = read_data().unwrap_or(0);
        send_command(cmd::WRITE_CONFIG);
        send_data(config | 0x02); // Enable port 2 interrupt
    }

    true
}

/// Global keyboard instance
static KEYBOARD: Mutex<Option<Arc<Ps2Keyboard>>> = Mutex::new(None);

/// Global mouse instance
static MOUSE: Mutex<Option<Arc<Ps2Mouse>>> = Mutex::new(None);

/// Initialize PS/2 devices
pub fn init() -> bool {
    if !init_controller() {
        return false;
    }

    // Initialize keyboard
    let keyboard = Arc::new(Ps2Keyboard::new());
    if keyboard.init() {
        let id = input::register_device(keyboard.clone());
        keyboard.set_device_id(id as u8);
        *KEYBOARD.lock() = Some(keyboard);
    }

    // Initialize mouse
    let mouse = Arc::new(Ps2Mouse::new());
    if mouse.init() {
        let id = input::register_device(mouse.clone());
        mouse.set_device_id(id as u8);
        *MOUSE.lock() = Some(mouse);
    }

    true
}

/// Handle keyboard interrupt (IRQ 1)
pub fn handle_keyboard_irq() {
    if let Some(keyboard) = KEYBOARD.lock().as_ref() {
        let scancode = unsafe { inb(DATA_PORT) };
        keyboard.handle_scancode(scancode);
    }
}

/// Handle mouse interrupt (IRQ 12)
pub fn handle_mouse_irq() {
    if let Some(mouse) = MOUSE.lock().as_ref() {
        let byte = unsafe { inb(DATA_PORT) };
        mouse.handle_byte(byte);
    }
}

/// Get keyboard device
pub fn keyboard() -> Option<Arc<Ps2Keyboard>> {
    KEYBOARD.lock().clone()
}

/// Get mouse device
pub fn mouse() -> Option<Arc<Ps2Mouse>> {
    MOUSE.lock().clone()
}
