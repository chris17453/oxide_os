//! Console Keyboard Handler — shared keyboard→VT bridge
//!
//! — GraveShift: Linux has kbd.c for this. Every input driver that produces key
//! events needs the same pipeline: modifier tracking → Ctrl codes → ANSI escape
//! sequences → keymap character lookup → push to VT layer. This module is that
//! pipeline. Both PS/2 and VirtIO call into it instead of duplicating the logic.
//!
//! Flow: driver gets (keycode, pressed) → calls `process_key_event()` → this
//! module tracks modifiers, converts to bytes, pushes to console callback.
//! LED state changes are returned so drivers can update their hardware LEDs.

use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

/// — GraveShift: raw serial write for ISR-context debug.
/// We can't call into any allocating/locking code from here.
#[inline]
unsafe fn serial_trace(msg: &[u8]) {
    for &b in msg {
        loop {
            let status: u8;
            core::arch::asm!("in al, dx", out("al") status, in("dx") 0x3FDu16, options(nomem, nostack, preserves_flags));
            if status & 0x20 != 0 { break; }
        }
        core::arch::asm!("out dx, al", in("al") b, in("dx") 0x3F8u16, options(nomem, nostack, preserves_flags));
    }
}

/// Debug counter for process_key_event calls
static KEY_EVENT_COUNT: AtomicUsize = AtomicUsize::new(0);

/// LED state change returned from `process_key_event`
#[derive(Debug, Clone, Copy)]
pub struct LedState {
    pub capslock: bool,
    pub numlock: bool,
    pub scrolllock: bool,
}

/// Result of processing a key event
pub enum KeyAction {
    /// No console output (modifier-only key, or key release)
    None,
    /// LED toggle occurred — driver should update hardware LEDs
    LedChange(LedState),
}

// — GraveShift: global modifier state. Shared across all keyboard drivers
// because there's one console and one user. AtomicBool because PS/2 calls
// from IRQ context — no locks allowed there.
static SHIFT: AtomicBool = AtomicBool::new(false);
static CTRL: AtomicBool = AtomicBool::new(false);
static ALT: AtomicBool = AtomicBool::new(false);
static ALTGR: AtomicBool = AtomicBool::new(false);
static CAPSLOCK: AtomicBool = AtomicBool::new(false);
static NUMLOCK: AtomicBool = AtomicBool::new(true);
static SCROLLLOCK: AtomicBool = AtomicBool::new(false);

/// Console push callback — pushes bytes to VT layer
static mut CONSOLE_CALLBACK: Option<fn(&[u8])> = None;

/// VT switch callback — for Alt+F1..F6
static mut VT_SWITCH_CALLBACK: Option<fn(usize)> = None;

/// Set the console callback for keyboard → VT bridge
///
/// # Safety
/// Must be called during single-threaded initialization
pub unsafe fn set_console_callback(callback: fn(&[u8])) {
    unsafe { CONSOLE_CALLBACK = Some(callback); }
}

/// Set the VT switch callback for Alt+F1..F6
///
/// # Safety
/// Must be called during single-threaded initialization
pub unsafe fn set_vt_switch_callback(callback: fn(usize)) {
    unsafe { VT_SWITCH_CALLBACK = Some(callback); }
}

/// Get current LED state (for drivers that need to sync hardware LEDs on init)
pub fn led_state() -> LedState {
    LedState {
        capslock: CAPSLOCK.load(Ordering::SeqCst),
        numlock: NUMLOCK.load(Ordering::SeqCst),
        scrolllock: SCROLLLOCK.load(Ordering::SeqCst),
    }
}

/// Get current modifier state (for external queries)
pub fn shift_pressed() -> bool { SHIFT.load(Ordering::SeqCst) }
pub fn ctrl_pressed() -> bool { CTRL.load(Ordering::SeqCst) }
pub fn alt_pressed() -> bool { ALT.load(Ordering::SeqCst) }

#[inline]
fn push_to_console(data: &[u8]) {
    unsafe {
        if let Some(cb) = CONSOLE_CALLBACK {
            // — GraveShift: trace what we push to console
            let count = KEY_EVENT_COUNT.load(Ordering::Relaxed);
            if count <= 20 {
                serial_trace(b"[KBD-PUSH] ");
                for &b in data {
                    let nibbles = [(b >> 4) & 0xF, b & 0xF];
                    for n in nibbles {
                        let c = if n < 10 { b'0' + n } else { b'a' + n - 10 };
                        serial_trace(&[c]);
                    }
                    serial_trace(b" ");
                }
                serial_trace(b"\r\n");
            }
            cb(data);
        } else {
            let count = KEY_EVENT_COUNT.load(Ordering::Relaxed);
            if count <= 5 {
                serial_trace(b"[KBD-PUSH] NO CALLBACK!\r\n");
            }
        }
    }
}

/// Process a keyboard event and push converted bytes to the console
///
/// — GraveShift: this is the central keyboard handler, like Linux's kbd_event().
/// Takes a Linux keycode and press/release state. Tracks modifiers, generates
/// control codes, ANSI escapes, and characters. Pushes everything to the VT
/// layer via the console callback.
///
/// Returns `KeyAction::LedChange` when a lock key toggles so the driver can
/// update hardware LEDs (PS/2 has LED commands, VirtIO has status queue).
pub fn process_key_event(keycode: u16, pressed: bool) -> KeyAction {
    // — GraveShift: trace first N events to serial for debugging
    let count = KEY_EVENT_COUNT.fetch_add(1, Ordering::Relaxed) + 1;
    if count <= 20 {
        unsafe {
            serial_trace(b"[KBD-PROC] kc=0x");
            let nibbles = [(keycode >> 12) as u8 & 0xF, (keycode >> 8) as u8 & 0xF,
                           (keycode >> 4) as u8 & 0xF, keycode as u8 & 0xF];
            for n in nibbles {
                let c = if n < 10 { b'0' + n } else { b'a' + n - 10 };
                serial_trace(&[c]);
            }
            serial_trace(if pressed { b" DOWN\r\n" } else { b" UP\r\n" });
        }
    }

    // — GraveShift: modifier keys update state but don't generate console output
    match keycode {
        crate::KEY_LEFTSHIFT | crate::KEY_RIGHTSHIFT => {
            SHIFT.store(pressed, Ordering::SeqCst);
            return KeyAction::None;
        }
        crate::KEY_LEFTCTRL | crate::KEY_RIGHTCTRL => {
            CTRL.store(pressed, Ordering::SeqCst);
            return KeyAction::None;
        }
        crate::KEY_LEFTALT => {
            ALT.store(pressed, Ordering::SeqCst);
            return KeyAction::None;
        }
        crate::KEY_RIGHTALT => {
            ALTGR.store(pressed, Ordering::SeqCst);
            return KeyAction::None;
        }
        crate::KEY_CAPSLOCK => {
            if pressed {
                let new = !CAPSLOCK.load(Ordering::SeqCst);
                CAPSLOCK.store(new, Ordering::SeqCst);
                return KeyAction::LedChange(led_state());
            }
            return KeyAction::None;
        }
        crate::KEY_NUMLOCK => {
            if pressed {
                let new = !NUMLOCK.load(Ordering::SeqCst);
                NUMLOCK.store(new, Ordering::SeqCst);
                return KeyAction::LedChange(led_state());
            }
            return KeyAction::None;
        }
        crate::KEY_SCROLLLOCK => {
            if pressed {
                let new = !SCROLLLOCK.load(Ordering::SeqCst);
                SCROLLLOCK.store(new, Ordering::SeqCst);
                return KeyAction::LedChange(led_state());
            }
            return KeyAction::None;
        }
        _ => {}
    }

    // Only generate console input on press, not release
    if !pressed {
        return KeyAction::None;
    }

    let shift = SHIFT.load(Ordering::SeqCst);
    let ctrl = CTRL.load(Ordering::SeqCst);
    let alt = ALT.load(Ordering::SeqCst);
    let altgr = ALTGR.load(Ordering::SeqCst);
    let capslock = CAPSLOCK.load(Ordering::SeqCst);
    let numlock = NUMLOCK.load(Ordering::SeqCst);

    // — GraveShift: Ctrl+key → control codes (0x01-0x1A). The terminal's nervous system.
    if ctrl && !altgr {
        let ctrl_char = match keycode {
            crate::KEY_A => Some(0x01u8),
            crate::KEY_B => Some(0x02),
            crate::KEY_C => Some(0x03), // SIGINT
            crate::KEY_D => Some(0x04), // EOF
            crate::KEY_E => Some(0x05),
            crate::KEY_F => Some(0x06),
            crate::KEY_G => Some(0x07),
            crate::KEY_H => Some(0x08),
            crate::KEY_I => Some(0x09),
            crate::KEY_J => Some(0x0A),
            crate::KEY_K => Some(0x0B),
            crate::KEY_L => Some(0x0C),
            crate::KEY_M => Some(0x0D),
            crate::KEY_N => Some(0x0E),
            crate::KEY_O => Some(0x0F),
            crate::KEY_P => Some(0x10),
            crate::KEY_Q => Some(0x11),
            crate::KEY_R => Some(0x12),
            crate::KEY_S => Some(0x13),
            crate::KEY_T => Some(0x14),
            crate::KEY_U => Some(0x15),
            crate::KEY_V => Some(0x16),
            crate::KEY_W => Some(0x17),
            crate::KEY_X => Some(0x18),
            crate::KEY_Y => Some(0x19),
            crate::KEY_Z => Some(0x1A),
            _ => None,
        };
        if let Some(ch) = ctrl_char {
            push_to_console(&[ch]);
            return KeyAction::None;
        }
    }

    // — GraveShift: numpad navigation mode (Num Lock off)
    if !numlock {
        let nav: Option<&[u8]> = match keycode {
            crate::KEY_KP8 => Some(b"\x1b[A"),
            crate::KEY_KP2 => Some(b"\x1b[B"),
            crate::KEY_KP6 => Some(b"\x1b[C"),
            crate::KEY_KP4 => Some(b"\x1b[D"),
            crate::KEY_KP7 => Some(b"\x1b[H"),
            crate::KEY_KP1 => Some(b"\x1b[F"),
            crate::KEY_KP9 => Some(b"\x1b[5~"),
            crate::KEY_KP3 => Some(b"\x1b[6~"),
            crate::KEY_KP0 => Some(b"\x1b[2~"),
            crate::KEY_KPDOT => Some(b"\x1b[3~"),
            crate::KEY_KP5 => Some(b"\x1b[E"),
            _ => None,
        };
        if let Some(seq) = nav {
            push_to_console(seq);
            return KeyAction::None;
        }
    }

    // Numpad numeric mode (Num Lock on)
    if numlock {
        let kp: Option<u8> = match keycode {
            crate::KEY_KP0 => Some(b'0'),
            crate::KEY_KP1 => Some(b'1'),
            crate::KEY_KP2 => Some(b'2'),
            crate::KEY_KP3 => Some(b'3'),
            crate::KEY_KP4 => Some(b'4'),
            crate::KEY_KP5 => Some(b'5'),
            crate::KEY_KP6 => Some(b'6'),
            crate::KEY_KP7 => Some(b'7'),
            crate::KEY_KP8 => Some(b'8'),
            crate::KEY_KP9 => Some(b'9'),
            crate::KEY_KPDOT => Some(b'.'),
            crate::KEY_KPENTER => Some(b'\n'),
            crate::KEY_KPPLUS => Some(b'+'),
            crate::KEY_KPMINUS => Some(b'-'),
            crate::KEY_KPASTERISK => Some(b'*'),
            crate::KEY_KPSLASH => Some(b'/'),
            _ => None,
        };
        if let Some(ch) = kp {
            push_to_console(&[ch]);
            return KeyAction::None;
        }
    }

    // — GraveShift: Alt+F1..F6 → VT switch. The only way to escape your mistakes.
    if alt || altgr {
        let vt = match keycode {
            crate::KEY_F1 => Some(0usize),
            crate::KEY_F2 => Some(1),
            crate::KEY_F3 => Some(2),
            crate::KEY_F4 => Some(3),
            crate::KEY_F5 => Some(4),
            crate::KEY_F6 => Some(5),
            _ => None,
        };
        if let Some(vt_num) = vt {
            unsafe {
                if let Some(cb) = VT_SWITCH_CALLBACK {
                    cb(vt_num);
                }
            }
            return KeyAction::None;
        }
    }

    // — GraveShift: special keys → ANSI escape sequences. xterm-256color compatible.
    let ansi: Option<&[u8]> = match keycode {
        crate::KEY_UP => Some(b"\x1b[A"),
        crate::KEY_DOWN => Some(b"\x1b[B"),
        crate::KEY_RIGHT => Some(b"\x1b[C"),
        crate::KEY_LEFT => Some(b"\x1b[D"),
        crate::KEY_HOME => Some(b"\x1b[H"),
        crate::KEY_END => Some(b"\x1b[F"),
        crate::KEY_INSERT => Some(b"\x1b[2~"),
        crate::KEY_DELETE => Some(b"\x1b[3~"),
        crate::KEY_PAGEUP => Some(b"\x1b[5~"),
        crate::KEY_PAGEDOWN => Some(b"\x1b[6~"),
        crate::KEY_F1 => Some(b"\x1bOP"),
        crate::KEY_F2 => Some(b"\x1bOQ"),
        crate::KEY_F3 => Some(b"\x1bOR"),
        crate::KEY_F4 => Some(b"\x1bOS"),
        crate::KEY_F5 => Some(b"\x1b[15~"),
        crate::KEY_F6 => Some(b"\x1b[17~"),
        crate::KEY_F7 => Some(b"\x1b[18~"),
        crate::KEY_F8 => Some(b"\x1b[19~"),
        crate::KEY_F9 => Some(b"\x1b[20~"),
        crate::KEY_F10 => Some(b"\x1b[21~"),
        crate::KEY_F11 => Some(b"\x1b[23~"),
        crate::KEY_F12 => Some(b"\x1b[24~"),
        crate::KEY_F13 => Some(b"\x1b[25~"),
        crate::KEY_F14 => Some(b"\x1b[26~"),
        crate::KEY_F15 => Some(b"\x1b[28~"),
        crate::KEY_F16 => Some(b"\x1b[29~"),
        crate::KEY_F17 => Some(b"\x1b[31~"),
        crate::KEY_F18 => Some(b"\x1b[32~"),
        crate::KEY_F19 => Some(b"\x1b[33~"),
        crate::KEY_F20 => Some(b"\x1b[34~"),
        crate::KEY_F21 => Some(b"\x1b[23;2~"),
        crate::KEY_F22 => Some(b"\x1b[24;2~"),
        crate::KEY_F23 => Some(b"\x1b[25;2~"),
        crate::KEY_F24 => Some(b"\x1b[26;2~"),
        crate::KEY_ESC => Some(b"\x1b"),
        _ => None,
    };
    if let Some(seq) = ansi {
        push_to_console(seq);
        return KeyAction::None;
    }

    // — GraveShift: regular key → character via keymap. Respects layout + capslock.
    if let Some(ch) = crate::keymap::keycode_to_char_current(keycode, shift, altgr, capslock) {
        let mut buf = [0u8; 4];
        let s = ch.encode_utf8(&mut buf);
        push_to_console(s.as_bytes());
    }

    KeyAction::None
}
