# EFFLUX Input Subsystem Specification

**Version:** 1.0
**Status:** Draft
**License:** MIT

---

## 0) Overview

EFFLUX provides a unified input subsystem supporting:

- Keyboard (PS/2, USB HID, virtio-input)
- Mouse/Trackpad (PS/2, USB HID, virtio-input)
- Touch screens
- Game controllers

**Key Features:**
- Scancode → Keycode → Character translation
- Multiple keyboard layouts
- Full Unicode/UTF-8 support
- Dead keys and compose sequences
- Input method support (for CJK, etc.)

---

## 1) Architecture

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                           Applications                                       │
│                                │                                             │
│                                ▼                                             │
│   ┌─────────────────────────────────────────────────────────────────────┐   │
│   │                        Input API                                     │   │
│   │    • Character input (UTF-8)                                        │   │
│   │    • Raw key events                                                 │   │
│   │    • Mouse/touch events                                             │   │
│   └───────────────────────────────┬─────────────────────────────────────┘   │
│                                   │                                          │
│   ┌───────────────────────────────┼───────────────────────────────────────┐ │
│   │                    Input Processing                                   │ │
│   │                                                                       │ │
│   │   ┌─────────────┐   ┌─────────────┐   ┌─────────────────────────┐   │ │
│   │   │   Keymap    │   │  Compose    │   │   Input Method          │   │ │
│   │   │   Layer     │   │  Sequences  │   │   Editor (IME)          │   │ │
│   │   │             │   │             │   │                         │   │ │
│   │   │ Scancode    │   │ Dead keys   │   │ CJK input, etc.        │   │ │
│   │   │ → Keycode   │   │ Accents     │   │                         │   │ │
│   │   │ → Char      │   │             │   │                         │   │ │
│   │   └──────┬──────┘   └──────┬──────┘   └───────────┬─────────────┘   │ │
│   └──────────┼─────────────────┼─────────────────────┼───────────────────┘ │
│              │                 │                     │                      │
│   ┌──────────┴─────────────────┴─────────────────────┴───────────────────┐ │
│   │                      Input Event Layer                                │ │
│   │                                                                       │ │
│   │   ┌─────────────────────────────────────────────────────────────┐   │ │
│   │   │  InputEvent { device, type, code, value, timestamp }        │   │ │
│   │   └─────────────────────────────────────────────────────────────┘   │ │
│   │                                                                       │ │
│   │   ┌──────────────┐  ┌──────────────┐  ┌──────────────────────┐     │ │
│   │   │ /dev/input/  │  │ /dev/input/  │  │ /dev/input/mice      │     │ │
│   │   │ event0       │  │ event1       │  │ (multiplexed)        │     │ │
│   │   └──────┬───────┘  └──────┬───────┘  └──────────┬───────────┘     │ │
│   └──────────┼─────────────────┼─────────────────────┼───────────────────┘ │
│              │                 │                     │                      │
│   ┌──────────┴─────────────────┴─────────────────────┴───────────────────┐ │
│   │                      Input Drivers                                    │ │
│   │                                                                       │ │
│   │   ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────────────┐   │ │
│   │   │   PS/2   │  │   USB    │  │  virtio- │  │   Serial         │   │ │
│   │   │ Keyboard │  │   HID    │  │  input   │  │   Console        │   │ │
│   │   │ & Mouse  │  │          │  │          │  │                  │   │ │
│   │   └────┬─────┘  └────┬─────┘  └────┬─────┘  └────────┬─────────┘   │ │
│   └────────┼─────────────┼─────────────┼─────────────────┼─────────────┘ │
│            │             │             │                 │               │
│            ▼             ▼             ▼                 ▼               │
│        Hardware       Hardware     Hypervisor      Serial Port           │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## 2) Input Event System

### 2.1 Event Types (Linux evdev compatible)

```rust
/// Raw input event (matches Linux struct input_event)
#[repr(C)]
pub struct InputEvent {
    pub time: Timeval,
    pub event_type: u16,
    pub code: u16,
    pub value: i32,
}

/// Event types
pub mod EventType {
    pub const EV_SYN: u16 = 0x00;       // Synchronization
    pub const EV_KEY: u16 = 0x01;       // Key/button
    pub const EV_REL: u16 = 0x02;       // Relative axis (mouse)
    pub const EV_ABS: u16 = 0x03;       // Absolute axis (touch)
    pub const EV_MSC: u16 = 0x04;       // Miscellaneous
    pub const EV_SW: u16 = 0x05;        // Switch
    pub const EV_LED: u16 = 0x11;       // LED
    pub const EV_SND: u16 = 0x12;       // Sound
    pub const EV_REP: u16 = 0x14;       // Auto-repeat
}

/// Key event values
pub const KEY_RELEASE: i32 = 0;
pub const KEY_PRESS: i32 = 1;
pub const KEY_REPEAT: i32 = 2;

/// Relative axis codes
pub mod RelAxis {
    pub const REL_X: u16 = 0x00;
    pub const REL_Y: u16 = 0x01;
    pub const REL_Z: u16 = 0x02;
    pub const REL_WHEEL: u16 = 0x08;
    pub const REL_HWHEEL: u16 = 0x06;
}

/// Absolute axis codes
pub mod AbsAxis {
    pub const ABS_X: u16 = 0x00;
    pub const ABS_Y: u16 = 0x01;
    pub const ABS_Z: u16 = 0x02;
    pub const ABS_PRESSURE: u16 = 0x18;
    pub const ABS_MT_SLOT: u16 = 0x2F;
    pub const ABS_MT_POSITION_X: u16 = 0x35;
    pub const ABS_MT_POSITION_Y: u16 = 0x36;
    pub const ABS_MT_TRACKING_ID: u16 = 0x39;
}
```

### 2.2 Input Device

```rust
pub struct InputDevice {
    pub name: String,
    pub phys: String,               // Physical path
    pub id: InputId,
    pub capabilities: InputCaps,
    pub event_queue: VecDeque<InputEvent>,
    pub grab_holder: Option<ProcessId>,
}

#[repr(C)]
pub struct InputId {
    pub bustype: u16,
    pub vendor: u16,
    pub product: u16,
    pub version: u16,
}

pub struct InputCaps {
    pub event_types: BitSet,        // Supported EV_* types
    pub keys: BitSet,               // Supported KEY_* codes
    pub rel_axes: BitSet,           // Supported REL_* codes
    pub abs_axes: BitSet,           // Supported ABS_* codes
    pub leds: BitSet,               // Supported LED_* codes
}
```

---

## 3) Keyboard Handling

### 3.1 Scancode Sets

Keyboards send **scancodes** which must be translated to **keycodes**.

```rust
/// Scancode sets (PS/2)
pub enum ScancodeSet {
    Set1,   // XT (legacy, used by most BIOSes)
    Set2,   // AT (default for PS/2)
    Set3,   // PS/2 (rarely used)
}

/// PS/2 Set 1 scancode to keycode translation
pub struct ScancodeTranslator {
    set: ScancodeSet,
    extended: bool,         // E0 prefix received
    release: bool,          // F0 (set 2) or 0x80 bit (set 1)
}

impl ScancodeTranslator {
    pub fn translate(&mut self, scancode: u8) -> Option<(Keycode, bool)> {
        match self.set {
            ScancodeSet::Set1 => self.translate_set1(scancode),
            ScancodeSet::Set2 => self.translate_set2(scancode),
            ScancodeSet::Set3 => self.translate_set3(scancode),
        }
    }

    fn translate_set1(&mut self, scancode: u8) -> Option<(Keycode, bool)> {
        // Handle extended prefix
        if scancode == 0xE0 {
            self.extended = true;
            return None;
        }
        if scancode == 0xE1 {
            // Pause key (special 6-byte sequence)
            return None;
        }

        let released = scancode & 0x80 != 0;
        let code = scancode & 0x7F;

        let keycode = if self.extended {
            self.extended = false;
            SCANCODE_SET1_EXTENDED[code as usize]
        } else {
            SCANCODE_SET1[code as usize]
        };

        if keycode != Keycode::None {
            Some((keycode, !released))
        } else {
            None
        }
    }
}
```

### 3.2 Keycodes (Hardware-Independent)

```rust
/// Hardware-independent key codes
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u16)]
pub enum Keycode {
    None = 0,

    // Letters
    A = 0x04, B = 0x05, C = 0x06, D = 0x07, E = 0x08, F = 0x09,
    G = 0x0A, H = 0x0B, I = 0x0C, J = 0x0D, K = 0x0E, L = 0x0F,
    M = 0x10, N = 0x11, O = 0x12, P = 0x13, Q = 0x14, R = 0x15,
    S = 0x16, T = 0x17, U = 0x18, V = 0x19, W = 0x1A, X = 0x1B,
    Y = 0x1C, Z = 0x1D,

    // Numbers
    Key1 = 0x1E, Key2 = 0x1F, Key3 = 0x20, Key4 = 0x21, Key5 = 0x22,
    Key6 = 0x23, Key7 = 0x24, Key8 = 0x25, Key9 = 0x26, Key0 = 0x27,

    // Function keys
    F1 = 0x3A, F2 = 0x3B, F3 = 0x3C, F4 = 0x3D, F5 = 0x3E, F6 = 0x3F,
    F7 = 0x40, F8 = 0x41, F9 = 0x42, F10 = 0x43, F11 = 0x44, F12 = 0x45,
    F13 = 0x68, F14 = 0x69, F15 = 0x6A, F16 = 0x6B, F17 = 0x6C, F18 = 0x6D,
    F19 = 0x6E, F20 = 0x6F, F21 = 0x70, F22 = 0x71, F23 = 0x72, F24 = 0x73,

    // Modifiers
    LeftCtrl = 0xE0, LeftShift = 0xE1, LeftAlt = 0xE2, LeftMeta = 0xE3,
    RightCtrl = 0xE4, RightShift = 0xE5, RightAlt = 0xE6, RightMeta = 0xE7,

    // Special keys
    Enter = 0x28, Escape = 0x29, Backspace = 0x2A, Tab = 0x2B,
    Space = 0x2C, Minus = 0x2D, Equal = 0x2E, LeftBracket = 0x2F,
    RightBracket = 0x30, Backslash = 0x31, Semicolon = 0x33,
    Apostrophe = 0x34, Grave = 0x35, Comma = 0x36, Period = 0x37,
    Slash = 0x38, CapsLock = 0x39,

    // Navigation
    PrintScreen = 0x46, ScrollLock = 0x47, Pause = 0x48,
    Insert = 0x49, Home = 0x4A, PageUp = 0x4B, Delete = 0x4C,
    End = 0x4D, PageDown = 0x4E, Right = 0x4F, Left = 0x50,
    Down = 0x51, Up = 0x52, NumLock = 0x53,

    // Keypad
    KpDivide = 0x54, KpMultiply = 0x55, KpMinus = 0x56, KpPlus = 0x57,
    KpEnter = 0x58, Kp1 = 0x59, Kp2 = 0x5A, Kp3 = 0x5B, Kp4 = 0x5C,
    Kp5 = 0x5D, Kp6 = 0x5E, Kp7 = 0x5F, Kp8 = 0x60, Kp9 = 0x61,
    Kp0 = 0x62, KpPeriod = 0x63, KpEqual = 0x67,

    // International keys
    IntlBackslash = 0x64,   // Between left shift and Z (ISO keyboards)
    IntlRo = 0x87,          // Japanese Ro key
    IntlYen = 0x89,         // Japanese Yen key
    IntlHash = 0x32,        // UK # key (between ' and Enter)

    // Media keys
    Mute = 0x7F, VolumeUp = 0x80, VolumeDown = 0x81,
    MediaPlayPause = 0xE8, MediaStop = 0xE9,
    MediaPrev = 0xEA, MediaNext = 0xEB,

    // ... more keys
}
```

### 3.3 Modifier State

```rust
bitflags! {
    pub struct Modifiers: u16 {
        const SHIFT     = 0x0001;
        const CTRL      = 0x0002;
        const ALT       = 0x0004;
        const META      = 0x0008;   // Windows/Super/Command
        const CAPS_LOCK = 0x0010;
        const NUM_LOCK  = 0x0020;
        const SCROLL_LOCK = 0x0040;
        const ALT_GR    = 0x0080;   // Right Alt on international keyboards
    }
}

pub struct ModifierState {
    pub pressed: Modifiers,     // Currently held
    pub locked: Modifiers,      // Toggled (Caps, Num, Scroll)
}

impl ModifierState {
    pub fn effective(&self) -> Modifiers {
        let mut mods = self.pressed;

        // Caps Lock affects shift state
        if self.locked.contains(Modifiers::CAPS_LOCK) {
            mods.toggle(Modifiers::SHIFT);
        }

        mods
    }

    pub fn update(&mut self, keycode: Keycode, pressed: bool) {
        match keycode {
            Keycode::LeftShift | Keycode::RightShift => {
                self.pressed.set(Modifiers::SHIFT, pressed);
            }
            Keycode::LeftCtrl | Keycode::RightCtrl => {
                self.pressed.set(Modifiers::CTRL, pressed);
            }
            Keycode::LeftAlt => {
                self.pressed.set(Modifiers::ALT, pressed);
            }
            Keycode::RightAlt => {
                // AltGr on international layouts
                self.pressed.set(Modifiers::ALT_GR, pressed);
            }
            Keycode::LeftMeta | Keycode::RightMeta => {
                self.pressed.set(Modifiers::META, pressed);
            }
            Keycode::CapsLock if pressed => {
                self.locked.toggle(Modifiers::CAPS_LOCK);
            }
            Keycode::NumLock if pressed => {
                self.locked.toggle(Modifiers::NUM_LOCK);
            }
            Keycode::ScrollLock if pressed => {
                self.locked.toggle(Modifiers::SCROLL_LOCK);
            }
            _ => {}
        }
    }
}
```

---

## 4) Keyboard Layouts

### 4.1 Layout Definition

```rust
/// Keyboard layout definition
pub struct KeyboardLayout {
    pub name: &'static str,         // "us", "uk", "de", "fr", etc.
    pub description: &'static str,  // "US English", "German", etc.
    pub map: &'static KeyMap,
}

/// Key mapping table
pub struct KeyMap {
    /// Normal key mappings (no modifiers)
    pub normal: [KeyOutput; 256],

    /// With Shift
    pub shift: [KeyOutput; 256],

    /// With AltGr (Right Alt)
    pub altgr: [KeyOutput; 256],

    /// With Shift+AltGr
    pub shift_altgr: [KeyOutput; 256],

    /// Dead key sequences
    pub dead_keys: &'static [DeadKeyDef],
}

/// Output for a key press
#[derive(Clone, Copy)]
pub enum KeyOutput {
    None,
    Char(char),                     // Single character
    String(&'static str),           // Multiple characters
    Dead(DeadKeyType),              // Dead key (accent)
    Function(KeyFunction),          // Special function
}

/// Dead key types (for accented characters)
#[derive(Clone, Copy)]
pub enum DeadKeyType {
    Acute,          // ´ (á, é, í, ó, ú)
    Grave,          // ` (à, è, ì, ò, ù)
    Circumflex,     // ^ (â, ê, î, ô, û)
    Tilde,          // ~ (ã, ñ, õ)
    Diaeresis,      // ¨ (ä, ë, ï, ö, ü)
    Cedilla,        // ¸ (ç)
    Ring,           // ° (å)
    Macron,         // ¯ (ā, ē, ī, ō, ū)
    Caron,          // ˇ (č, š, ž)
    Breve,          // ˘
    DotAbove,       // ˙
    DoubleAcute,    // ˝
    Ogonek,         // ˛
    Stroke,         // / (ø, đ)
}

/// Dead key resolution table
pub struct DeadKeyDef {
    pub dead_key: DeadKeyType,
    pub combinations: &'static [(char, char)],  // (base, result)
}

/// Special key functions
pub enum KeyFunction {
    ConsoleSwitch(u8),      // Switch to tty N
    Copy,
    Paste,
    Cut,
    Undo,
    Redo,
    // ... etc
}
```

### 4.2 Example Layouts

```rust
/// US English keyboard layout
pub static LAYOUT_US: KeyboardLayout = KeyboardLayout {
    name: "us",
    description: "US English",
    map: &KEYMAP_US,
};

static KEYMAP_US: KeyMap = KeyMap {
    normal: [
        // Index by Keycode
        // ...
        /* A */  KeyOutput::Char('a'),
        /* B */  KeyOutput::Char('b'),
        /* C */  KeyOutput::Char('c'),
        // ...
        /* 1 */  KeyOutput::Char('1'),
        /* 2 */  KeyOutput::Char('2'),
        // ...
        /* Grave */  KeyOutput::Char('`'),
        /* Minus */  KeyOutput::Char('-'),
        /* Equal */  KeyOutput::Char('='),
        // ...
    ],
    shift: [
        /* A */  KeyOutput::Char('A'),
        /* B */  KeyOutput::Char('B'),
        // ...
        /* 1 */  KeyOutput::Char('!'),
        /* 2 */  KeyOutput::Char('@'),
        /* 3 */  KeyOutput::Char('#'),
        /* 4 */  KeyOutput::Char('$'),
        /* 5 */  KeyOutput::Char('%'),
        /* 6 */  KeyOutput::Char('^'),
        /* 7 */  KeyOutput::Char('&'),
        /* 8 */  KeyOutput::Char('*'),
        /* 9 */  KeyOutput::Char('('),
        /* 0 */  KeyOutput::Char(')'),
        /* Grave */  KeyOutput::Char('~'),
        /* Minus */  KeyOutput::Char('_'),
        /* Equal */  KeyOutput::Char('+'),
        // ...
    ],
    altgr: [KeyOutput::None; 256],      // US doesn't use AltGr
    shift_altgr: [KeyOutput::None; 256],
    dead_keys: &[],
};

/// German keyboard layout
pub static LAYOUT_DE: KeyboardLayout = KeyboardLayout {
    name: "de",
    description: "German",
    map: &KEYMAP_DE,
};

static KEYMAP_DE: KeyMap = KeyMap {
    normal: [
        /* Y */  KeyOutput::Char('z'),  // Y and Z swapped
        /* Z */  KeyOutput::Char('y'),
        /* Semicolon */  KeyOutput::Char('ö'),
        /* Apostrophe */  KeyOutput::Char('ä'),
        /* LeftBracket */  KeyOutput::Char('ü'),
        /* Minus */  KeyOutput::Char('ß'),
        /* Grave */  KeyOutput::Dead(DeadKeyType::Circumflex),
        /* Equal */  KeyOutput::Dead(DeadKeyType::Acute),
        // ...
    ],
    shift: [
        /* 1 */  KeyOutput::Char('!'),
        /* 2 */  KeyOutput::Char('"'),  // Different from US
        /* 3 */  KeyOutput::Char('§'),
        /* 6 */  KeyOutput::Char('&'),  // Different from US
        /* 7 */  KeyOutput::Char('/'),
        // ...
    ],
    altgr: [
        /* Q */  KeyOutput::Char('@'),
        /* E */  KeyOutput::Char('€'),
        /* 2 */  KeyOutput::Char('²'),
        /* 3 */  KeyOutput::Char('³'),
        /* 7 */  KeyOutput::Char('{'),
        /* 8 */  KeyOutput::Char('['),
        /* 9 */  KeyOutput::Char(']'),
        /* 0 */  KeyOutput::Char('}'),
        /* Minus */  KeyOutput::Char('\\'),
        // ...
    ],
    shift_altgr: [KeyOutput::None; 256],
    dead_keys: &DEAD_KEYS_DE,
};

static DEAD_KEYS_DE: [DeadKeyDef; 3] = [
    DeadKeyDef {
        dead_key: DeadKeyType::Circumflex,
        combinations: &[
            ('a', 'â'), ('e', 'ê'), ('i', 'î'), ('o', 'ô'), ('u', 'û'),
            ('A', 'Â'), ('E', 'Ê'), ('I', 'Î'), ('O', 'Ô'), ('U', 'Û'),
            (' ', '^'),  // Space produces the dead key character itself
        ],
    },
    DeadKeyDef {
        dead_key: DeadKeyType::Acute,
        combinations: &[
            ('a', 'á'), ('e', 'é'), ('i', 'í'), ('o', 'ó'), ('u', 'ú'),
            ('A', 'Á'), ('E', 'É'), ('I', 'Í'), ('O', 'Ó'), ('U', 'Ú'),
            (' ', '´'),
        ],
    },
    DeadKeyDef {
        dead_key: DeadKeyType::Grave,
        combinations: &[
            ('a', 'à'), ('e', 'è'), ('i', 'ì'), ('o', 'ò'), ('u', 'ù'),
            ('A', 'À'), ('E', 'È'), ('I', 'Ì'), ('O', 'Ò'), ('U', 'Ù'),
            (' ', '`'),
        ],
    },
];
```

### 4.3 Supported Layouts

| Code | Name | Region |
|------|------|--------|
| `us` | US English | United States |
| `uk` | UK English | United Kingdom |
| `de` | German | Germany, Austria |
| `fr` | French (AZERTY) | France |
| `es` | Spanish | Spain |
| `it` | Italian | Italy |
| `pt` | Portuguese | Portugal |
| `br` | Brazilian Portuguese | Brazil |
| `ru` | Russian | Russia |
| `jp` | Japanese | Japan |
| `kr` | Korean | South Korea |
| `cn` | Chinese (Pinyin) | China |
| `dvorak` | Dvorak | Alternative |
| `colemak` | Colemak | Alternative |

---

## 5) Character Encoding & Unicode

### 5.1 UTF-8 Throughout

EFFLUX uses **UTF-8** as the native character encoding everywhere:
- Terminal I/O
- File names
- Text files
- API strings

```rust
/// Keyboard input produces UTF-8
pub struct KeyboardInput {
    /// Raw key event
    pub keycode: Keycode,
    pub pressed: bool,
    pub modifiers: Modifiers,

    /// Translated character(s) in UTF-8
    pub chars: Option<ArrayString<8>>,  // Up to 8 bytes for edge cases
}
```

### 5.2 Legacy Codepage Support

For compatibility with DOS programs and legacy files:

```rust
/// Codepage conversion
pub trait Codepage {
    fn name(&self) -> &'static str;
    fn to_unicode(&self, byte: u8) -> char;
    fn from_unicode(&self, c: char) -> Option<u8>;
}

/// Common codepages
pub struct Cp437;   // DOS/IBM PC (box drawing, symbols)
pub struct Cp850;   // DOS Western European
pub struct Cp1252;  // Windows Western European
pub struct Iso8859_1;  // Latin-1
pub struct Iso8859_15; // Latin-9 (with Euro)
pub struct Koi8R;   // Russian

impl Codepage for Cp437 {
    fn name(&self) -> &'static str { "CP437" }

    fn to_unicode(&self, byte: u8) -> char {
        if byte < 128 {
            byte as char
        } else {
            CP437_HIGH[byte as usize - 128]
        }
    }

    fn from_unicode(&self, c: char) -> Option<u8> {
        if (c as u32) < 128 {
            Some(c as u8)
        } else {
            CP437_HIGH.iter()
                .position(|&x| x == c)
                .map(|i| (i + 128) as u8)
        }
    }
}

/// CP437 high characters (128-255)
static CP437_HIGH: [char; 128] = [
    'Ç', 'ü', 'é', 'â', 'ä', 'à', 'å', 'ç', 'ê', 'ë', 'è', 'ï', 'î', 'ì', 'Ä', 'Å',
    'É', 'æ', 'Æ', 'ô', 'ö', 'ò', 'û', 'ù', 'ÿ', 'Ö', 'Ü', '¢', '£', '¥', '₧', 'ƒ',
    'á', 'í', 'ó', 'ú', 'ñ', 'Ñ', 'ª', 'º', '¿', '⌐', '¬', '½', '¼', '¡', '«', '»',
    '░', '▒', '▓', '│', '┤', '╡', '╢', '╖', '╕', '╣', '║', '╗', '╝', '╜', '╛', '┐',
    '└', '┴', '┬', '├', '─', '┼', '╞', '╟', '╚', '╔', '╩', '╦', '╠', '═', '╬', '╧',
    '╨', '╤', '╥', '╙', '╘', '╒', '╓', '╫', '╪', '┘', '┌', '█', '▄', '▌', '▐', '▀',
    'α', 'ß', 'Γ', 'π', 'Σ', 'σ', 'µ', 'τ', 'Φ', 'Θ', 'Ω', 'δ', '∞', 'φ', 'ε', '∩',
    '≡', '±', '≥', '≤', '⌠', '⌡', '÷', '≈', '°', '∙', '·', '√', 'ⁿ', '²', '■', ' ',
];
```

### 5.3 Unicode Normalization

```rust
/// Unicode normalization forms
pub enum NormalizationForm {
    Nfd,    // Canonical Decomposition
    Nfc,    // Canonical Decomposition + Composition
    Nfkd,   // Compatibility Decomposition
    Nfkc,   // Compatibility Decomposition + Composition
}

pub fn normalize(s: &str, form: NormalizationForm) -> String;
```

---

## 6) Compose Sequences

For entering special characters without dead keys:

```rust
/// Compose key sequences
pub struct ComposeTable {
    sequences: &'static [ComposeEntry],
}

pub struct ComposeEntry {
    pub sequence: &'static [Keycode],
    pub output: char,
}

/// Default compose sequences (X11-style)
pub static COMPOSE_TABLE: ComposeTable = ComposeTable {
    sequences: &[
        // Currency
        ComposeEntry { sequence: &[Keycode::C, Keycode::Equal], output: '€' },
        ComposeEntry { sequence: &[Keycode::L, Keycode::Minus], output: '£' },
        ComposeEntry { sequence: &[Keycode::Y, Keycode::Equal], output: '¥' },

        // Punctuation
        ComposeEntry { sequence: &[Keycode::Period, Keycode::Period], output: '…' },
        ComposeEntry { sequence: &[Keycode::Minus, Keycode::Minus], output: '—' },
        ComposeEntry { sequence: &[Keycode::Less, Keycode::Less], output: '«' },
        ComposeEntry { sequence: &[Keycode::Greater, Keycode::Greater], output: '»' },

        // Accented letters (alternative to dead keys)
        ComposeEntry { sequence: &[Keycode::Apostrophe, Keycode::A], output: 'á' },
        ComposeEntry { sequence: &[Keycode::Apostrophe, Keycode::E], output: 'é' },
        ComposeEntry { sequence: &[Keycode::Grave, Keycode::A], output: 'à' },
        ComposeEntry { sequence: &[Keycode::Circumflex, Keycode::A], output: 'â' },
        ComposeEntry { sequence: &[Keycode::Tilde, Keycode::N], output: 'ñ' },
        ComposeEntry { sequence: &[Keycode::Diaeresis, Keycode::U], output: 'ü' },

        // Symbols
        ComposeEntry { sequence: &[Keycode::O, Keycode::C], output: '©' },
        ComposeEntry { sequence: &[Keycode::O, Keycode::R], output: '®' },
        ComposeEntry { sequence: &[Keycode::T, Keycode::M], output: '™' },
        ComposeEntry { sequence: &[Keycode::Plus, Keycode::Minus], output: '±' },
        ComposeEntry { sequence: &[Keycode::Key1, Keycode::Key2], output: '½' },
        ComposeEntry { sequence: &[Keycode::Key1, Keycode::Key4], output: '¼' },

        // Greek (for math)
        ComposeEntry { sequence: &[Keycode::Asterisk, Keycode::A], output: 'α' },
        ComposeEntry { sequence: &[Keycode::Asterisk, Keycode::B], output: 'β' },
        ComposeEntry { sequence: &[Keycode::Asterisk, Keycode::G], output: 'γ' },
        ComposeEntry { sequence: &[Keycode::Asterisk, Keycode::D], output: 'δ' },
        ComposeEntry { sequence: &[Keycode::Asterisk, Keycode::P], output: 'π' },

        // ... many more
    ],
};
```

---

## 7) Input Method Support (IME)

For CJK and complex script input:

```rust
/// Input Method Editor interface
pub trait InputMethod {
    fn name(&self) -> &str;
    fn language(&self) -> &str;

    /// Process key event
    fn process_key(&mut self, key: Keycode, modifiers: Modifiers) -> ImeResult;

    /// Get preedit string (composing text)
    fn preedit(&self) -> Option<&str>;

    /// Get preedit cursor position
    fn preedit_cursor(&self) -> usize;

    /// Get candidate list
    fn candidates(&self) -> Option<&[String]>;

    /// Select candidate
    fn select_candidate(&mut self, index: usize) -> Option<String>;

    /// Reset state
    fn reset(&mut self);
}

pub enum ImeResult {
    /// Pass through to application
    Passthrough,

    /// Character(s) committed
    Commit(String),

    /// Preedit updated (show composing text)
    PreeditUpdate,

    /// Show candidate window
    ShowCandidates,

    /// Key consumed, no output
    Consumed,
}

/// Pinyin input method for Chinese
pub struct PinyinIme {
    preedit: String,
    candidates: Vec<String>,
    dictionary: PinyinDictionary,
}

/// Japanese input method
pub struct JapaneseIme {
    mode: JapaneseMode,
    preedit: String,
    candidates: Vec<String>,
}

pub enum JapaneseMode {
    Hiragana,
    Katakana,
    Romaji,
}
```

---

## 8) Input Drivers

### 8.1 PS/2 Keyboard Driver

```rust
pub struct Ps2Keyboard {
    command_port: u16,      // 0x64
    data_port: u16,         // 0x60
    translator: ScancodeTranslator,
    buffer: VecDeque<InputEvent>,
}

impl Ps2Keyboard {
    pub fn new() -> Self {
        Self {
            command_port: 0x64,
            data_port: 0x60,
            translator: ScancodeTranslator::new(ScancodeSet::Set1),
            buffer: VecDeque::new(),
        }
    }

    pub fn init(&mut self) -> Result<()> {
        // Disable scanning
        self.send_command(0xAD)?;

        // Flush buffer
        while self.status() & 0x01 != 0 {
            self.read_data();
        }

        // Self-test
        self.send_command(0xAA)?;
        if self.read_data() != 0x55 {
            return Err(Error::SelfTestFailed);
        }

        // Enable scanning
        self.send_command(0xAE)?;

        // Set scancode set 2 (translated to set 1 by controller)
        self.write_data(0xF0)?;
        self.write_data(0x02)?;

        // Enable interrupts
        self.send_command(0x20)?;  // Read config
        let config = self.read_data();
        self.send_command(0x60)?;  // Write config
        self.write_data(config | 0x01)?;  // Enable IRQ1

        Ok(())
    }

    /// IRQ1 handler
    pub fn handle_interrupt(&mut self) {
        let scancode = self.read_data();

        if let Some((keycode, pressed)) = self.translator.translate(scancode) {
            self.buffer.push_back(InputEvent {
                time: current_time(),
                event_type: EventType::EV_KEY,
                code: keycode as u16,
                value: if pressed { KEY_PRESS } else { KEY_RELEASE },
            });
        }
    }
}
```

### 8.2 PS/2 Mouse Driver

```rust
pub struct Ps2Mouse {
    command_port: u16,
    data_port: u16,
    packet: [u8; 4],
    packet_index: usize,
    has_scroll_wheel: bool,
}

impl Ps2Mouse {
    pub fn init(&mut self) -> Result<()> {
        // Enable auxiliary device
        self.aux_command(0xA8)?;

        // Get device ID
        self.aux_write(0xF2)?;
        let id = self.aux_read()?;

        // Try to enable scroll wheel (Intellimouse)
        self.set_sample_rate(200)?;
        self.set_sample_rate(100)?;
        self.set_sample_rate(80)?;

        self.aux_write(0xF2)?;
        let id = self.aux_read()?;
        self.has_scroll_wheel = id == 0x03;

        // Enable data reporting
        self.aux_write(0xF4)?;

        Ok(())
    }

    pub fn handle_interrupt(&mut self) -> Option<InputEvent> {
        let byte = self.aux_read().ok()?;
        self.packet[self.packet_index] = byte;
        self.packet_index += 1;

        let packet_size = if self.has_scroll_wheel { 4 } else { 3 };

        if self.packet_index >= packet_size {
            self.packet_index = 0;

            let buttons = self.packet[0];
            let dx = self.packet[1] as i8 as i32;
            let dy = -(self.packet[2] as i8 as i32);  // Invert Y
            let dz = if self.has_scroll_wheel {
                self.packet[3] as i8 as i32
            } else {
                0
            };

            // Generate events...
        }

        None
    }
}
```

### 8.3 USB HID Driver

```rust
pub struct UsbHidKeyboard {
    device: UsbDevice,
    endpoint: u8,
    last_report: [u8; 8],
}

impl UsbHidKeyboard {
    /// Standard HID keyboard report format:
    /// Byte 0: Modifier keys (Ctrl, Shift, Alt, etc.)
    /// Byte 1: Reserved
    /// Bytes 2-7: Up to 6 simultaneous key codes

    pub fn process_report(&mut self, report: &[u8; 8]) -> Vec<InputEvent> {
        let mut events = Vec::new();

        // Check modifier changes
        let mod_diff = report[0] ^ self.last_report[0];
        for i in 0..8 {
            if mod_diff & (1 << i) != 0 {
                let pressed = report[0] & (1 << i) != 0;
                let keycode = USB_MODIFIER_KEYS[i];
                events.push(InputEvent::key(keycode, pressed));
            }
        }

        // Check key releases
        for &old_key in &self.last_report[2..8] {
            if old_key != 0 && !report[2..8].contains(&old_key) {
                if let Some(keycode) = usb_to_keycode(old_key) {
                    events.push(InputEvent::key(keycode, false));
                }
            }
        }

        // Check key presses
        for &new_key in &report[2..8] {
            if new_key != 0 && !self.last_report[2..8].contains(&new_key) {
                if let Some(keycode) = usb_to_keycode(new_key) {
                    events.push(InputEvent::key(keycode, true));
                }
            }
        }

        self.last_report = *report;
        events
    }
}
```

### 8.4 virtio-input Driver (for VMs)

```rust
pub struct VirtioInput {
    device: VirtioDevice,
    event_queue: VirtQueue,
    status_queue: VirtQueue,
    device_type: VirtioInputType,
}

pub enum VirtioInputType {
    Keyboard,
    Mouse,
    Tablet,
    Touchscreen,
}

impl VirtioInput {
    pub fn poll(&mut self) -> Option<InputEvent> {
        if let Some(buffer) = self.event_queue.pop_used() {
            let event: VirtioInputEvent = buffer.read();
            Some(InputEvent {
                time: current_time(),
                event_type: event.type_,
                code: event.code,
                value: event.value as i32,
            })
        } else {
            None
        }
    }
}

#[repr(C)]
struct VirtioInputEvent {
    type_: u16,
    code: u16,
    value: u32,
}
```

---

## 9) Configuration

### 9.1 Keyboard Configuration

```toml
# /etc/efflux/keyboard.conf

[keyboard]
# Default layout
layout = "us"

# Additional layouts (switch with hotkey)
layouts = ["us", "de"]

# Layout switch hotkey
switch_hotkey = "alt+shift"

# Compose key
compose_key = "right_alt"  # or "menu", "scroll_lock", etc.

# Key repeat
repeat_delay = 500      # ms before repeat starts
repeat_rate = 30        # repeats per second

# Caps Lock behavior
# "capslock" = normal, "ctrl" = act as Ctrl, "escape" = act as Escape
caps_lock_behavior = "capslock"

# Num Lock on boot
num_lock_default = true

[mouse]
# Acceleration
acceleration = 1.0
threshold = 4

# Natural scrolling (reversed)
natural_scrolling = false

# Scroll speed
scroll_speed = 3
```

### 9.2 /etc/efflux/keyboard.d/

Additional layout files:
```
/etc/efflux/keyboard.d/
├── custom.layout       # User-defined layout
└── programmer.layout   # Programming-optimized layout
```

---

## 10) devfs Entries

```
/dev/input/
├── event0          # First input device (keyboard)
├── event1          # Second input device (mouse)
├── event2          # etc.
├── mice            # Multiplexed mouse (all mice combined)
├── mouse0          # First mouse
├── mouse1          # Second mouse
└── by-id/
    ├── usb-Logitech_USB_Keyboard-event-kbd
    └── usb-Logitech_USB_Mouse-event-mouse
```

---

## 11) Exit Criteria

### Phase 13a: Basic Keyboard
- [ ] PS/2 keyboard driver works
- [ ] Scancodes translated to keycodes
- [ ] US layout produces correct characters
- [ ] Modifier keys work

### Phase 13b: Layouts & Unicode
- [ ] Multiple layouts supported
- [ ] Dead keys work
- [ ] Compose sequences work
- [ ] UTF-8 output correct

### Phase 13c: Mouse & Touch
- [ ] PS/2 mouse works
- [ ] Scroll wheel works
- [ ] Touch events work (virtio)

### Phase 13d: USB HID
- [ ] USB keyboard works
- [ ] USB mouse works
- [ ] Hot-plug works

---

*End of EFFLUX Input Subsystem Specification*
