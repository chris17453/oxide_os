//! Keyboard Keymap
//!
//! Maps scan codes to keycodes.

use crate::keycodes::*;

/// Scan code set 1 (XT) to keycode mapping
pub static SCANCODE_SET1: [u16; 128] = [
    KEY_RESERVED,   // 0x00
    KEY_ESC,        // 0x01
    KEY_1,          // 0x02
    KEY_2,          // 0x03
    KEY_3,          // 0x04
    KEY_4,          // 0x05
    KEY_5,          // 0x06
    KEY_6,          // 0x07
    KEY_7,          // 0x08
    KEY_8,          // 0x09
    KEY_9,          // 0x0A
    KEY_0,          // 0x0B
    KEY_MINUS,      // 0x0C
    KEY_EQUAL,      // 0x0D
    KEY_BACKSPACE,  // 0x0E
    KEY_TAB,        // 0x0F
    KEY_Q,          // 0x10
    KEY_W,          // 0x11
    KEY_E,          // 0x12
    KEY_R,          // 0x13
    KEY_T,          // 0x14
    KEY_Y,          // 0x15
    KEY_U,          // 0x16
    KEY_I,          // 0x17
    KEY_O,          // 0x18
    KEY_P,          // 0x19
    KEY_LEFTBRACE,  // 0x1A
    KEY_RIGHTBRACE, // 0x1B
    KEY_ENTER,      // 0x1C
    KEY_LEFTCTRL,   // 0x1D
    KEY_A,          // 0x1E
    KEY_S,          // 0x1F
    KEY_D,          // 0x20
    KEY_F,          // 0x21
    KEY_G,          // 0x22
    KEY_H,          // 0x23
    KEY_J,          // 0x24
    KEY_K,          // 0x25
    KEY_L,          // 0x26
    KEY_SEMICOLON,  // 0x27
    KEY_APOSTROPHE, // 0x28
    KEY_GRAVE,      // 0x29
    KEY_LEFTSHIFT,  // 0x2A
    KEY_BACKSLASH,  // 0x2B
    KEY_Z,          // 0x2C
    KEY_X,          // 0x2D
    KEY_C,          // 0x2E
    KEY_V,          // 0x2F
    KEY_B,          // 0x30
    KEY_N,          // 0x31
    KEY_M,          // 0x32
    KEY_COMMA,      // 0x33
    KEY_DOT,        // 0x34
    KEY_SLASH,      // 0x35
    KEY_RIGHTSHIFT, // 0x36
    KEY_KPASTERISK, // 0x37
    KEY_LEFTALT,    // 0x38
    KEY_SPACE,      // 0x39
    KEY_CAPSLOCK,   // 0x3A
    KEY_F1,         // 0x3B
    KEY_F2,         // 0x3C
    KEY_F3,         // 0x3D
    KEY_F4,         // 0x3E
    KEY_F5,         // 0x3F
    KEY_F6,         // 0x40
    KEY_F7,         // 0x41
    KEY_F8,         // 0x42
    KEY_F9,         // 0x43
    KEY_F10,        // 0x44
    KEY_NUMLOCK,    // 0x45
    KEY_SCROLLLOCK, // 0x46
    KEY_KP7,        // 0x47
    KEY_KP8,        // 0x48
    KEY_KP9,        // 0x49
    KEY_KPMINUS,    // 0x4A
    KEY_KP4,        // 0x4B
    KEY_KP5,        // 0x4C
    KEY_KP6,        // 0x4D
    KEY_KPPLUS,     // 0x4E
    KEY_KP1,        // 0x4F
    KEY_KP2,        // 0x50
    KEY_KP3,        // 0x51
    KEY_KP0,        // 0x52
    KEY_KPDOT,      // 0x53
    KEY_RESERVED,   // 0x54
    KEY_RESERVED,   // 0x55
    KEY_RESERVED,   // 0x56
    KEY_F11,        // 0x57
    KEY_F12,        // 0x58
    KEY_RESERVED,   // 0x59
    KEY_RESERVED,   // 0x5A
    KEY_RESERVED,   // 0x5B
    KEY_RESERVED,   // 0x5C
    KEY_RESERVED,   // 0x5D
    KEY_RESERVED,   // 0x5E
    KEY_RESERVED,   // 0x5F
    KEY_RESERVED,   // 0x60
    KEY_RESERVED,   // 0x61
    KEY_RESERVED,   // 0x62
    KEY_RESERVED,   // 0x63
    KEY_RESERVED,   // 0x64
    KEY_RESERVED,   // 0x65
    KEY_RESERVED,   // 0x66
    KEY_RESERVED,   // 0x67
    KEY_RESERVED,   // 0x68
    KEY_RESERVED,   // 0x69
    KEY_RESERVED,   // 0x6A
    KEY_RESERVED,   // 0x6B
    KEY_RESERVED,   // 0x6C
    KEY_RESERVED,   // 0x6D
    KEY_RESERVED,   // 0x6E
    KEY_RESERVED,   // 0x6F
    KEY_RESERVED,   // 0x70
    KEY_RESERVED,   // 0x71
    KEY_RESERVED,   // 0x72
    KEY_RESERVED,   // 0x73
    KEY_RESERVED,   // 0x74
    KEY_RESERVED,   // 0x75
    KEY_RESERVED,   // 0x76
    KEY_RESERVED,   // 0x77
    KEY_RESERVED,   // 0x78
    KEY_RESERVED,   // 0x79
    KEY_RESERVED,   // 0x7A
    KEY_RESERVED,   // 0x7B
    KEY_RESERVED,   // 0x7C
    KEY_RESERVED,   // 0x7D
    KEY_RESERVED,   // 0x7E
    KEY_RESERVED,   // 0x7F
];

/// Extended scan codes (0xE0 prefix) to keycode mapping
pub static SCANCODE_SET1_EXT: [u16; 128] = [
    KEY_RESERVED,  // 0x00
    KEY_RESERVED,  // 0x01
    KEY_RESERVED,  // 0x02
    KEY_RESERVED,  // 0x03
    KEY_RESERVED,  // 0x04
    KEY_RESERVED,  // 0x05
    KEY_RESERVED,  // 0x06
    KEY_RESERVED,  // 0x07
    KEY_RESERVED,  // 0x08
    KEY_RESERVED,  // 0x09
    KEY_RESERVED,  // 0x0A
    KEY_RESERVED,  // 0x0B
    KEY_RESERVED,  // 0x0C
    KEY_RESERVED,  // 0x0D
    KEY_RESERVED,  // 0x0E
    KEY_RESERVED,  // 0x0F
    KEY_RESERVED,  // 0x10
    KEY_RESERVED,  // 0x11
    KEY_RESERVED,  // 0x12
    KEY_RESERVED,  // 0x13
    KEY_RESERVED,  // 0x14
    KEY_RESERVED,  // 0x15
    KEY_RESERVED,  // 0x16
    KEY_RESERVED,  // 0x17
    KEY_RESERVED,  // 0x18
    KEY_RESERVED,  // 0x19
    KEY_RESERVED,  // 0x1A
    KEY_RESERVED,  // 0x1B
    KEY_KPENTER,   // 0x1C
    KEY_RIGHTCTRL, // 0x1D
    KEY_RESERVED,  // 0x1E
    KEY_RESERVED,  // 0x1F
    KEY_RESERVED,  // 0x20
    KEY_RESERVED,  // 0x21
    KEY_RESERVED,  // 0x22
    KEY_RESERVED,  // 0x23
    KEY_RESERVED,  // 0x24
    KEY_RESERVED,  // 0x25
    KEY_RESERVED,  // 0x26
    KEY_RESERVED,  // 0x27
    KEY_RESERVED,  // 0x28
    KEY_RESERVED,  // 0x29
    KEY_RESERVED,  // 0x2A
    KEY_RESERVED,  // 0x2B
    KEY_RESERVED,  // 0x2C
    KEY_RESERVED,  // 0x2D
    KEY_RESERVED,  // 0x2E
    KEY_RESERVED,  // 0x2F
    KEY_RESERVED,  // 0x30
    KEY_RESERVED,  // 0x31
    KEY_RESERVED,  // 0x32
    KEY_RESERVED,  // 0x33
    KEY_RESERVED,  // 0x34
    KEY_KPSLASH,   // 0x35
    KEY_RESERVED,  // 0x36
    KEY_SYSRQ,     // 0x37
    KEY_RIGHTALT,  // 0x38
    KEY_RESERVED,  // 0x39
    KEY_RESERVED,  // 0x3A
    KEY_RESERVED,  // 0x3B
    KEY_RESERVED,  // 0x3C
    KEY_RESERVED,  // 0x3D
    KEY_RESERVED,  // 0x3E
    KEY_RESERVED,  // 0x3F
    KEY_RESERVED,  // 0x40
    KEY_RESERVED,  // 0x41
    KEY_RESERVED,  // 0x42
    KEY_RESERVED,  // 0x43
    KEY_RESERVED,  // 0x44
    KEY_RESERVED,  // 0x45
    KEY_RESERVED,  // 0x46
    KEY_HOME,      // 0x47
    KEY_UP,        // 0x48
    KEY_PAGEUP,    // 0x49
    KEY_RESERVED,  // 0x4A
    KEY_LEFT,      // 0x4B
    KEY_RESERVED,  // 0x4C
    KEY_RIGHT,     // 0x4D
    KEY_RESERVED,  // 0x4E
    KEY_END,       // 0x4F
    KEY_DOWN,      // 0x50
    KEY_PAGEDOWN,  // 0x51
    KEY_INSERT,    // 0x52
    KEY_DELETE,    // 0x53
    KEY_RESERVED,  // 0x54
    KEY_RESERVED,  // 0x55
    KEY_RESERVED,  // 0x56
    KEY_RESERVED,  // 0x57
    KEY_RESERVED,  // 0x58
    KEY_RESERVED,  // 0x59
    KEY_RESERVED,  // 0x5A
    KEY_LEFTMETA,  // 0x5B
    KEY_RIGHTMETA, // 0x5C
    KEY_COMPOSE,   // 0x5D
    KEY_POWER,     // 0x5E
    KEY_RESERVED,  // 0x5F
    KEY_RESERVED,  // 0x60
    KEY_RESERVED,  // 0x61
    KEY_RESERVED,  // 0x62
    KEY_RESERVED,  // 0x63
    KEY_RESERVED,  // 0x64
    KEY_RESERVED,  // 0x65
    KEY_RESERVED,  // 0x66
    KEY_RESERVED,  // 0x67
    KEY_RESERVED,  // 0x68
    KEY_RESERVED,  // 0x69
    KEY_RESERVED,  // 0x6A
    KEY_RESERVED,  // 0x6B
    KEY_RESERVED,  // 0x6C
    KEY_RESERVED,  // 0x6D
    KEY_RESERVED,  // 0x6E
    KEY_RESERVED,  // 0x6F
    KEY_RESERVED,  // 0x70
    KEY_RESERVED,  // 0x71
    KEY_RESERVED,  // 0x72
    KEY_RESERVED,  // 0x73
    KEY_RESERVED,  // 0x74
    KEY_RESERVED,  // 0x75
    KEY_RESERVED,  // 0x76
    KEY_RESERVED,  // 0x77
    KEY_RESERVED,  // 0x78
    KEY_RESERVED,  // 0x79
    KEY_RESERVED,  // 0x7A
    KEY_RESERVED,  // 0x7B
    KEY_RESERVED,  // 0x7C
    KEY_RESERVED,  // 0x7D
    KEY_RESERVED,  // 0x7E
    KEY_RESERVED,  // 0x7F
];

/// Keymap for translating scan codes to keycodes
pub struct Keymap {
    /// Extended key flag (E0 prefix)
    extended: bool,
    /// Pause key sequence buffer (E1 prefix - 6 bytes total)
    pause_buffer: [u8; 6],
    /// Number of bytes collected in pause sequence
    pause_index: u8,
}

impl Keymap {
    /// Create a new keymap
    pub const fn new() -> Self {
        Keymap {
            extended: false,
            pause_buffer: [0; 6],
            pause_index: 0,
        }
    }

    /// Process a scan code byte
    /// Returns Some(keycode, pressed) or None if more bytes needed
    pub fn process_scancode(&mut self, scancode: u8) -> Option<(u16, bool)> {
        // Extended prefix (E0)
        if scancode == 0xE0 {
            self.extended = true;
            return None;
        }

        // 🔥 PAUSE KEY SUPPORT (Priority #12 Fix) 🔥
        // Pause/Break key uses E1 prefix followed by: 1D 45 E1 9D C5
        // This is a make-only key (no separate break code)
        if scancode == 0xE1 || self.pause_index > 0 {
            if scancode == 0xE1 {
                // Start of pause sequence
                self.pause_buffer[0] = 0xE1;
                self.pause_index = 1;
                return None;
            } else {
                // Collecting pause sequence
                self.pause_buffer[self.pause_index as usize] = scancode;
                self.pause_index += 1;

                // Check if we have the full sequence: E1 1D 45 E1 9D C5
                if self.pause_index == 6 {
                    let valid = self.pause_buffer[0] == 0xE1
                        && self.pause_buffer[1] == 0x1D
                        && self.pause_buffer[2] == 0x45
                        && self.pause_buffer[3] == 0xE1
                        && self.pause_buffer[4] == 0x9D
                        && self.pause_buffer[5] == 0xC5;

                    self.pause_index = 0; // Reset for next time

                    if valid {
                        return Some((KEY_PAUSE, true)); // Pause is make-only
                    }
                }

                return None; // Still collecting
            }
        }

        let pressed = scancode & 0x80 == 0;
        let code = scancode & 0x7F;

        let keycode = if self.extended {
            self.extended = false;
            SCANCODE_SET1_EXT
                .get(code as usize)
                .copied()
                .unwrap_or(KEY_RESERVED)
        } else {
            SCANCODE_SET1
                .get(code as usize)
                .copied()
                .unwrap_or(KEY_RESERVED)
        };

        if keycode != KEY_RESERVED {
            Some((keycode, pressed))
        } else {
            None
        }
    }
}

impl Default for Keymap {
    fn default() -> Self {
        Self::new()
    }
}

/// Convert keycode to ASCII character (US layout, lowercase)
/// This is the legacy function - prefer keycode_to_char with a layout
pub fn keycode_to_ascii(keycode: u16, shift: bool) -> Option<char> {
    // Use the default (US) layout, no capslock (legacy compat)
    crate::layouts::default_layout().get_char(keycode, shift, false, false)
}

/// Convert keycode to character using the specified layout
pub fn keycode_to_char(
    keycode: u16,
    layout: &crate::layouts::KeyboardLayout,
    shift: bool,
    altgr: bool,
    capslock: bool,
) -> Option<char> {
    layout.get_char(keycode, shift, altgr, capslock)
}

/// Global current keyboard layout
static CURRENT_LAYOUT: spin::Mutex<&'static crate::layouts::KeyboardLayout> =
    spin::Mutex::new(&crate::layouts::LAYOUT_US);

/// Set the current keyboard layout by name
pub fn set_layout(name: &str) -> bool {
    if let Some(layout) = crate::layouts::get_layout(name) {
        *CURRENT_LAYOUT.lock() = layout;
        true
    } else {
        false
    }
}

/// Get the current keyboard layout
pub fn current_layout() -> &'static crate::layouts::KeyboardLayout {
    *CURRENT_LAYOUT.lock()
}

/// Convert keycode to character using the current layout
///
/// **CAPS LOCK SUPPORT (Finally!):**
/// Pass the capslock state from the keyboard driver.
/// Caps Lock will uppercase letters but NOT affect numbers/symbols.
pub fn keycode_to_char_current(keycode: u16, shift: bool, altgr: bool, capslock: bool) -> Option<char> {
    current_layout().get_char(keycode, shift, altgr, capslock)
}
