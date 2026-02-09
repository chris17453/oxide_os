//! Shell Color Theme System
//!
//! Provides VIM-like color themes for the OXIDE shell (esh).
//! Themes define colors for:
//! - Prompt elements (user, host, path, symbol)
//! - Output (errors, warnings, info)
//! - Completion (directories, executables, files)
//! - Syntax (builtins, strings, variables)
//!
//! Themes can be loaded from files in /etc/esh/themes/ or ~/.esh/themes/
//!
//! — CipherVex: "Paint the terminal in your colors. Every hacker deserves their aesthetic."

/// ANSI color codes
#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Color {
    // Standard colors (30-37 fg, 40-47 bg)
    Black = 0,
    Red = 1,
    Green = 2,
    Yellow = 3,
    Blue = 4,
    Magenta = 5,
    Cyan = 6,
    White = 7,
    // Bright colors (90-97 fg, 100-107 bg)
    BrightBlack = 8,
    BrightRed = 9,
    BrightGreen = 10,
    BrightYellow = 11,
    BrightBlue = 12,
    BrightMagenta = 13,
    BrightCyan = 14,
    BrightWhite = 15,
    // Default (use terminal default)
    Default = 255,
}

impl Color {
    /// Get ANSI foreground code
    pub fn fg_code(&self) -> u8 {
        match *self {
            Color::Black => 30,
            Color::Red => 31,
            Color::Green => 32,
            Color::Yellow => 33,
            Color::Blue => 34,
            Color::Magenta => 35,
            Color::Cyan => 36,
            Color::White => 37,
            Color::BrightBlack => 90,
            Color::BrightRed => 91,
            Color::BrightGreen => 92,
            Color::BrightYellow => 93,
            Color::BrightBlue => 94,
            Color::BrightMagenta => 95,
            Color::BrightCyan => 96,
            Color::BrightWhite => 97,
            Color::Default => 39,
        }
    }

    /// Get ANSI background code
    pub fn bg_code(&self) -> u8 {
        match *self {
            Color::Black => 40,
            Color::Red => 41,
            Color::Green => 42,
            Color::Yellow => 43,
            Color::Blue => 44,
            Color::Magenta => 45,
            Color::Cyan => 46,
            Color::White => 47,
            Color::BrightBlack => 100,
            Color::BrightRed => 101,
            Color::BrightGreen => 102,
            Color::BrightYellow => 103,
            Color::BrightBlue => 104,
            Color::BrightMagenta => 105,
            Color::BrightCyan => 106,
            Color::BrightWhite => 107,
            Color::Default => 49,
        }
    }

    /// Parse color from name
    pub fn from_name(name: &[u8]) -> Option<Color> {
        if bytes_eq_ignore_case(name, b"black") {
            Some(Color::Black)
        } else if bytes_eq_ignore_case(name, b"red") {
            Some(Color::Red)
        } else if bytes_eq_ignore_case(name, b"green") {
            Some(Color::Green)
        } else if bytes_eq_ignore_case(name, b"yellow") {
            Some(Color::Yellow)
        } else if bytes_eq_ignore_case(name, b"blue") {
            Some(Color::Blue)
        } else if bytes_eq_ignore_case(name, b"magenta") {
            Some(Color::Magenta)
        } else if bytes_eq_ignore_case(name, b"cyan") {
            Some(Color::Cyan)
        } else if bytes_eq_ignore_case(name, b"white") {
            Some(Color::White)
        } else if bytes_eq_ignore_case(name, b"brightblack")
            || bytes_eq_ignore_case(name, b"gray")
            || bytes_eq_ignore_case(name, b"grey")
        {
            Some(Color::BrightBlack)
        } else if bytes_eq_ignore_case(name, b"brightred") {
            Some(Color::BrightRed)
        } else if bytes_eq_ignore_case(name, b"brightgreen") {
            Some(Color::BrightGreen)
        } else if bytes_eq_ignore_case(name, b"brightyellow") {
            Some(Color::BrightYellow)
        } else if bytes_eq_ignore_case(name, b"brightblue") {
            Some(Color::BrightBlue)
        } else if bytes_eq_ignore_case(name, b"brightmagenta") {
            Some(Color::BrightMagenta)
        } else if bytes_eq_ignore_case(name, b"brightcyan") {
            Some(Color::BrightCyan)
        } else if bytes_eq_ignore_case(name, b"brightwhite") {
            Some(Color::BrightWhite)
        } else if bytes_eq_ignore_case(name, b"default") || bytes_eq_ignore_case(name, b"none") {
            Some(Color::Default)
        } else {
            None
        }
    }
}

/// Text style attributes
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Style {
    pub bold: bool,
    pub dim: bool,
    pub italic: bool,
    pub underline: bool,
    pub blink: bool,
    pub reverse: bool,
}

impl Style {
    pub const fn none() -> Self {
        Style {
            bold: false,
            dim: false,
            italic: false,
            underline: false,
            blink: false,
            reverse: false,
        }
    }

    pub const fn bold() -> Self {
        Style {
            bold: true,
            dim: false,
            italic: false,
            underline: false,
            blink: false,
            reverse: false,
        }
    }
}

/// A complete color specification (fg + bg + style)
#[derive(Clone, Copy)]
pub struct ColorSpec {
    pub fg: Color,
    pub bg: Color,
    pub style: Style,
}

impl ColorSpec {
    pub const fn new(fg: Color, bg: Color, style: Style) -> Self {
        ColorSpec { fg, bg, style }
    }

    pub const fn fg_only(fg: Color) -> Self {
        ColorSpec {
            fg,
            bg: Color::Default,
            style: Style::none(),
        }
    }

    pub const fn default() -> Self {
        ColorSpec {
            fg: Color::Default,
            bg: Color::Default,
            style: Style::none(),
        }
    }

    /// Write ANSI escape sequence to buffer, returns bytes written
    pub fn write_escape(&self, buf: &mut [u8]) -> usize {
        let mut pos = 0;

        // Start escape sequence
        if pos + 2 > buf.len() {
            return 0;
        }
        buf[pos] = 0x1b; // ESC
        pos += 1;
        buf[pos] = b'[';
        pos += 1;

        let mut need_semi = false;

        // Reset first
        if pos + 1 > buf.len() {
            return 0;
        }
        buf[pos] = b'0';
        pos += 1;
        need_semi = true;

        // Style attributes
        if self.style.bold {
            if need_semi {
                buf[pos] = b';';
                pos += 1;
            }
            buf[pos] = b'1';
            pos += 1;
            need_semi = true;
        }
        if self.style.dim {
            if need_semi {
                buf[pos] = b';';
                pos += 1;
            }
            buf[pos] = b'2';
            pos += 1;
            need_semi = true;
        }
        if self.style.italic {
            if need_semi {
                buf[pos] = b';';
                pos += 1;
            }
            buf[pos] = b'3';
            pos += 1;
            need_semi = true;
        }
        if self.style.underline {
            if need_semi {
                buf[pos] = b';';
                pos += 1;
            }
            buf[pos] = b'4';
            pos += 1;
            need_semi = true;
        }
        if self.style.blink {
            if need_semi {
                buf[pos] = b';';
                pos += 1;
            }
            buf[pos] = b'5';
            pos += 1;
            need_semi = true;
        }
        if self.style.reverse {
            if need_semi {
                buf[pos] = b';';
                pos += 1;
            }
            buf[pos] = b'7';
            pos += 1;
            need_semi = true;
        }

        // Foreground color
        if self.fg != Color::Default {
            if need_semi {
                buf[pos] = b';';
                pos += 1;
            }
            let code = self.fg.fg_code();
            pos += write_u8(code, &mut buf[pos..]);
            need_semi = true;
        }

        // Background color
        if self.bg != Color::Default {
            if need_semi {
                buf[pos] = b';';
                pos += 1;
            }
            let code = self.bg.bg_code();
            pos += write_u8(code, &mut buf[pos..]);
        }

        // End sequence
        if pos < buf.len() {
            buf[pos] = b'm';
            pos += 1;
        }

        pos
    }
}

/// Shell color theme
#[derive(Clone, Copy)]
pub struct Theme {
    /// Theme name
    pub name: [u8; 32],

    // Prompt colors
    pub prompt_user: ColorSpec,
    pub prompt_host: ColorSpec,
    pub prompt_path: ColorSpec,
    pub prompt_symbol: ColorSpec,
    pub prompt_root: ColorSpec,

    // Output colors
    pub error: ColorSpec,
    pub warning: ColorSpec,
    pub info: ColorSpec,
    pub success: ColorSpec,

    // Completion colors
    pub comp_directory: ColorSpec,
    pub comp_executable: ColorSpec,
    pub comp_file: ColorSpec,
    pub comp_symlink: ColorSpec,

    // Syntax colors (for future syntax highlighting)
    pub syn_builtin: ColorSpec,
    pub syn_command: ColorSpec,
    pub syn_string: ColorSpec,
    pub syn_variable: ColorSpec,
    pub syn_operator: ColorSpec,
    pub syn_comment: ColorSpec,
}

impl Theme {
    /// Default theme (classic terminal look)
    pub const fn default() -> Self {
        Theme {
            name: *b"default\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
            prompt_user: ColorSpec::fg_only(Color::Green),
            prompt_host: ColorSpec::fg_only(Color::Green),
            prompt_path: ColorSpec::fg_only(Color::Blue),
            prompt_symbol: ColorSpec::fg_only(Color::Default),
            prompt_root: ColorSpec::fg_only(Color::Red),
            error: ColorSpec::new(Color::Red, Color::Default, Style::bold()),
            warning: ColorSpec::fg_only(Color::Yellow),
            info: ColorSpec::fg_only(Color::Cyan),
            success: ColorSpec::fg_only(Color::Green),
            comp_directory: ColorSpec::new(Color::Blue, Color::Default, Style::bold()),
            comp_executable: ColorSpec::new(Color::Green, Color::Default, Style::bold()),
            comp_file: ColorSpec::default(),
            comp_symlink: ColorSpec::fg_only(Color::Cyan),
            syn_builtin: ColorSpec::fg_only(Color::Cyan),
            syn_command: ColorSpec::fg_only(Color::Green),
            syn_string: ColorSpec::fg_only(Color::Yellow),
            syn_variable: ColorSpec::fg_only(Color::Magenta),
            syn_operator: ColorSpec::fg_only(Color::White),
            syn_comment: ColorSpec::fg_only(Color::BrightBlack),
        }
    }

    /// Cyberpunk theme (neon colors)
    pub const fn cyberpunk() -> Self {
        Theme {
            name: *b"cyberpunk\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
            prompt_user: ColorSpec::fg_only(Color::BrightMagenta),
            prompt_host: ColorSpec::fg_only(Color::BrightCyan),
            prompt_path: ColorSpec::fg_only(Color::BrightYellow),
            prompt_symbol: ColorSpec::fg_only(Color::BrightGreen),
            prompt_root: ColorSpec::new(Color::BrightRed, Color::Default, Style::bold()),
            error: ColorSpec::new(Color::BrightRed, Color::Default, Style::bold()),
            warning: ColorSpec::fg_only(Color::BrightYellow),
            info: ColorSpec::fg_only(Color::BrightCyan),
            success: ColorSpec::fg_only(Color::BrightGreen),
            comp_directory: ColorSpec::new(Color::BrightMagenta, Color::Default, Style::bold()),
            comp_executable: ColorSpec::new(Color::BrightGreen, Color::Default, Style::bold()),
            comp_file: ColorSpec::fg_only(Color::BrightWhite),
            comp_symlink: ColorSpec::fg_only(Color::BrightCyan),
            syn_builtin: ColorSpec::fg_only(Color::BrightCyan),
            syn_command: ColorSpec::fg_only(Color::BrightGreen),
            syn_string: ColorSpec::fg_only(Color::BrightYellow),
            syn_variable: ColorSpec::fg_only(Color::BrightMagenta),
            syn_operator: ColorSpec::fg_only(Color::BrightWhite),
            syn_comment: ColorSpec::fg_only(Color::BrightBlack),
        }
    }

    /// Monokai theme (inspired by the popular editor theme)
    pub const fn monokai() -> Self {
        Theme {
            name: *b"monokai\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
            prompt_user: ColorSpec::fg_only(Color::Green),
            prompt_host: ColorSpec::fg_only(Color::Yellow),
            prompt_path: ColorSpec::fg_only(Color::Cyan),
            prompt_symbol: ColorSpec::fg_only(Color::Magenta),
            prompt_root: ColorSpec::new(Color::Red, Color::Default, Style::bold()),
            error: ColorSpec::new(Color::Red, Color::Default, Style::bold()),
            warning: ColorSpec::fg_only(Color::Yellow),
            info: ColorSpec::fg_only(Color::Blue),
            success: ColorSpec::fg_only(Color::Green),
            comp_directory: ColorSpec::new(Color::Blue, Color::Default, Style::bold()),
            comp_executable: ColorSpec::new(Color::Green, Color::Default, Style::bold()),
            comp_file: ColorSpec::fg_only(Color::White),
            comp_symlink: ColorSpec::fg_only(Color::Cyan),
            syn_builtin: ColorSpec::fg_only(Color::Cyan),
            syn_command: ColorSpec::fg_only(Color::Green),
            syn_string: ColorSpec::fg_only(Color::Yellow),
            syn_variable: ColorSpec::fg_only(Color::Magenta),
            syn_operator: ColorSpec::fg_only(Color::Red),
            syn_comment: ColorSpec::fg_only(Color::BrightBlack),
        }
    }

    /// Solarized Dark theme
    pub const fn solarized() -> Self {
        Theme {
            name: *b"solarized\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
            prompt_user: ColorSpec::fg_only(Color::Green),
            prompt_host: ColorSpec::fg_only(Color::Cyan),
            prompt_path: ColorSpec::fg_only(Color::Blue),
            prompt_symbol: ColorSpec::fg_only(Color::Yellow),
            prompt_root: ColorSpec::new(Color::Red, Color::Default, Style::bold()),
            error: ColorSpec::new(Color::Red, Color::Default, Style::bold()),
            warning: ColorSpec::fg_only(Color::Yellow),
            info: ColorSpec::fg_only(Color::Cyan),
            success: ColorSpec::fg_only(Color::Green),
            comp_directory: ColorSpec::new(Color::Blue, Color::Default, Style::bold()),
            comp_executable: ColorSpec::new(Color::Green, Color::Default, Style::bold()),
            comp_file: ColorSpec::fg_only(Color::White),
            comp_symlink: ColorSpec::fg_only(Color::Magenta),
            syn_builtin: ColorSpec::fg_only(Color::Blue),
            syn_command: ColorSpec::fg_only(Color::Green),
            syn_string: ColorSpec::fg_only(Color::Cyan),
            syn_variable: ColorSpec::fg_only(Color::Yellow),
            syn_operator: ColorSpec::fg_only(Color::White),
            syn_comment: ColorSpec::fg_only(Color::BrightBlack),
        }
    }

    /// Minimal theme (no colors)
    pub const fn minimal() -> Self {
        Theme {
            name: *b"minimal\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
            prompt_user: ColorSpec::default(),
            prompt_host: ColorSpec::default(),
            prompt_path: ColorSpec::default(),
            prompt_symbol: ColorSpec::default(),
            prompt_root: ColorSpec::default(),
            error: ColorSpec::default(),
            warning: ColorSpec::default(),
            info: ColorSpec::default(),
            success: ColorSpec::default(),
            comp_directory: ColorSpec::default(),
            comp_executable: ColorSpec::default(),
            comp_file: ColorSpec::default(),
            comp_symlink: ColorSpec::default(),
            syn_builtin: ColorSpec::default(),
            syn_command: ColorSpec::default(),
            syn_string: ColorSpec::default(),
            syn_variable: ColorSpec::default(),
            syn_operator: ColorSpec::default(),
            syn_comment: ColorSpec::default(),
        }
    }

    /// Get built-in theme by name
    pub fn builtin(name: &[u8]) -> Option<Theme> {
        if bytes_eq_ignore_case(name, b"default") {
            Some(Theme::default())
        } else if bytes_eq_ignore_case(name, b"cyberpunk") {
            Some(Theme::cyberpunk())
        } else if bytes_eq_ignore_case(name, b"monokai") {
            Some(Theme::monokai())
        } else if bytes_eq_ignore_case(name, b"solarized") {
            Some(Theme::solarized())
        } else if bytes_eq_ignore_case(name, b"minimal") || bytes_eq_ignore_case(name, b"none") {
            Some(Theme::minimal())
        } else {
            None
        }
    }
}

/// Current active theme (global state)
static mut CURRENT_THEME: Theme = Theme::monokai();
static mut THEME_ENABLED: bool = true;

/// Get the current theme
pub fn current_theme() -> &'static Theme {
    unsafe { &*core::ptr::addr_of!(CURRENT_THEME) }
}

/// Set the current theme
pub fn set_theme(theme: Theme) {
    unsafe {
        let ptr = core::ptr::addr_of_mut!(CURRENT_THEME);
        *ptr = theme;
    }
}

/// Enable/disable color output
pub fn set_colors_enabled(enabled: bool) {
    unsafe {
        let ptr = core::ptr::addr_of_mut!(THEME_ENABLED);
        *ptr = enabled;
    }
}

/// Check if colors are enabled
pub fn colors_enabled() -> bool {
    unsafe { *core::ptr::addr_of!(THEME_ENABLED) }
}

/// Load theme by name (builtin or from file)
pub fn load_theme(name: &[u8]) -> bool {
    // Try builtin first
    if let Some(theme) = Theme::builtin(name) {
        set_theme(theme);
        return true;
    }

    // Try loading from file
    // Search paths: ~/.esh/themes/<name>, /etc/esh/themes/<name>
    // (File loading would be implemented here if needed)

    false
}

/// Reset to default colors (writes escape sequence)
pub fn reset_colors(buf: &mut [u8]) -> usize {
    if !colors_enabled() {
        return 0;
    }
    // \x1b[0m
    if buf.len() >= 4 {
        buf[0] = 0x1b;
        buf[1] = b'[';
        buf[2] = b'0';
        buf[3] = b'm';
        4
    } else {
        0
    }
}

/// Write a colored string to buffer (color + text + reset)
pub fn colorize(text: &[u8], spec: &ColorSpec, buf: &mut [u8]) -> usize {
    if !colors_enabled() {
        // Just copy text
        let len = text_len(text).min(buf.len());
        buf[..len].copy_from_slice(&text[..len]);
        return len;
    }

    let mut pos = 0;

    // Write color escape
    pos += spec.write_escape(&mut buf[pos..]);

    // Write text
    let text_l = text_len(text);
    let copy_len = text_l.min(buf.len() - pos);
    buf[pos..pos + copy_len].copy_from_slice(&text[..copy_len]);
    pos += copy_len;

    // Reset
    pos += reset_colors(&mut buf[pos..]);

    pos
}

// Helper functions

fn text_len(s: &[u8]) -> usize {
    for (i, &b) in s.iter().enumerate() {
        if b == 0 {
            return i;
        }
    }
    s.len()
}

fn write_u8(n: u8, buf: &mut [u8]) -> usize {
    if n >= 100 {
        if buf.len() >= 3 {
            buf[0] = b'0' + n / 100;
            buf[1] = b'0' + (n / 10) % 10;
            buf[2] = b'0' + n % 10;
            return 3;
        }
    } else if n >= 10 {
        if buf.len() >= 2 {
            buf[0] = b'0' + n / 10;
            buf[1] = b'0' + n % 10;
            return 2;
        }
    } else if !buf.is_empty() {
        buf[0] = b'0' + n;
        return 1;
    }
    0
}

fn bytes_eq_ignore_case(a: &[u8], b: &[u8]) -> bool {
    let a_len = text_len(a);
    let b_len = text_len(b);
    if a_len != b_len {
        return false;
    }
    for i in 0..a_len {
        let a_ch = a[i].to_ascii_lowercase();
        let b_ch = b[i].to_ascii_lowercase();
        if a_ch != b_ch {
            return false;
        }
    }
    true
}

/// List of available theme names
pub const THEME_NAMES: &[&[u8]] = &[
    b"default",
    b"cyberpunk",
    b"monokai",
    b"solarized",
    b"minimal",
];
