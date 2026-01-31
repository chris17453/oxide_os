//! Graphical mouse cursor for the framebuffer
//!
//! Renders a pixel-level arrow cursor on top of the framebuffer contents.
//! Uses save/restore of underlying pixels to avoid artifacts.

use crate::color::Color;
use crate::framebuffer::Framebuffer;
use alloc::sync::Arc;
use core::ptr;

/// Cursor sprite dimensions
const CURSOR_WIDTH: usize = 12;
const CURSOR_HEIGHT: usize = 19;

/// Maximum save buffer size (CURSOR_WIDTH * CURSOR_HEIGHT * 4 bytes per pixel)
const SAVE_BUF_SIZE: usize = CURSOR_WIDTH * CURSOR_HEIGHT * 4;

/// Cursor sprite: 0 = transparent, 1 = black (outline), 2 = white (fill)
static CURSOR_SPRITE: [[u8; CURSOR_WIDTH]; CURSOR_HEIGHT] = [
    [1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    [1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    [1, 2, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    [1, 2, 2, 1, 0, 0, 0, 0, 0, 0, 0, 0],
    [1, 2, 2, 2, 1, 0, 0, 0, 0, 0, 0, 0],
    [1, 2, 2, 2, 2, 1, 0, 0, 0, 0, 0, 0],
    [1, 2, 2, 2, 2, 2, 1, 0, 0, 0, 0, 0],
    [1, 2, 2, 2, 2, 2, 2, 1, 0, 0, 0, 0],
    [1, 2, 2, 2, 2, 2, 2, 2, 1, 0, 0, 0],
    [1, 2, 2, 2, 2, 2, 2, 2, 2, 1, 0, 0],
    [1, 2, 2, 2, 2, 2, 2, 2, 2, 2, 1, 0],
    [1, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 1],
    [1, 2, 2, 2, 2, 2, 2, 1, 1, 1, 1, 1],
    [1, 2, 2, 2, 1, 2, 2, 1, 0, 0, 0, 0],
    [1, 2, 2, 1, 0, 1, 2, 2, 1, 0, 0, 0],
    [1, 2, 1, 0, 0, 1, 2, 2, 1, 0, 0, 0],
    [1, 1, 0, 0, 0, 0, 1, 2, 2, 1, 0, 0],
    [1, 0, 0, 0, 0, 0, 1, 2, 2, 1, 0, 0],
    [0, 0, 0, 0, 0, 0, 0, 1, 1, 0, 0, 0],
];

/// Graphical mouse cursor
pub struct MouseCursor {
    /// Current X position in pixels
    x: i32,
    /// Current Y position in pixels
    y: i32,
    /// Whether the cursor is visible
    visible: bool,
    /// Save buffer for pixels under the cursor
    save_buffer: [u8; SAVE_BUF_SIZE],
    /// Whether save_buffer contains valid data
    save_valid: bool,
    /// Saved position (where save_buffer was captured from)
    save_x: i32,
    save_y: i32,
    /// Screen dimensions (cached)
    screen_w: i32,
    screen_h: i32,
}

impl MouseCursor {
    /// Create a new mouse cursor centered on screen
    pub fn new(screen_w: u32, screen_h: u32) -> Self {
        MouseCursor {
            x: screen_w as i32 / 2,
            y: screen_h as i32 / 2,
            visible: true,
            save_buffer: [0; SAVE_BUF_SIZE],
            save_valid: false,
            save_x: 0,
            save_y: 0,
            screen_w: screen_w as i32,
            screen_h: screen_h as i32,
        }
    }

    /// Get current cursor position
    pub fn position(&self) -> (i32, i32) {
        (self.x, self.y)
    }

    /// Check if cursor is visible
    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Show the cursor
    pub fn show(&mut self) {
        self.visible = true;
    }

    /// Hide the cursor (erases from framebuffer)
    pub fn hide(&mut self, fb: &dyn Framebuffer) {
        if self.save_valid {
            self.restore_under(fb);
        }
        self.visible = false;
    }

    /// Move cursor by relative delta and redraw
    ///
    /// This is the main entry point called from terminal_tick().
    /// Performs erase → save → draw in one atomic operation.
    pub fn move_by(&mut self, dx: i32, dy: i32, fb: &dyn Framebuffer) {
        if dx == 0 && dy == 0 {
            return;
        }

        let new_x = (self.x + dx).clamp(0, self.screen_w - 1);
        let new_y = (self.y + dy).clamp(0, self.screen_h - 1);

        self.move_to(new_x, new_y, fb);
    }

    /// Move cursor to absolute position and redraw
    pub fn move_to(&mut self, new_x: i32, new_y: i32, fb: &dyn Framebuffer) {
        if !self.visible {
            self.x = new_x;
            self.y = new_y;
            return;
        }

        // Erase old cursor
        if self.save_valid {
            self.restore_under(fb);
        }

        // Update position
        self.x = new_x;
        self.y = new_y;

        // Save pixels under new position and draw cursor
        self.save_under(fb);
        self.draw_sprite(fb);
    }

    /// Draw cursor at current position (without save/restore)
    ///
    /// Call this after a full screen redraw to put the cursor back on top.
    pub fn redraw(&mut self, fb: &dyn Framebuffer) {
        if !self.visible {
            return;
        }
        self.save_under(fb);
        self.draw_sprite(fb);
    }

    /// Erase cursor from framebuffer (restore saved pixels)
    ///
    /// Call this before a full screen redraw.
    pub fn erase(&mut self, fb: &dyn Framebuffer) {
        if self.save_valid {
            self.restore_under(fb);
        }
    }

    /// Save the pixels under the cursor at the current position
    fn save_under(&mut self, fb: &dyn Framebuffer) {
        let bpp = fb.format().bytes_per_pixel() as usize;
        let stride = fb.stride() as usize;
        let buffer = fb.buffer();
        let fb_w = fb.width() as i32;
        let fb_h = fb.height() as i32;

        self.save_x = self.x;
        self.save_y = self.y;

        for row in 0..CURSOR_HEIGHT {
            let py = self.y + row as i32;
            if py < 0 || py >= fb_h {
                continue;
            }
            for col in 0..CURSOR_WIDTH {
                let px = self.x + col as i32;
                if px < 0 || px >= fb_w {
                    continue;
                }
                if CURSOR_SPRITE[row][col] == 0 {
                    continue; // Transparent — no need to save
                }
                let fb_offset = py as usize * stride + px as usize * bpp;
                let save_offset = (row * CURSOR_WIDTH + col) * bpp;
                if save_offset + bpp <= SAVE_BUF_SIZE {
                    unsafe {
                        ptr::copy_nonoverlapping(
                            buffer.add(fb_offset),
                            self.save_buffer.as_mut_ptr().add(save_offset),
                            bpp,
                        );
                    }
                }
            }
        }

        self.save_valid = true;
    }

    /// Restore saved pixels under the cursor
    fn restore_under(&mut self, fb: &dyn Framebuffer) {
        if !self.save_valid {
            return;
        }

        let bpp = fb.format().bytes_per_pixel() as usize;
        let stride = fb.stride() as usize;
        let buffer = fb.buffer();
        let fb_w = fb.width() as i32;
        let fb_h = fb.height() as i32;

        for row in 0..CURSOR_HEIGHT {
            let py = self.save_y + row as i32;
            if py < 0 || py >= fb_h {
                continue;
            }
            for col in 0..CURSOR_WIDTH {
                let px = self.save_x + col as i32;
                if px < 0 || px >= fb_w {
                    continue;
                }
                if CURSOR_SPRITE[row][col] == 0 {
                    continue;
                }
                let fb_offset = py as usize * stride + px as usize * bpp;
                let save_offset = (row * CURSOR_WIDTH + col) * bpp;
                if save_offset + bpp <= SAVE_BUF_SIZE {
                    unsafe {
                        ptr::copy_nonoverlapping(
                            self.save_buffer.as_ptr().add(save_offset),
                            buffer.add(fb_offset),
                            bpp,
                        );
                    }
                }
            }
        }

        self.save_valid = false;
    }

    /// Draw the cursor sprite at current position
    fn draw_sprite(&self, fb: &dyn Framebuffer) {
        let bpp = fb.format().bytes_per_pixel() as usize;
        let stride = fb.stride() as usize;
        let buffer = fb.buffer();
        let fb_w = fb.width() as i32;
        let fb_h = fb.height() as i32;

        let black = Color::new(0, 0, 0);
        let white = Color::new(255, 255, 255);

        let mut black_bytes = [0u8; 4];
        let mut white_bytes = [0u8; 4];
        black.write_to(&mut black_bytes, fb.format());
        white.write_to(&mut white_bytes, fb.format());

        for row in 0..CURSOR_HEIGHT {
            let py = self.y + row as i32;
            if py < 0 || py >= fb_h {
                continue;
            }
            for col in 0..CURSOR_WIDTH {
                let px = self.x + col as i32;
                if px < 0 || px >= fb_w {
                    continue;
                }
                let pixel = CURSOR_SPRITE[row][col];
                if pixel == 0 {
                    continue; // Transparent
                }
                let color_bytes = if pixel == 1 {
                    &black_bytes
                } else {
                    &white_bytes
                };
                let fb_offset = py as usize * stride + px as usize * bpp;
                unsafe {
                    ptr::copy_nonoverlapping(
                        color_bytes.as_ptr(),
                        buffer.add(fb_offset),
                        bpp,
                    );
                }
            }
        }
    }
}
