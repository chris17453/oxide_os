//! Terminal renderer
//!
//! Renders terminal content to framebuffer with double buffering.
//! Now with FontManager fallback chain — box drawing, block elements,
//! and wide character rendering. The pixels bow to no 7-bit tyrant. — SoftGlyph

extern crate alloc;

use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use fb::{Color, Font, FontManager, Framebuffer, GlyphData, PSF2_FONT};
use vte::ScreenBuffer;
use vte::{Cell, CellAttrs, CellFlags, Cursor, CursorShape};

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

    /// Get min/max dirty row indices (inclusive)
    /// — GlassSignal: the bounding box of what actually changed — nothing more
    pub fn dirty_bounds(&self) -> Option<(u32, u32)> {
        let mut min_row = None;
        let mut max_row = None;
        for (i, &dirty) in self.dirty_rows.iter().enumerate() {
            if dirty || self.full_redraw {
                let row = i as u32;
                if min_row.is_none() {
                    min_row = Some(row);
                }
                max_row = Some(row);
            }
        }
        min_row.zip(max_row)
    }
}

/// Terminal renderer
/// Wields a FontManager fallback chain for full Unicode glyph resolution.
/// Wide chars get two cells. Box drawing gets real lines. — SoftGlyph
pub struct Renderer {
    /// Framebuffer reference
    fb: Arc<dyn Framebuffer>,
    /// Legacy font reference (kept for cell dimension calculations)
    font: &'static Font,
    /// Extended font manager with fallback chain — SoftGlyph
    font_manager: FontManager,
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
    /// Blink state counter — text blink toggles every N renders
    /// Ticks up each render; blink text visible when bit 4 is set (~500ms at 30fps). — SoftGlyph
    blink_counter: u32,
    /// Active selection range: (start_col, start_row, end_col, end_row)
    /// — InputShade: Ghost coordinates marking the user's text selection.
    /// When set, selected cells render with swapped fg/bg for that inverted glow.
    selection: Option<(u32, u32, u32, u32)>,
}

impl Renderer {
    /// Create a new renderer
    pub fn new(fb: Arc<dyn Framebuffer>) -> Self {
        let font = &PSF2_FONT;
        // Bootstrap the font manager with the extended built-in font — SoftGlyph
        let font_manager = FontManager::with_builtin();
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
            font_manager,
            cols,
            rows,
            back_buffer,
            dirty: DirtyRegion::new(rows),
            last_cursor_row: 0,
            last_cursor_col: 0,
            last_cursor_visible: false,
            blink_counter: 0,
            selection: None,
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

    /// Set the active text selection range for highlight rendering.
    /// Coordinates are (start_col, start_row, end_col, end_row).
    /// Pass None to clear selection. — InputShade
    pub fn set_selection(&mut self, selection: Option<(u32, u32, u32, u32)>) {
        self.selection = selection;
    }

    /// Check if a cell at (row, col) falls within the active selection.
    /// Returns true if the cell should be rendered with inverted colors.
    /// — InputShade: Selection geometry — the neon highlight mask.
    fn is_cell_selected(&self, row: u32, col: u32) -> bool {
        let (sc, sr, ec, er) = match self.selection {
            Some(sel) => sel,
            None => return false,
        };

        // Normalize so (r1, c1) <= (r2, c2)
        let ((c1, r1), (c2, r2)) = if sr < er || (sr == er && sc <= ec) {
            ((sc, sr), (ec, er))
        } else {
            ((ec, er), (sc, sr))
        };

        if row < r1 || row > r2 {
            return false;
        }
        if r1 == r2 {
            // Single row selection
            col >= c1 && col <= c2
        } else if row == r1 {
            col >= c1
        } else if row == r2 {
            col <= c2
        } else {
            // Middle rows are fully selected
            true
        }
    }

    /// Render the entire screen
    pub fn render(&mut self, buffer: &ScreenBuffer, cursor: &Cursor) {
        // Advance blink counter — text blink toggles at ~2Hz (every 15 frames at 30fps)
        // When blink state transitions, mark all rows dirty so blink cells redraw. — SoftGlyph
        let old_blink_on = (self.blink_counter / 15) % 2 == 0;
        self.blink_counter = self.blink_counter.wrapping_add(1);
        let new_blink_on = (self.blink_counter / 15) % 2 == 0;
        if old_blink_on != new_blink_on {
            self.dirty.mark_all();
        }

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

        // Compute dirty pixel bounds BEFORE clearing flags
        // — GlassSignal: row-granularity → pixel-rect → surgical GPU flush
        let dirty_pixel_bounds = self.dirty.dirty_bounds().map(|(min_row, max_row)| {
            let y = min_row * self.font.height;
            let h = (max_row - min_row + 1) * self.font.height;
            (0u32, y, self.fb.width(), h)
        });

        // Clear dirty flags
        self.dirty.clear();

        // Track cursor position for next render
        self.last_cursor_row = cursor.row;
        self.last_cursor_col = cursor.col;
        self.last_cursor_visible = cursor.visible && cursor.blink_on;

        // Flush only the dirty region to hardware
        // — GlassSignal: 1 dirty row on a 60-row terminal = 1/60th the bandwidth
        if let Some((x, y, w, h)) = dirty_pixel_bounds {
            self.fb.flush_region(x, y, w, h);
        }

        // Record performance metrics
        fb::record_pixels(pixel_count);
        fb::record_flush();
    }

    /// Render a single row
    /// Handles WIDE cells (2-cell glyphs) and skips WIDE_CONTINUATION placeholders.
    /// — SoftGlyph: Now also checks selection state to invert selected cells.
    fn render_row(&self, buffer: &ScreenBuffer, row: u32) {
        let py = row * self.font.height;

        let mut col = 0u32;
        while col < self.cols {
            if let Some(cell) = buffer.get(row, col) {
                // Skip continuation cells — they're just placeholders for wide chars
                if cell.attrs.flags.contains(CellFlags::WIDE_CONTINUATION) {
                    col += 1;
                    continue;
                }

                let px = col * self.font.width;
                let selected = self.is_cell_selected(row, col);

                if cell.attrs.flags.contains(CellFlags::WIDE) {
                    // Wide character: render across 2 cells
                    self.render_cell_inner(px, py, cell, 2, selected);
                    col += 2;
                } else {
                    self.render_cell_inner(px, py, cell, 1, selected);
                    col += 1;
                }
            } else {
                col += 1;
            }
        }
    }

    /// Check if text blink is currently in the "visible" phase
    /// Blink text shows for ~500ms, hides for ~500ms. — SoftGlyph
    fn blink_text_visible(&self) -> bool {
        (self.blink_counter / 15) % 2 == 0
    }

    /// Inner render logic shared by normal and wide cell paths.
    /// — InputShade: `selected` flag triggers reverse-video for the selection overlay.
    fn render_cell_inner(&self, px: u32, py: u32, cell: &Cell, cell_count: u32, selected: bool) {
        // -- GlassSignal: bridge VTE RGB tuples to framebuffer Color
        let (r, g, b) = cell.attrs.effective_fg().to_rgb(true);
        let mut fg_color = Color::new(r, g, b);
        let (r, g, b) = cell.attrs.effective_bg().to_rgb(false);
        let mut bg_color = Color::new(r, g, b);

        // Apply bold by brightening foreground
        fg_color = if cell.attrs.flags.contains(CellFlags::BOLD) {
            brighten_color(fg_color)
        } else {
            fg_color
        };

        // — InputShade: Selection inversion — swap fg/bg for that neon highlight.
        // Applied after bold so the highlighted text stays readable.
        if selected {
            core::mem::swap(&mut fg_color, &mut bg_color);
        }

        let total_width = self.font.width * cell_count;

        // Draw background (covers all cells for wide chars)
        self.fb
            .fill_rect(px, py, total_width, self.font.height, bg_color);

        // Blink text: when in "off" phase, only draw background — SoftGlyph
        let is_blink = cell.attrs.flags.contains(CellFlags::BLINK);
        if is_blink && !self.blink_text_visible() {
            return;
        }

        // Draw character (if not space and not hidden)
        if cell.ch != ' ' && !cell.attrs.flags.contains(CellFlags::HIDDEN) {
            let resolved = self.font_manager.resolve(cell.ch);
            let is_bold = cell.attrs.flags.contains(CellFlags::BOLD);
            let is_italic = cell.attrs.flags.contains(CellFlags::ITALIC);

            match resolved.data {
                GlyphData::Bitmap { width, height, data } => {
                    if is_italic {
                        self.draw_bitmap_italic(px, py, width, height, data, fg_color);
                        if is_bold {
                            self.draw_bitmap_italic(px + 1, py, width, height, data, fg_color);
                        }
                    } else if is_bold {
                        self.draw_bitmap_glyph(px, py, width, height, data, fg_color);
                        self.draw_bitmap_glyph(px + 1, py, width, height, data, fg_color);
                    } else {
                        self.draw_bitmap_glyph(px, py, width, height, data, fg_color);
                    }
                }
                GlyphData::Rgba { width, height, data } => {
                    // RGBA emoji/color glyph — alpha blend onto background
                    // Bold/italic don't apply to color glyphs — SoftGlyph
                    self.draw_rgba_glyph(px, py, width, height, data);
                }
            }
        }

        // Draw underline (spans full cell width)
        if cell.attrs.flags.contains(CellFlags::UNDERLINE) {
            let underline_y = py + self.font.height - 2;
            self.fb.hline(px, underline_y, total_width, fg_color);
        }

        // Draw strikethrough (spans full cell width)
        if cell.attrs.flags.contains(CellFlags::STRIKETHROUGH) {
            let strike_y = py + self.font.height / 2;
            self.fb.hline(px, strike_y, total_width, fg_color);
        }
    }

    /// Draw a 1-bit monochrome bitmap glyph with optimized pixel writes — SoftGlyph
    fn draw_bitmap_glyph(&self, px: u32, py: u32, glyph_w: u32, glyph_h: u32, glyph_data: &[u8], color: Color) {
        let bpp = self.fb.format().bytes_per_pixel() as usize;
        let stride = self.fb.stride() as usize;
        let buffer = self.fb.buffer();
        let color_bytes = color.to_bytes(self.fb.format());
        let bytes_per_row = (glyph_w + 7) / 8;

        unsafe {
            match bpp {
                4 => {
                    let pixel_value = u32::from_le_bytes([
                        color_bytes[0],
                        color_bytes[1],
                        color_bytes[2],
                        color_bytes[3],
                    ]);

                    for y in 0..glyph_h {
                        let line_offset = ((py + y) as usize * stride) + (px as usize * 4);
                        let line_ptr = buffer.add(line_offset) as *mut u32;

                        for x in 0..glyph_w {
                            let byte_idx = (y * bytes_per_row + x / 8) as usize;
                            let bit_idx = 7 - (x % 8);
                            if byte_idx < glyph_data.len() && (glyph_data[byte_idx] >> bit_idx) & 1 != 0 {
                                core::ptr::write(line_ptr.add(x as usize), pixel_value);
                            }
                        }
                    }
                }
                2 => {
                    let pixel_value = u16::from_le_bytes([color_bytes[0], color_bytes[1]]);

                    for y in 0..glyph_h {
                        let line_offset = ((py + y) as usize * stride) + (px as usize * 2);
                        let line_ptr = buffer.add(line_offset) as *mut u16;

                        for x in 0..glyph_w {
                            let byte_idx = (y * bytes_per_row + x / 8) as usize;
                            let bit_idx = 7 - (x % 8);
                            if byte_idx < glyph_data.len() && (glyph_data[byte_idx] >> bit_idx) & 1 != 0 {
                                core::ptr::write(line_ptr.add(x as usize), pixel_value);
                            }
                        }
                    }
                }
                _ => {
                    for y in 0..glyph_h {
                        for x in 0..glyph_w {
                            let byte_idx = (y * bytes_per_row + x / 8) as usize;
                            let bit_idx = 7 - (x % 8);
                            if byte_idx < glyph_data.len() && (glyph_data[byte_idx] >> bit_idx) & 1 != 0 {
                                self.fb.set_pixel(px + x, py + y, color);
                            }
                        }
                    }
                }
            }
        }
    }

    /// Draw a bitmap glyph with synthetic italic slant
    /// Top rows shift right, bottom rows don't — a lean, mean pixel machine. — SoftGlyph
    fn draw_bitmap_italic(&self, px: u32, py: u32, glyph_w: u32, glyph_h: u32, glyph_data: &[u8], color: Color) {
        let bytes_per_row = (glyph_w + 7) / 8;

        for y in 0..glyph_h {
            let slant_offset = if glyph_h > 0 {
                ((glyph_h - y - 1) * 2) / glyph_h
            } else {
                0
            };

            for x in 0..glyph_w {
                let byte_idx = (y * bytes_per_row + x / 8) as usize;
                let bit_idx = 7 - (x % 8);
                if byte_idx < glyph_data.len() && (glyph_data[byte_idx] >> bit_idx) & 1 != 0 {
                    let offset_x = px + x + slant_offset;
                    self.fb.set_pixel(offset_x, py + y, color);
                }
            }
        }
    }

    /// Draw an RGBA color glyph with alpha blending
    /// Integer-only: (src * alpha + dst * (255 - alpha)) / 255. No FPU here. — SoftGlyph
    fn draw_rgba_glyph(&self, px: u32, py: u32, glyph_w: u32, glyph_h: u32, glyph_data: &[u8]) {
        let stride = self.fb.stride() as usize;
        let bpp = self.fb.format().bytes_per_pixel() as usize;
        let buffer = self.fb.buffer();

        for y in 0..glyph_h {
            for x in 0..glyph_w {
                let src_offset = ((y * glyph_w + x) * 4) as usize;
                if src_offset + 3 >= glyph_data.len() {
                    continue;
                }
                let sr = glyph_data[src_offset] as u32;
                let sg = glyph_data[src_offset + 1] as u32;
                let sb = glyph_data[src_offset + 2] as u32;
                let sa = glyph_data[src_offset + 3] as u32;

                if sa == 0 {
                    continue;
                }

                let dst_x = px + x;
                let dst_y = py + y;
                let dst_offset = dst_y as usize * stride + dst_x as usize * bpp;

                if sa == 255 {
                    unsafe {
                        let dst = buffer.add(dst_offset);
                        *dst = sr as u8;
                        *dst.add(1) = sg as u8;
                        *dst.add(2) = sb as u8;
                        if bpp == 4 {
                            *dst.add(3) = 0xFF;
                        }
                    }
                } else {
                    unsafe {
                        let dst = buffer.add(dst_offset);
                        let dr = *dst as u32;
                        let dg = *dst.add(1) as u32;
                        let db = *dst.add(2) as u32;
                        let inv_a = 255 - sa;
                        *dst = ((sr * sa + dr * inv_a) / 255) as u8;
                        *dst.add(1) = ((sg * sa + dg * inv_a) / 255) as u8;
                        *dst.add(2) = ((sb * sa + db * inv_a) / 255) as u8;
                        if bpp == 4 {
                            *dst.add(3) = 0xFF;
                        }
                    }
                }
            }
        }
    }

    /// Render cursor
    /// Block cursor: inverted cell. Bar: 2px vertical. Underline: 2px horizontal.
    /// Now resolves the glyph under cursor through FontManager too. — SoftGlyph
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
                    let resolved = self.font_manager.resolve(cell.ch);
                    if let GlyphData::Bitmap { width, height, data } = resolved.data {
                        self.draw_bitmap_glyph(px, py, width, height, data, fg_color);
                    }
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

    /// Paint a single cell to framebuffer — our fbcon_putcs() for one glyph.
    /// Cost: exactly 1 cell worth of pixels. No dirty tracking, no row iteration.
    /// — SoftGlyph: Synchronous glyph blit. The pixel that matters, nothing more.
    pub fn render_cell(&self, buffer: &ScreenBuffer, row: u32, col: u32) {
        if row >= self.rows || col >= self.cols {
            return;
        }
        if let Some(cell) = buffer.get(row, col) {
            if cell.attrs.flags.contains(CellFlags::WIDE_CONTINUATION) {
                return;
            }
            let px = col * self.font.width;
            let py = row * self.font.height;
            let selected = self.is_cell_selected(row, col);
            let cell_count = if cell.attrs.flags.contains(CellFlags::WIDE) { 2 } else { 1 };
            self.render_cell_inner(px, py, cell, cell_count, selected);
        }
    }

    /// Pixel-level scroll — memmove framebuffer up by N text rows.
    /// — GraveShift: This is what makes synchronous render fast. Instead of
    /// repainting 24×80 cells after scroll, we memmove the pixels and clear
    /// the bottom row. The fb already has copy_rect for overlapping regions.
    pub fn scroll_up_pixels(&self, lines: u32, bg_color: Color) {
        let pixel_rows = lines * self.font.height;
        let total_pixel_height = self.rows * self.font.height;
        if pixel_rows >= total_pixel_height {
            return;
        }
        // — GraveShift: Shift all scanlines up by pixel_rows. copy_rect handles overlap.
        self.fb.copy_rect(
            0, pixel_rows,
            0, 0,
            self.fb.width(), total_pixel_height - pixel_rows,
        );
        // Clear the vacated bottom rows
        let clear_y = total_pixel_height - pixel_rows;
        self.fb.fill_rect(0, clear_y, self.fb.width(), pixel_rows, bg_color);
    }

    /// Paint cursor at current position — call after writes for immediate visibility.
    /// — SoftGlyph: The blinking caret, rendered on demand.
    pub fn paint_cursor(&self, buffer: &ScreenBuffer, cursor: &Cursor) {
        if cursor.visible && cursor.blink_on {
            self.render_cursor(buffer, cursor);
        }
    }

    /// Erase cursor from previous position by repainting the cell underneath.
    /// — SoftGlyph: Ghost cursor exorcism — repaint what was there before the caret landed.
    pub fn erase_cursor(&self, buffer: &ScreenBuffer) {
        if self.last_cursor_visible && self.last_cursor_row < self.rows && self.last_cursor_col < self.cols {
            self.render_cell(buffer, self.last_cursor_row, self.last_cursor_col);
        }
    }

    /// Update cursor tracking after inline rendering so tick() knows where cursor was.
    /// — SoftGlyph: Breadcrumbs for the next erase_cursor call.
    pub fn update_cursor_tracking(&mut self, cursor: &Cursor) {
        self.last_cursor_row = cursor.row;
        self.last_cursor_col = cursor.col;
        self.last_cursor_visible = cursor.visible && cursor.blink_on;
    }

    /// Check if any rows are dirty (from CSI multi-row ops that used mark_all_dirty).
    /// — GraveShift: The per-glyph path handles Print/LF inline, but bulk ops
    /// like ED/IL/DL still set dirty flags. This tells write() to flush them.
    pub fn has_dirty(&self) -> bool {
        if self.dirty.full_redraw {
            return true;
        }
        self.dirty.dirty_rows.iter().any(|&d| d)
    }

    /// Force full redraw on next render
    pub fn invalidate(&mut self) {
        self.dirty.mark_all();
    }

    /// Flush framebuffer to hardware — call after per-glyph rendering.
    /// — SoftGlyph: The final handshake. Pixels committed.
    pub fn flush_fb(&self) {
        self.fb.flush();
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
