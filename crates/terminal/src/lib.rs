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

extern crate alloc;

pub mod buffer;
pub mod cell;
pub mod color;
pub mod handler;
pub mod parser;
pub mod renderer;

use alloc::sync::Arc;
use fb::Framebuffer;
use spin::Mutex;

use crate::buffer::{ScreenBuffer, ScrollbackBuffer};
use crate::cell::{CellAttrs, Cursor};
use crate::handler::Handler;
use crate::parser::{Action, Parser};
use crate::renderer::Renderer;

pub use crate::cell::{Cell, CellFlags, CursorShape};
pub use crate::color::TermColor;
pub use crate::handler::TerminalModes;

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
    /// Whether a render is needed (dirty flag)
    needs_render: bool,
}

impl TerminalEmulator {
    /// Create a new terminal emulator with the given framebuffer
    pub fn new(fb: Arc<dyn Framebuffer>) -> Self {
        let renderer = Renderer::new(fb);
        let (cols, rows) = renderer.dimensions();

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

                let scrolled = if is_alt {
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
        if self.handler.cursor.visible {
            // Re-render cursor
            let is_alt = self.handler.modes.contains(TerminalModes::ALT_SCREEN);
            let buffer = if is_alt {
                &self.alternate
            } else {
                &self.primary
            };
            let cursor = self.handler.cursor;
            self.renderer.render(buffer, &cursor);
        }
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

/// Global terminal instance
static TERMINAL: Mutex<Option<TerminalEmulator>> = Mutex::new(None);

/// Initialize global terminal with framebuffer
pub fn init(fb: Arc<dyn Framebuffer>) {
    let terminal = TerminalEmulator::new(fb);
    *TERMINAL.lock() = Some(terminal);
}

/// Check if terminal is initialized
pub fn is_initialized() -> bool {
    TERMINAL.lock().is_some()
}

/// Write bytes to global terminal
pub fn write(data: &[u8]) {
    if let Some(ref mut terminal) = *TERMINAL.lock() {
        terminal.write(data);
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
    if let Some(ref mut terminal) = *TERMINAL.lock() {
        terminal.toggle_cursor_blink();
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
    }
    // If lock is held, skip this tick - next one will catch up
}

/// Write and immediately render (for urgent/interactive output)
pub fn write_immediate(data: &[u8]) {
    if let Some(ref mut terminal) = *TERMINAL.lock() {
        terminal.write_immediate(data);
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
