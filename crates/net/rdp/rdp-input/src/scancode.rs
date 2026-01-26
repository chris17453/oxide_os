//! Scancode Translation
//!
//! Translates RDP scancodes (IBM PC AT Set 1 compatible) to Linux evdev keycodes.
//! RDP uses scancodes from the keyboard's perspective, which need to be
//! mapped to the logical keycodes used by the input subsystem.

use input::keycodes::*;

/// Scancode translator
pub struct ScancodeTranslator {
    // Future: could store keyboard layout info here
}

impl ScancodeTranslator {
    /// Create a new scancode translator
    pub fn new() -> Self {
        Self {}
    }

    /// Translate RDP scancode to evdev keycode
    ///
    /// `extended` indicates the E0 prefix (right ctrl, numpad enter, etc.)
    /// `extended1` indicates the E1 prefix (Pause/Break)
    pub fn translate(&self, scancode: u16, extended: bool, extended1: bool) -> u16 {
        if extended1 {
            // E1 prefix - only used for Pause/Break
            if scancode == 0x1D {
                return KEY_PAUSE;
            }
            return 0;
        }

        if extended {
            rdp_scancode_to_evdev_extended(scancode)
        } else {
            rdp_scancode_to_evdev(scancode)
        }
    }
}

impl Default for ScancodeTranslator {
    fn default() -> Self {
        Self::new()
    }
}

/// Convert RDP scancode (non-extended) to evdev keycode
pub fn rdp_scancode_to_evdev(scancode: u16) -> u16 {
    // RDP scancodes are IBM PC AT Set 1 compatible
    // Most map directly to evdev keycodes
    match scancode {
        0x01 => KEY_ESC,
        0x02 => KEY_1,
        0x03 => KEY_2,
        0x04 => KEY_3,
        0x05 => KEY_4,
        0x06 => KEY_5,
        0x07 => KEY_6,
        0x08 => KEY_7,
        0x09 => KEY_8,
        0x0A => KEY_9,
        0x0B => KEY_0,
        0x0C => KEY_MINUS,
        0x0D => KEY_EQUAL,
        0x0E => KEY_BACKSPACE,
        0x0F => KEY_TAB,
        0x10 => KEY_Q,
        0x11 => KEY_W,
        0x12 => KEY_E,
        0x13 => KEY_R,
        0x14 => KEY_T,
        0x15 => KEY_Y,
        0x16 => KEY_U,
        0x17 => KEY_I,
        0x18 => KEY_O,
        0x19 => KEY_P,
        0x1A => KEY_LEFTBRACE,
        0x1B => KEY_RIGHTBRACE,
        0x1C => KEY_ENTER,
        0x1D => KEY_LEFTCTRL,
        0x1E => KEY_A,
        0x1F => KEY_S,
        0x20 => KEY_D,
        0x21 => KEY_F,
        0x22 => KEY_G,
        0x23 => KEY_H,
        0x24 => KEY_J,
        0x25 => KEY_K,
        0x26 => KEY_L,
        0x27 => KEY_SEMICOLON,
        0x28 => KEY_APOSTROPHE,
        0x29 => KEY_GRAVE,
        0x2A => KEY_LEFTSHIFT,
        0x2B => KEY_BACKSLASH,
        0x2C => KEY_Z,
        0x2D => KEY_X,
        0x2E => KEY_C,
        0x2F => KEY_V,
        0x30 => KEY_B,
        0x31 => KEY_N,
        0x32 => KEY_M,
        0x33 => KEY_COMMA,
        0x34 => KEY_DOT,
        0x35 => KEY_SLASH,
        0x36 => KEY_RIGHTSHIFT,
        0x37 => KEY_KPASTERISK,
        0x38 => KEY_LEFTALT,
        0x39 => KEY_SPACE,
        0x3A => KEY_CAPSLOCK,
        0x3B => KEY_F1,
        0x3C => KEY_F2,
        0x3D => KEY_F3,
        0x3E => KEY_F4,
        0x3F => KEY_F5,
        0x40 => KEY_F6,
        0x41 => KEY_F7,
        0x42 => KEY_F8,
        0x43 => KEY_F9,
        0x44 => KEY_F10,
        0x45 => KEY_NUMLOCK,
        0x46 => KEY_SCROLLLOCK,
        0x47 => KEY_KP7,
        0x48 => KEY_KP8,
        0x49 => KEY_KP9,
        0x4A => KEY_KPMINUS,
        0x4B => KEY_KP4,
        0x4C => KEY_KP5,
        0x4D => KEY_KP6,
        0x4E => KEY_KPPLUS,
        0x4F => KEY_KP1,
        0x50 => KEY_KP2,
        0x51 => KEY_KP3,
        0x52 => KEY_KP0,
        0x53 => KEY_KPDOT,
        0x57 => KEY_F11,
        0x58 => KEY_F12,
        // Function keys F13-F24
        0x64 => KEY_F13,
        0x65 => KEY_F14,
        0x66 => KEY_F15,
        0x67 => KEY_F16,
        0x68 => KEY_F17,
        0x69 => KEY_F18,
        0x6A => KEY_F19,
        0x6B => KEY_F20,
        0x6C => KEY_F21,
        0x6D => KEY_F22,
        0x6E => KEY_F23,
        0x76 => KEY_F24,
        _ => 0, // Unknown scancode
    }
}

/// Convert RDP scancode (extended / E0 prefix) to evdev keycode
fn rdp_scancode_to_evdev_extended(scancode: u16) -> u16 {
    match scancode {
        0x1C => KEY_KPENTER,   // Numpad Enter
        0x1D => KEY_RIGHTCTRL, // Right Control
        0x35 => KEY_KPSLASH,   // Numpad /
        0x37 => KEY_SYSRQ,     // Print Screen
        0x38 => KEY_RIGHTALT,  // Right Alt
        0x47 => KEY_HOME,      // Home
        0x48 => KEY_UP,        // Up Arrow
        0x49 => KEY_PAGEUP,    // Page Up
        0x4B => KEY_LEFT,      // Left Arrow
        0x4D => KEY_RIGHT,     // Right Arrow
        0x4F => KEY_END,       // End
        0x50 => KEY_DOWN,      // Down Arrow
        0x51 => KEY_PAGEDOWN,  // Page Down
        0x52 => KEY_INSERT,    // Insert
        0x53 => KEY_DELETE,    // Delete
        0x5B => KEY_LEFTMETA,  // Left Windows
        0x5C => KEY_RIGHTMETA, // Right Windows
        0x5D => KEY_COMPOSE,   // Menu/Compose
        // Media keys
        0x20 => KEY_MUTE,       // Mute
        0x2E => KEY_VOLUMEDOWN, // Volume Down
        0x30 => KEY_VOLUMEUP,   // Volume Up
        0x5E => KEY_POWER,      // Power
        _ => 0,                 // Unknown extended scancode
    }
}

/// Convert evdev keycode back to RDP scancode
/// Used for keyboard LED sync
pub fn evdev_to_rdp_scancode(keycode: u16) -> (u16, bool) {
    // Returns (scancode, is_extended)
    match keycode {
        KEY_ESC => (0x01, false),
        KEY_1 => (0x02, false),
        KEY_2 => (0x03, false),
        KEY_3 => (0x04, false),
        KEY_4 => (0x05, false),
        KEY_5 => (0x06, false),
        KEY_6 => (0x07, false),
        KEY_7 => (0x08, false),
        KEY_8 => (0x09, false),
        KEY_9 => (0x0A, false),
        KEY_0 => (0x0B, false),
        KEY_ENTER => (0x1C, false),
        KEY_SPACE => (0x39, false),
        KEY_RIGHTCTRL => (0x1D, true),
        KEY_RIGHTALT => (0x38, true),
        KEY_HOME => (0x47, true),
        KEY_UP => (0x48, true),
        KEY_PAGEUP => (0x49, true),
        KEY_LEFT => (0x4B, true),
        KEY_RIGHT => (0x4D, true),
        KEY_END => (0x4F, true),
        KEY_DOWN => (0x50, true),
        KEY_PAGEDOWN => (0x51, true),
        KEY_INSERT => (0x52, true),
        KEY_DELETE => (0x53, true),
        KEY_LEFTMETA => (0x5B, true),
        KEY_RIGHTMETA => (0x5C, true),
        _ => (0, false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_scancodes() {
        let translator = ScancodeTranslator::new();

        // Test some basic keys
        assert_eq!(translator.translate(0x01, false, false), KEY_ESC);
        assert_eq!(translator.translate(0x1C, false, false), KEY_ENTER);
        assert_eq!(translator.translate(0x39, false, false), KEY_SPACE);
    }

    #[test]
    fn test_extended_scancodes() {
        let translator = ScancodeTranslator::new();

        // Test extended keys
        assert_eq!(translator.translate(0x48, true, false), KEY_UP);
        assert_eq!(translator.translate(0x50, true, false), KEY_DOWN);
        assert_eq!(translator.translate(0x4B, true, false), KEY_LEFT);
        assert_eq!(translator.translate(0x4D, true, false), KEY_RIGHT);
    }

    #[test]
    fn test_pause_key() {
        let translator = ScancodeTranslator::new();

        // Pause/Break uses E1 prefix
        assert_eq!(translator.translate(0x1D, false, true), KEY_PAUSE);
    }
}
