//! PS/2 Controller and Device Drivers
//!
//! Implements 8042 PS/2 controller for keyboard and mouse on x86.

#![no_std]

extern crate alloc;

/// Debug macro for keyboard input events
macro_rules! debug_input {
    ($($arg:tt)*) => {
        // No-op: keyboard events visible via evtest /dev/input/event0
        // Avoid inline asm in non-arch crates
    };
}

use alloc::string::String;
use alloc::sync::Arc;
use core::sync::atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering};
use spin::Mutex;

use input::{
    BTN_LEFT, BTN_MIDDLE, BTN_RIGHT, InputDevice, InputDeviceInfo, InputDeviceType, KeyValue,
    Keymap, REL_WHEEL, REL_WHEEL_HI_RES, REL_X, REL_Y,
};

/// 8042 controller data port
const DATA_PORT: u16 = 0x60;

/// 8042 controller status/command port
const STATUS_PORT: u16 = 0x64;
const COMMAND_PORT: u16 = 0x64;

/// Status register bits
#[allow(unused)]
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
#[allow(unused)]
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
#[allow(unused)]
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
#[allow(unused)]
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
    /// Alt pressed (left alt)
    alt: AtomicBool,
    /// AltGr pressed (right alt)
    altgr: AtomicBool,
    /// Num Lock state
    numlock: AtomicBool,
}

impl Ps2Keyboard {
    /// Create a new PS/2 keyboard driver
    pub fn new() -> Self {
        Ps2Keyboard {
            device_id: AtomicU8::new(255),
            keymap: Mutex::new(Keymap::new()),
            leds: AtomicU8::new(0x02), // Num Lock on by default
            shift: AtomicBool::new(false),
            ctrl: AtomicBool::new(false),
            alt: AtomicBool::new(false),
            altgr: AtomicBool::new(false),
            numlock: AtomicBool::new(true),
        }
    }

    /// Initialize the keyboard
    pub fn init(&self) -> bool {
        // Minimal approach: keyboard should already be working in QEMU
        // Just verify it's responsive
        // Set initial LED state (Num Lock on by default)
        self.update_leds();
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
        debug_input!("[PS2] Scancode: 0x{:02x}", scancode);
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
                input::KEY_LEFTALT => {
                    self.alt.store(pressed, Ordering::SeqCst);
                }
                input::KEY_RIGHTALT => {
                    // Right Alt = AltGr on most international layouts
                    self.altgr.store(pressed, Ordering::SeqCst);
                }
                input::KEY_NUMLOCK => {
                    if pressed {
                        let new_state = !self.numlock.load(Ordering::SeqCst);
                        self.numlock.store(new_state, Ordering::SeqCst);
                        let mut leds = self.leds.load(Ordering::SeqCst);
                        if new_state {
                            leds |= 0x02;
                        } else {
                            leds &= !0x02;
                        }
                        self.leds.store(leds, Ordering::SeqCst);
                        self.update_leds();
                    }
                    // Don't forward NumLock itself to console
                    return;
                }
                _ => {}
            }

            // Report to input subsystem
            let value = if pressed {
                KeyValue::Pressed
            } else {
                KeyValue::Released
            };
            debug_input!("[INPUT] PS/2 KB dev{} keycode={} state={:?}", self.device_id(), keycode, value);
            input::report_key(self.device_id() as usize, keycode, value);
            input::report_sync(self.device_id() as usize);

            // Push to console on key press (not release)
            if pressed {
                let shift = self.shift.load(Ordering::SeqCst);
                let ctrl = self.ctrl.load(Ordering::SeqCst);
                let altgr = self.altgr.load(Ordering::SeqCst);
                let numlock = self.numlock.load(Ordering::SeqCst);

                // Handle Ctrl+key combinations (send control codes)
                // But not if AltGr is pressed (Ctrl+Alt is often used for AltGr on some systems)
                if ctrl && !altgr {
                    let ctrl_char = match keycode {
                        input::KEY_A => Some(0x01), // Ctrl+A
                        input::KEY_B => Some(0x02),
                        input::KEY_C => Some(0x03), // Ctrl+C (SIGINT)
                        input::KEY_D => Some(0x04), // Ctrl+D (EOF)
                        input::KEY_E => Some(0x05),
                        input::KEY_F => Some(0x06),
                        input::KEY_G => Some(0x07),
                        input::KEY_H => Some(0x08),
                        input::KEY_I => Some(0x09),
                        input::KEY_J => Some(0x0A),
                        input::KEY_K => {
                            // Visual indicator for Ctrl+K
                            #[cfg(feature = "debug-input")]
                            {
                                // Send visual feedback to console via callback
                                push_to_console(b"\n\r*** CTRL+K PRESSED ***\n\r");
                            }
                            Some(0x0B)
                        },
                        input::KEY_L => Some(0x0C),
                        input::KEY_M => Some(0x0D),
                        input::KEY_N => Some(0x0E),
                        input::KEY_O => Some(0x0F),
                        input::KEY_P => Some(0x10),
                        input::KEY_Q => Some(0x11),
                        input::KEY_R => Some(0x12),
                        input::KEY_S => Some(0x13),
                        input::KEY_T => Some(0x14),
                        input::KEY_U => Some(0x15),
                        input::KEY_V => Some(0x16),
                        input::KEY_W => Some(0x17),
                        input::KEY_X => Some(0x18),
                        input::KEY_Y => Some(0x19),
                        input::KEY_Z => Some(0x1A), // Ctrl+Z (SIGTSTP)
                        _ => None,
                    };
                    if let Some(ch) = ctrl_char {
                        push_to_console(&[ch]);
                        return;
                    }
                }

                // Handle keypad in navigation mode (Num Lock off)
                if !numlock {
                    let nav_seq: Option<&[u8]> = match keycode {
                        input::KEY_KP8 => Some(b"\x1b[A"),
                        input::KEY_KP2 => Some(b"\x1b[B"),
                        input::KEY_KP6 => Some(b"\x1b[C"),
                        input::KEY_KP4 => Some(b"\x1b[D"),
                        input::KEY_KP7 => Some(b"\x1b[H"),
                        input::KEY_KP1 => Some(b"\x1b[F"),
                        input::KEY_KP9 => Some(b"\x1b[5~"),
                        input::KEY_KP3 => Some(b"\x1b[6~"),
                        input::KEY_KP0 => Some(b"\x1b[2~"),
                        input::KEY_KPDOT => Some(b"\x1b[3~"),
                        input::KEY_KP5 => Some(b"\x1b[E"),
                        _ => None,
                    };
                    if let Some(seq) = nav_seq {
                        push_to_console(seq);
                        return;
                    }
                }

                // Handle keypad in numeric mode (Num Lock on) to ensure digits always emit
                if numlock {
                    let kp_char: Option<u8> = match keycode {
                        input::KEY_KP0 => Some(b'0'),
                        input::KEY_KP1 => Some(b'1'),
                        input::KEY_KP2 => Some(b'2'),
                        input::KEY_KP3 => Some(b'3'),
                        input::KEY_KP4 => Some(b'4'),
                        input::KEY_KP5 => Some(b'5'),
                        input::KEY_KP6 => Some(b'6'),
                        input::KEY_KP7 => Some(b'7'),
                        input::KEY_KP8 => Some(b'8'),
                        input::KEY_KP9 => Some(b'9'),
                        input::KEY_KPDOT => Some(b'.'),
                        input::KEY_KPENTER => Some(b'\n'),
                        input::KEY_KPPLUS => Some(b'+'),
                        input::KEY_KPMINUS => Some(b'-'),
                        input::KEY_KPASTERISK => Some(b'*'),
                        input::KEY_KPSLASH => Some(b'/'),
                        _ => None,
                    };
                    if let Some(ch) = kp_char {
                        push_to_console(&[ch]);
                        return;
                    }
                }

                // Handle special keys (arrow keys, etc.) - send ANSI escape sequences
                let ansi_seq: Option<&[u8]> = match keycode {
                    input::KEY_UP => Some(b"\x1b[A"),
                    input::KEY_DOWN => Some(b"\x1b[B"),
                    input::KEY_RIGHT => Some(b"\x1b[C"),
                    input::KEY_LEFT => Some(b"\x1b[D"),
                    input::KEY_HOME => Some(b"\x1b[H"),
                    input::KEY_END => Some(b"\x1b[F"),
                    input::KEY_INSERT => {
                        // Visual indicator for INSERT key
                        #[cfg(feature = "debug-input")]
                        {
                            // Send visual feedback to console via callback
                            push_to_console(b"\n\r*** INSERT KEY PRESSED ***\n\r");
                        }
                        Some(b"\x1b[2~")
                    },
                    input::KEY_DELETE => Some(b"\x1b[3~"),
                    input::KEY_PAGEUP => Some(b"\x1b[5~"),
                    input::KEY_PAGEDOWN => Some(b"\x1b[6~"),
                    input::KEY_F1 => Some(b"\x1bOP"),
                    input::KEY_F2 => Some(b"\x1bOQ"),
                    input::KEY_F3 => Some(b"\x1bOR"),
                    input::KEY_F4 => Some(b"\x1bOS"),
                    input::KEY_F5 => Some(b"\x1b[15~"),
                    input::KEY_F6 => Some(b"\x1b[17~"),
                    input::KEY_F7 => Some(b"\x1b[18~"),
                    input::KEY_F8 => Some(b"\x1b[19~"),
                    input::KEY_F9 => Some(b"\x1b[20~"),
                    input::KEY_F10 => Some(b"\x1b[21~"),
                    input::KEY_F11 => Some(b"\x1b[23~"),
                    input::KEY_F12 => Some(b"\x1b[24~"),
                    input::KEY_ESC => Some(b"\x1b"),
                    _ => None,
                };

                if let Some(seq) = ansi_seq {
                    push_to_console(seq);
                    return;
                }

                // Convert to character using current keyboard layout (with AltGr support)
                if let Some(ch) = input::keymap::keycode_to_char_current(keycode, shift, altgr) {
                    let mut buf = [0u8; 4];
                    let s = ch.encode_utf8(&mut buf);
                    push_to_console(s.as_bytes());
                }
            }
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

        let packet_size = if self.has_wheel.load(Ordering::SeqCst) {
            4
        } else {
            3
        };

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
            let value = if new_buttons & 0x01 != 0 {
                KeyValue::Pressed
            } else {
                KeyValue::Released
            };
            input::report_key(device_id, BTN_LEFT, value);
        }

        // Right button
        if (old_buttons ^ new_buttons) & 0x02 != 0 {
            let value = if new_buttons & 0x02 != 0 {
                KeyValue::Pressed
            } else {
                KeyValue::Released
            };
            input::report_key(device_id, BTN_RIGHT, value);
        }

        // Middle button
        if (old_buttons ^ new_buttons) & 0x04 != 0 {
            let value = if new_buttons & 0x04 != 0 {
                KeyValue::Pressed
            } else {
                KeyValue::Released
            };
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
                let wheel_val = -(dz as i32);
                input::report_rel(device_id, REL_WHEEL, wheel_val);
                // Hi-res scroll: 120 units per PS/2 notch (Linux 5.0+ convention)
                input::report_rel(device_id, REL_WHEEL_HI_RES, wheel_val * 120);
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
    // Minimal approach: just ensure keyboard interrupts are enabled
    // QEMU/BIOS has already set up the controller correctly

    // Flush any pending data
    while unsafe { inb(STATUS_PORT) } & status::OUTPUT_FULL != 0 {
        let _ = unsafe { inb(DATA_PORT) };
    }

    // Read current config
    send_command(cmd::READ_CONFIG);
    let config = read_data().unwrap_or(0);

    // Enable keyboard interrupt (bit 0) if not already enabled
    if config & 0x01 == 0 {
        send_command(cmd::WRITE_CONFIG);
        send_data(config | 0x01);
    }

    true
}

/// Global keyboard instance
static KEYBOARD: Mutex<Option<Arc<Ps2Keyboard>>> = Mutex::new(None);

/// Global mouse instance
static MOUSE: Mutex<Option<Arc<Ps2Mouse>>> = Mutex::new(None);

/// Console character callback type
/// Called with bytes to push to console input buffer (may be single char or ANSI sequence)
pub type ConsoleCharCallback = fn(&[u8]);

/// Global console callback
static mut CONSOLE_CALLBACK: Option<ConsoleCharCallback> = None;

/// Set the console character callback
///
/// # Safety
/// Must be called during single-threaded initialization
pub unsafe fn set_console_callback(callback: ConsoleCharCallback) {
    unsafe {
        CONSOLE_CALLBACK = Some(callback);
    }
}

/// Call the console callback with character(s)
fn push_to_console(data: &[u8]) {
    unsafe {
        if let Some(callback) = CONSOLE_CALLBACK {
            callback(data);
        }
    }
}

/// Debug flag to track init status
static INIT_STATUS: AtomicU8 = AtomicU8::new(0);
// 0 = not started, 1 = controller init failed, 2 = keyboard init failed,
// 3 = keyboard init success, 4 = mouse init failed, 5 = both success

/// Get initialization status for debugging
pub fn init_status() -> u8 {
    INIT_STATUS.load(Ordering::Relaxed)
}

/// Initialize PS/2 devices
pub fn init() -> bool {
    if !init_controller() {
        INIT_STATUS.store(1, Ordering::Relaxed);
        return false;
    }

    // Initialize keyboard
    let keyboard = Arc::new(Ps2Keyboard::new());
    if keyboard.init() {
        let id = input::register_device(keyboard.clone());
        keyboard.set_device_id(id as u8);
        *KEYBOARD.lock() = Some(keyboard);
        INIT_STATUS.store(3, Ordering::Relaxed);
    } else {
        INIT_STATUS.store(2, Ordering::Relaxed);
    }

    // Initialize mouse
    let mouse = Arc::new(Ps2Mouse::new());
    if mouse.init() {
        let id = input::register_device(mouse.clone());
        mouse.set_device_id(id as u8);
        *MOUSE.lock() = Some(mouse);
        if INIT_STATUS.load(Ordering::Relaxed) == 3 {
            INIT_STATUS.store(5, Ordering::Relaxed);
        }
    } else if INIT_STATUS.load(Ordering::Relaxed) == 3 {
        INIT_STATUS.store(4, Ordering::Relaxed);
    }

    true
}

/// Debug counter for keyboard interrupts
static KEYBOARD_IRQ_COUNT: AtomicUsize = AtomicUsize::new(0);

/// Last scancode received (for debugging)
static LAST_SCANCODE: AtomicU8 = AtomicU8::new(0);

/// Scancode log for debugging (lock-free, only written by IRQ handler)
static SCANCODE_LOG: Mutex<[u8; 20]> = Mutex::new([0; 20]);

/// Get last scancode (for debugging)
pub fn last_scancode() -> u8 {
    LAST_SCANCODE.load(Ordering::Relaxed)
}

/// Handle keyboard interrupt (IRQ 1)
pub fn handle_keyboard_irq() {
    KEYBOARD_IRQ_COUNT.fetch_add(1, Ordering::Relaxed);

    if let Some(keyboard) = KEYBOARD.lock().as_ref() {
        let scancode = unsafe { inb(DATA_PORT) };

        // Store in debug log
        let count = KEYBOARD_IRQ_COUNT.load(Ordering::Relaxed);
        if count <= 50 {
            // Increase debug window
            let mut log = SCANCODE_LOG.lock();
            let len = log.len();
            log[(count - 1) % len] = scancode;
        }
        LAST_SCANCODE.store(scancode, Ordering::Relaxed);

        keyboard.handle_scancode(scancode);
    }
}

/// Get scancode log for debugging
pub fn get_scancode_log() -> [u8; 20] {
    *SCANCODE_LOG.lock()
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

/// Get keyboard IRQ count (for debugging)
pub fn keyboard_irq_count() -> usize {
    KEYBOARD_IRQ_COUNT.load(Ordering::Relaxed)
}

/// Check if keyboard is initialized
pub fn is_keyboard_initialized() -> bool {
    KEYBOARD.lock().is_some()
}

/// Read PS/2 status register (for debugging)
pub fn read_status() -> u8 {
    unsafe { inb(STATUS_PORT) }
}
