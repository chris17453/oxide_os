//! Terminal renderer
//!
//! Renders terminal content to framebuffer with double buffering.

extern crate alloc;

use vte::ScreenBuffer;
use vte::{Cell, CellAttrs, CellFlags, Cursor, CursorShape};
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use fb::{Color, Font, Framebuffer, PSF2_FONT};

/// Dirty region tracking
pub struct DirtyRegion {
    /// Dirty rows (true = needs redraw)
    dirty_rows: Vec<bool>,
    /// Full redraw needed
    full_redraw: bool,
}

impl DirtyRegion {
    /// Create new dirty region tracker
    pub fn new(rows: u32) -> Self {
        DirtyRegion {
            dirty_rows: vec![true; rows as usize],
            full_redraw: true,
        }
    }

    /// Mark a row as dirty
    pub fn mark_row(&mut self, row: u32) {
        if let Some(dirty) = self.dirty_rows.get_mut(row as usize) {
            *dirty = true;
        }
    }

    /// Mark range of rows as dirty
    pub fn mark_rows(&mut self, start: u32, end: u32) {
        for row in start..=end {
            self.mark_row(row);
        }
    }

    /// Mark entire screen dirty
    pub fn mark_all(&mut self) {
        self.full_redraw = true;
        for dirty in self.dirty_rows.iter_mut() {
            *dirty = true;
        }
    }

    /// Check if row is dirty
    pub fn is_row_dirty(&self, row: u32) -> bool {
        self.full_redraw || self.dirty_rows.get(row as usize).copied().unwrap_or(false)
    }

    /// Clear all dirty flags
    pub fn clear(&mut self) {
        self.full_redraw = false;
        for dirty in self.dirty_rows.iter_mut() {
            *dirty = false;
        }
    }
}

/// Terminal renderer
pub struct Renderer {
    /// Framebuffer reference
    fb: Arc<dyn Framebuffer>,
    /// Font reference
    font: &'static Font,
    /// Number of columns
    cols: u32,
    /// Number of rows
    rows: u32,
    /// Back buffer for double buffering
    back_buffer: Option<Vec<u8>>,
    /// Dirty region tracking
    dirty: DirtyRegion,
    /// Last rendered cursor position for XOR restore
    last_cursor_row: u32,
    last_cursor_col: u32,
    last_cursor_visible: bool,
}

impl Renderer {
    /// Create a new renderer
    pub fn new(fb: Arc<dyn Framebuffer>) -> Self {
        let font = &PSF2_FONT;
        let cols = fb.width() / font.width;
        let rows = fb.height() / font.height;

        // Allocate back buffer for double buffering
        let back_buffer = if fb.size() > 0 {
            Some(vec![0u8; fb.size()])
        } else {
            None
        };

        Renderer {
            fb,
            font,
            cols,
            rows,
            back_buffer,
            dirty: DirtyRegion::new(rows),
            last_cursor_row: 0,
            last_cursor_col: 0,
            last_cursor_visible: false,
        }
    }

    /// Get terminal dimensions (cols, rows)
    pub fn dimensions(&self) -> (u32, u32) {
        (self.cols, self.rows)
    }

    /// Get cell dimensions in pixels (width, height)
    pub fn cell_dimensions(&self) -> (u32, u32) {
        (self.font.width, self.font.height)
    }

    /// Mark row dirty
    pub fn mark_dirty(&mut self, row: u32) {
        self.dirty.mark_row(row);
    }

    /// Mark all rows dirty
    pub fn mark_all_dirty(&mut self) {
        self.dirty.mark_all();
    }

    /// Render the entire screen
    pub fn render(&mut self, buffer: &ScreenBuffer, cursor: &Cursor) {
        // Ensure rows containing current and previous cursor are redrawn so the cursor
        // doesn't leave artifacts when moving or blinking.
        if self.last_cursor_visible && self.last_cursor_row < self.rows {
            self.dirty.mark_row(self.last_cursor_row);
        }
        if cursor.visible && cursor.blink_on && cursor.row < self.rows {
            self.dirty.mark_row(cursor.row);
        }

        // Render dirty rows
        let mut pixel_count = 0u64;
        for row in 0..self.rows {
            if self.dirty.is_row_dirty(row) {
                self.render_row(buffer, row);
                pixel_count += (self.cols * self.font.width * self.font.height) as u64;
            }
        }

        // Render cursor
        if cursor.visible && cursor.blink_on {
            self.render_cursor(buffer, cursor);
            pixel_count += (self.font.width * self.font.height) as u64;
        }

        // Clear dirty flags
        self.dirty.clear();

        // Track cursor position for next render
        self.last_cursor_row = cursor.row;
        self.last_cursor_col = cursor.col;
        self.last_cursor_visible = cursor.visible && cursor.blink_on;

        // Flush to hardware for immediate display
        self.fb.flush();

        // Record performance metrics
        fb::record_pixels(pixel_count);
        fb::record_flush();
    }

    /// Render a single row
    fn render_row(&self, buffer: &ScreenBuffer, row: u32) {
        let py = row * self.font.height;

        for col in 0..self.cols {
            if let Some(cell) = buffer.get(row, col) {
                let px = col * self.font.width;
                self.render_cell(px, py, cell);
            }
        }
    }

    /// Render a single cell
    fn render_cell(&self, px: u32, py: u32, cell: &Cell) {
        // -- GlassSignal: bridge VTE RGB tuples to framebuffer Color
        let (r, g, b) = cell.attrs.effective_fg().to_rgb(true);
        let fg_color = Color::new(r, g, b);
        let (r, g, b) = cell.attrs.effective_bg().to_rgb(false);
        let bg_color = Color::new(r, g, b);

        // Apply bold by brightening foreground
        let fg_color = if cell.attrs.flags.contains(CellFlags::BOLD) {
            brighten_color(fg_color)
        } else {
            fg_color
        };

        // Draw background
        self.fb
            .fill_rect(px, py, self.font.width, self.font.height, bg_color);

        // Draw character (if not space and not hidden)
        if cell.ch != ' ' && !cell.attrs.flags.contains(CellFlags::HIDDEN) {
            let is_bold = cell.attrs.flags.contains(CellFlags::BOLD);
            let is_italic = cell.attrs.flags.contains(CellFlags::ITALIC);

            if is_italic {
                // Synthetic italic: draw with slant
                self.draw_glyph_italic(px, py, cell.ch, fg_color);
                // For bold italic, add synthetic bold
                if is_bold {
                    self.draw_glyph_italic(px + 1, py, cell.ch, fg_color);
                }
            } else if is_bold {
                // Synthetic bold: draw glyph twice, offset by 1 pixel
                self.draw_glyph(px, py, cell.ch, fg_color);
                self.draw_glyph(px + 1, py, cell.ch, fg_color);
            } else {
                // Normal rendering
                self.draw_glyph(px, py, cell.ch, fg_color);
            }
        }

        // Draw underline
        if cell.attrs.flags.contains(CellFlags::UNDERLINE) {
            let underline_y = py + self.font.height - 2;
            self.fb.hline(px, underline_y, self.font.width, fg_color);
        }

        // Draw strikethrough
        if cell.attrs.flags.contains(CellFlags::STRIKETHROUGH) {
            let strike_y = py + self.font.height / 2;
            self.fb.hline(px, strike_y, self.font.width, fg_color);
        }
    }

    /// Draw a glyph with italic slant
    fn draw_glyph_italic(&self, px: u32, py: u32, ch: char, color: Color) {
        let glyph = self.font.glyph_or_replacement(ch);

        // Simple slant: shift pixels based on row
        // Top rows shift right, bottom rows shift left (or no shift)
        for y in 0..glyph.height {
            // Calculate slant offset: more slant at top, less at bottom
            let slant_offset = if glyph.height > 0 {
                ((glyph.height - y - 1) * 2) / glyph.height  // 0-2 pixel shift
            } else {
                0
            };

            for x in 0..glyph.width {
                if glyph.pixel(x, y) {
                    let offset_x = px + x + slant_offset;
                    self.fb.set_pixel(offset_x, py + y, color);
                }
            }
        }
    }

    /// Draw a glyph
    fn draw_glyph(&self, px: u32, py: u32, ch: char, color: Color) {
        let glyph = self.font.glyph_or_replacement(ch);
        let bpp = self.fb.format().bytes_per_pixel() as usize;
        let stride = self.fb.stride() as usize;
        let buffer = self.fb.buffer();
        let color_bytes = color.to_bytes(self.fb.format());

        unsafe {
            match bpp {
                4 => {
                    // 32-bit: optimized u32 writes
                    let pixel_value = u32::from_le_bytes([
                        color_bytes[0],
                        color_bytes[1],
                        color_bytes[2],
                        color_bytes[3],
                    ]);

                    for y in 0..glyph.height {
                        let line_offset = ((py + y) as usize * stride) + (px as usize * 4);
                        let line_ptr = buffer.add(line_offset) as *mut u32;

                        for x in 0..glyph.width {
                            if glyph.pixel(x, y) {
                                core::ptr::write(line_ptr.add(x as usize), pixel_value);
                            }
                        }
                    }
                }
                2 => {
                    // 16-bit: optimized u16 writes
                    let pixel_value = u16::from_le_bytes([color_bytes[0], color_bytes[1]]);

                    for y in 0..glyph.height {
                        let line_offset = ((py + y) as usize * stride) + (px as usize * 2);
                        let line_ptr = buffer.add(line_offset) as *mut u16;

                        for x in 0..glyph.width {
                            if glyph.pixel(x, y) {
                                core::ptr::write(line_ptr.add(x as usize), pixel_value);
                            }
                        }
                    }
                }
                _ => {
                    // Fallback: pixel-by-pixel
                    for y in 0..glyph.height {
                        for x in 0..glyph.width {
                            if glyph.pixel(x, y) {
                                self.fb.set_pixel(px + x, py + y, color);
                            }
                        }
                    }
                }
            }
        }
    }

    /// Render cursor
    fn render_cursor(&self, buffer: &ScreenBuffer, cursor: &Cursor) {
        if cursor.row >= self.rows || cursor.col >= self.cols {
            return;
        }

        let px = cursor.col * self.font.width;
        let py = cursor.row * self.font.height;

        // Get cell under cursor
        let cell = buffer
            .get(cursor.row, cursor.col)
            .copied()
            .unwrap_or_default();

        // Determine cursor colors (inverted from cell)
        // -- GlassSignal: invert fg/bg for cursor visibility
        let (r, g, b) = cell.attrs.effective_bg().to_rgb(false);
        let fg_color = Color::new(r, g, b);
        let (r, g, b) = cell.attrs.effective_fg().to_rgb(true);
        let bg_color = Color::new(r, g, b);

        match cursor.shape {
            CursorShape::Block => {
                // Draw inverted cell
                self.fb
                    .fill_rect(px, py, self.font.width, self.font.height, bg_color);
                if cell.ch != ' ' {
                    self.draw_glyph(px, py, cell.ch, fg_color);
                }
            }
            CursorShape::Underline => {
                // Draw underline cursor
                let cursor_y = py + self.font.height - 2;
                self.fb
                    .fill_rect(px, cursor_y, self.font.width, 2, bg_color);
            }
            CursorShape::Bar => {
                // Draw vertical bar cursor
                self.fb.fill_rect(px, py, 2, self.font.height, bg_color);
            }
        }
    }

    /// Clear the screen with background color
    pub fn clear(&self, attrs: &CellAttrs) {
        let (r, g, b) = attrs.effective_bg().to_rgb(false);
        let bg = Color::new(r, g, b);
        self.fb.clear(bg);
    }

    /// Scroll the display (for fast scrolling)
    pub fn scroll_up(&self, lines: u32, bg_color: Color) {
        let line_height = self.font.height;
        let scroll_pixels = lines * line_height;
        let total_height = self.rows * line_height;

        if scroll_pixels < total_height {
            // Copy screen content up
            self.fb.copy_rect(
                0,
                scroll_pixels,
                0,
                0,
                self.fb.width(),
                total_height - scroll_pixels,
            );

            // Clear bottom area
            self.fb.fill_rect(
                0,
                total_height - scroll_pixels,
                self.fb.width(),
                scroll_pixels,
                bg_color,
            );
        } else {
            // Scroll more than screen height - just clear
            self.fb.clear(bg_color);
        }
    }

    /// Scroll the display down
    pub fn scroll_down(&self, lines: u32, bg_color: Color) {
        let line_height = self.font.height;
        let scroll_pixels = lines * line_height;
        let total_height = self.rows * line_height;

        if scroll_pixels < total_height {
            // Copy screen content down
            self.fb.copy_rect(
                0,
                0,
                0,
                scroll_pixels,
                self.fb.width(),
                total_height - scroll_pixels,
            );

            // Clear top area
            self.fb
                .fill_rect(0, 0, self.fb.width(), scroll_pixels, bg_color);
        } else {
            self.fb.clear(bg_color);
        }
    }

    /// Force full redraw on next render
    pub fn invalidate(&mut self) {
        self.dirty.mark_all();
    }

    /// Draw a single pixel at the given coordinates
    ///
    /// 🔥 PRIORITY #5 FIX - Sixel graphics rendering support 🔥
    /// Used by Sixel renderer to draw individual pixels directly to framebuffer
    pub fn draw_pixel(&mut self, x: u32, y: u32, color: Color) {
        if x < self.fb.width() && y < self.fb.height() {
            self.fb.set_pixel(x, y, color);
        }
    }
}

/// Brighten a color (for bold text)
fn brighten_color(color: Color) -> Color {
    Color::new(
        color.r.saturating_add(64),
        color.g.saturating_add(64),
        color.b.saturating_add(64),
    )
}
