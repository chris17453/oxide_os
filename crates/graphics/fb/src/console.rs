//! Framebuffer Text Console

use alloc::sync::Arc;
use alloc::vec::Vec;
use crate::color::Color;
use crate::framebuffer::Framebuffer;
use crate::font::{Font, PSF2_FONT};

/// Console cell
#[derive(Clone, Copy)]
pub struct Cell {
    /// Character
    pub ch: char,
    /// Foreground color
    pub fg: Color,
    /// Background color
    pub bg: Color,
    /// Dirty flag (needs redraw)
    pub dirty: bool,
}

impl Default for Cell {
    fn default() -> Self {
        Cell {
            ch: ' ',
            fg: Color::VGA_LIGHT_GRAY,
            bg: Color::VGA_BLACK,
            dirty: true,
        }
    }
}

/// Framebuffer console
pub struct FbConsole {
    /// Framebuffer
    fb: Arc<dyn Framebuffer>,
    /// Font
    font: &'static Font,
    /// Number of columns
    cols: u32,
    /// Number of rows
    rows: u32,
    /// Cursor X position (column)
    cursor_x: u32,
    /// Cursor Y position (row)
    cursor_y: u32,
    /// Foreground color
    fg_color: Color,
    /// Background color
    bg_color: Color,
    /// Character buffer
    buffer: Vec<Cell>,
    /// Cursor visible
    cursor_visible: bool,
    /// Tab width
    tab_width: u32,
    /// Dirty cells for batched rendering
    dirty_cells: Vec<(u32, u32)>,
}

impl FbConsole {
    /// Create a new framebuffer console
    pub fn new(fb: Arc<dyn Framebuffer>) -> Self {
        let font = &PSF2_FONT;
        let cols = fb.width() / font.width;
        let rows = fb.height() / font.height;

        let mut buffer = Vec::with_capacity((cols * rows) as usize);
        for _ in 0..(cols * rows) {
            buffer.push(Cell::default());
        }

        let mut console = FbConsole {
            fb,
            font,
            cols,
            rows,
            cursor_x: 0,
            cursor_y: 0,
            fg_color: Color::VGA_LIGHT_GRAY,
            bg_color: Color::VGA_BLACK,
            buffer,
            cursor_visible: true,
            tab_width: 8,
            dirty_cells: Vec::new(),
        };

        // Clear screen
        console.clear();

        console
    }

    /// Get number of columns
    pub fn cols(&self) -> u32 {
        self.cols
    }

    /// Get number of rows
    pub fn rows(&self) -> u32 {
        self.rows
    }

    /// Get cursor position
    pub fn cursor(&self) -> (u32, u32) {
        (self.cursor_x, self.cursor_y)
    }

    /// Set cursor position
    pub fn set_cursor(&mut self, x: u32, y: u32) {
        self.cursor_x = x.min(self.cols - 1);
        self.cursor_y = y.min(self.rows - 1);
    }

    /// Set foreground color
    pub fn set_fg_color(&mut self, color: Color) {
        self.fg_color = color;
    }

    /// Set background color
    pub fn set_bg_color(&mut self, color: Color) {
        self.bg_color = color;
    }

    /// Get foreground color
    pub fn fg_color(&self) -> Color {
        self.fg_color
    }

    /// Get background color
    pub fn bg_color(&self) -> Color {
        self.bg_color
    }

    /// Clear the screen
    pub fn clear(&mut self) {
        // Clear buffer
        for cell in self.buffer.iter_mut() {
            cell.ch = ' ';
            cell.fg = self.fg_color;
            cell.bg = self.bg_color;
            cell.dirty = false;
        }

        // Clear framebuffer
        self.fb.clear(self.bg_color);

        // Reset cursor
        self.cursor_x = 0;
        self.cursor_y = 0;
    }

    /// Put a character at current cursor position
    pub fn putchar(&mut self, ch: char) {
        match ch {
            '\n' => {
                self.cursor_x = 0;
                self.cursor_y += 1;
                if self.cursor_y >= self.rows {
                    self.scroll();
                }
            }
            '\r' => {
                self.cursor_x = 0;
            }
            '\t' => {
                let spaces = self.tab_width - (self.cursor_x % self.tab_width);
                for _ in 0..spaces {
                    self.putchar(' ');
                }
            }
            '\x08' => {
                // Backspace
                if self.cursor_x > 0 {
                    self.cursor_x -= 1;
                    self.put_cell(self.cursor_x, self.cursor_y, ' ');
                }
            }
            '\x7F' => {
                // Delete - same as backspace for now
                if self.cursor_x > 0 {
                    self.cursor_x -= 1;
                    self.put_cell(self.cursor_x, self.cursor_y, ' ');
                }
            }
            _ => {
                if ch >= ' ' {
                    self.put_cell(self.cursor_x, self.cursor_y, ch);
                    self.cursor_x += 1;
                    if self.cursor_x >= self.cols {
                        self.cursor_x = 0;
                        self.cursor_y += 1;
                        if self.cursor_y >= self.rows {
                            self.scroll();
                        }
                    }
                }
            }
        }
    }

    /// Put a character at a specific position
    fn put_cell(&mut self, x: u32, y: u32, ch: char) {
        if x >= self.cols || y >= self.rows {
            return;
        }

        let index = (y * self.cols + x) as usize;
        self.buffer[index] = Cell {
            ch,
            fg: self.fg_color,
            bg: self.bg_color,
            dirty: true,
        };

        // Mark for batched rendering instead of immediate draw
        self.dirty_cells.push((x, y));
    }

    /// Draw a cell to the framebuffer
    fn draw_cell(&self, col: u32, row: u32) {
        let index = (row * self.cols + col) as usize;
        let cell = &self.buffer[index];

        let px = col * self.font.width;
        let py = row * self.font.height;

        // Fast background fill using optimized rect fill
        self.fast_fill_rect(px, py, self.font.width, self.font.height, cell.bg);

        // Draw character
        if cell.ch != ' ' {
            self.draw_glyph(px, py, cell.ch, cell.fg);
        }
    }

    /// Draw a glyph at pixel position (ULTRA-OPTIMIZED)
    fn draw_glyph(&self, px: u32, py: u32, ch: char, color: Color) {
        let glyph = self.font.glyph_or_replacement(ch);
        let bpp = self.fb.format().bytes_per_pixel() as usize;
        let stride = self.fb.stride() as usize;
        let buffer = self.fb.buffer();
        
        // Convert color to raw bytes for all pixel formats
        let color_bytes = color.to_bytes(self.fb.format());
        
        unsafe {
            match bpp {
                4 => {
                    // 32-bit pixels: Use u32 writes for maximum speed
                    let pixel_value = u32::from_le_bytes([
                        color_bytes[0], color_bytes[1], color_bytes[2], color_bytes[3]
                    ]);
                    
                    for y in 0..glyph.height {
                        let line_offset = ((py + y) as usize * stride) + (px as usize * 4);
                        let line_ptr = buffer.add(line_offset) as *mut u32;
                        
                        // Batch pixels horizontally - write up to 8 pixels at once
                        let mut x = 0;
                        while x < glyph.width {
                            let batch_end = (x + 8).min(glyph.width);
                            let mut batch_mask = 0u8;
                            
                            // Build bitmask for this batch
                            for bx in x..batch_end {
                                if glyph.pixel(bx, y) {
                                    batch_mask |= 1 << (bx - x);
                                }
                            }
                            
                            // Write pixels in batch based on mask
                            for bx in x..batch_end {
                                if (batch_mask >> (bx - x)) & 1 != 0 {
                                    core::ptr::write_volatile(line_ptr.add((bx) as usize), pixel_value);
                                }
                            }
                            
                            x = batch_end;
                        }
                    }
                },
                3 => {
                    // 24-bit pixels: Use optimized line copying
                    for y in 0..glyph.height {
                        let line_offset = ((py + y) as usize * stride) + (px as usize * 3);
                        let line_ptr = buffer.add(line_offset);
                        
                        // Collect consecutive pixels into runs for bulk copying
                        let mut run_start = None;
                        for x in 0..glyph.width {
                            if glyph.pixel(x, y) {
                                if run_start.is_none() {
                                    run_start = Some(x);
                                }
                            } else {
                                if let Some(start) = run_start {
                                    // Copy run of pixels
                                    let run_len = x - start;
                                    let run_offset = start as usize * 3;
                                    for px_offset in (0..run_len as usize * 3).step_by(3) {
                                        core::ptr::copy_nonoverlapping(
                                            color_bytes.as_ptr(),
                                            line_ptr.add(run_offset + px_offset),
                                            3
                                        );
                                    }
                                    run_start = None;
                                }
                            }
                        }
                        // Handle final run
                        if let Some(start) = run_start {
                            let run_len = glyph.width - start;
                            let run_offset = start as usize * 3;
                            for px_offset in (0..run_len as usize * 3).step_by(3) {
                                core::ptr::copy_nonoverlapping(
                                    color_bytes.as_ptr(),
                                    line_ptr.add(run_offset + px_offset),
                                    3
                                );
                            }
                        }
                    }
                },
                2 => {
                    // 16-bit pixels: Use u16 writes
                    let pixel_value = u16::from_le_bytes([color_bytes[0], color_bytes[1]]);
                    
                    for y in 0..glyph.height {
                        let line_offset = ((py + y) as usize * stride) + (px as usize * 2);
                        let line_ptr = buffer.add(line_offset) as *mut u16;
                        
                        for x in 0..glyph.width {
                            if glyph.pixel(x, y) {
                                core::ptr::write_volatile(line_ptr.add(x as usize), pixel_value);
                            }
                        }
                    }
                },
                _ => {
                    // Fallback for unknown formats - still optimized line-by-line
                    for y in 0..glyph.height {
                        let line_offset = ((py + y) as usize * stride) + (px as usize * bpp);
                        let line_ptr = buffer.add(line_offset);
                        
                        for x in 0..glyph.width {
                            if glyph.pixel(x, y) {
                                let pixel_offset = x as usize * bpp;
                                core::ptr::copy_nonoverlapping(
                                    color_bytes.as_ptr(),
                                    line_ptr.add(pixel_offset),
                                    bpp
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    /// Fast rectangle fill optimized for small character backgrounds
    fn fast_fill_rect(&self, x: u32, y: u32, width: u32, height: u32, color: Color) {
        // Always use the framebuffer's optimized fill_rect - it's much faster
        // than our pixel-by-pixel approach, even for small rectangles
        self.fb.fill_rect(x, y, width, height, color);
    }

    /// Scroll the console up by one line
    fn scroll(&mut self) {
        // Move lines up in buffer
        let cols = self.cols as usize;
        for row in 1..(self.rows as usize) {
            for col in 0..cols {
                self.buffer[(row - 1) * cols + col] = self.buffer[row * cols + col];
            }
        }

        // Clear last line
        let last_row = (self.rows - 1) as usize;
        for col in 0..cols {
            self.buffer[last_row * cols + col] = Cell {
                ch: ' ',
                fg: self.fg_color,
                bg: self.bg_color,
                dirty: true,
            };
        }

        // Scroll framebuffer
        let line_height = self.font.height;
        let total_height = self.rows * line_height;

        self.fb.copy_rect(
            0, line_height,  // src
            0, 0,            // dst
            self.fb.width(),
            total_height - line_height,
        );

        // Clear last line on framebuffer
        self.fb.fill_rect(
            0,
            total_height - line_height,
            self.fb.width(),
            line_height,
            self.bg_color,
        );

        // Clear dirty cells since we just scrolled everything
        self.dirty_cells.clear();

        self.cursor_y = self.rows - 1;
    }

    /// Write a string
    pub fn write_str(&mut self, s: &str) {
        for ch in s.chars() {
            self.putchar(ch);
        }
        // Flush batched rendering after writing string
        self.flush();
    }

    /// Write with ANSI escape sequence parsing (basic subset)
    pub fn write_ansi(&mut self, s: &str) {
        let mut chars = s.chars().peekable();

        while let Some(ch) = chars.next() {
            if ch == '\x1B' {
                // Escape sequence
                if chars.peek() == Some(&'[') {
                    chars.next(); // consume '['
                    self.parse_csi(&mut chars);
                }
            } else {
                self.putchar(ch);
            }
        }
    }

    /// Parse CSI sequence (ESC [ ...)
    fn parse_csi<I: Iterator<Item = char>>(&mut self, chars: &mut I) {
        let mut params = [0u32; 8];
        let mut param_count = 0;
        let mut current_param = 0u32;

        loop {
            match chars.next() {
                Some(ch @ '0'..='9') => {
                    current_param = current_param * 10 + (ch as u32 - '0' as u32);
                }
                Some(';') => {
                    if param_count < 8 {
                        params[param_count] = current_param;
                        param_count += 1;
                    }
                    current_param = 0;
                }
                Some('m') => {
                    // SGR - Select Graphic Rendition
                    if param_count < 8 {
                        params[param_count] = current_param;
                        param_count += 1;
                    }
                    self.handle_sgr(&params[..param_count]);
                    break;
                }
                Some('H') | Some('f') => {
                    // CUP - Cursor Position
                    if param_count < 8 {
                        params[param_count] = current_param;
                        param_count += 1;
                    }
                    let row = params.first().copied().unwrap_or(1).saturating_sub(1);
                    let col = params.get(1).copied().unwrap_or(1).saturating_sub(1);
                    self.set_cursor(col, row);
                    break;
                }
                Some('J') => {
                    // ED - Erase Display
                    if current_param == 2 {
                        self.clear();
                    }
                    break;
                }
                Some('K') => {
                    // EL - Erase Line (not fully implemented)
                    break;
                }
                Some('A') => {
                    // CUU - Cursor Up
                    let n = current_param.max(1);
                    self.cursor_y = self.cursor_y.saturating_sub(n);
                    break;
                }
                Some('B') => {
                    // CUD - Cursor Down
                    let n = current_param.max(1);
                    self.cursor_y = (self.cursor_y + n).min(self.rows - 1);
                    break;
                }
                Some('C') => {
                    // CUF - Cursor Forward
                    let n = current_param.max(1);
                    self.cursor_x = (self.cursor_x + n).min(self.cols - 1);
                    break;
                }
                Some('D') => {
                    // CUB - Cursor Back
                    let n = current_param.max(1);
                    self.cursor_x = self.cursor_x.saturating_sub(n);
                    break;
                }
                _ => break,
            }
        }
    }

    /// Handle SGR (Select Graphic Rendition) parameters
    fn handle_sgr(&mut self, params: &[u32]) {
        if params.is_empty() {
            // Reset
            self.fg_color = Color::VGA_LIGHT_GRAY;
            self.bg_color = Color::VGA_BLACK;
            return;
        }

        let mut i = 0;
        while i < params.len() {
            match params[i] {
                0 => {
                    // Reset
                    self.fg_color = Color::VGA_LIGHT_GRAY;
                    self.bg_color = Color::VGA_BLACK;
                }
                1 => {
                    // Bold (use bright colors)
                    self.fg_color = self.brighten(self.fg_color);
                }
                30..=37 => {
                    // Foreground color
                    self.fg_color = self.ansi_color(params[i] - 30);
                }
                38 => {
                    // Extended foreground
                    if i + 2 < params.len() && params[i + 1] == 5 {
                        // 256-color mode
                        self.fg_color = self.color_256(params[i + 2]);
                        i += 2;
                    }
                }
                39 => {
                    // Default foreground
                    self.fg_color = Color::VGA_LIGHT_GRAY;
                }
                40..=47 => {
                    // Background color
                    self.bg_color = self.ansi_color(params[i] - 40);
                }
                48 => {
                    // Extended background
                    if i + 2 < params.len() && params[i + 1] == 5 {
                        // 256-color mode
                        self.bg_color = self.color_256(params[i + 2]);
                        i += 2;
                    }
                }
                49 => {
                    // Default background
                    self.bg_color = Color::VGA_BLACK;
                }
                90..=97 => {
                    // Bright foreground
                    self.fg_color = self.ansi_color(params[i] - 90 + 8);
                }
                100..=107 => {
                    // Bright background
                    self.bg_color = self.ansi_color(params[i] - 100 + 8);
                }
                _ => {}
            }
            i += 1;
        }
    }

    /// Get ANSI color (0-15)
    fn ansi_color(&self, n: u32) -> Color {
        match n {
            0 => Color::VGA_BLACK,
            1 => Color::VGA_RED,
            2 => Color::VGA_GREEN,
            3 => Color::VGA_BROWN,
            4 => Color::VGA_BLUE,
            5 => Color::VGA_MAGENTA,
            6 => Color::VGA_CYAN,
            7 => Color::VGA_LIGHT_GRAY,
            8 => Color::VGA_DARK_GRAY,
            9 => Color::VGA_LIGHT_RED,
            10 => Color::VGA_LIGHT_GREEN,
            11 => Color::VGA_YELLOW,
            12 => Color::VGA_LIGHT_BLUE,
            13 => Color::VGA_LIGHT_MAGENTA,
            14 => Color::VGA_LIGHT_CYAN,
            15 => Color::VGA_WHITE,
            _ => Color::VGA_LIGHT_GRAY,
        }
    }

    /// Get 256-color palette color
    fn color_256(&self, n: u32) -> Color {
        if n < 16 {
            self.ansi_color(n)
        } else if n < 232 {
            // 6x6x6 color cube
            let n = n - 16;
            let r = ((n / 36) % 6) as u8 * 51;
            let g = ((n / 6) % 6) as u8 * 51;
            let b = (n % 6) as u8 * 51;
            Color::new(r, g, b)
        } else {
            // Grayscale
            let gray = ((n - 232) * 10 + 8) as u8;
            Color::new(gray, gray, gray)
        }
    }

    /// Brighten a color
    fn brighten(&self, color: Color) -> Color {
        Color::new(
            color.r.saturating_add(64),
            color.g.saturating_add(64),
            color.b.saturating_add(64),
        )
    }

    /// Flush all dirty cells to the framebuffer
    pub fn flush(&mut self) {
        // Render all dirty cells in batch
        for &(x, y) in &self.dirty_cells {
            self.draw_cell(x, y);
        }
        self.dirty_cells.clear();
    }
}

impl core::fmt::Write for FbConsole {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        self.write_str(s);
        Ok(())
    }
}
