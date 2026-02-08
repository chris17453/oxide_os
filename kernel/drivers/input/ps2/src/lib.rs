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
    /// Caps Lock state (🔥 NOW ACTUALLY IMPLEMENTED 🔥)
    capslock: AtomicBool,
    /// Scroll Lock state (🔥 ALSO IMPLEMENTED 🔥)
    scrolllock: AtomicBool,
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
            capslock: AtomicBool::new(false), // Caps Lock off by default (normal behavior)
            scrolllock: AtomicBool::new(false), // Scroll Lock off by default
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
                            leds |= 0x02; // Num Lock LED bit
                        } else {
                            leds &= !0x02;
                        }
                        self.leds.store(leds, Ordering::SeqCst);
                        self.update_leds();
                    }
                    // Don't forward NumLock itself to console
                    return;
                }
                input::KEY_CAPSLOCK => {
                    // 🔥 CAPS LOCK: THE FIX THE WORLD NEEDED 🔥
                    //
                    // OLD CODE: Key pressed, nothing happens, users cry
                    // NEW CODE: Actually toggles Caps Lock state and LED like it's 2077
                    if pressed {
                        let new_state = !self.capslock.load(Ordering::SeqCst);
                        self.capslock.store(new_state, Ordering::SeqCst);
                        let mut leds = self.leds.load(Ordering::SeqCst);
                        if new_state {
                            leds |= 0x04; // Caps Lock LED bit (bit 2)
                        } else {
                            leds &= !0x04;
                        }
                        self.leds.store(leds, Ordering::SeqCst);
                        self.update_leds();
                    }
                    // Don't forward Caps Lock key itself to console
                    return;
                }
                input::KEY_SCROLLLOCK => {
                    // 🔥 SCROLL LOCK: WELCOME TO THE LOCK PARTY (Priority #11) 🔥
                    //
                    // Before: Key pressed, nothing happens, LED stays dark
                    // After: Toggles Scroll Lock state and LED
                    //
                    // Note: Traditional Scroll Lock behavior (pause terminal output)
                    // is not implemented yet - this just tracks the LED state.
                    if pressed {
                        let new_state = !self.scrolllock.load(Ordering::SeqCst);
                        self.scrolllock.store(new_state, Ordering::SeqCst);
                        let mut leds = self.leds.load(Ordering::SeqCst);
                        if new_state {
                            leds |= 0x01; // Scroll Lock LED bit (bit 0)
                        } else {
                            leds &= !0x01;
                        }
                        self.leds.store(leds, Ordering::SeqCst);
                        self.update_leds();
                    }
                    // Don't forward Scroll Lock key itself to console
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
            debug_input!(
                "[INPUT] PS/2 KB dev{} keycode={} state={:?}",
                self.device_id(),
                keycode,
                value
            );
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
                        }
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

                // 🔥 VT SWITCHING: Alt+F1 through Alt+F6 (v3 analysis fix) 🔥
                // Before: Alt+F1 just sent escape sequence
                // After: Switches to VT 0-5 respectively
                let alt = self.alt.load(Ordering::SeqCst);
                let altgr = self.altgr.load(Ordering::SeqCst);
                if alt || altgr {
                    let vt_num = match keycode {
                        input::KEY_F1 => Some(0),
                        input::KEY_F2 => Some(1),
                        input::KEY_F3 => Some(2),
                        input::KEY_F4 => Some(3),
                        input::KEY_F5 => Some(4),
                        input::KEY_F6 => Some(5),
                        _ => None,
                    };

                    if let Some(vt) = vt_num {
                        // Call VT switch callback
                        unsafe {
                            if let Some(callback) = VT_SWITCH_CALLBACK {
                                callback(vt);
                            }
                        }
                        return; // Don't generate escape sequence
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
                    }
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
                    // F13-F24 keys for advanced terminal applications
                    // -- ByteRiot: Extended function keys for power users
                    input::KEY_F13 => Some(b"\x1b[25~"),
                    input::KEY_F14 => Some(b"\x1b[26~"),
                    input::KEY_F15 => Some(b"\x1b[28~"),
                    input::KEY_F16 => Some(b"\x1b[29~"),
                    input::KEY_F17 => Some(b"\x1b[31~"),
                    input::KEY_F18 => Some(b"\x1b[32~"),
                    input::KEY_F19 => Some(b"\x1b[33~"),
                    input::KEY_F20 => Some(b"\x1b[34~"),
                    // F21-F24 use modified sequences
                    input::KEY_F21 => Some(b"\x1b[23;2~"), // Shift+F11
                    input::KEY_F22 => Some(b"\x1b[24;2~"), // Shift+F12
                    input::KEY_F23 => Some(b"\x1b[25;2~"), // Shift+F13
                    input::KEY_F24 => Some(b"\x1b[26;2~"), // Shift+F14
                    input::KEY_ESC => Some(b"\x1b"),
                    _ => None,
                };

                if let Some(seq) = ansi_seq {
                    push_to_console(seq);
                    return;
                }

                // 🔥 CAPS LOCK NOW ACTUALLY WORKS 🔥
                // Caps Lock XORs with Shift for LETTERS ONLY (not symbols)
                // - Caps + 'a' = 'A'
                // - Caps + Shift + 'a' = 'a' (they cancel)
                // - Caps + '1' = '1' (NOT '!', Caps Lock ignores non-letters)
                let capslock = self.capslock.load(Ordering::SeqCst);
                if let Some(ch) =
                    input::keymap::keycode_to_char_current(keycode, shift, altgr, capslock)
                {
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
    /// -- TorqueJax: Full PS/2 mouse init with IntelliMouse scroll wheel detection
    pub fn init(&self) -> bool {
        serial_debug(b"[PS2-MOUSE] enable port2\r\n");
        send_command(cmd::ENABLE_PORT2);

        serial_debug(b"[PS2-MOUSE] reset\r\n");
        send_command(cmd::WRITE_PORT2);
        send_data(mouse_cmd::RESET);

        // Wait for ACK and self-test result
        serial_debug(b"[PS2-MOUSE] read ACK\r\n");
        let _ = read_data(); // ACK (0xFA)
        serial_debug(b"[PS2-MOUSE] read self-test\r\n");
        let _ = read_data(); // Self-test (0xAA)
        serial_debug(b"[PS2-MOUSE] read ID\r\n");
        let _ = read_data(); // Mouse ID (0x00)

        // Try to enable scroll wheel (IntelliMouse)
        // Magic sequence: sample rates 200, 100, 80
        serial_debug(b"[PS2-MOUSE] intellimouse detect\r\n");
        self.set_sample_rate(200);
        self.set_sample_rate(100);
        self.set_sample_rate(80);

        // Get device ID
        serial_debug(b"[PS2-MOUSE] get ID\r\n");
        send_command(cmd::WRITE_PORT2);
        send_data(mouse_cmd::GET_ID);
        let _ = read_data(); // ACK
        if let Some(id) = read_data() {
            if id == 0x03 {
                self.has_wheel.store(true, Ordering::SeqCst);
                serial_debug(b"[PS2-MOUSE] scroll wheel detected\r\n");
            }
        }

        // Set sample rate to 100
        serial_debug(b"[PS2-MOUSE] set rate 100\r\n");
        self.set_sample_rate(100);

        // Enable data reporting
        serial_debug(b"[PS2-MOUSE] enable reporting\r\n");
        send_command(cmd::WRITE_PORT2);
        send_data(mouse_cmd::ENABLE_DATA);
        let _ = read_data(); // ACK

        serial_debug(b"[PS2-MOUSE] init done\r\n");
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

/// Write a debug string to COM1 serial port (no locks, no deps)
/// -- TorqueJax: Raw serial for tracing init before anything else is up
fn serial_debug(msg: &[u8]) {
    for &b in msg {
        // Wait for THRE (transmit holding register empty)
        while unsafe { inb(0x3FD) } & 0x20 == 0 {}
        unsafe { outb(0x3F8, b) };
    }
}

/// Disable interrupts (x86 CLI)
/// -- TorqueJax: Gate IRQs during init so handlers can't steal our port data
#[inline]
unsafe fn cli() {
    unsafe {
        core::arch::asm!("cli", options(nomem, nostack, preserves_flags));
    }
}

/// Enable interrupts (x86 STI)
#[inline]
unsafe fn sti() {
    unsafe {
        core::arch::asm!("sti", options(nomem, nostack, preserves_flags));
    }
}

/// Initialize PS/2 controller
/// -- TorqueJax: Reset controller state, disable mouse IRQ during init
pub fn init_controller() -> bool {
    serial_debug(b"[PS2] init_controller: flush\r\n");

    // Flush any pending data (bounded to prevent infinite loop if
    // mouse is actively streaming or IRQ handler keeps feeding data)
    for _ in 0..256 {
        if unsafe { inb(STATUS_PORT) } & status::OUTPUT_FULL == 0 {
            break;
        }
        let _ = unsafe { inb(DATA_PORT) };
    }

    serial_debug(b"[PS2] init_controller: read config\r\n");

    // Read current config
    send_command(cmd::READ_CONFIG);
    let config = read_data().unwrap_or(0);

    // Enable keyboard IRQ (bit 0), DISABLE mouse IRQ (bit 1) during init.
    // init_ps2_keyboard() enables mouse IRQ prematurely — we gate it here
    // to prevent IRQ 12 storms while mouse.init() talks to the hardware.
    // -- TorqueJax: Mouse IRQ re-enabled after mouse driver is wired up
    let init_config = (config | 0x01) & !0x02;
    send_command(cmd::WRITE_CONFIG);
    send_data(init_config);

    serial_debug(b"[PS2] init_controller: done\r\n");
    true
}

/// Global keyboard instance
static KEYBOARD: Mutex<Option<Arc<Ps2Keyboard>>> = Mutex::new(None);

/// Global mouse instance
static MOUSE: Mutex<Option<Arc<Ps2Mouse>>> = Mutex::new(None);

/// Console character callback type
/// Called with bytes to push to console input buffer (may be single char or ANSI sequence)
pub type ConsoleCharCallback = fn(&[u8]);

/// VT switch callback type
/// Called with VT number (0-5) when Alt+F1 through Alt+F6 is pressed
pub type VtSwitchCallback = fn(usize);

/// Global console callback
static mut CONSOLE_CALLBACK: Option<ConsoleCharCallback> = None;

/// Global VT switch callback
static mut VT_SWITCH_CALLBACK: Option<VtSwitchCallback> = None;

/// Set the console character callback
///
/// # Safety
/// Must be called during single-threaded initialization
pub unsafe fn set_console_callback(callback: ConsoleCharCallback) {
    unsafe {
        CONSOLE_CALLBACK = Some(callback);
    }
}

/// Set the VT switch callback
///
/// # Safety
/// Must be called during single-threaded initialization
pub unsafe fn set_vt_switch_callback(callback: VtSwitchCallback) {
    unsafe {
        VT_SWITCH_CALLBACK = Some(callback);
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
/// -- TorqueJax: Entire init runs with interrupts disabled. The arch-level
/// keyboard IRQ handler (even in drain mode) races with our polling read_data()
/// calls — it steals config bytes and mouse responses from the output buffer,
/// causing read_data() to return None and us to write corrupt config values.
/// cli/sti gates the whole sequence so port 0x60 data is exclusively ours.
pub fn init() -> bool {
    // -- TorqueJax: Kill all interrupts. No IRQ handler touches port 0x60
    // while we're configuring the controller. This prevents:
    //   1. Config byte stolen by keyboard IRQ → wrong config written
    //   2. Mouse ACK/self-test stolen by drain path → timeouts
    //   3. Spinlock deadlock if handler fires while we hold KEYBOARD/MOUSE
    unsafe { cli(); }

    serial_debug(b"[PS2] init: starting (IRQs disabled)\r\n");

    if !init_controller() {
        serial_debug(b"[PS2] init: controller FAILED\r\n");
        INIT_STATUS.store(1, Ordering::Relaxed);
        unsafe { sti(); }
        return false;
    }

    // Initialize keyboard
    serial_debug(b"[PS2] init: keyboard init\r\n");
    let keyboard = Arc::new(Ps2Keyboard::new());
    if keyboard.init() {
        serial_debug(b"[PS2] init: keyboard OK, registering\r\n");
        let id = input::register_device(keyboard.clone());
        keyboard.set_device_id(id as u8);
        *KEYBOARD.lock() = Some(keyboard);
        INIT_STATUS.store(3, Ordering::Relaxed);
    } else {
        serial_debug(b"[PS2] init: keyboard FAILED\r\n");
        INIT_STATUS.store(2, Ordering::Relaxed);
    }

    // Initialize mouse (IRQ 12 is disabled in controller config — no storm)
    serial_debug(b"[PS2] init: mouse init\r\n");
    let mouse = Arc::new(Ps2Mouse::new());
    if mouse.init() {
        serial_debug(b"[PS2] init: mouse OK, registering\r\n");
        let id = input::register_device(mouse.clone());
        mouse.set_device_id(id as u8);
        *MOUSE.lock() = Some(mouse);
        if INIT_STATUS.load(Ordering::Relaxed) == 3 {
            INIT_STATUS.store(5, Ordering::Relaxed);
        }
    } else if INIT_STATUS.load(Ordering::Relaxed) == 3 {
        serial_debug(b"[PS2] init: mouse FAILED\r\n");
        INIT_STATUS.store(4, Ordering::Relaxed);
    }

    // Re-enable mouse IRQ (bit 1) now that the driver is initialized
    // -- TorqueJax: Safe to unmask IRQ 12 now, MOUSE global is populated.
    // With interrupts disabled, this read_data() won't race with handlers.
    serial_debug(b"[PS2] init: re-enabling mouse IRQ\r\n");
    send_command(cmd::READ_CONFIG);
    let config = read_data().unwrap_or(0);
    send_command(cmd::WRITE_CONFIG);
    send_data(config | 0x02);

    serial_debug(b"[PS2] init: done\r\n");

    // -- TorqueJax: Do NOT call sti() here! Kernel enables interrupts globally later.
    // ps2::init() is called during early boot before arch::X86_64::enable_interrupts().
    // Calling sti() here causes deadlock when subsequent code tries to take locks
    // (like CONSOLE.lock in writeln!) before the kernel is ready for interrupts.
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
/// -- TorqueJax: DRAIN FIRST, lock second. The 8042 uses level-triggered
/// interrupts via IOAPIC — if port 0x60 isn't read before EOI, IRQ 1
/// re-fires immediately and we get an infinite interrupt storm.
/// See docs/agents/irq-handler-drain.md for the full rule.
pub fn handle_keyboard_irq() {
    // ALWAYS read the data byte BEFORE touching any locks.
    // This clears the output buffer and deasserts the IRQ line.
    let scancode = unsafe { inb(DATA_PORT) };

    KEYBOARD_IRQ_COUNT.fetch_add(1, Ordering::Relaxed);
    LAST_SCANCODE.store(scancode, Ordering::Relaxed);

    // [TRACE] Log first few keyboard IRQs — InputShade: Debug intermittent input
    let count = KEYBOARD_IRQ_COUNT.load(Ordering::Relaxed);
    if count <= 5 {
        unsafe {
            let msg = b"[KBD-IRQ] sc=0x";
            for &byte in msg.iter() {
                while inb(0x3FD) & 0x20 == 0 {}
                outb(0x3F8, byte);
            }
            let nibbles = [(scancode >> 4) & 0xF, scancode & 0xF];
            for nibble in nibbles {
                let hex_char = if nibble < 10 { b'0' + nibble } else { b'a' + nibble - 10 };
                outb(0x3F8, hex_char);
            }
            let msg2 = b"\r\n";
            for &byte in msg2.iter() {
                while inb(0x3FD) & 0x20 == 0 {}
                outb(0x3F8, byte);
            }
        }
    }

    // Store in debug log (try_lock — never spin in IRQ context)
    if count <= 50 {
        if let Some(mut log) = SCANCODE_LOG.try_lock() {
            let len = log.len();
            log[(count - 1) % len] = scancode;
        }
    }

    // try_lock prevents deadlock if init() holds the lock.
    // If lock is contended, the byte is already drained — just drop it.
    if let Some(guard) = KEYBOARD.try_lock() {
        if let Some(keyboard) = guard.as_ref() {
            keyboard.handle_scancode(scancode);
        }
    }
}

/// Get scancode log for debugging
pub fn get_scancode_log() -> [u8; 20] {
    *SCANCODE_LOG.lock()
}

/// Handle mouse interrupt (IRQ 12)
/// -- TorqueJax: DRAIN FIRST, lock second. Same rule as keyboard —
/// level-triggered IRQ 12 re-fires if port 0x60 isn't read before EOI.
/// See docs/agents/irq-handler-drain.md
pub fn handle_mouse_irq() {
    // ALWAYS read the data byte BEFORE touching any locks.
    let byte = unsafe { inb(DATA_PORT) };

    // [TRACE] Log first few mouse IRQs — InputShade: Debug intermittent input
    static MOUSE_IRQ_COUNT: AtomicUsize = AtomicUsize::new(0);
    let count = MOUSE_IRQ_COUNT.fetch_add(1, Ordering::Relaxed) + 1;
    if count <= 5 {
        unsafe {
            let msg = b"[MOUSE-IRQ] byte=0x";
            for &byte_ch in msg.iter() {
                while inb(0x3FD) & 0x20 == 0 {}
                outb(0x3F8, byte_ch);
            }
            let nibbles = [(byte >> 4) & 0xF, byte & 0xF];
            for nibble in nibbles {
                let hex_char = if nibble < 10 { b'0' + nibble } else { b'a' + nibble - 10 };
                outb(0x3F8, hex_char);
            }
            let msg2 = b"\r\n";
            for &byte_ch in msg2.iter() {
                while inb(0x3FD) & 0x20 == 0 {}
                outb(0x3F8, byte_ch);
            }
        }
    }

    // try_lock prevents deadlock if init() holds the lock.
    if let Some(guard) = MOUSE.try_lock() {
        if let Some(mouse) = guard.as_ref() {
            mouse.handle_byte(byte);
        }
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
