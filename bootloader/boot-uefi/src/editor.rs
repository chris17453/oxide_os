//! Boot Options Line Editor
//!
//! A minimal single-line text editor for editing kernel boot options.
//! Fixed 256-char buffer, cursor movement, insert/delete — everything
//! you need and nothing you don't.
//!
//! — InputShade: vi for people who only get one line and no escape (well, Escape cancels)

use crate::efi::EfiInputKey;
use crate::efi::text::*;

use crate::config::MAX_OPTIONS;

/// Result of processing a key in the editor
pub enum EditorAction {
    /// Keep editing, editor state changed
    Continue,
    /// User accepted the edit (Enter)
    Accept,
    /// User cancelled the edit (Escape)
    Cancel,
}

/// Single-line text editor with fixed-size buffer
/// — InputShade: 256 bytes of carefully curated boot options
pub struct LineEditor {
    /// The edit buffer
    buf: [u8; MAX_OPTIONS],
    /// Current content length
    len: usize,
    /// Cursor position (0 = before first char)
    cursor: usize,
}

impl LineEditor {
    pub fn new() -> Self {
        Self {
            buf: [0u8; MAX_OPTIONS],
            len: 0,
            cursor: 0,
        }
    }

    /// Initialize editor with existing content
    pub fn set_content(&mut self, content: &[u8]) {
        let copy_len = content.len().min(MAX_OPTIONS - 1);
        self.buf[..copy_len].copy_from_slice(&content[..copy_len]);
        self.len = copy_len;
        self.cursor = copy_len; // cursor at end
    }

    /// Get the current buffer contents
    pub fn buffer(&self) -> &[u8; MAX_OPTIONS] {
        &self.buf
    }

    /// Get current content length
    pub fn len(&self) -> usize {
        self.len
    }

    /// Get cursor position
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// Process a key event. Returns what the caller should do next.
    /// — InputShade: the soul of the editor — each key press carefully considered
    pub fn process_key(&mut self, key: EfiInputKey) -> EditorAction {
        // — InputShade: special keys first. VirtIO keyboard sets scan_code for ALL keys,
        // so unrecognized scan codes fall through to unicode_char check.
        if key.scan_code != SCAN_NULL {
            match key.scan_code {
                SCAN_LEFT => {
                    if self.cursor > 0 {
                        self.cursor -= 1;
                    }
                    return EditorAction::Continue;
                }
                SCAN_RIGHT => {
                    if self.cursor < self.len {
                        self.cursor += 1;
                    }
                    return EditorAction::Continue;
                }
                SCAN_HOME => {
                    self.cursor = 0;
                    return EditorAction::Continue;
                }
                SCAN_END => {
                    self.cursor = self.len;
                    return EditorAction::Continue;
                }
                SCAN_DELETE => {
                    if self.cursor < self.len {
                        // Shift everything after cursor left by one
                        for i in self.cursor..self.len - 1 {
                            self.buf[i] = self.buf[i + 1];
                        }
                        self.len -= 1;
                        self.buf[self.len] = 0;
                    }
                    return EditorAction::Continue;
                }
                SCAN_ESC => {
                    return EditorAction::Cancel;
                }
                _ => {} // — InputShade: fall through to unicode_char for VirtIO compat
            }
        }

        // Printable characters
        let c = key.unicode_char;
        if c == 0 {
            return EditorAction::Continue;
        }

        if c == 0x000D || c == 0x000A {
            // Enter — accept
            return EditorAction::Accept;
        }

        if c == 8 || c == 127 {
            // Backspace
            if self.cursor > 0 {
                for i in (self.cursor - 1)..self.len.saturating_sub(1) {
                    self.buf[i] = self.buf[i + 1];
                }
                self.cursor -= 1;
                self.len -= 1;
                self.buf[self.len] = 0;
            }
            return EditorAction::Continue;
        }

        if c == 21 {
            // Ctrl+U — clear line
            self.buf = [0u8; MAX_OPTIONS];
            self.len = 0;
            self.cursor = 0;
            return EditorAction::Continue;
        }

        // Regular character insert
        if c >= 32 && c < 127 && self.len < MAX_OPTIONS - 1 {
            // Shift everything at and after cursor right by one
            if self.cursor < self.len {
                let mut i = self.len;
                while i > self.cursor {
                    self.buf[i] = self.buf[i - 1];
                    i -= 1;
                }
            }
            self.buf[self.cursor] = c as u8;
            self.cursor += 1;
            self.len += 1;
            return EditorAction::Continue;
        }

        EditorAction::Continue
    }
}
