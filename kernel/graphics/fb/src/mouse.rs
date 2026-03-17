//! Graphical mouse cursor for the framebuffer
//!
//! Renders a pixel-level arrow cursor on top of the framebuffer contents.
//! Uses save/restore of underlying pixels to avoid artifacts.

use crate::color::Color;
use crate::framebuffer::Framebuffer;
use alloc::sync::Arc;
use core::ptr;

/// Cursor sprite dimensions (source pattern)
const CURSOR_WIDTH: usize = 12;
const CURSOR_HEIGHT: usize = 19;

/// — NeonVale: render scale factor. 2x makes the cursor actually visible
/// on 1280x800+ displays instead of being a 12px speck you need a magnifying
/// glass to find. Each sprite pixel becomes a SCALE×SCALE block on hw_fb.
const CURSOR_SCALE: usize = 2;

/// Rendered cursor dimensions on screen
const RENDER_WIDTH: usize = CURSOR_WIDTH * CURSOR_SCALE;
const RENDER_HEIGHT: usize = CURSOR_HEIGHT * CURSOR_SCALE;

/// Maximum save buffer size (RENDER_WIDTH * RENDER_HEIGHT * 4 bytes per pixel)
const SAVE_BUF_SIZE: usize = RENDER_WIDTH * RENDER_HEIGHT * 4;

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
    pub x: i32,
    /// Current Y position in pixels
    pub y: i32,
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
    pub screen_w: i32,
    pub screen_h: i32,
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

    /// — NeonVale: get the cursor's bounding box for dirty rect tracking.
    /// Returns (x, y, w, h) clamped to screen. Covers both current position
    /// and saved position (erase area) for correct damage accumulation.
    pub fn bounds(&self) -> (u32, u32, u32, u32) {
        let cx = self.x.max(0) as u32;
        let cy = self.y.max(0) as u32;
        let sx = self.save_x.max(0) as u32;
        let sy = self.save_y.max(0) as u32;
        let cw = RENDER_WIDTH as u32;
        let ch = RENDER_HEIGHT as u32;
        // — NeonVale: union of current and saved positions
        let x_min = cx.min(sx);
        let y_min = cy.min(sy);
        let x_max = (cx + cw).max(sx + cw).min(self.screen_w as u32);
        let y_max = (cy + ch).max(sy + ch).min(self.screen_h as u32);
        (x_min, y_min, x_max.saturating_sub(x_min), y_max.saturating_sub(y_min))
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

    /// — NeonVale: Save the pixels under the cursor at the current position.
    /// Uses RENDER_WIDTH/RENDER_HEIGHT (scaled dimensions) so save covers
    /// the full 2x rendered area on hw_fb. Each sprite pixel maps to a
    /// SCALE×SCALE block — we save every pixel in those blocks.
    fn save_under(&mut self, fb: &dyn Framebuffer) {
        let bpp = fb.format().bytes_per_pixel() as usize;
        let stride = fb.stride() as usize;
        let buffer = fb.buffer();
        let fb_w = fb.width() as i32;
        let fb_h = fb.height() as i32;

        self.save_x = self.x;
        self.save_y = self.y;

        for row in 0..RENDER_HEIGHT {
            let py = self.y + row as i32;
            if py < 0 || py >= fb_h {
                continue;
            }
            let sprite_row = row / CURSOR_SCALE;
            for col in 0..RENDER_WIDTH {
                let px = self.x + col as i32;
                if px < 0 || px >= fb_w {
                    continue;
                }
                let sprite_col = col / CURSOR_SCALE;
                if CURSOR_SPRITE[sprite_row][sprite_col] == 0 {
                    continue;
                }
                let fb_offset = py as usize * stride + px as usize * bpp;
                let save_offset = (row * RENDER_WIDTH + col) * bpp;
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

    /// Restore saved pixels under the cursor (scaled dimensions)
    fn restore_under(&mut self, fb: &dyn Framebuffer) {
        if !self.save_valid {
            return;
        }

        let bpp = fb.format().bytes_per_pixel() as usize;
        let stride = fb.stride() as usize;
        let buffer = fb.buffer();
        let fb_w = fb.width() as i32;
        let fb_h = fb.height() as i32;

        for row in 0..RENDER_HEIGHT {
            let py = self.save_y + row as i32;
            if py < 0 || py >= fb_h {
                continue;
            }
            let sprite_row = row / CURSOR_SCALE;
            for col in 0..RENDER_WIDTH {
                let px = self.save_x + col as i32;
                if px < 0 || px >= fb_w {
                    continue;
                }
                let sprite_col = col / CURSOR_SCALE;
                if CURSOR_SPRITE[sprite_row][sprite_col] == 0 {
                    continue;
                }
                let fb_offset = py as usize * stride + px as usize * bpp;
                let save_offset = (row * RENDER_WIDTH + col) * bpp;
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

    /// — NeonVale: Draw the cursor sprite at current position, scaled 2x.
    /// Outline uses dark gray (48,48,48) instead of pure black so you can
    /// actually SEE it on dark backgrounds. Each sprite pixel becomes a
    /// SCALE×SCALE block on the framebuffer. The difference between
    /// "where the hell is my cursor" and "oh there it is." — NeonVale
    fn draw_sprite(&self, fb: &dyn Framebuffer) {
        let bpp = fb.format().bytes_per_pixel() as usize;
        let stride = fb.stride() as usize;
        let buffer = fb.buffer();
        let fb_w = fb.width() as i32;
        let fb_h = fb.height() as i32;

        // — NeonVale: outline = dark gray, visible on both light AND dark backgrounds.
        // Pure black outline on a black terminal = invisible cursor = user rage.
        let outline = Color::new(48, 48, 48);
        let fill = Color::new(255, 255, 255);

        let mut outline_bytes = [0u8; 4];
        let mut fill_bytes = [0u8; 4];
        outline.write_to(&mut outline_bytes, fb.format());
        fill.write_to(&mut fill_bytes, fb.format());

        for row in 0..CURSOR_HEIGHT {
            for col in 0..CURSOR_WIDTH {
                let pixel = CURSOR_SPRITE[row][col];
                if pixel == 0 {
                    continue;
                }
                let color_bytes = if pixel == 1 {
                    &outline_bytes
                } else {
                    &fill_bytes
                };
                // — NeonVale: stamp a SCALE×SCALE block for each sprite pixel
                for sy in 0..CURSOR_SCALE {
                    let py = self.y + (row * CURSOR_SCALE + sy) as i32;
                    if py < 0 || py >= fb_h {
                        continue;
                    }
                    for sx in 0..CURSOR_SCALE {
                        let px = self.x + (col * CURSOR_SCALE + sx) as i32;
                        if px < 0 || px >= fb_w {
                            continue;
                        }
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
    }
}
