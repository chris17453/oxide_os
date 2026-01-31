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

pub mod buffer;
pub mod cell;
pub mod color;
pub mod handler;
pub mod parser;
pub mod renderer;

use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, Ordering};
use fb::Framebuffer;
use spin::Mutex;

use crate::buffer::{ScreenBuffer, ScrollbackBuffer};
use crate::cell::{CellAttrs, Cursor};
use crate::handler::Handler;
use crate::parser::{Action, Parser};
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

pub use crate::cell::{Cell, CellFlags, CursorShape};
pub use crate::color::TermColor;
pub use crate::handler::{MouseEncoding, MouseMode, TerminalModes};

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
}

impl TerminalEmulator {
    /// Create a new terminal emulator with the given framebuffer
    pub fn new(fb: Arc<dyn Framebuffer>) -> Self {
        let renderer = Renderer::new(fb);
        let (cols, rows) = renderer.dimensions();
        let (cell_width, cell_height) = renderer.cell_dimensions();

        TerminalEmulator {
            parser: Parser::new(),
            handler: Handler::new(cols, rows),
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

    /// Get current cell attributes
    pub fn attrs(&self) -> &CellAttrs {
        &self.handler.attrs
    }

    /// Write bytes to the terminal (rendering deferred to tick())
    pub fn write(&mut self, data: &[u8]) {
        for &byte in data {
            self.process_byte(byte);
        }
        self.needs_render = true;
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

    /// Write and immediately render (for urgent output)
    pub fn write_immediate(&mut self, data: &[u8]) {
        for &byte in data {
            self.process_byte(byte);
        }
        self.render();
        self.needs_render = false;
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
                self.handler.put_char(ch, buffer);
                self.renderer.mark_dirty(self.handler.cursor.row);
            }
            Action::Execute(byte) => {
                self.execute_control(byte);
            }
            Action::CsiDispatch {
                params,
                intermediates,
                final_char,
            } => {
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

                // Mark affected rows dirty
                self.renderer.mark_all_dirty();
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
                self.renderer.mark_all_dirty();
            }
            Action::OscDispatch(_data) => {
                // OSC commands (title, colors, etc.) - mostly ignored for now
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
                // BS - Backspace
                self.handler.backspace();
                self.renderer.mark_dirty(self.handler.cursor.row);
            }
            0x09 => {
                // HT - Horizontal Tab
                self.handler.tab();
            }
            0x0A | 0x0B | 0x0C => {
                // LF, VT, FF - Line feed with implicit CR (standard terminal behavior)
                let old_row = self.handler.cursor.row;

                // Implicit carriage return - most terminal output expects this
                self.handler.carriage_return();

                let is_alt = self.handler.modes.contains(TerminalModes::ALT_SCREEN);

                let _scrolled = if is_alt {
                    self.handler.linefeed(&mut self.alternate, None)
                } else {
                    self.handler
                        .linefeed(&mut self.primary, Some(&mut self.scrollback))
                };

                self.renderer.mark_dirty(old_row);
                self.renderer.mark_dirty(self.handler.cursor.row);

                // If we scrolled (cursor stayed at same row), mark all rows for redraw
                if self.handler.cursor.row == old_row {
                    self.renderer.mark_all_dirty();
                }
            }
            0x0D => {
                // CR - Carriage return (explicit, move to column 0)
                self.handler.carriage_return();
            }
            0x0E => {
                // SO - Shift Out (ignored)
            }
            0x0F => {
                // SI - Shift In (ignored)
            }
            _ => {}
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

    /// Render terminal to framebuffer
    pub fn render(&mut self) {
        // Reset scroll offset when content changes
        self.scroll_offset = 0;

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
    fn render_with_scrollback(&mut self) {
        // If in alternate screen, scrollback doesn't apply
        let is_alt = self.handler.modes.contains(TerminalModes::ALT_SCREEN);
        if is_alt {
            self.render();
            return;
        }

        // For now, just do a normal render
        // Full scrollback rendering would need to composite scrollback + primary buffer
        self.render();
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

/// Write bytes to global terminal
/// NOTE: data may point to user memory, so we need STAC/CLAC for SMAP
pub fn write(data: &[u8]) {
    // Enable access to user pages (STAC - Supervisor-Mode Access Prevention Clear)
    unsafe {
        core::arch::asm!("stac", options(nomem, nostack));
    }

    if let Some(ref mut terminal) = *TERMINAL.lock() {
        terminal.write(data);
    }

    // Disable access to user pages (CLAC - Supervisor-Mode Access Prevention Clear)
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

/// Tick function - call at 30 FPS from timer interrupt to render pending changes
/// Uses try_lock to avoid deadlock if main thread holds the lock
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
