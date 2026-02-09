//! Terminal Emulator for OXIDE OS
//!
//! Provides VT100/ANSI terminal emulation at the kernel driver level.
//! Applications write bytes to stdout; this crate parses escape sequences,
//! manages terminal state, and renders to the framebuffer.
//!
//! # Architecture
//!
//! ```text
//! Application (shell, vim, etc.)
//!     |
//!     | write(stdout, bytes)
//!     v
//! /dev/console
//!     |
//!     v
//! TerminalEmulator (this crate)
//!     |
//!     ├── Parser (VT100 state machine)
//!     ├── Handler (sequence processing)
//!     ├── ScreenBuffer (cell storage)
//!     └── Renderer (framebuffer output)
//! ```
//!
//! # Usage
//!
//! ```ignore
//! use terminal::TerminalEmulator;
//!
//! // Create terminal with framebuffer
//! let terminal = TerminalEmulator::new(framebuffer);
//!
//! // Write bytes from application
//! terminal.write(b"Hello \x1b[31mWorld\x1b[0m!\n");
//! ```

#![no_std]
#![allow(unused)]

extern crate alloc;

pub mod renderer;

// Re-export VTE types (parser, handler, buffer, cell, color, wcwidth extracted to userspace/libs/vte)
pub use vte::TermColor;
pub use vte::wcwidth;
pub use vte::{Action, Parser, State};
pub use vte::{Cell, CellAttrs, CellFlags, Cursor, CursorShape};
pub use vte::{Handler, MouseEncoding, MouseMode, TerminalModes};
pub use vte::{ScreenBuffer, ScrollbackBuffer};

use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, Ordering};
use fb::{Color, Framebuffer};
use spin::Mutex;

use crate::renderer::Renderer;

/// Emit a lock contention warning to serial port (ISR-safe, no dependencies).
///
/// Uses direct x86 port I/O to COM1 (0x3F8) to avoid any lock dependencies.
/// Only compiled when `debug-lock` feature is enabled.
#[cfg(feature = "debug-lock")]
#[inline(never)]
fn lock_contention_warning(lock_name: &str) {
    #[cfg(target_arch = "x86_64")]
    unsafe {
        for &b in b"[LOCK] terminal::" {
            core::arch::asm!("out dx, al", in("dx") 0x3F8u16, in("al") b, options(nomem, nostack));
        }
        for &b in lock_name.as_bytes() {
            core::arch::asm!("out dx, al", in("dx") 0x3F8u16, in("al") b, options(nomem, nostack));
        }
        for &b in b" contention\n" {
            core::arch::asm!("out dx, al", in("dx") 0x3F8u16, in("al") b, options(nomem, nostack));
        }
    }
}

/// Default scrollback buffer size (lines)
const DEFAULT_SCROLLBACK: usize = 10000;

/// Terminal emulator state
pub struct TerminalEmulator {
    /// VT100 parser
    parser: Parser,
    /// Sequence handler
    handler: Handler,
    /// Primary screen buffer
    primary: ScreenBuffer,
    /// Alternate screen buffer (for full-screen apps)
    alternate: ScreenBuffer,
    /// Scrollback buffer
    scrollback: ScrollbackBuffer,
    /// Renderer
    renderer: Renderer,
    /// Current scroll offset (0 = at bottom/current, >0 = scrolled up)
    scroll_offset: usize,
    /// Terminal width in columns
    cols: u32,
    /// Terminal height in rows
    rows: u32,
    /// Cell width in pixels (for mouse coordinate conversion)
    cell_width: u32,
    /// Cell height in pixels (for mouse coordinate conversion)
    cell_height: u32,
    /// Whether a render is needed (dirty flag)
    needs_render: bool,
    /// Window title (set via OSC sequences)
    title: String,
    /// Buffered output for synchronized mode
    sync_buffer: Vec<u8>,
    /// Custom color palette (256 colors for ANSI256 mode)
    palette: [Color; 256],
    /// Custom default foreground color (None = use standard)
    custom_fg: Option<Color>,
    /// Custom default background color (None = use standard)
    custom_bg: Option<Color>,
    /// Custom cursor color (None = use standard)
    custom_cursor: Option<Color>,
    /// Clipboard storage (OSC 52)
    clipboard: String,
    /// Mouse selection state
    selection: Option<Selection>,
}

/// Mouse text selection
#[derive(Debug, Clone, Copy)]
struct Selection {
    /// Selection start (col, row) in terminal coordinates
    start: (u32, u32),
    /// Selection end (col, row) in terminal coordinates
    end: (u32, u32),
    /// Whether selection is active (mouse still pressed)
    active: bool,
}

impl TerminalEmulator {
    /// Create a new terminal emulator with the given framebuffer
    pub fn new(fb: Arc<dyn Framebuffer>) -> Self {
        let renderer = Renderer::new(fb);
        let (cols, rows) = renderer.dimensions();
        let (cell_width, cell_height) = renderer.cell_dimensions();

        // Initialize default 256-color palette from VTE RGB tuples
        let mut palette = [Color::VGA_BLACK; 256];
        for i in 0..256 {
            let (r, g, b) = vte::color::ansi256_to_rgb(i as u8);
            palette[i] = Color::new(r, g, b);
        }

        // Wire VTE handler response callback to kernel's send_response
        let mut handler = Handler::new(cols, rows);
        handler.set_response_callback(crate::send_response);

        TerminalEmulator {
            parser: Parser::new(),
            handler,
            primary: ScreenBuffer::new(cols, rows),
            alternate: ScreenBuffer::new(cols, rows),
            scrollback: ScrollbackBuffer::new(DEFAULT_SCROLLBACK),
            renderer,
            scroll_offset: 0,
            cols,
            rows,
            cell_width,
            cell_height,
            needs_render: true,
            title: String::from("OXIDE Terminal"),
            sync_buffer: Vec::new(),
            palette,
            custom_fg: None,
            custom_bg: None,
            custom_cursor: None,
            clipboard: String::new(),
            selection: None,
        }
    }

    /// Get terminal dimensions (cols, rows)
    pub fn dimensions(&self) -> (u32, u32) {
        (self.cols, self.rows)
    }

    /// Get current cursor position
    pub fn cursor(&self) -> &Cursor {
        &self.handler.cursor
    }

    /// Get the window title (set via OSC sequences)
    pub fn title(&self) -> &str {
        &self.title
    }

    /// Get current cell attributes
    pub fn attrs(&self) -> &CellAttrs {
        &self.handler.attrs
    }

    /// Write bytes to the terminal with per-glyph rendering (Linux fbcon_putcs style).
    /// — SoftGlyph: Each Print action paints its glyph inline. Each LF scroll does a
    /// pixel memmove. CSI bulk ops (ED/IL/DL) still set dirty flags for tick() catch-up.
    /// After processing all bytes, we paint the cursor and flush the framebuffer.
    pub fn write(&mut self, data: &[u8]) {
        // If synchronized output mode is active, buffer the data
        if self
            .handler
            .modes
            .contains(TerminalModes::SYNCHRONIZED_OUTPUT)
        {
            self.sync_buffer.extend_from_slice(data);
        } else {
            // — SoftGlyph: Pass selection state to renderer before any rendering
            self.push_selection_to_renderer();

            // — SoftGlyph: Erase cursor once at start of write, not per-byte.
            // During the write loop, cursor isn't drawn — we paint it at the end.
            {
                let is_alt = self.handler.modes.contains(TerminalModes::ALT_SCREEN);
                let buffer = if is_alt {
                    &self.alternate
                } else {
                    &self.primary
                };
                self.renderer.erase_cursor(buffer);
            }

            for &byte in data {
                self.process_byte(byte);
            }

            // — GraveShift: Scrolls handled inline via scroll_up_pixels() now.
            // Dirty flags only set by CSI bulk ops (ED/IL/DL etc). Render those
            // synchronously — like Linux's do_con_write() calling con_flush().
            if self.renderer.has_dirty() {
                self.render();
                self.needs_render = false;
            }

            // — SoftGlyph: Paint cursor at final position and flush.
            {
                let is_alt = self.handler.modes.contains(TerminalModes::ALT_SCREEN);
                let buffer = if is_alt {
                    &self.alternate
                } else {
                    &self.primary
                };
                let cursor = self.handler.cursor;
                self.renderer.paint_cursor(buffer, &cursor);
                self.renderer.update_cursor_tracking(&cursor);
                self.renderer.flush_fb();
            }
        }
    }

    /// Tick function - call from timer at desired FPS to render
    pub fn tick(&mut self) {
        if self.needs_render {
            self.render();
            self.needs_render = false;
        }
    }

    /// Force immediate render
    pub fn flush(&mut self) {
        self.render();
        self.needs_render = false;
    }

    /// Write and immediately render (for urgent output).
    /// — SoftGlyph: Same as write() now — per-glyph rendering is always synchronous.
    pub fn write_immediate(&mut self, data: &[u8]) {
        self.write(data);
    }

    /// Check if render is pending
    pub fn needs_render(&self) -> bool {
        self.needs_render
    }

    /// Write a string to the terminal
    pub fn write_str(&mut self, s: &str) {
        self.write(s.as_bytes());
    }

    /// Process a single byte
    fn process_byte(&mut self, byte: u8) {
        let action = self.parser.advance(byte);
        self.handle_action(action);
    }

    /// Handle a parsed action
    fn handle_action(&mut self, action: Action) {
        match action {
            Action::Print(ch) => {
                let is_alt = self.handler.modes.contains(TerminalModes::ALT_SCREEN);
                let buffer = if is_alt {
                    &mut self.alternate
                } else {
                    &mut self.primary
                };

                // Track cursor position before put_char to detect autowrap scroll
                let old_row = self.handler.cursor.row;
                let old_col = self.handler.cursor.col;

                self.handler.put_char(ch, buffer);

                // — SoftGlyph: Check if put_char triggered autowrap+scroll.
                // put_char wraps when cursor.col >= cols at entry. If cursor was past
                // the edge AND we stayed on the same row, linefeed() scrolled internally.
                // If cursor moved to next row, it was a normal wrap (no scroll needed).
                let scrolled = old_col >= self.cols && self.handler.cursor.row == old_row;

                if scrolled {
                    // — GraveShift: Autowrap triggered a scroll. Pixel memmove the
                    // framebuffer up by one row, then paint the new character.
                    // Framebuffer is WB-cached so copy_rect is fast (CPU cache, not
                    // slow MMIO reads). This is the Linux fbcon way — no dirty flags.
                    let (r, g, b) = self.handler.attrs.effective_bg().to_rgb(false);
                    let bg = Color::new(r, g, b);
                    self.renderer.scroll_up_pixels(1, bg);

                    // Now paint the character that was placed after the scroll
                    let buffer = if is_alt {
                        &self.alternate
                    } else {
                        &self.primary
                    };
                    let char_col = if self.handler.cursor.col > 0 {
                        self.handler.cursor.col - 1
                    } else {
                        0
                    };
                    self.renderer
                        .render_cell(buffer, self.handler.cursor.row, char_col);
                } else {
                    // — SoftGlyph: No scroll — paint the glyph directly to framebuffer.
                    // This is our fbcon_putcs(). The cursor.col is already past the char,
                    // so the char we just wrote is at (cursor.col - 1) for single-width.
                    let char_col = if self.handler.cursor.col > 0 {
                        self.handler.cursor.col - 1
                    } else {
                        0
                    };
                    self.renderer
                        .render_cell(buffer, self.handler.cursor.row, char_col);

                    // Wide char: also render the previous column if it's the lead cell
                    if char_col > 0 {
                        if let Some(prev) = buffer.get(self.handler.cursor.row, char_col - 1) {
                            if prev.attrs.flags.contains(CellFlags::WIDE) {
                                self.renderer.render_cell(
                                    buffer,
                                    self.handler.cursor.row,
                                    char_col - 1,
                                );
                            }
                        }
                    }
                }
            }
            Action::Execute(byte) => {
                self.execute_control(byte);
            }
            Action::CsiDispatch {
                params,
                intermediates,
                final_char,
            } => {
                // Track if synchronized mode was on before
                let was_sync = self
                    .handler
                    .modes
                    .contains(TerminalModes::SYNCHRONIZED_OUTPUT);

                // Need scrollback only if not on alternate screen
                let is_alt = self.handler.modes.contains(TerminalModes::ALT_SCREEN);

                if is_alt {
                    self.handler.handle_csi(
                        &params,
                        &intermediates,
                        final_char,
                        &mut self.alternate,
                        None,
                    );
                } else {
                    self.handler.handle_csi(
                        &params,
                        &intermediates,
                        final_char,
                        &mut self.primary,
                        Some(&mut self.scrollback),
                    );
                }

                // If synchronized mode was just turned off, flush buffered output
                let is_sync = self
                    .handler
                    .modes
                    .contains(TerminalModes::SYNCHRONIZED_OUTPUT);
                if was_sync && !is_sync && !self.sync_buffer.is_empty() {
                    let buffer = core::mem::take(&mut self.sync_buffer);
                    for &byte in buffer.iter() {
                        self.process_byte(byte);
                    }
                }

                // — GraveShift: Smart dirty marking - don't nuke the entire screen for cursor moves.
                // Previously mark_all_dirty() was called unconditionally, causing ~24x more row
                // renders than needed. Now we classify CSI commands:
                // - Cursor-only (ABCDEFGH, f, d, s, u): no marking needed (renderer handles cursor)
                // - Attribute-only (m): no marking needed (takes effect on next writes)
                // - Single-row (K): mark cursor row only
                // - Multi-row (J, L, M, P, S, T, X, @, etc): mark all
                match final_char {
                    // Cursor movement - renderer tracks cursor row changes automatically
                    b'A' | b'B' | b'C' | b'D' | b'E' | b'F' | b'G' | b'H' | b'f' | b'd' |
                    b's' | b'u' | // save/restore cursor
                    b'n' => {} // device status report (cursor position query)

                    // SGR (attributes) - doesn't modify screen content
                    b'm' => {}

                    // EL (erase line) - only affects cursor row
                    b'K' => self.renderer.mark_dirty(self.handler.cursor.row),

                    // Everything else might affect multiple rows - mark all dirty
                    // J (ED), L (IL), M (DL), P (DCH), S (SU), T (SD), X (ECH), @ (ICH),
                    // r (DECSTBM), h/l (modes), etc.
                    _ => self.renderer.mark_all_dirty(),
                }
            }
            Action::EscDispatch {
                intermediates,
                final_char,
            } => {
                let is_alt = self.handler.modes.contains(TerminalModes::ALT_SCREEN);
                let buffer = if is_alt {
                    &mut self.alternate
                } else {
                    &mut self.primary
                };
                self.handler.handle_esc(&intermediates, final_char, buffer);

                // — GraveShift: Smart dirty marking for ESC sequences too
                match (intermediates.first(), final_char) {
                    // Cursor save/restore, tab set - no screen content changes
                    (None, b'7') | (None, b'8') | (None, b'H') => {}
                    // Character set selection - no screen content changes
                    (Some(b'('), _) | (Some(b')'), _) => {}
                    // DECDHL/DECSWL/DECDWL line attrs - just metadata
                    (Some(b'#'), b'3')
                    | (Some(b'#'), b'4')
                    | (Some(b'#'), b'5')
                    | (Some(b'#'), b'6') => {}
                    // Linefeed/scroll, reset, DECALN, etc - might affect multiple rows
                    _ => self.renderer.mark_all_dirty(),
                }
            }
            Action::OscDispatch(data) => {
                // OSC commands (title, colors, etc.)
                self.handle_osc(&data);
            }
            Action::DcsDispatch {
                params,
                intermediates,
                final_char,
                data,
            } => {
                // DCS commands (Sixel, etc.)
                self.handle_dcs(&params, &intermediates, final_char, &data);
            }
            Action::None => {}
        }
    }

    /// Execute a control character
    fn execute_control(&mut self, byte: u8) {
        match byte {
            0x07 => {
                // BEL - Bell (ignored)
            }
            0x08 => {
                // BS - Backspace — cursor moves left, repaint affected cells
                // — SoftGlyph: Cursor already erased at write() start. Just move and repaint.
                let old_col = self.handler.cursor.col;
                self.handler.backspace();
                {
                    let is_alt = self.handler.modes.contains(TerminalModes::ALT_SCREEN);
                    let buffer = if is_alt {
                        &self.alternate
                    } else {
                        &self.primary
                    };
                    self.renderer
                        .render_cell(buffer, self.handler.cursor.row, old_col);
                    self.renderer.render_cell(
                        buffer,
                        self.handler.cursor.row,
                        self.handler.cursor.col,
                    );
                }
            }
            0x09 => {
                // HT - Horizontal Tab
                self.handler.tab();
            }
            0x0A | 0x0B | 0x0C => {
                // LF, VT, FF - Line feed with implicit CR (standard terminal behavior)
                let is_alt = self.handler.modes.contains(TerminalModes::ALT_SCREEN);

                // Implicit carriage return - most terminal output expects this
                self.handler.carriage_return();

                let scrolled = if is_alt {
                    self.handler.linefeed(&mut self.alternate, None)
                } else {
                    self.handler
                        .linefeed(&mut self.primary, Some(&mut self.scrollback))
                };

                if scrolled {
                    // — GraveShift: LF caused a scroll. Pixel memmove up by one row.
                    // Framebuffer is WB-cached — copy_rect reads from CPU cache, fast.
                    let (r, g, b) = self.handler.attrs.effective_bg().to_rgb(false);
                    let bg = Color::new(r, g, b);
                    self.renderer.scroll_up_pixels(1, bg);
                }
                // — SoftGlyph: No dirty mark needed for non-scroll LF.
                // Characters already rendered per-glyph. Cursor position change
                // handled by erase_cursor/paint_cursor at write() boundaries.
            }
            0x0D => {
                // CR - Carriage return (explicit, move to column 0)
                self.handler.carriage_return();
            }
            0x0E => {
                // SO - Shift Out (activate G1 charset)
                self.handler.active_g1 = true;
            }
            0x0F => {
                // SI - Shift In (activate G0 charset)
                self.handler.active_g1 = false;
            }
            _ => {}
        }
    }

    /// Base64 decode (simple implementation for clipboard)
    fn base64_decode(input: &str) -> Option<Vec<u8>> {
        const TABLE: &[u8; 128] = &[
            255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
            255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
            255, 255, 255, 255, 255, 255, 255, 255, 255, 62, 255, 255, 255, 63, 52, 53, 54, 55, 56,
            57, 58, 59, 60, 61, 255, 255, 255, 254, 255, 255, 255, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9,
            10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 255, 255, 255, 255,
            255, 255, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44,
            45, 46, 47, 48, 49, 50, 51, 255, 255, 255, 255, 255,
        ];

        let mut result = Vec::new();
        let bytes = input.as_bytes();
        let mut i = 0;

        while i < bytes.len() {
            let mut buf = [0u8; 4];
            let mut j = 0;

            while j < 4 && i < bytes.len() {
                let b = bytes[i];
                if b == b'=' || b > 127 {
                    break;
                }
                let val = TABLE[b as usize];
                if val == 255 {
                    i += 1;
                    continue;
                }
                buf[j] = val;
                j += 1;
                i += 1;
            }

            if j >= 2 {
                result.push((buf[0] << 2) | (buf[1] >> 4));
            }
            if j >= 3 {
                result.push((buf[1] << 4) | (buf[2] >> 2));
            }
            if j >= 4 {
                result.push((buf[2] << 6) | buf[3]);
            }
        }

        Some(result)
    }

    /// Base64 encode (simple implementation for clipboard)
    fn base64_encode(input: &[u8]) -> String {
        const TABLE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut result = String::new();

        for chunk in input.chunks(3) {
            let b0 = chunk[0];
            let b1 = chunk.get(1).copied().unwrap_or(0);
            let b2 = chunk.get(2).copied().unwrap_or(0);

            result.push(TABLE[(b0 >> 2) as usize] as char);
            result.push(TABLE[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize] as char);

            if chunk.len() > 1 {
                result.push(TABLE[(((b1 & 0x0F) << 2) | (b2 >> 6)) as usize] as char);
            } else {
                result.push('=');
            }

            if chunk.len() > 2 {
                result.push(TABLE[(b2 & 0x3F) as usize] as char);
            } else {
                result.push('=');
            }
        }

        result
    }

    /// Parse color specification from OSC sequence
    /// Supports formats: rgb:RRRR/GGGG/BBBB, #RRGGBB, #RGB
    fn parse_osc_color(color_str: &str) -> Option<Color> {
        let color_str = color_str.trim();

        if color_str.starts_with("rgb:") {
            // rgb:RRRR/GGGG/BBBB format
            let parts: Vec<&str> = color_str[4..].split('/').collect();
            if parts.len() == 3 {
                // Parse hex components (take first 2 digits of each for 8-bit color)
                let r = u8::from_str_radix(&parts[0][..parts[0].len().min(2)], 16).ok()?;
                let g = u8::from_str_radix(&parts[1][..parts[1].len().min(2)], 16).ok()?;
                let b = u8::from_str_radix(&parts[2][..parts[2].len().min(2)], 16).ok()?;
                return Some(Color::new(r, g, b));
            }
        } else if color_str.starts_with('#') {
            // #RRGGBB or #RGB format
            let hex = &color_str[1..];
            if hex.len() == 6 {
                let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
                let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
                let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
                return Some(Color::new(r, g, b));
            } else if hex.len() == 3 {
                let r = u8::from_str_radix(&hex[0..1], 16).ok()? * 17;
                let g = u8::from_str_radix(&hex[1..2], 16).ok()? * 17;
                let b = u8::from_str_radix(&hex[2..3], 16).ok()? * 17;
                return Some(Color::new(r, g, b));
            }
        }
        None
    }

    /// Handle OSC (Operating System Command) sequences
    ///
    /// 🔥 OSC SUPPORT: FROM IGNORED TO IMPLEMENTED 🔥
    /// Before: All OSC sequences ignored
    /// After: Title setting and color customization work
    ///
    /// Implements Phase 1 & 2 from term_analysis.md
    fn handle_osc(&mut self, data: &[u8]) {
        // Parse OSC: number ; parameters
        let s = core::str::from_utf8(data).unwrap_or("");
        let mut parts = s.splitn(2, ';');

        let num_str = parts.next().unwrap_or("");
        let params = parts.next().unwrap_or("");

        // Parse the OSC number
        if let Ok(num) = num_str.parse::<u32>() {
            match num {
                // OSC 0 ; title - Set icon name and window title
                0 => {
                    self.title = String::from(params);
                    #[cfg(feature = "debug-terminal")]
                    os_log::println!("[TERM-OSC] Set title: {}", params);
                }

                // OSC 1 ; title - Set icon name (we just use it as title)
                1 => {
                    self.title = String::from(params);
                }

                // OSC 2 ; title - Set window title
                2 => {
                    self.title = String::from(params);
                }

                // OSC 4 ; index ; colorspec - Set ANSI color
                4 => {
                    // Format: OSC 4 ; index ; colorspec ST
                    let mut parts = params.splitn(2, ';');
                    if let Some(index_str) = parts.next() {
                        if let Some(color_str) = parts.next() {
                            if let Ok(index) = index_str.parse::<u8>() {
                                if let Some(color) = Self::parse_osc_color(color_str) {
                                    self.palette[index as usize] = color;
                                }
                            }
                        }
                    }
                }

                // OSC 10 ; colorspec - Set default foreground color
                10 => {
                    if let Some(color) = Self::parse_osc_color(params) {
                        self.custom_fg = Some(color);
                    }
                }

                // OSC 11 ; colorspec - Set default background color
                11 => {
                    if let Some(color) = Self::parse_osc_color(params) {
                        self.custom_bg = Some(color);
                    }
                }

                // OSC 12 ; colorspec - Set cursor color
                12 => {
                    if let Some(color) = Self::parse_osc_color(params) {
                        self.custom_cursor = Some(color);
                    }
                }

                // OSC 52 ; selection ; data - Clipboard operations
                // Format: OSC 52 ; c ; base64data (c=clipboard, p=primary, s=select)
                // Query: OSC 52 ; c ; ? (responds with current clipboard)
                52 => {
                    let mut parts = params.splitn(2, ';');
                    let _selection = parts.next().unwrap_or("c"); // c=clipboard, p=primary, s=select
                    if let Some(data) = parts.next() {
                        if data.trim() == "?" {
                            // Query clipboard - send response
                            let encoded = Self::base64_encode(self.clipboard.as_bytes());
                            let response = alloc::format!("\x1b]52;c;{}\x07", encoded);
                            crate::send_response(response.as_bytes());
                        } else {
                            // Set clipboard
                            if let Some(decoded) = Self::base64_decode(data.trim()) {
                                if let Ok(text) = core::str::from_utf8(&decoded) {
                                    self.clipboard = String::from(text);
                                }
                            }
                        }
                    }
                }

                // OSC 7 ; file://host/path - Set current working directory
                // -- NightDoc: Track shell CWD for intelligent tools
                7 => {
                    // Format: OSC 7 ; file://hostname/path ST
                    // Shell integration - track current directory
                    // Used by terminal emulators to open new tabs/windows in same dir
                    #[cfg(feature = "debug-terminal")]
                    os_log::println!("[TERM-OSC] Set CWD: {}", params);
                    // We don't store this yet, but acknowledge it
                }

                // OSC 8 ; params ; URI - Hyperlink
                // -- NightDoc: Clickable links in terminal output
                8 => {
                    // Format: OSC 8 ; id=xxx ; URI ST text OSC 8 ;; ST
                    // Hyperlinks in terminal - tmux/editors use this
                    // We acknowledge but don't render differently yet
                    #[cfg(feature = "debug-terminal")]
                    os_log::println!("[TERM-OSC] Hyperlink: {}", params);
                }

                // OSC 104 ; index - Reset ANSI color (or all if no index)
                104 => {
                    if params.is_empty() {
                        // Reset entire palette to defaults
                        for i in 0..256 {
                            let (r, g, b) = vte::color::ansi256_to_rgb(i as u8);
                            self.palette[i] = Color::new(r, g, b);
                        }
                    } else {
                        // Reset specific color
                        if let Ok(index) = params.parse::<u8>() {
                            let (r, g, b) = vte::color::ansi256_to_rgb(index);
                            self.palette[index as usize] = Color::new(r, g, b);
                        }
                    }
                }

                // OSC 110 - Reset default foreground color
                110 => {
                    self.custom_fg = None;
                }

                // OSC 111 - Reset default background color
                111 => {
                    self.custom_bg = None;
                }

                // OSC 112 - Reset cursor color
                112 => {
                    self.custom_cursor = None;
                }

                _ => {
                    // Unknown OSC command - ignore
                    #[cfg(feature = "debug-terminal")]
                    os_log::println!("[TERM-OSC] Unknown OSC {}: {}", num, params);
                }
            }
        }
    }

    /// Handle DCS (Device Control String) sequences
    ///
    /// Implements Phase 3 from term_analysis.md - DCS framework and Sixel graphics
    fn handle_dcs(&mut self, params: &[i32], intermediates: &[u8], final_char: u8, data: &[u8]) {
        // Check for Sixel graphics: DCS P1 ; P2 ; P3 q data ST
        if final_char == b'q' && intermediates.is_empty() {
            // 🔥 PRIORITY #5 FIX - Sixel graphics rendering 🔥
            #[cfg(feature = "debug-terminal")]
            os_log::println!(
                "[TERM-DCS] Sixel graphics ({} bytes) - rendering",
                data.len()
            );

            // Parse and render Sixel data
            self.render_sixel(params, data);
            return;
        }

        // Check for DECRQSS (Request Status String): DCS $ q Pt ST
        // -- IronGhost: Terminal state queries so vim/tmux know what we can do
        // -- NightDoc: Without this, vim screams about unknown terminal caps
        if final_char == b'q' && intermediates.first() == Some(&b'$') {
            #[cfg(feature = "debug-terminal")]
            os_log::println!("[TERM-DCS] DECRQSS query: {:?}", core::str::from_utf8(data));

            self.handle_decrqss(data);
            return;
        }

        // Unknown DCS sequence
        #[cfg(feature = "debug-terminal")]
        os_log::println!(
            "[TERM-DCS] Unknown DCS final={} intermediates={:?}",
            final_char as char,
            intermediates
        );
    }

    /// Handle DECRQSS (DEC Request Status String)
    ///
    /// Responds to terminal state queries from applications like vim/tmux.
    /// Request: DCS $ q Pt ST  →  Response: DCS Ps $ r Pt ST
    /// Ps=1 for valid, Ps=0 for invalid query.
    ///
    /// -- IronGhost: The terminal speaks when asked, vim listens
    /// -- NightDoc: Without DECRQSS, vim can't know our SGR state or cursor shape
    fn handle_decrqss(&mut self, data: &[u8]) {
        use alloc::vec::Vec;

        // Parse query parameter from DCS data
        let query = core::str::from_utf8(data).unwrap_or("");

        match query {
            // SGR (Select Graphic Rendition) attributes query
            "m" => {
                let mut response: Vec<u8> = Vec::with_capacity(64);
                // DCS 1 $ r
                response.extend_from_slice(b"\x1bP1$r");

                // Encode current SGR attributes
                let attrs = &self.handler.attrs;
                let flags = attrs.flags;
                let mut params: Vec<u8> = Vec::new();

                // Attribute flags → SGR codes
                if flags.contains(CellFlags::BOLD) {
                    if !params.is_empty() {
                        params.push(b';');
                    }
                    params.push(b'1');
                }
                if flags.contains(CellFlags::DIM) {
                    if !params.is_empty() {
                        params.push(b';');
                    }
                    params.push(b'2');
                }
                if flags.contains(CellFlags::ITALIC) {
                    if !params.is_empty() {
                        params.push(b';');
                    }
                    params.push(b'3');
                }
                if flags.contains(CellFlags::UNDERLINE) {
                    if !params.is_empty() {
                        params.push(b';');
                    }
                    params.push(b'4');
                }
                if flags.contains(CellFlags::BLINK) {
                    if !params.is_empty() {
                        params.push(b';');
                    }
                    params.push(b'5');
                }
                if flags.contains(CellFlags::REVERSE) {
                    if !params.is_empty() {
                        params.push(b';');
                    }
                    params.push(b'7');
                }
                if flags.contains(CellFlags::HIDDEN) {
                    if !params.is_empty() {
                        params.push(b';');
                    }
                    params.push(b'8');
                }
                if flags.contains(CellFlags::STRIKETHROUGH) {
                    if !params.is_empty() {
                        params.push(b';');
                    }
                    params.push(b'9');
                }

                // Foreground color
                Self::encode_sgr_color(&mut params, &attrs.fg, false);

                // Background color
                Self::encode_sgr_color(&mut params, &attrs.bg, true);

                // Default to "0" (reset) if no attributes set
                if params.is_empty() {
                    params.push(b'0');
                }

                response.extend_from_slice(&params);
                response.push(b'm');
                // ST (String Terminator)
                response.extend_from_slice(b"\x1b\\");

                crate::send_response(&response);
            }

            // DECSCUSR (cursor style) query
            "\" q" | " q" => {
                let shape_code = match self.handler.cursor.shape {
                    CursorShape::Block => b'2',     // steady block
                    CursorShape::Underline => b'4', // steady underline
                    CursorShape::Bar => b'6',       // steady bar
                };

                let response = [
                    0x1b, b'P', b'1', b'$', b'r', shape_code, b' ', b'q', 0x1b, b'\\',
                ];
                crate::send_response(&response);
            }

            // DECSTBM (scroll region) query
            "r" => {
                let top = self.handler.scroll_top + 1;
                let bottom = self.handler.scroll_bottom + 1;
                let region = alloc::format!("\x1bP1$r{};{}r\x1b\\", top, bottom);
                crate::send_response(region.as_bytes());
            }

            // Unknown/unsupported query → invalid response
            _ => {
                #[cfg(feature = "debug-terminal")]
                os_log::println!("[TERM-DCS] DECRQSS unsupported query: {:?}", query);

                // DCS 0 $ r ST (invalid)
                crate::send_response(b"\x1bP0$r\x1b\\");
            }
        }
    }

    /// Encode a terminal color as SGR parameter bytes
    /// -- IronGhost: Translating our color model back into ANSI escape dialect
    fn encode_sgr_color(params: &mut alloc::vec::Vec<u8>, color: &TermColor, is_bg: bool) {
        let base = if is_bg { 40u8 } else { 30u8 };
        let bright_base = if is_bg { 100u8 } else { 90u8 };

        match color {
            TermColor::Ansi16(n) if *n < 8 => {
                if !params.is_empty() {
                    params.push(b';');
                }
                let code = base + n;
                if code >= 100 {
                    params.push(b'0' + code / 100);
                }
                if code >= 10 {
                    params.push(b'0' + (code / 10) % 10);
                }
                params.push(b'0' + code % 10);
            }
            TermColor::Ansi16(n) => {
                if !params.is_empty() {
                    params.push(b';');
                }
                let code = bright_base + (n - 8);
                if code >= 100 {
                    params.push(b'0' + code / 100);
                }
                if code >= 10 {
                    params.push(b'0' + (code / 10) % 10);
                }
                params.push(b'0' + code % 10);
            }
            TermColor::Ansi256(n) => {
                if !params.is_empty() {
                    params.push(b';');
                }
                let ext = if is_bg { b"48;5;" } else { b"38;5;" };
                params.extend_from_slice(ext);
                Self::write_decimal_to(params, *n as u32);
            }
            TermColor::Rgb(r, g, b) => {
                if !params.is_empty() {
                    params.push(b';');
                }
                let ext = if is_bg { b"48;2;" } else { b"38;2;" };
                params.extend_from_slice(ext);
                Self::write_decimal_to(params, *r as u32);
                params.push(b';');
                Self::write_decimal_to(params, *g as u32);
                params.push(b';');
                Self::write_decimal_to(params, *b as u32);
            }
            TermColor::DefaultFg | TermColor::DefaultBg => {
                // Default colors — no SGR code needed
            }
        }
    }

    /// Write a u32 as decimal ASCII bytes into a Vec
    fn write_decimal_to(buf: &mut alloc::vec::Vec<u8>, value: u32) {
        if value == 0 {
            buf.push(b'0');
            return;
        }
        let mut digits = [0u8; 10];
        let mut i = 0;
        let mut v = value;
        while v > 0 {
            digits[i] = b'0' + (v % 10) as u8;
            v /= 10;
            i += 1;
        }
        for d in digits[..i].iter().rev() {
            buf.push(*d);
        }
    }

    /// Render Sixel graphics
    ///
    /// 🔥 PRIORITY #5 FIX - Sixel graphics rendering 🔥
    ///
    /// Sixel Format:
    /// DCS P1 ; P2 ; P3 q data ST
    /// P1 = pixel aspect ratio (optional, 0-9)
    /// P2 = background fill mode (1=transparent, 2=opaque)
    /// P3 = horizontal grid size (optional)
    ///
    /// Sixel Data Commands:
    /// #N         - Select color register N
    /// #N;R;G;B   - Define color N with RGB (0-100 scale)
    /// ! N ch     - Repeat character ch N times
    /// $ - Carriage return (move to start of current sixel line)
    /// - - Line feed (move down 6 pixels)
    /// ? through ~ - Sixel data bytes (6 vertical pixels, offset by 0x3F)
    fn render_sixel(&mut self, params: &[i32], data: &[u8]) {
        // Parse parameters
        let _aspect_ratio = params.get(0).copied().unwrap_or(0);
        let background_mode = params.get(1).copied().unwrap_or(1);
        let _grid_size = params.get(2).copied().unwrap_or(0);

        // Initialize Sixel renderer state
        let mut palette = [Color::VGA_BLACK; 256];
        // Initialize default VT340 palette (first 16 colors)
        palette[0] = Color::new(0, 0, 0); // Black
        palette[1] = Color::new(51, 102, 179); // Blue
        palette[2] = Color::new(179, 0, 0); // Red
        palette[3] = Color::new(51, 179, 51); // Green
        palette[4] = Color::new(179, 0, 179); // Magenta
        palette[5] = Color::new(51, 179, 179); // Cyan
        palette[6] = Color::new(179, 179, 0); // Yellow
        palette[7] = Color::new(204, 204, 204); // Gray
        palette[8] = Color::new(102, 102, 102); // Dark gray
        palette[9] = Color::new(102, 153, 230); // Light blue
        palette[10] = Color::new(230, 102, 102); // Light red
        palette[11] = Color::new(102, 230, 102); // Light green
        palette[12] = Color::new(230, 102, 230); // Light magenta
        palette[13] = Color::new(102, 230, 230); // Light cyan
        palette[14] = Color::new(230, 230, 102); // Light yellow
        palette[15] = Color::new(255, 255, 255); // White

        let mut current_color = 0usize;
        let mut x = 0u32;
        let mut y = 0u32;
        let start_x = self.handler.cursor.col * self.cell_width;
        let start_y = self.handler.cursor.row * self.cell_height;

        let mut i = 0;
        while i < data.len() {
            let byte = data[i];
            i += 1;

            match byte {
                b'#' => {
                    // Color select or define
                    let mut num = 0u32;
                    while i < data.len() && data[i].is_ascii_digit() {
                        num = num * 10 + (data[i] - b'0') as u32;
                        i += 1;
                    }

                    if i < data.len() && data[i] == b';' {
                        // Color definition: #N;mode;R;G;B or #N;R;G;B
                        i += 1;
                        let mut components = Vec::new();
                        loop {
                            let mut comp = 0u32;
                            while i < data.len() && data[i].is_ascii_digit() {
                                comp = comp * 10 + (data[i] - b'0') as u32;
                                i += 1;
                            }
                            components.push(comp);
                            if i >= data.len() || data[i] != b';' {
                                break;
                            }
                            i += 1;
                        }

                        // Parse color definition
                        if components.len() >= 3 {
                            // Could be HLS or RGB mode, assume RGB for simplicity
                            let r = (components[components.len() - 3].min(100) * 255 / 100) as u8;
                            let g = (components[components.len() - 2].min(100) * 255 / 100) as u8;
                            let b = (components[components.len() - 1].min(100) * 255 / 100) as u8;
                            if (num as usize) < palette.len() {
                                palette[num as usize] = Color::new(r, g, b);
                            }
                        }
                    } else {
                        // Color select
                        current_color = (num as usize).min(255);
                    }
                }
                b'!' => {
                    // Repeat next character
                    let mut count = 0u32;
                    while i < data.len() && data[i].is_ascii_digit() {
                        count = count * 10 + (data[i] - b'0') as u32;
                        i += 1;
                    }
                    if i < data.len() {
                        let ch = data[i];
                        i += 1;
                        if ch >= b'?' && ch <= b'~' {
                            let sixel = ch - b'?';
                            for _ in 0..count {
                                self.render_sixel_byte(
                                    sixel,
                                    palette[current_color],
                                    x + start_x,
                                    y + start_y,
                                );
                                x += 1;
                            }
                        }
                    }
                }
                b'$' => {
                    // Carriage return
                    x = 0;
                }
                b'-' => {
                    // Line feed (6 pixels down)
                    x = 0;
                    y += 6;
                }
                b'?'..=b'~' => {
                    // Sixel data byte
                    let sixel = byte - b'?';
                    self.render_sixel_byte(sixel, palette[current_color], x + start_x, y + start_y);
                    x += 1;
                }
                _ => {
                    // Ignore other bytes
                }
            }
        }

        // Mark as dirty for rendering
        self.needs_render = true;
    }

    /// Render a single Sixel byte (6 vertical pixels)
    fn render_sixel_byte(&mut self, sixel: u8, color: Color, x: u32, y: u32) {
        // Each sixel byte encodes 6 vertical pixels
        // Bit 0 (LSB) = top pixel, bit 5 = bottom pixel
        for bit in 0..6 {
            if sixel & (1 << bit) != 0 {
                let px_x = x;
                let px_y = y + bit;
                // Draw pixel via renderer
                self.renderer.draw_pixel(px_x, px_y, color);
            }
        }
    }

    /// Check if any mouse tracking mode is active
    pub fn has_mouse_mode(&self) -> bool {
        self.handler.mouse_mode != MouseMode::None
    }

    /// Get the current mouse mode
    pub fn mouse_mode(&self) -> MouseMode {
        self.handler.mouse_mode
    }

    /// Get the current mouse encoding
    pub fn mouse_encoding(&self) -> MouseEncoding {
        self.handler.mouse_encoding
    }

    /// Generate a mouse escape sequence for a mouse event
    ///
    /// Converts pixel coordinates to cell coordinates and generates the
    /// appropriate escape sequence based on the current mouse mode and encoding.
    ///
    /// # Arguments
    /// * `button` - Button code (0=left, 1=middle, 2=right, 3=release, 64=wheel up, 65=wheel down)
    /// * `x_px` - X position in pixels
    /// * `y_px` - Y position in pixels
    /// * `pressed` - Whether the button was pressed (true) or released (false)
    /// * `motion` - Whether this is a motion event
    ///
    /// Returns the escape sequence bytes, or None if mouse mode doesn't want this event.
    pub fn mouse_event(
        &self,
        button: u8,
        x_px: i32,
        y_px: i32,
        pressed: bool,
        motion: bool,
    ) -> Option<Vec<u8>> {
        if self.handler.mouse_mode == MouseMode::None {
            return None;
        }

        // Convert pixel coordinates to 1-based cell coordinates
        let col = if self.cell_width > 0 {
            (x_px as u32 / self.cell_width) + 1
        } else {
            1
        };
        let row = if self.cell_height > 0 {
            (y_px as u32 / self.cell_height) + 1
        } else {
            1
        };

        // Clamp to terminal dimensions
        let col = col.min(self.cols).max(1);
        let row = row.min(self.rows).max(1);

        // Check if this event should be reported based on mode
        match self.handler.mouse_mode {
            MouseMode::None => return None,
            MouseMode::X10 => {
                // X10: only button press events
                if !pressed || motion {
                    return None;
                }
            }
            MouseMode::Normal => {
                // Normal: press and release, no motion
                if motion {
                    return None;
                }
            }
            MouseMode::ButtonMotion => {
                // Button-event: press, release, and motion while button held
                // (motion events have button + 32)
            }
            MouseMode::AnyMotion => {
                // Any-event: all events including motion without buttons
            }
        }

        // Build the button byte
        let mut btn = button;
        if motion {
            btn += 32; // Motion flag
        }

        // Generate escape sequence based on encoding
        match self.handler.mouse_encoding {
            MouseEncoding::Sgr => {
                // SGR format: ESC [ < btn ; col ; row M (press) or m (release)
                let suffix = if pressed { b'M' } else { b'm' };
                let mut seq = Vec::new();
                seq.extend_from_slice(b"\x1b[<");
                write_decimal(&mut seq, btn as u32);
                seq.push(b';');
                write_decimal(&mut seq, col);
                seq.push(b';');
                write_decimal(&mut seq, row);
                seq.push(suffix);
                Some(seq)
            }
            MouseEncoding::Urxvt => {
                // Urxvt format: ESC [ btn+32 ; col ; row M
                let mut seq = Vec::new();
                seq.extend_from_slice(b"\x1b[");
                write_decimal(&mut seq, (btn as u32) + 32);
                seq.push(b';');
                write_decimal(&mut seq, col);
                seq.push(b';');
                write_decimal(&mut seq, row);
                seq.push(b'M');
                Some(seq)
            }
            MouseEncoding::X10 | MouseEncoding::Utf8 => {
                // X10 format: ESC [ M btn+32 col+32 row+32
                // UTF-8 extends the range but uses same basic format
                let cb = btn + 32;
                let cx = if col > 223 { 0u8 } else { (col as u8) + 32 };
                let cy = if row > 223 { 0u8 } else { (row as u8) + 32 };
                Some(vec![0x1b, b'[', b'M', cb, cx, cy])
            }
        }
    }

    /// Start text selection at pixel coordinates
    ///
    /// Called when left mouse button is pressed without modifiers.
    /// Converts pixel coordinates to cell coordinates. — InputShade
    pub fn start_selection(&mut self, x_px: i32, y_px: i32) {
        let col = if self.cell_width > 0 {
            (x_px as u32 / self.cell_width).min(self.cols - 1)
        } else {
            0
        };
        let row = if self.cell_height > 0 {
            (y_px as u32 / self.cell_height).min(self.rows - 1)
        } else {
            0
        };

        self.selection = Some(Selection {
            start: (col, row),
            end: (col, row),
            active: true,
        });

        // Mark dirty to show selection highlight
        self.renderer.mark_all_dirty();
    }

    /// Update selection end point during mouse drag
    ///
    /// Called on mouse motion while button is held. — InputShade
    pub fn update_selection(&mut self, x_px: i32, y_px: i32) {
        if let Some(ref mut sel) = self.selection {
            if !sel.active {
                return;
            }

            let col = if self.cell_width > 0 {
                (x_px as u32 / self.cell_width).min(self.cols - 1)
            } else {
                0
            };
            let row = if self.cell_height > 0 {
                (y_px as u32 / self.cell_height).min(self.rows - 1)
            } else {
                0
            };

            sel.end = (col, row);
            self.renderer.mark_all_dirty();
        }
    }

    /// Finish selection and copy to clipboard
    ///
    /// Called when left mouse button is released. Extracts selected text
    /// and stores in clipboard. — InputShade
    pub fn finish_selection(&mut self) {
        if let Some(ref mut sel) = self.selection {
            sel.active = false;

            // Extract selected text
            let is_alt = self.handler.modes.contains(TerminalModes::ALT_SCREEN);
            let buffer = if is_alt {
                &self.alternate
            } else {
                &self.primary
            };

            let (start_col, start_row) = sel.start;
            let (end_col, end_row) = sel.end;

            // Normalize selection (ensure start <= end)
            let ((c1, r1), (c2, r2)) =
                if start_row < end_row || (start_row == end_row && start_col <= end_col) {
                    ((start_col, start_row), (end_col, end_row))
                } else {
                    ((end_col, end_row), (start_col, start_row))
                };

            let mut text = String::new();
            for row in r1..=r2 {
                if row >= self.rows {
                    break;
                }
                let start = if row == r1 { c1 } else { 0 };
                let end = if row == r2 { c2 + 1 } else { self.cols };

                for col in start..end {
                    if col >= self.cols {
                        break;
                    }
                    if let Some(cell) = buffer.get(row, col) {
                        text.push(cell.ch);
                    }
                }
                // Add newline between rows (except last)
                if row < r2 {
                    text.push('\n');
                }
            }

            self.clipboard = text;
            self.renderer.mark_all_dirty();
        }
    }

    /// Clear selection
    ///
    /// Called on any terminal write or when user clicks without dragging. — InputShade
    pub fn clear_selection(&mut self) {
        if self.selection.is_some() {
            self.selection = None;
            self.renderer.set_selection(None);
            self.renderer.mark_all_dirty();
        }
    }

    /// Push current selection coordinates to the renderer for highlight painting.
    /// — InputShade: The renderer needs the raw (col, row) range to invert cells.
    fn push_selection_to_renderer(&mut self) {
        let sel_range = self
            .selection
            .map(|sel| (sel.start.0, sel.start.1, sel.end.0, sel.end.1));
        self.renderer.set_selection(sel_range);
    }

    /// Paste from clipboard
    ///
    /// Returns the clipboard content as bytes to be injected as input. — InputShade
    pub fn paste_clipboard(&self) -> Vec<u8> {
        self.clipboard.as_bytes().to_vec()
    }

    /// Render terminal to framebuffer
    /// — GraveShift: Pushes selection state to renderer before paint, so the
    /// inverted highlight is visible. Clears it after to avoid stale ghost rects.
    pub fn render(&mut self) {
        // Reset scroll offset when content changes
        self.scroll_offset = 0;

        // Pass selection to renderer BEFORE borrowing buffer
        self.push_selection_to_renderer();

        let is_alt = self.handler.modes.contains(TerminalModes::ALT_SCREEN);
        let buffer = if is_alt {
            &self.alternate
        } else {
            &self.primary
        };
        let cursor = self.handler.cursor;

        self.renderer.render(buffer, &cursor);
    }

    /// Clear the terminal
    pub fn clear(&mut self) {
        let is_alt = self.handler.modes.contains(TerminalModes::ALT_SCREEN);
        let buffer = if is_alt {
            &mut self.alternate
        } else {
            &mut self.primary
        };
        buffer.clear();
        self.handler.cursor.row = 0;
        self.handler.cursor.col = 0;
        let attrs = self.handler.attrs;
        self.renderer.clear(&attrs);
        self.renderer.invalidate();
    }

    /// Toggle cursor blink state (called by timer)
    pub fn toggle_cursor_blink(&mut self) {
        self.handler.cursor.blink_on = !self.handler.cursor.blink_on;
        // Push selection state before borrowing buffer
        self.push_selection_to_renderer();
        // Always render so the previous cursor cell is cleared; mark both rows dirty happens in renderer
        let is_alt = self.handler.modes.contains(TerminalModes::ALT_SCREEN);
        let buffer = if is_alt {
            &self.alternate
        } else {
            &self.primary
        };
        let cursor = self.handler.cursor;
        self.renderer.render(buffer, &cursor);
    }

    /// Scroll up in scrollback (for viewing history)
    pub fn scroll_view_up(&mut self, lines: usize) {
        let max_offset = self.scrollback.len();
        self.scroll_offset = (self.scroll_offset + lines).min(max_offset);
        self.render_with_scrollback();
    }

    /// Scroll down in scrollback (towards current)
    pub fn scroll_view_down(&mut self, lines: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(lines);
        self.render_with_scrollback();
    }

    /// Render with scrollback offset
    /// — GraveShift: Composites scrollback history + primary buffer into a temp
    /// ScreenBuffer based on scroll_offset. When offset=0, we're at the live view.
    /// When offset>0, older content scrolls into view from the top.
    fn render_with_scrollback(&mut self) {
        // If in alternate screen, scrollback doesn't apply
        let is_alt = self.handler.modes.contains(TerminalModes::ALT_SCREEN);
        if is_alt {
            self.render();
            return;
        }

        if self.scroll_offset == 0 {
            // At live view — normal render path
            self.push_selection_to_renderer();
            let buffer = &self.primary;
            let cursor = self.handler.cursor;
            self.renderer.render(buffer, &cursor);
            return;
        }

        // — GraveShift: Build composited buffer from scrollback + primary.
        // Full content is: [scrollback_line_0 .. scrollback_line_N] [primary_row_0 .. primary_row_M]
        // Viewport shows rows starting from (total_lines - scroll_offset - visible_rows).
        let scrollback_len = self.scrollback.len();
        let visible_rows = self.rows as usize;
        let total_lines = scrollback_len + visible_rows;
        let viewport_start = total_lines.saturating_sub(self.scroll_offset + visible_rows);

        let mut composite = ScreenBuffer::new(self.cols, self.rows);

        for vis_row in 0..self.rows {
            let content_line = viewport_start + vis_row as usize;

            if content_line < scrollback_len {
                // This row comes from scrollback history
                if let Some(sb_line) = self.scrollback.get(content_line) {
                    for (col, cell) in sb_line.iter().enumerate() {
                        if (col as u32) < self.cols {
                            composite.set(vis_row, col as u32, *cell);
                        }
                    }
                }
            } else {
                // This row comes from the primary screen buffer
                let primary_row = (content_line - scrollback_len) as u32;
                if primary_row < self.rows {
                    for col in 0..self.cols {
                        if let Some(cell) = self.primary.get(primary_row, col) {
                            composite.set(vis_row, col, *cell);
                        }
                    }
                }
            }
        }

        // — GraveShift: Hide cursor when scrolled up — it lives in the primary
        // buffer which may not be visible, and rendering it in the wrong row
        // would be confusing.
        let mut cursor = self.handler.cursor;
        cursor.visible = false;

        self.push_selection_to_renderer();
        self.renderer.render(&composite, &cursor);
    }

    /// Reset terminal to initial state
    pub fn reset(&mut self) {
        self.parser.reset();
        self.handler.reset(&mut self.primary);
        self.alternate.clear();
        self.scrollback.clear();
        self.scroll_offset = 0;
        self.renderer.clear(&CellAttrs::default());
        self.renderer.invalidate();
    }

    /// Check if using alternate screen buffer
    pub fn is_alternate_screen(&self) -> bool {
        self.handler.modes.contains(TerminalModes::ALT_SCREEN)
    }

    /// Get scrollback buffer length
    pub fn scrollback_len(&self) -> usize {
        self.scrollback.len()
    }

    /// Enter alternate screen buffer explicitly
    pub fn enter_alternate_screen(&mut self) {
        if !self.handler.modes.contains(TerminalModes::ALT_SCREEN) {
            self.handler.save_cursor();
            self.handler.modes |= TerminalModes::ALT_SCREEN;
            self.alternate.clear();
            self.renderer.invalidate();
            self.render();
        }
    }

    /// Leave alternate screen buffer explicitly
    pub fn leave_alternate_screen(&mut self) {
        if self.handler.modes.contains(TerminalModes::ALT_SCREEN) {
            self.handler.modes &= !TerminalModes::ALT_SCREEN;
            self.handler.restore_cursor();
            self.renderer.invalidate();
            self.render();
        }
    }

    /// Save terminal state for later restoration
    pub fn save_state(&mut self) {
        self.handler.save_cursor();
    }

    /// Restore previously saved terminal state
    pub fn restore_state(&mut self) {
        self.handler.restore_cursor();
        self.renderer.invalidate();
        self.render();
    }
}

/// Write a u32 as decimal digits to a byte vector
fn write_decimal(buf: &mut Vec<u8>, value: u32) {
    if value == 0 {
        buf.push(b'0');
        return;
    }
    let mut digits = [0u8; 10];
    let mut i = 0;
    let mut v = value;
    while v > 0 {
        digits[i] = b'0' + (v % 10) as u8;
        v /= 10;
        i += 1;
    }
    // Reverse: digits are stored least-significant first
    for d in digits[..i].iter().rev() {
        buf.push(*d);
    }
}

/// Global terminal instance
static TERMINAL: Mutex<Option<TerminalEmulator>> = Mutex::new(None);

/// Atomic flag for lock-free initialization check (safe from interrupt context)
static TERMINAL_INITIALIZED: AtomicBool = AtomicBool::new(false);

/// Callback type for terminal query responses (DSR, DA, etc.)
///
/// When terminal receives query sequences like CSI 6 n (cursor position request),
/// it needs to send the response back to the application's stdin.
pub type ResponseCallback = fn(&[u8]);

/// Global response callback for injecting terminal responses into TTY input
static mut RESPONSE_CALLBACK: Option<ResponseCallback> = None;

/// Set the response callback for terminal queries
///
/// # Safety
/// Must be called during single-threaded initialization before any terminal queries
pub unsafe fn set_response_callback(callback: ResponseCallback) {
    unsafe {
        RESPONSE_CALLBACK = Some(callback);
    }
}

/// Send a response to a terminal query (internal helper)
fn send_response(data: &[u8]) {
    #[cfg(feature = "debug-tty-read")]
    {
        os_log::print!("[TERM-RESP] Sending {} bytes: ", data.len());
        for &b in data {
            os_log::print!("{:02x} ", b);
        }
        os_log::println!();
    }

    unsafe {
        if let Some(callback) = RESPONSE_CALLBACK {
            callback(data);
        } else {
            #[cfg(feature = "debug-tty-read")]
            os_log::println!("[TERM-RESP] ERROR: No callback registered!");
        }
    }
}

/// Initialize global terminal with framebuffer
pub fn init(fb: Arc<dyn Framebuffer>) {
    let terminal = TerminalEmulator::new(fb);
    *TERMINAL.lock() = Some(terminal);
    TERMINAL_INITIALIZED.store(true, Ordering::Release);
}

/// Check if terminal is initialized (lock-free, safe from interrupt context)
pub fn is_initialized() -> bool {
    TERMINAL_INITIALIZED.load(Ordering::Acquire)
}

/// Write bytes to global terminal with per-glyph rendering.
/// NOTE: data may point to user memory, so we need STAC/CLAC for SMAP
///
/// — GraveShift: Linux fbcon_putcs() style — synchronous per-glyph rendering.
///
/// Each printable character is painted directly to the framebuffer as it's
/// processed. Scrolls use fb.copy_rect() (pixel memmove) instead of
/// repainting all rows. CSI bulk ops (ED/IL/DL) still set dirty flags
/// for tick() catch-up. Cost is proportional to characters written,
/// not dirty rows — same as Linux's do_con_write() → fbcon_putcs().
///
/// Timer ISR tick() handles cursor blink + catch-up for CSI dirty rows.
pub fn write(data: &[u8]) {
    // — SableWire: Enable access to user pages (STAC - Supervisor-Mode Access Prevention Clear)
    unsafe {
        core::arch::asm!("stac", options(nomem, nostack));
    }

    // — GraveShift: Single lock, per-glyph render, flush, release.
    // write() now paints glyphs inline and flushes the framebuffer before releasing.
    if let Some(ref mut terminal) = *TERMINAL.lock() {
        terminal.write(data);
    }

    // — SableWire: Disable access to user pages (CLAC)
    unsafe {
        core::arch::asm!("clac", options(nomem, nostack));
    }
}

/// Write a single character to global terminal
pub fn putchar(ch: char) {
    let mut buf = [0u8; 4];
    let s = ch.encode_utf8(&mut buf);
    write(s.as_bytes());
}

/// Write a string to global terminal
pub fn puts(s: &str) {
    write(s.as_bytes());
}

/// Clear global terminal
pub fn clear() {
    if let Some(ref mut terminal) = *TERMINAL.lock() {
        terminal.clear();
    }
}

/// Get terminal dimensions
pub fn dimensions() -> Option<(u32, u32)> {
    TERMINAL.lock().as_ref().map(|t| t.dimensions())
}

/// Toggle cursor blink (call from timer)
pub fn toggle_cursor_blink() {
    if let Some(mut guard) = TERMINAL.try_lock() {
        if let Some(ref mut terminal) = *guard {
            terminal.toggle_cursor_blink();
        }
    } else {
        #[cfg(feature = "debug-lock")]
        lock_contention_warning("TERMINAL (toggle_cursor_blink)");
    }
}

/// Reset terminal
pub fn reset() {
    if let Some(ref mut terminal) = *TERMINAL.lock() {
        terminal.reset();
    }
}

/// Timer tick — cursor blink + catch-up render for CSI bulk ops.
/// — GraveShift: write() now renders per-glyph inline (Linux fbcon_putcs style).
/// This tick handles cursor blink (like Linux's fb_flashcursor) and renders
/// any leftover dirty rows from CSI bulk ops (ED/IL/DL/etc that mark_all_dirty).
/// Uses try_lock because we're in ISR context — if write() holds the lock, we
/// skip this tick (the buffer is being updated, next tick will catch it).
pub fn tick() {
    if let Some(mut guard) = TERMINAL.try_lock() {
        if let Some(ref mut terminal) = *guard {
            terminal.tick();
        }
    } else {
        #[cfg(feature = "debug-lock")]
        lock_contention_warning("TERMINAL (tick)");
    }
}

/// Write and immediately render (for urgent/interactive output)
/// NOTE: data may point to user memory, so we need STAC/CLAC for SMAP
pub fn write_immediate(data: &[u8]) {
    // Enable access to user pages (STAC)
    unsafe {
        core::arch::asm!("stac", options(nomem, nostack));
    }

    if let Some(ref mut terminal) = *TERMINAL.lock() {
        terminal.write_immediate(data);
    }

    // Disable access to user pages (CLAC)
    unsafe {
        core::arch::asm!("clac", options(nomem, nostack));
    }
}

/// Check if render is needed
pub fn is_dirty() -> bool {
    if let Some(ref terminal) = *TERMINAL.lock() {
        terminal.needs_render()
    } else {
        false
    }
}

/// Enter alternate screen buffer (for full-screen apps)
pub fn enter_alternate_screen() {
    if let Some(ref mut terminal) = *TERMINAL.lock() {
        terminal.enter_alternate_screen();
    }
}

/// Leave alternate screen buffer (restore primary screen)
pub fn leave_alternate_screen() {
    if let Some(ref mut terminal) = *TERMINAL.lock() {
        terminal.leave_alternate_screen();
    }
}

/// Save current terminal state
pub fn save_state() {
    if let Some(ref mut terminal) = *TERMINAL.lock() {
        terminal.save_state();
    }
}

/// Restore terminal state
pub fn restore_state() {
    if let Some(ref mut terminal) = *TERMINAL.lock() {
        terminal.restore_state();
    }
}

/// Check if mouse tracking mode is active
///
/// Uses try_lock because this is called from timer ISR context (terminal_tick).
/// If the lock is held by process-context code (e.g., terminal::write), we
/// skip rather than deadlock.
pub fn has_mouse_mode() -> bool {
    if let Some(guard) = TERMINAL.try_lock() {
        if let Some(ref terminal) = *guard {
            return terminal.has_mouse_mode();
        }
    } else {
        #[cfg(feature = "debug-lock")]
        lock_contention_warning("TERMINAL (has_mouse_mode)");
    }
    false
}

/// Generate mouse escape sequence for a mouse event
///
/// Returns the escape sequence bytes, or None if mouse mode is not active
/// or doesn't want this event type.
///
/// Uses try_lock because this is called from timer ISR context (terminal_tick).
pub fn mouse_event(
    button: u8,
    x_px: i32,
    y_px: i32,
    pressed: bool,
    motion: bool,
) -> Option<Vec<u8>> {
    if let Some(guard) = TERMINAL.try_lock() {
        if let Some(ref terminal) = *guard {
            return terminal.mouse_event(button, x_px, y_px, pressed, motion);
        }
    } else {
        #[cfg(feature = "debug-lock")]
        lock_contention_warning("TERMINAL (mouse_event)");
    }
    None
}

/// Force render/flush to framebuffer
pub fn flush() {
    if let Some(ref mut terminal) = *TERMINAL.lock() {
        terminal.render();
    }
}

/// Disable terminal rendering (for direct framebuffer access)
pub fn disable() {
    // Terminal stays initialized but stops rendering
    // Graphics apps can write directly to /dev/fb0
}

/// Re-enable terminal rendering
pub fn enable() {
    if let Some(ref mut terminal) = *TERMINAL.lock() {
        terminal.renderer.invalidate();
        terminal.render();
    }
}

/// Scroll terminal view up (towards history) by N lines
///
/// Called from terminal_tick on mouse wheel events. Uses try_lock
/// to avoid deadlock in ISR context. — NeonRoot
pub fn scroll_up(lines: usize) {
    if let Some(mut guard) = TERMINAL.try_lock() {
        if let Some(ref mut terminal) = *guard {
            terminal.scroll_view_up(lines);
        }
    } else {
        #[cfg(feature = "debug-lock")]
        lock_contention_warning("TERMINAL (scroll_up)");
    }
}

/// Scroll terminal view down (towards current) by N lines
///
/// Called from terminal_tick on mouse wheel events. Uses try_lock
/// to avoid deadlock in ISR context. — NeonRoot
pub fn scroll_down(lines: usize) {
    if let Some(mut guard) = TERMINAL.try_lock() {
        if let Some(ref mut terminal) = *guard {
            terminal.scroll_view_down(lines);
        }
    } else {
        #[cfg(feature = "debug-lock")]
        lock_contention_warning("TERMINAL (scroll_down)");
    }
}

/// Start text selection at pixel coordinates
///
/// Called from terminal_tick when left mouse button pressed. — InputShade
pub fn start_selection(x_px: i32, y_px: i32) {
    if let Some(mut guard) = TERMINAL.try_lock() {
        if let Some(ref mut terminal) = *guard {
            terminal.start_selection(x_px, y_px);
        }
    } else {
        #[cfg(feature = "debug-lock")]
        lock_contention_warning("TERMINAL (start_selection)");
    }
}

/// Update selection during mouse drag
///
/// Called from terminal_tick on mouse motion with button held. — InputShade
pub fn update_selection(x_px: i32, y_px: i32) {
    if let Some(mut guard) = TERMINAL.try_lock() {
        if let Some(ref mut terminal) = *guard {
            terminal.update_selection(x_px, y_px);
        }
    } else {
        #[cfg(feature = "debug-lock")]
        lock_contention_warning("TERMINAL (update_selection)");
    }
}

/// Finish selection and copy to clipboard
///
/// Called from terminal_tick when left mouse button released. — InputShade
pub fn finish_selection() {
    if let Some(mut guard) = TERMINAL.try_lock() {
        if let Some(ref mut terminal) = *guard {
            terminal.finish_selection();
        }
    } else {
        #[cfg(feature = "debug-lock")]
        lock_contention_warning("TERMINAL (finish_selection)");
    }
}

/// Clear current selection
///
/// Called when terminal output occurs or user clicks without dragging. — InputShade
pub fn clear_selection() {
    if let Some(mut guard) = TERMINAL.try_lock() {
        if let Some(ref mut terminal) = *guard {
            terminal.clear_selection();
        }
    } else {
        #[cfg(feature = "debug-lock")]
        lock_contention_warning("TERMINAL (clear_selection)");
    }
}

/// Paste clipboard content as input
///
/// Called from terminal_tick on middle-click or Shift+Insert. — InputShade
pub fn paste_clipboard() -> Vec<u8> {
    if let Some(guard) = TERMINAL.try_lock() {
        if let Some(ref terminal) = *guard {
            return terminal.paste_clipboard();
        }
    } else {
        #[cfg(feature = "debug-lock")]
        lock_contention_warning("TERMINAL (paste_clipboard)");
    }
    Vec::new()
}

/// Dump terminal screen buffer to serial port for debugging
///
/// — GraveShift: The nuclear option when you need to see WTF is on screen.
/// Bypasses all buffers, writes raw text directly to COM1. Because sometimes
/// the framebuffer lies and serial is the only truth left.
pub fn debug_dump_screen_to_serial() {
    use arch_x86_64 as arch;

    // Helper to write a byte to serial port 0x3F8 (COM1)
    unsafe fn serial_write(byte: u8) {
        // Wait for transmit holding register empty (THRE)
        unsafe {
            while arch::inb(0x3FD) & 0x20 == 0 {}
            arch::outb(0x3F8, byte);
        }
    }

    // Helper to write a string to serial
    unsafe fn serial_write_str(s: &str) {
        for &byte in s.as_bytes() {
            unsafe {
                serial_write(byte);
            }
        }
    }

    // Helper to write a decimal number
    unsafe fn serial_write_u32(mut n: u32) {
        if n == 0 {
            unsafe {
                serial_write(b'0');
            }
            return;
        }
        let mut buf = [0u8; 10];
        let mut i = 0;
        while n > 0 {
            buf[i] = b'0' + (n % 10) as u8;
            n /= 10;
            i += 1;
        }
        // Write digits in reverse order
        while i > 0 {
            i -= 1;
            unsafe {
                serial_write(buf[i]);
            }
        }
    }

    unsafe {
        serial_write_str("\r\n");
        serial_write_str(
            "╔════════════════════════════════════════════════════════════════════════════╗\r\n",
        );
        serial_write_str(
            "║                    VT SCREEN BUFFER DUMP (SERIAL)                         ║\r\n",
        );
        serial_write_str(
            "╠════════════════════════════════════════════════════════════════════════════╣\r\n",
        );
    }

    if let Some(guard) = TERMINAL.try_lock() {
        if let Some(ref terminal) = *guard {
            let (cols, rows) = terminal.dimensions();

            unsafe {
                serial_write_str("║ Dimensions: ");
                serial_write_u32(cols);
                serial_write_str(" x ");
                serial_write_u32(rows);
                serial_write_str("\r\n");
                serial_write_str(
                    "╠════════════════════════════════════════════════════════════════════════════╣\r\n",
                );
            }

            // Access the primary screen buffer
            let buffer = &terminal.primary;

            // Dump each row
            for row in 0..rows {
                unsafe {
                    serial_write_str("║ ");
                }

                for col in 0..cols {
                    if let Some(cell) = buffer.get(row, col) {
                        let ch = cell.ch;
                        // Convert char to UTF-8 bytes and write
                        let mut utf8_buf = [0u8; 4];
                        let utf8_str = ch.encode_utf8(&mut utf8_buf);
                        unsafe {
                            for &byte in utf8_str.as_bytes() {
                                serial_write(byte);
                            }
                        }
                    } else {
                        unsafe {
                            serial_write(b' ');
                        }
                    }
                }

                unsafe {
                    serial_write_str(" ║\r\n");
                }
            }

            unsafe {
                serial_write_str(
                    "╚════════════════════════════════════════════════════════════════════════════╝\r\n",
                );
                serial_write_str("\r\n");
            }
        } else {
            unsafe {
                serial_write_str(
                    "║ ERROR: Terminal not initialized                                           ║\r\n",
                );
                serial_write_str(
                    "╚════════════════════════════════════════════════════════════════════════════╝\r\n",
                );
            }
        }
    } else {
        unsafe {
            serial_write_str(
                "║ ERROR: Could not lock TERMINAL mutex                                      ║\r\n",
            );
            serial_write_str(
                "╚════════════════════════════════════════════════════════════════════════════╝\r\n",
            );
        }
    }
}
