//! Terminal renderer — double-buffered
//!
//! All rendering targets the software back_buffer (regular RAM, fast).
//! The final blit copies only dirty scanlines to the MMIO framebuffer.
//! This turns a 25-second ESC[2J into milliseconds.
//! — GlassSignal: The MMIO graveyard is closed. Pixels live in RAM now.

extern crate alloc;
extern crate perf;

use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use core::cell::Cell;
use core::ptr;
use fb::{Color, Font, FontManager, Framebuffer, GlyphData, PSF2_FONT};
use vte::ScreenBuffer;
use vte::{Cell as VteCell, CellAttrs, CellFlags, Cursor, CursorShape};

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
    /// Framebuffer reference (MMIO — only touched during blit)
    fb: Arc<dyn Framebuffer>,
    /// Legacy font reference (kept for cell dimension calculations)
    font: &'static Font,
    /// Extended font manager with fallback chain — SoftGlyph
    font_manager: FontManager,
    /// Number of columns
    cols: u32,
    /// Number of rows
    rows: u32,
    /// Back buffer for double buffering — all rendering targets this
    /// — GlassSignal: RAM is fast, MMIO is a graveyard. Render here, blit once.
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
    /// Blit tracking — pixel Y range that needs copying to MMIO
    /// — GlassSignal: Cell<> for interior mutability through &self rendering methods
    blit_y_min: Cell<u32>,
    blit_y_max: Cell<u32>,
    blit_pending: Cell<bool>,
}

impl Renderer {
    /// Create a new renderer.
    /// — GlassSignal: `double_buffer` controls whether we allocate a separate RAM
    /// back-buffer for rendering. When the target fb is already RAM (compositor
    /// BackingFramebuffer), double-buffering is redundant — renders go directly to
    /// the backing fb and the compositor blits to hardware at 30Hz. Skip the 4MB
    /// heap alloc per VT and save 24MB across 6 terminals. Tearing in a text
    /// console? Nobody cares. — GlassSignal
    pub fn new(fb: Arc<dyn Framebuffer>, double_buffer: bool) -> Self {
        let font = &PSF2_FONT;
        // Bootstrap the font manager with the extended built-in font — SoftGlyph
        let font_manager = FontManager::with_builtin();
        let cols = fb.width() / font.width;
        let rows = fb.height() / font.height;

        // — GlassSignal: only allocate back buffer when double-buffering is requested.
        // VT backing framebuffers are RAM — no MMIO penalty, no need for double buffer.
        let back_buffer = if double_buffer && fb.size() > 0 {
            unsafe {
                os_log::write_str_raw("[REND] alloc bb size=0x");
                os_log::write_u64_hex_raw(fb.size() as u64);
                os_log::write_str_raw("\n");
            }
            Some(vec![0u8; fb.size()])
        } else {
            if fb.size() == 0 {
                unsafe { os_log::write_str_raw("[REND] NO bb (size=0)!\n"); }
            } else {
                unsafe { os_log::write_str_raw("[REND] direct render (no bb)\n"); }
            }
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
            blit_y_min: Cell::new(0),
            blit_y_max: Cell::new(0),
            blit_pending: Cell::new(false),
        }
    }

    /// — CrashBloom: Get the raw framebuffer pointer for null-safety checks.
    /// A corrupted Arc<dyn Framebuffer> can have a null data pointer from heap
    /// corruption. Callers check this before calling render() to avoid a null
    /// write in the spin lock release path that kills the CPU.
    pub fn fb_ptr(&self) -> *mut u8 {
        self.fb.buffer()
    }

    /// Resize the renderer with a new framebuffer.
    /// — GlassSignal: viewport changed — swap to resized backing buffer, recompute
    /// grid dims, nuke dirty state. Same double_buffer policy as original init.
    /// This is the layout-change path; update_framebuffer is the GPU-swap path.
    pub fn resize(&mut self, fb: Arc<dyn Framebuffer>) {
        let cols = fb.width() / self.font.width;
        let rows = fb.height() / self.font.height;

        let had_back_buffer = self.back_buffer.is_some();
        let back_buffer = if had_back_buffer && fb.size() > 0 {
            Some(vec![0u8; fb.size()])
        } else {
            None
        };

        self.fb = fb;
        self.cols = cols;
        self.rows = rows;
        self.back_buffer = back_buffer;
        self.dirty = DirtyRegion::new(rows);
        self.dirty.mark_all();
        self.last_cursor_row = 0;
        self.last_cursor_col = 0;
        self.last_cursor_visible = false;
        self.selection = None;
        self.blit_y_min.set(0);
        self.blit_y_max.set(0);
        self.blit_pending.set(false);
    }

    /// Hot-swap the framebuffer after GPU driver init.
    /// — GlassSignal: when VirtIO-GPU replaces the UEFI GOP buffer, the renderer
    /// must follow or we're painting on a disconnected canvas. Invalidates everything
    /// so the next render pushes the full terminal state to the new buffer.
    /// — GlassSignal: hot-swap with same double_buffer policy as the original init.
    /// Inherits the current back_buffer strategy: if we had one, allocate a new one;
    /// if we were direct-rendering, stay direct.
    pub fn update_framebuffer(&mut self, fb: Arc<dyn Framebuffer>) {
        let cols = fb.width() / self.font.width;
        let rows = fb.height() / self.font.height;

        let had_back_buffer = self.back_buffer.is_some();
        let back_buffer = if had_back_buffer && fb.size() > 0 {
            Some(vec![0u8; fb.size()])
        } else {
            None
        };

        self.fb = fb;
        self.cols = cols;
        self.rows = rows;
        self.back_buffer = back_buffer;
        self.dirty = DirtyRegion::new(rows);
        self.dirty.mark_all();
        self.blit_y_min.set(0);
        self.blit_y_max.set(0);
        self.blit_pending.set(false);
    }

    // ── Double-buffer helpers ──────────────────────────────────────────
    // — GlassSignal: All rendering writes to render_target() (back buffer if
    //   available, MMIO fallback if not). Blit tracking records which scanlines
    //   changed. flush_fb() copies only dirty scanlines to the real framebuffer.
    //   This turns ESC[2J from 25 seconds of per-pixel MMIO torture into a
    //   sub-millisecond RAM render + one fast sequential blit.

    /// Render target — back buffer pointer if available, MMIO pointer as fallback
    fn render_target(&self) -> *mut u8 {
        if let Some(ref bb) = self.back_buffer {
            bb.as_ptr() as *mut u8
        } else {
            self.fb.buffer()
        }
    }

    /// Record pixel region that needs blitting to MMIO
    fn extend_blit_region(&self, y: u32, h: u32) {
        if self.back_buffer.is_none() {
            return;
        }
        let y_end = (y + h).min(self.fb.height());
        if !self.blit_pending.get() {
            self.blit_y_min.set(y);
            self.blit_y_max.set(y_end);
            self.blit_pending.set(true);
        } else {
            self.blit_y_min.set(self.blit_y_min.get().min(y));
            self.blit_y_max.set(self.blit_y_max.get().max(y_end));
        }
    }

    /// Fill rectangle on the render target (non-volatile, fast for RAM)
    /// — GlassSignal: no volatile writes, no VM exits, just raw speed
    fn bb_fill_rect(&self, x: u32, y: u32, w: u32, h: u32, color: Color) {
        let target = self.render_target();
        if target.is_null() { return; }
        let bpp = self.fb.format().bytes_per_pixel() as usize;
        let stride = self.fb.stride() as usize;
        let fb_size = self.fb.size();

        let x_end = (x + w).min(self.fb.width());
        let y_end = (y + h).min(self.fb.height());

        if x >= x_end || y >= y_end {
            return;
        }

        let mut color_bytes = [0u8; 4];
        color.write_to(&mut color_bytes, self.fb.format());

        // — GlassSignal: wrapping_add for pointer arithmetic because kernel
        // virtual addresses are in the upper half (0xFFFF_8xxx...) and
        // ptr::add panics on overflow in debug builds. The bounds check
        // on row_end vs fb_size catches actual out-of-bounds before we write.
        unsafe {
            for py in y..y_end {
                let row_start = py as usize * stride + x as usize * bpp;
                let row_end = row_start + (x_end - x) as usize * bpp;
                if row_end > fb_size { continue; }
                match bpp {
                    4 => {
                        let pixel_value = u32::from_le_bytes([
                            color_bytes[0],
                            color_bytes[1],
                            color_bytes[2],
                            color_bytes[3],
                        ]);
                        let line_ptr = target.wrapping_add(row_start) as *mut u32;
                        for i in 0..(x_end - x) as usize {
                            ptr::write(line_ptr.wrapping_add(i), pixel_value);
                        }
                    }
                    2 => {
                        let pixel_value = u16::from_le_bytes([color_bytes[0], color_bytes[1]]);
                        let line_ptr = target.wrapping_add(row_start) as *mut u16;
                        for i in 0..(x_end - x) as usize {
                            ptr::write(line_ptr.wrapping_add(i), pixel_value);
                        }
                    }
                    _ => {
                        for i in 0..(x_end - x) as usize {
                            ptr::copy_nonoverlapping(
                                color_bytes.as_ptr(),
                                target.wrapping_add(row_start + i * bpp),
                                bpp,
                            );
                        }
                    }
                }
            }
        }

        self.extend_blit_region(y, y_end - y);
    }

    /// Set single pixel on the render target
    fn bb_set_pixel(&self, x: u32, y: u32, color: Color) {
        if x >= self.fb.width() || y >= self.fb.height() {
            return;
        }
        let target = self.render_target();
        if target.is_null() { return; }
        let bpp = self.fb.format().bytes_per_pixel() as usize;
        let offset = y as usize * self.fb.stride() as usize + x as usize * bpp;
        if offset + bpp > self.fb.size() { return; }
        unsafe {
            let mut bytes = [0u8; 4];
            color.write_to(&mut bytes, self.fb.format());
            ptr::copy_nonoverlapping(bytes.as_ptr(), target.wrapping_add(offset), bpp);
        }
        self.extend_blit_region(y, 1);
    }

    /// Horizontal line on the render target
    fn bb_hline(&self, x: u32, y: u32, w: u32, color: Color) {
        self.bb_fill_rect(x, y, w, 1, color);
    }

    /// Copy rectangle within the render target (handles overlapping regions)
    fn bb_copy_rect(&self, src_x: u32, src_y: u32, dst_x: u32, dst_y: u32, w: u32, h: u32) {
        let target = self.render_target();
        if target.is_null() { return; }
        let bpp = self.fb.format().bytes_per_pixel() as usize;
        let stride = self.fb.stride() as usize;
        let row_bytes = w as usize * bpp;
        let fb_size = self.fb.size();

        let copy_forward = dst_y < src_y || (dst_y == src_y && dst_x <= src_x);

        if copy_forward {
            for row in 0..h {
                let src_offset = (src_y + row) as usize * stride + src_x as usize * bpp;
                let dst_offset = (dst_y + row) as usize * stride + dst_x as usize * bpp;
                if src_offset + row_bytes > fb_size || dst_offset + row_bytes > fb_size { continue; }
                unsafe {
                    ptr::copy(target.wrapping_add(src_offset), target.wrapping_add(dst_offset), row_bytes);
                }
            }
        } else {
            for row in (0..h).rev() {
                let src_offset = (src_y + row) as usize * stride + src_x as usize * bpp;
                let dst_offset = (dst_y + row) as usize * stride + dst_x as usize * bpp;
                if src_offset + row_bytes > fb_size || dst_offset + row_bytes > fb_size { continue; }
                unsafe {
                    ptr::copy(target.wrapping_add(src_offset), target.wrapping_add(dst_offset), row_bytes);
                }
            }
        }

        // Mark destination region for blit
        let min_y = dst_y;
        let max_y = dst_y + h;
        self.extend_blit_region(min_y, max_y - min_y);
    }

    /// Clear entire render target with color
    fn bb_clear(&self, color: Color) {
        self.bb_fill_rect(0, 0, self.fb.width(), self.fb.height(), color);
    }

    /// Blit dirty scanlines from back_buffer to MMIO framebuffer.
    /// — GlassSignal: u64-aligned blit — compiler_builtins memcpy generates
    /// rep movsb which is byte-granularity in QEMU TCG. For a 7MB framebuffer
    /// that's 7M byte writes. This uses rep movsq (8 bytes/iteration) cutting
    /// the operation count by 8×. The difference between 25 seconds and <1 second.
    fn blit_to_fb(&self) {
        if !self.blit_pending.get() {
            return;
        }
        if let Some(ref bb) = self.back_buffer {
            let stride = self.fb.stride() as usize;
            let fb_ptr = self.fb.buffer();
            let bb_ptr = bb.as_ptr();
            let y_start = self.blit_y_min.get() as usize;
            let y_end = self.blit_y_max.get().min(self.fb.height()) as usize;

            // — GraveShift: Trace blit region for debug-console
            // — GlassSignal: blit debug trace exorcised of raw asm — os_core handles the port whispers now
            #[cfg(feature = "debug-terminal")]
            unsafe {
                for &b in b"[BLIT] y=" {
                    os_core::outb(0x3F8, b);
                }
                let mut n = y_start;
                let mut digits = [0u8; 10];
                let mut i = 0;
                if n == 0 { digits[0] = b'0'; i = 1; } else {
                    while n > 0 { digits[i] = b'0' + (n % 10) as u8; n /= 10; i += 1; }
                }
                for d in digits[..i].iter().rev() {
                    os_core::outb(0x3F8, *d);
                }
                for &b in b".." {
                    os_core::outb(0x3F8, b);
                }
                n = y_end;
                i = 0;
                if n == 0 { digits[0] = b'0'; i = 1; } else {
                    while n > 0 { digits[i] = b'0' + (n % 10) as u8; n /= 10; i += 1; }
                }
                for d in digits[..i].iter().rev() {
                    os_core::outb(0x3F8, *d);
                }
                let nl = b'\n';
                os_core::outb(0x3F8, nl);
            }

            if y_start < y_end {
                let byte_offset = y_start * stride;
                let byte_count = (y_end - y_start) * stride;

                unsafe {
                    let src = bb_ptr.wrapping_add(byte_offset);
                    let dst = fb_ptr.wrapping_add(byte_offset);

                    // — CrashBloom: Guard against null or overlapping pointers.
                    // Rust nightly 2024 panics on UB in debug mode. If the back
                    // buffer or framebuffer pointer is garbage, catch it here.
                    if src.is_null() || dst.is_null() || src == dst {
                        os_log::write_str_raw("[BLIT-FATAL] null/overlap in blit_to_fb! src=");
                        os_log::write_u64_hex_raw(src as u64);
                        os_log::write_str_raw(" dst=");
                        os_log::write_u64_hex_raw(dst as u64);
                        os_log::write_str_raw("\n");
                        return;
                    }

                    // — GlassSignal: replaced rep movsq asm with ptr::copy_nonoverlapping —
                    // the compiler knows how to emit the same thing, and now we're arch-clean
                    core::ptr::copy_nonoverlapping(src, dst, byte_count);
                }
            }

            // Fire GPU flush callback for the blitted region
            self.fb.flush_region(
                0,
                self.blit_y_min.get(),
                self.fb.width(),
                self.blit_y_max.get() - self.blit_y_min.get(),
            );
        }
        self.blit_pending.set(false);
    }

    // ── End double-buffer helpers ──────────────────────────────────────

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

        // Render dirty rows to back buffer
        let mut pixel_count = 0u64;
        let mut dirty_count = 0u32;
        for row in 0..self.rows {
            if self.dirty.is_row_dirty(row) {
                self.render_row(buffer, row);
                pixel_count += (self.cols * self.font.width * self.font.height) as u64;
                dirty_count += 1;
            }
        }

        // Render cursor to back buffer
        if cursor.visible && cursor.blink_on {
            self.render_cursor(buffer, cursor);
            pixel_count += (self.font.width * self.font.height) as u64;
        }

        // — PatchBay: record bulk render stats
        perf::counters().record_term_bulk_render(dirty_count as u64);

        // Clear dirty flags
        self.dirty.clear();

        // Track cursor position for next render
        self.last_cursor_row = cursor.row;
        self.last_cursor_col = cursor.col;
        self.last_cursor_visible = cursor.visible && cursor.blink_on;

        // Blit dirty region from back buffer to MMIO
        // — GlassSignal: one sequential copy, not a million volatile writes
        self.blit_to_fb();

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
    /// All pixel writes go to the back buffer (fast RAM).
    /// — InputShade: `selected` flag triggers reverse-video for the selection overlay.
    fn render_cell_inner(&self, px: u32, py: u32, cell: &VteCell, cell_count: u32, selected: bool) {
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

        // Draw background to back buffer
        self.bb_fill_rect(px, py, total_width, self.font.height, bg_color);

        // Blink text: when in "off" phase, only draw background — SoftGlyph
        let is_blink = cell.attrs.flags.contains(CellFlags::BLINK);
        if is_blink && !self.blink_text_visible() {
            // — GlassSignal: Still need to mark the blit region for the background we drew
            self.extend_blit_region(py, self.font.height);
            return;
        }

        // Draw character (if not space and not hidden)
        if cell.ch != ' ' && !cell.attrs.flags.contains(CellFlags::HIDDEN) {
            let resolved = self.font_manager.resolve(cell.ch);
            let is_bold = cell.attrs.flags.contains(CellFlags::BOLD);
            let is_italic = cell.attrs.flags.contains(CellFlags::ITALIC);

            match resolved.data {
                GlyphData::Bitmap {
                    width,
                    height,
                    data,
                } => {
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
                GlyphData::Rgba {
                    width,
                    height,
                    data,
                } => {
                    // RGBA emoji/color glyph — alpha blend onto background
                    // Bold/italic don't apply to color glyphs — SoftGlyph
                    self.draw_rgba_glyph(px, py, width, height, data);
                }
            }
        }

        // Draw underline (spans full cell width)
        if cell.attrs.flags.contains(CellFlags::UNDERLINE) {
            let underline_y = py + self.font.height - 2;
            self.bb_hline(px, underline_y, total_width, fg_color);
        }

        // Draw strikethrough (spans full cell width)
        if cell.attrs.flags.contains(CellFlags::STRIKETHROUGH) {
            let strike_y = py + self.font.height / 2;
            self.bb_hline(px, strike_y, total_width, fg_color);
        }

        // — GlassSignal: Mark this cell's pixel region for blit to MMIO framebuffer.
        // Without this, per-glyph inline rendering writes to the back buffer but
        // flush_fb() → blit_to_fb() sees blit_pending=false and skips the copy.
        // The screen stays black. Every. Single. Time. — GlassSignal
        self.extend_blit_region(py, self.font.height);
    }

    /// Draw a 1-bit monochrome bitmap glyph to back buffer — SoftGlyph
    fn draw_bitmap_glyph(
        &self,
        px: u32,
        py: u32,
        glyph_w: u32,
        glyph_h: u32,
        glyph_data: &[u8],
        color: Color,
    ) {
        let bpp = self.fb.format().bytes_per_pixel() as usize;
        let stride = self.fb.stride() as usize;
        let buffer = self.render_target();
        if buffer.is_null() { return; }
        let color_bytes = color.to_bytes(self.fb.format());
        let bytes_per_row = (glyph_w + 7) / 8;
        let fb_size = self.fb.size();

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
                        let line_end = line_offset + glyph_w as usize * 4;
                        if line_end > fb_size { continue; }
                        let line_ptr = buffer.wrapping_add(line_offset) as *mut u32;

                        for x in 0..glyph_w {
                            let byte_idx = (y * bytes_per_row + x / 8) as usize;
                            let bit_idx = 7 - (x % 8);
                            if byte_idx < glyph_data.len()
                                && (glyph_data[byte_idx] >> bit_idx) & 1 != 0
                            {
                                ptr::write(line_ptr.wrapping_add(x as usize), pixel_value);
                            }
                        }
                    }
                }
                2 => {
                    let pixel_value = u16::from_le_bytes([color_bytes[0], color_bytes[1]]);

                    for y in 0..glyph_h {
                        let line_offset = ((py + y) as usize * stride) + (px as usize * 2);
                        let line_end = line_offset + glyph_w as usize * 2;
                        if line_end > fb_size { continue; }
                        let line_ptr = buffer.wrapping_add(line_offset) as *mut u16;

                        for x in 0..glyph_w {
                            let byte_idx = (y * bytes_per_row + x / 8) as usize;
                            let bit_idx = 7 - (x % 8);
                            if byte_idx < glyph_data.len()
                                && (glyph_data[byte_idx] >> bit_idx) & 1 != 0
                            {
                                ptr::write(line_ptr.wrapping_add(x as usize), pixel_value);
                            }
                        }
                    }
                }
                _ => {
                    for y in 0..glyph_h {
                        for x in 0..glyph_w {
                            let byte_idx = (y * bytes_per_row + x / 8) as usize;
                            let bit_idx = 7 - (x % 8);
                            if byte_idx < glyph_data.len()
                                && (glyph_data[byte_idx] >> bit_idx) & 1 != 0
                            {
                                self.bb_set_pixel(px + x, py + y, color);
                            }
                        }
                    }
                }
            }
        }

        // Blit region tracked by bb_fill_rect (background) already covers this
    }

    /// Draw a bitmap glyph with synthetic italic slant to back buffer
    /// Top rows shift right, bottom rows don't — a lean, mean pixel machine. — SoftGlyph
    fn draw_bitmap_italic(
        &self,
        px: u32,
        py: u32,
        glyph_w: u32,
        glyph_h: u32,
        glyph_data: &[u8],
        color: Color,
    ) {
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
                    self.bb_set_pixel(offset_x, py + y, color);
                }
            }
        }
    }

    /// Draw an RGBA color glyph with alpha blending to back buffer
    /// Integer-only: (src * alpha + dst * (255 - alpha)) / 255. No FPU here. — SoftGlyph
    fn draw_rgba_glyph(&self, px: u32, py: u32, glyph_w: u32, glyph_h: u32, glyph_data: &[u8]) {
        let stride = self.fb.stride() as usize;
        let bpp = self.fb.format().bytes_per_pixel() as usize;
        let buffer = self.render_target();
        if buffer.is_null() { return; }
        let fb_size = self.fb.size();

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
                if dst_offset + bpp > fb_size { continue; }

                if sa == 255 {
                    unsafe {
                        let dst = buffer.wrapping_add(dst_offset);
                        *dst = sr as u8;
                        *dst.wrapping_add(1) = sg as u8;
                        *dst.wrapping_add(2) = sb as u8;
                        if bpp == 4 {
                            *dst.wrapping_add(3) = 0xFF;
                        }
                    }
                } else {
                    unsafe {
                        let dst = buffer.wrapping_add(dst_offset);
                        let dr = *dst as u32;
                        let dg = *dst.wrapping_add(1) as u32;
                        let db = *dst.wrapping_add(2) as u32;
                        let inv_a = 255 - sa;
                        *dst = ((sr * sa + dr * inv_a) / 255) as u8;
                        *dst.wrapping_add(1) = ((sg * sa + dg * inv_a) / 255) as u8;
                        *dst.wrapping_add(2) = ((sb * sa + db * inv_a) / 255) as u8;
                        if bpp == 4 {
                            *dst.wrapping_add(3) = 0xFF;
                        }
                    }
                }
            }
        }
        // Blit region tracked by bb_fill_rect (background) already covers this
    }

    /// Render cursor to back buffer
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
                self.bb_fill_rect(px, py, self.font.width, self.font.height, bg_color);
                if cell.ch != ' ' {
                    let resolved = self.font_manager.resolve(cell.ch);
                    if let GlyphData::Bitmap {
                        width,
                        height,
                        data,
                    } = resolved.data
                    {
                        self.draw_bitmap_glyph(px, py, width, height, data, fg_color);
                    }
                }
            }
            CursorShape::Underline => {
                // Draw underline cursor
                let cursor_y = py + self.font.height - 2;
                self.bb_fill_rect(px, cursor_y, self.font.width, 2, bg_color);
            }
            CursorShape::Bar => {
                // Draw vertical bar cursor
                self.bb_fill_rect(px, py, 2, self.font.height, bg_color);
            }
        }

        // — GlassSignal: Mark cursor region for blit — same disease as render_cell_inner.
        self.extend_blit_region(py, self.font.height);
    }

    /// Clear the screen with background color (writes to back buffer)
    pub fn clear(&self, attrs: &CellAttrs) {
        let (r, g, b) = attrs.effective_bg().to_rgb(false);
        let bg = Color::new(r, g, b);
        self.bb_clear(bg);
        // — GlassSignal: Mark entire screen for blit after clear
        self.extend_blit_region(0, self.fb.height());
    }

    /// Scroll the display up (writes to back buffer)
    pub fn scroll_up(&self, lines: u32, bg_color: Color) {
        let line_height = self.font.height;
        let scroll_pixels = lines * line_height;
        let total_height = self.rows * line_height;

        if scroll_pixels < total_height {
            // Copy screen content up within back buffer
            self.bb_copy_rect(
                0,
                scroll_pixels,
                0,
                0,
                self.fb.width(),
                total_height - scroll_pixels,
            );

            // Clear bottom area
            self.bb_fill_rect(
                0,
                total_height - scroll_pixels,
                self.fb.width(),
                scroll_pixels,
                bg_color,
            );
            // — GlassSignal: bb_fill_rect doesn't mark blit region — do it here
            self.extend_blit_region(total_height - scroll_pixels, scroll_pixels);
        } else {
            // Scroll more than screen height - just clear
            self.bb_clear(bg_color);
            self.extend_blit_region(0, total_height);
        }
    }

    /// Scroll the display down (writes to back buffer)
    pub fn scroll_down(&self, lines: u32, bg_color: Color) {
        let line_height = self.font.height;
        let scroll_pixels = lines * line_height;
        let total_height = self.rows * line_height;

        if scroll_pixels < total_height {
            // Copy screen content down within back buffer
            self.bb_copy_rect(
                0,
                0,
                0,
                scroll_pixels,
                self.fb.width(),
                total_height - scroll_pixels,
            );

            // Clear top area
            self.bb_fill_rect(0, 0, self.fb.width(), scroll_pixels, bg_color);
            // — GlassSignal: bb_fill_rect doesn't mark blit region — do it here
            self.extend_blit_region(0, scroll_pixels);
        } else {
            self.bb_clear(bg_color);
            self.extend_blit_region(0, total_height);
        }
    }

    /// Paint a single cell to back buffer — our fbcon_putcs() for one glyph.
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
            let cell_count = if cell.attrs.flags.contains(CellFlags::WIDE) {
                2
            } else {
                1
            };
            // — PatchBay: measure per-glyph rasterization cost
            let g_start = perf::rdtsc();
            self.render_cell_inner(px, py, cell, cell_count, selected);
            let g_end = perf::rdtsc();
            perf::counters().record_term_glyph();
            perf::counters().record_term_glyph_cycles(g_end.saturating_sub(g_start));
        }
    }

    /// Pixel-level scroll — memmove back buffer up by N text rows.
    /// — GraveShift: This is what makes synchronous render fast. Instead of
    /// repainting 24×80 cells after scroll, we memmove the pixels and clear
    /// the bottom row. The back buffer has copy_rect for overlapping regions.
    pub fn scroll_up_pixels(&self, lines: u32, bg_color: Color) {
        let scroll_start = perf::rdtsc();
        let pixel_rows = lines * self.font.height;
        let total_pixel_height = self.rows * self.font.height;
        if pixel_rows >= total_pixel_height {
            perf::counters().record_term_scroll(perf::rdtsc().saturating_sub(scroll_start));
            return;
        }
        // — GraveShift: Shift all scanlines up by pixel_rows within back buffer.
        self.bb_copy_rect(
            0,
            pixel_rows,
            0,
            0,
            self.fb.width(),
            total_pixel_height - pixel_rows,
        );
        // Clear the vacated bottom rows
        let clear_y = total_pixel_height - pixel_rows;
        self.bb_fill_rect(0, clear_y, self.fb.width(), pixel_rows, bg_color);
        // — GlassSignal: Mark cleared bottom region for blit
        self.extend_blit_region(clear_y, pixel_rows);
        let scroll_end = perf::rdtsc();
        perf::counters().record_term_scroll(scroll_end.saturating_sub(scroll_start));
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
        if self.last_cursor_visible
            && self.last_cursor_row < self.rows
            && self.last_cursor_col < self.cols
        {
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

    /// Flush back buffer to MMIO framebuffer — the final blit.
    /// — GlassSignal: All accumulated renders become visible in one sequential copy.
    pub fn flush_fb(&self) {
        self.blit_to_fb();
    }

    /// Draw a single pixel at the given coordinates
    ///
    /// Used by Sixel renderer to draw individual pixels directly
    pub fn draw_pixel(&mut self, x: u32, y: u32, color: Color) {
        if x < self.fb.width() && y < self.fb.height() {
            self.bb_set_pixel(x, y, color);
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
