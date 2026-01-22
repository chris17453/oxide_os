//! Keyboard Layouts
//!
//! Provides keyboard layout definitions for different regions/languages.
//! Each layout maps keycodes to characters with support for:
//! - Normal (unmodified)
//! - Shift
//! - AltGr (Right Alt)
//! - Shift+AltGr

/// A keyboard layout definition
#[derive(Clone, Copy)]
pub struct KeyboardLayout {
    /// Layout name (e.g., "us", "uk", "de")
    pub name: &'static str,
    /// Layout description
    pub description: &'static str,
    /// Normal character mappings (keycode -> char, 0 means no mapping)
    pub normal: &'static [char; 128],
    /// Shift character mappings
    pub shift: &'static [char; 128],
    /// AltGr character mappings (for European layouts)
    pub altgr: &'static [char; 128],
    /// Shift+AltGr character mappings
    pub shift_altgr: &'static [char; 128],
}

impl KeyboardLayout {
    /// Get character for keycode with given modifiers
    pub fn get_char(&self, keycode: u16, shift: bool, altgr: bool) -> Option<char> {
        if keycode >= 128 {
            return None;
        }
        let idx = keycode as usize;
        let ch = match (shift, altgr) {
            (false, false) => self.normal[idx],
            (true, false) => self.shift[idx],
            (false, true) => self.altgr[idx],
            (true, true) => self.shift_altgr[idx],
        };
        if ch == '\0' {
            None
        } else {
            Some(ch)
        }
    }
}

// Key indices (matching keycodes from keycodes.rs)
const KEY_ESC: usize = 1;
const KEY_1: usize = 2;
const KEY_2: usize = 3;
const KEY_3: usize = 4;
const KEY_4: usize = 5;
const KEY_5: usize = 6;
const KEY_6: usize = 7;
const KEY_7: usize = 8;
const KEY_8: usize = 9;
const KEY_9: usize = 10;
const KEY_0: usize = 11;
const KEY_MINUS: usize = 12;
const KEY_EQUAL: usize = 13;
const KEY_BACKSPACE: usize = 14;
const KEY_TAB: usize = 15;
const KEY_Q: usize = 16;
const KEY_W: usize = 17;
const KEY_E: usize = 18;
const KEY_R: usize = 19;
const KEY_T: usize = 20;
const KEY_Y: usize = 21;
const KEY_U: usize = 22;
const KEY_I: usize = 23;
const KEY_O: usize = 24;
const KEY_P: usize = 25;
const KEY_LEFTBRACE: usize = 26;
const KEY_RIGHTBRACE: usize = 27;
const KEY_ENTER: usize = 28;
const KEY_A: usize = 30;
const KEY_S: usize = 31;
const KEY_D: usize = 32;
const KEY_F: usize = 33;
const KEY_G: usize = 34;
const KEY_H: usize = 35;
const KEY_J: usize = 36;
const KEY_K: usize = 37;
const KEY_L: usize = 38;
const KEY_SEMICOLON: usize = 39;
const KEY_APOSTROPHE: usize = 40;
const KEY_GRAVE: usize = 41;
const KEY_BACKSLASH: usize = 43;
const KEY_Z: usize = 44;
const KEY_X: usize = 45;
const KEY_C: usize = 46;
const KEY_V: usize = 47;
const KEY_B: usize = 48;
const KEY_N: usize = 49;
const KEY_M: usize = 50;
const KEY_COMMA: usize = 51;
const KEY_DOT: usize = 52;
const KEY_SLASH: usize = 53;
const KEY_KPASTERISK: usize = 55;
const KEY_SPACE: usize = 57;
const KEY_KP7: usize = 71;
const KEY_KP8: usize = 72;
const KEY_KP9: usize = 73;
const KEY_KPMINUS: usize = 74;
const KEY_KP4: usize = 75;
const KEY_KP5: usize = 76;
const KEY_KP6: usize = 77;
const KEY_KPPLUS: usize = 78;
const KEY_KP1: usize = 79;
const KEY_KP2: usize = 80;
const KEY_KP3: usize = 81;
const KEY_KP0: usize = 82;
const KEY_KPDOT: usize = 83;
const KEY_KPENTER: usize = 96;
const KEY_KPSLASH: usize = 98;

/// Helper macro to create a character map array
macro_rules! charmap {
    ($($idx:expr => $ch:expr),* $(,)?) => {{
        let mut map = ['\0'; 128];
        $(map[$idx] = $ch;)*
        map
    }};
}

// ============================================================================
// US QWERTY Layout
// ============================================================================

static US_NORMAL: [char; 128] = charmap! {
    KEY_ESC => '\x1b',
    KEY_1 => '1', KEY_2 => '2', KEY_3 => '3', KEY_4 => '4', KEY_5 => '5',
    KEY_6 => '6', KEY_7 => '7', KEY_8 => '8', KEY_9 => '9', KEY_0 => '0',
    KEY_MINUS => '-', KEY_EQUAL => '=',
    KEY_BACKSPACE => '\x08', KEY_TAB => '\t',
    KEY_Q => 'q', KEY_W => 'w', KEY_E => 'e', KEY_R => 'r', KEY_T => 't',
    KEY_Y => 'y', KEY_U => 'u', KEY_I => 'i', KEY_O => 'o', KEY_P => 'p',
    KEY_LEFTBRACE => '[', KEY_RIGHTBRACE => ']',
    KEY_ENTER => '\n',
    KEY_A => 'a', KEY_S => 's', KEY_D => 'd', KEY_F => 'f', KEY_G => 'g',
    KEY_H => 'h', KEY_J => 'j', KEY_K => 'k', KEY_L => 'l',
    KEY_SEMICOLON => ';', KEY_APOSTROPHE => '\'', KEY_GRAVE => '`',
    KEY_BACKSLASH => '\\',
    KEY_Z => 'z', KEY_X => 'x', KEY_C => 'c', KEY_V => 'v', KEY_B => 'b',
    KEY_N => 'n', KEY_M => 'm',
    KEY_COMMA => ',', KEY_DOT => '.', KEY_SLASH => '/',
    KEY_KPASTERISK => '*',
    KEY_SPACE => ' ',
    KEY_KP7 => '7', KEY_KP8 => '8', KEY_KP9 => '9',
    KEY_KPMINUS => '-',
    KEY_KP4 => '4', KEY_KP5 => '5', KEY_KP6 => '6',
    KEY_KPPLUS => '+',
    KEY_KP1 => '1', KEY_KP2 => '2', KEY_KP3 => '3',
    KEY_KP0 => '0', KEY_KPDOT => '.',
    KEY_KPENTER => '\n',
    KEY_KPSLASH => '/',
};

static US_SHIFT: [char; 128] = charmap! {
    KEY_ESC => '\x1b',
    KEY_1 => '!', KEY_2 => '@', KEY_3 => '#', KEY_4 => '$', KEY_5 => '%',
    KEY_6 => '^', KEY_7 => '&', KEY_8 => '*', KEY_9 => '(', KEY_0 => ')',
    KEY_MINUS => '_', KEY_EQUAL => '+',
    KEY_BACKSPACE => '\x08', KEY_TAB => '\t',
    KEY_Q => 'Q', KEY_W => 'W', KEY_E => 'E', KEY_R => 'R', KEY_T => 'T',
    KEY_Y => 'Y', KEY_U => 'U', KEY_I => 'I', KEY_O => 'O', KEY_P => 'P',
    KEY_LEFTBRACE => '{', KEY_RIGHTBRACE => '}',
    KEY_ENTER => '\n',
    KEY_A => 'A', KEY_S => 'S', KEY_D => 'D', KEY_F => 'F', KEY_G => 'G',
    KEY_H => 'H', KEY_J => 'J', KEY_K => 'K', KEY_L => 'L',
    KEY_SEMICOLON => ':', KEY_APOSTROPHE => '"', KEY_GRAVE => '~',
    KEY_BACKSLASH => '|',
    KEY_Z => 'Z', KEY_X => 'X', KEY_C => 'C', KEY_V => 'V', KEY_B => 'B',
    KEY_N => 'N', KEY_M => 'M',
    KEY_COMMA => '<', KEY_DOT => '>', KEY_SLASH => '?',
    KEY_KPASTERISK => '*',
    KEY_SPACE => ' ',
    KEY_KP7 => '7', KEY_KP8 => '8', KEY_KP9 => '9',
    KEY_KPMINUS => '-',
    KEY_KP4 => '4', KEY_KP5 => '5', KEY_KP6 => '6',
    KEY_KPPLUS => '+',
    KEY_KP1 => '1', KEY_KP2 => '2', KEY_KP3 => '3',
    KEY_KP0 => '0', KEY_KPDOT => '.',
    KEY_KPENTER => '\n',
    KEY_KPSLASH => '/',
};

static US_ALTGR: [char; 128] = ['\0'; 128];  // US has no AltGr mappings
static US_SHIFT_ALTGR: [char; 128] = ['\0'; 128];

/// US QWERTY keyboard layout
pub static LAYOUT_US: KeyboardLayout = KeyboardLayout {
    name: "us",
    description: "US QWERTY",
    normal: &US_NORMAL,
    shift: &US_SHIFT,
    altgr: &US_ALTGR,
    shift_altgr: &US_SHIFT_ALTGR,
};

// ============================================================================
// UK QWERTY Layout
// ============================================================================

static UK_NORMAL: [char; 128] = charmap! {
    KEY_ESC => '\x1b',
    KEY_1 => '1', KEY_2 => '2', KEY_3 => '3', KEY_4 => '4', KEY_5 => '5',
    KEY_6 => '6', KEY_7 => '7', KEY_8 => '8', KEY_9 => '9', KEY_0 => '0',
    KEY_MINUS => '-', KEY_EQUAL => '=',
    KEY_BACKSPACE => '\x08', KEY_TAB => '\t',
    KEY_Q => 'q', KEY_W => 'w', KEY_E => 'e', KEY_R => 'r', KEY_T => 't',
    KEY_Y => 'y', KEY_U => 'u', KEY_I => 'i', KEY_O => 'o', KEY_P => 'p',
    KEY_LEFTBRACE => '[', KEY_RIGHTBRACE => ']',
    KEY_ENTER => '\n',
    KEY_A => 'a', KEY_S => 's', KEY_D => 'd', KEY_F => 'f', KEY_G => 'g',
    KEY_H => 'h', KEY_J => 'j', KEY_K => 'k', KEY_L => 'l',
    KEY_SEMICOLON => ';', KEY_APOSTROPHE => '\'', KEY_GRAVE => '`',
    KEY_BACKSLASH => '#',  // UK uses # here
    KEY_Z => 'z', KEY_X => 'x', KEY_C => 'c', KEY_V => 'v', KEY_B => 'b',
    KEY_N => 'n', KEY_M => 'm',
    KEY_COMMA => ',', KEY_DOT => '.', KEY_SLASH => '/',
    KEY_SPACE => ' ',
};

static UK_SHIFT: [char; 128] = charmap! {
    KEY_ESC => '\x1b',
    KEY_1 => '!', KEY_2 => '"', KEY_3 => '£', KEY_4 => '$', KEY_5 => '%',
    KEY_6 => '^', KEY_7 => '&', KEY_8 => '*', KEY_9 => '(', KEY_0 => ')',
    KEY_MINUS => '_', KEY_EQUAL => '+',
    KEY_BACKSPACE => '\x08', KEY_TAB => '\t',
    KEY_Q => 'Q', KEY_W => 'W', KEY_E => 'E', KEY_R => 'R', KEY_T => 'T',
    KEY_Y => 'Y', KEY_U => 'U', KEY_I => 'I', KEY_O => 'O', KEY_P => 'P',
    KEY_LEFTBRACE => '{', KEY_RIGHTBRACE => '}',
    KEY_ENTER => '\n',
    KEY_A => 'A', KEY_S => 'S', KEY_D => 'D', KEY_F => 'F', KEY_G => 'G',
    KEY_H => 'H', KEY_J => 'J', KEY_K => 'K', KEY_L => 'L',
    KEY_SEMICOLON => ':', KEY_APOSTROPHE => '@', KEY_GRAVE => '¬',
    KEY_BACKSLASH => '~',
    KEY_Z => 'Z', KEY_X => 'X', KEY_C => 'C', KEY_V => 'V', KEY_B => 'B',
    KEY_N => 'N', KEY_M => 'M',
    KEY_COMMA => '<', KEY_DOT => '>', KEY_SLASH => '?',
    KEY_SPACE => ' ',
};

static UK_ALTGR: [char; 128] = charmap! {
    KEY_4 => '€',  // Euro symbol on AltGr+4
};

static UK_SHIFT_ALTGR: [char; 128] = ['\0'; 128];

/// UK QWERTY keyboard layout
pub static LAYOUT_UK: KeyboardLayout = KeyboardLayout {
    name: "uk",
    description: "UK QWERTY",
    normal: &UK_NORMAL,
    shift: &UK_SHIFT,
    altgr: &UK_ALTGR,
    shift_altgr: &UK_SHIFT_ALTGR,
};

// ============================================================================
// German QWERTZ Layout
// ============================================================================

static DE_NORMAL: [char; 128] = charmap! {
    KEY_ESC => '\x1b',
    KEY_1 => '1', KEY_2 => '2', KEY_3 => '3', KEY_4 => '4', KEY_5 => '5',
    KEY_6 => '6', KEY_7 => '7', KEY_8 => '8', KEY_9 => '9', KEY_0 => '0',
    KEY_MINUS => 'ß', KEY_EQUAL => '´',
    KEY_BACKSPACE => '\x08', KEY_TAB => '\t',
    KEY_Q => 'q', KEY_W => 'w', KEY_E => 'e', KEY_R => 'r', KEY_T => 't',
    KEY_Y => 'z', KEY_U => 'u', KEY_I => 'i', KEY_O => 'o', KEY_P => 'p',  // Z and Y swapped
    KEY_LEFTBRACE => 'ü', KEY_RIGHTBRACE => '+',
    KEY_ENTER => '\n',
    KEY_A => 'a', KEY_S => 's', KEY_D => 'd', KEY_F => 'f', KEY_G => 'g',
    KEY_H => 'h', KEY_J => 'j', KEY_K => 'k', KEY_L => 'l',
    KEY_SEMICOLON => 'ö', KEY_APOSTROPHE => 'ä', KEY_GRAVE => '^',
    KEY_BACKSLASH => '#',
    KEY_Z => 'y', KEY_X => 'x', KEY_C => 'c', KEY_V => 'v', KEY_B => 'b',  // Z and Y swapped
    KEY_N => 'n', KEY_M => 'm',
    KEY_COMMA => ',', KEY_DOT => '.', KEY_SLASH => '-',
    KEY_SPACE => ' ',
};

static DE_SHIFT: [char; 128] = charmap! {
    KEY_ESC => '\x1b',
    KEY_1 => '!', KEY_2 => '"', KEY_3 => '§', KEY_4 => '$', KEY_5 => '%',
    KEY_6 => '&', KEY_7 => '/', KEY_8 => '(', KEY_9 => ')', KEY_0 => '=',
    KEY_MINUS => '?', KEY_EQUAL => '`',
    KEY_BACKSPACE => '\x08', KEY_TAB => '\t',
    KEY_Q => 'Q', KEY_W => 'W', KEY_E => 'E', KEY_R => 'R', KEY_T => 'T',
    KEY_Y => 'Z', KEY_U => 'U', KEY_I => 'I', KEY_O => 'O', KEY_P => 'P',
    KEY_LEFTBRACE => 'Ü', KEY_RIGHTBRACE => '*',
    KEY_ENTER => '\n',
    KEY_A => 'A', KEY_S => 'S', KEY_D => 'D', KEY_F => 'F', KEY_G => 'G',
    KEY_H => 'H', KEY_J => 'J', KEY_K => 'K', KEY_L => 'L',
    KEY_SEMICOLON => 'Ö', KEY_APOSTROPHE => 'Ä', KEY_GRAVE => '°',
    KEY_BACKSLASH => '\'',
    KEY_Z => 'Y', KEY_X => 'X', KEY_C => 'C', KEY_V => 'V', KEY_B => 'B',
    KEY_N => 'N', KEY_M => 'M',
    KEY_COMMA => ';', KEY_DOT => ':', KEY_SLASH => '_',
    KEY_SPACE => ' ',
};

static DE_ALTGR: [char; 128] = charmap! {
    KEY_2 => '²', KEY_3 => '³',
    KEY_7 => '{', KEY_8 => '[', KEY_9 => ']', KEY_0 => '}',
    KEY_MINUS => '\\',
    KEY_Q => '@',
    KEY_E => '€',
    KEY_RIGHTBRACE => '~',
    KEY_SLASH => '|',
};

static DE_SHIFT_ALTGR: [char; 128] = ['\0'; 128];

/// German QWERTZ keyboard layout
pub static LAYOUT_DE: KeyboardLayout = KeyboardLayout {
    name: "de",
    description: "German QWERTZ",
    normal: &DE_NORMAL,
    shift: &DE_SHIFT,
    altgr: &DE_ALTGR,
    shift_altgr: &DE_SHIFT_ALTGR,
};

// ============================================================================
// French AZERTY Layout
// ============================================================================

static FR_NORMAL: [char; 128] = charmap! {
    KEY_ESC => '\x1b',
    KEY_1 => '&', KEY_2 => 'é', KEY_3 => '"', KEY_4 => '\'', KEY_5 => '(',
    KEY_6 => '-', KEY_7 => 'è', KEY_8 => '_', KEY_9 => 'ç', KEY_0 => 'à',
    KEY_MINUS => ')', KEY_EQUAL => '=',
    KEY_BACKSPACE => '\x08', KEY_TAB => '\t',
    KEY_Q => 'a', KEY_W => 'z', KEY_E => 'e', KEY_R => 'r', KEY_T => 't',  // AZERTY
    KEY_Y => 'y', KEY_U => 'u', KEY_I => 'i', KEY_O => 'o', KEY_P => 'p',
    KEY_LEFTBRACE => '^', KEY_RIGHTBRACE => '$',
    KEY_ENTER => '\n',
    KEY_A => 'q', KEY_S => 's', KEY_D => 'd', KEY_F => 'f', KEY_G => 'g',  // AZERTY
    KEY_H => 'h', KEY_J => 'j', KEY_K => 'k', KEY_L => 'l',
    KEY_SEMICOLON => 'm', KEY_APOSTROPHE => 'ù', KEY_GRAVE => '²',
    KEY_BACKSLASH => '*',
    KEY_Z => 'w', KEY_X => 'x', KEY_C => 'c', KEY_V => 'v', KEY_B => 'b',  // AZERTY
    KEY_N => 'n', KEY_M => ',',
    KEY_COMMA => ';', KEY_DOT => ':', KEY_SLASH => '!',
    KEY_SPACE => ' ',
};

static FR_SHIFT: [char; 128] = charmap! {
    KEY_ESC => '\x1b',
    KEY_1 => '1', KEY_2 => '2', KEY_3 => '3', KEY_4 => '4', KEY_5 => '5',
    KEY_6 => '6', KEY_7 => '7', KEY_8 => '8', KEY_9 => '9', KEY_0 => '0',
    KEY_MINUS => '°', KEY_EQUAL => '+',
    KEY_BACKSPACE => '\x08', KEY_TAB => '\t',
    KEY_Q => 'A', KEY_W => 'Z', KEY_E => 'E', KEY_R => 'R', KEY_T => 'T',
    KEY_Y => 'Y', KEY_U => 'U', KEY_I => 'I', KEY_O => 'O', KEY_P => 'P',
    KEY_LEFTBRACE => '¨', KEY_RIGHTBRACE => '£',
    KEY_ENTER => '\n',
    KEY_A => 'Q', KEY_S => 'S', KEY_D => 'D', KEY_F => 'F', KEY_G => 'G',
    KEY_H => 'H', KEY_J => 'J', KEY_K => 'K', KEY_L => 'L',
    KEY_SEMICOLON => 'M', KEY_APOSTROPHE => '%', KEY_GRAVE => '~',
    KEY_BACKSLASH => 'µ',
    KEY_Z => 'W', KEY_X => 'X', KEY_C => 'C', KEY_V => 'V', KEY_B => 'B',
    KEY_N => 'N', KEY_M => '?',
    KEY_COMMA => '.', KEY_DOT => '/', KEY_SLASH => '§',
    KEY_SPACE => ' ',
};

static FR_ALTGR: [char; 128] = charmap! {
    KEY_2 => '~', KEY_3 => '#', KEY_4 => '{', KEY_5 => '[',
    KEY_6 => '|', KEY_7 => '`', KEY_8 => '\\', KEY_9 => '^', KEY_0 => '@',
    KEY_MINUS => ']', KEY_EQUAL => '}',
    KEY_E => '€',
};

static FR_SHIFT_ALTGR: [char; 128] = ['\0'; 128];

/// French AZERTY keyboard layout
pub static LAYOUT_FR: KeyboardLayout = KeyboardLayout {
    name: "fr",
    description: "French AZERTY",
    normal: &FR_NORMAL,
    shift: &FR_SHIFT,
    altgr: &FR_ALTGR,
    shift_altgr: &FR_SHIFT_ALTGR,
};

// ============================================================================
// Layout Registry
// ============================================================================

/// All available keyboard layouts
pub static LAYOUTS: &[&KeyboardLayout] = &[
    &LAYOUT_US,
    &LAYOUT_UK,
    &LAYOUT_DE,
    &LAYOUT_FR,
];

/// Get a layout by name
pub fn get_layout(name: &str) -> Option<&'static KeyboardLayout> {
    LAYOUTS.iter().find(|l| l.name == name).copied()
}

/// Get the default layout (US)
pub fn default_layout() -> &'static KeyboardLayout {
    &LAYOUT_US
}
