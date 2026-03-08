//! Tiling VT Compositor for OXIDE OS
//!
//! — SableWire: one ring to blit them all, one ring to find them,
//!   one ring to bring them all, and on the framebuffer bind them.
//!
//! Every VT gets its own backing pixel buffer. The compositor is the ONLY
//! thing that writes to the hardware framebuffer. Terminal renderers, /dev/fb0,
//! graphics apps — they all paint into their VT's backing buffer. The compositor
//! blits visible buffers into viewport rectangles on the physical display.

#![no_std]

extern crate alloc;

pub mod backing_fb;
pub mod layout;

use alloc::sync::Arc;
use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use spin::Mutex;

use backing_fb::BackingFramebuffer;
use fb::Framebuffer;
use layout::{Layout, LayoutManager, Viewport, MAX_VTS, MAX_TILES};

/// VT display mode — text terminal or raw graphics
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum VtMode {
    /// Terminal emulator active — ANSI parsing, text rendering, scrollback
    Text,
    /// Raw graphics mode — /dev/fb0 writes go here, no terminal processing
    Graphics,
}

/// Per-VT dirty flags — set by writers, cleared by compositor
/// — SableWire: atomics because terminal write + compositor blit can race.
/// Dirty flag = "this VT has new pixels since last composite." That's it.
/// Sized to MAX_VTS — unused slots stay false forever, zero cost.
static VT_DIRTY: [AtomicBool; MAX_VTS] = [
    AtomicBool::new(false),
    AtomicBool::new(false),
    AtomicBool::new(false),
    AtomicBool::new(false),
    AtomicBool::new(false),
    AtomicBool::new(false),
];

/// Global full-redraw flag — set on layout change or VT switch
static FULL_REDRAW: AtomicBool = AtomicBool::new(true);

/// Active VT for input routing (mirrors vt::ACTIVE_VT but compositor-managed)
static COMPOSITOR_FOCUS_VT: AtomicUsize = AtomicUsize::new(0);

/// The global compositor instance
static COMPOSITOR: Mutex<Option<Compositor>> = Mutex::new(None);

/// Compositor state
pub struct Compositor {
    /// The real hardware framebuffer — ONLY the compositor touches this
    hw_fb: Arc<dyn Framebuffer>,
    /// Per-VT backing buffers (pixel canvases) — allocated lazily on first use.
    /// — SableWire: only VT0 gets a buffer at init. The rest spawn on demand
    /// when you split the screen or switch VTs. ~4MB each, allocated from
    /// buddy allocator physical frames, freed on Drop. No waste.
    vt_buffers: [Option<Arc<BackingFramebuffer>>; MAX_VTS],
    /// Per-VT display mode
    vt_modes: [VtMode; MAX_VTS],
    /// Layout manager — viewport geometry
    layout: LayoutManager,
    /// Border color for split-mode dividers (cyan highlight)
    border_color: u32,
    /// Focus highlight color
    focus_color: u32,
}

impl Compositor {
    /// Create a new compositor. Only VT0 gets a backing buffer at init —
    /// the rest are allocated on demand when split/switch triggers them.
    /// — NeonRoot: saves ~15MB at boot. Buffers appear when you need them.
    fn new(hw_fb: Arc<dyn Framebuffer>) -> Self {
        let width = hw_fb.width();
        let height = hw_fb.height();
        let stride = hw_fb.stride();
        let format = hw_fb.format();

        os_log::println!("[COMP] init {}x{} stride={} bpp={} (lazy alloc, VT0 only)",
            width, height, stride, format.bytes_per_pixel() * 8);

        // — NeonRoot: only VT0 gets a buffer now. Rest are None until first use.
        let vt0_buf = BackingFramebuffer::new(width, height, stride, format);
        os_log::println!("[COMP] VT0 buffer: {}KB", vt0_buf.size() / 1024);

        let mut vt_buffers: [Option<Arc<BackingFramebuffer>>; MAX_VTS] =
            core::array::from_fn(|_| None);
        vt_buffers[0] = Some(Arc::new(vt0_buf));

        let layout = LayoutManager::new(width, height);

        // — GlassSignal: border colors — dark gray divider, cyan focus highlight
        let border_color = 0xFF333333; // dark gray ARGB
        let focus_color = 0xFF00AACC;  // cyan ARGB

        Compositor {
            hw_fb,
            vt_buffers,
            vt_modes: [VtMode::Text; MAX_VTS],
            layout,
            border_color,
            focus_color,
        }
    }

    /// Ensure a VT has a backing buffer, allocating one on demand if needed.
    /// — SableWire: the lazy allocation hot path. First split/switch to a VT
    /// triggers a ~4MB buddy alloc. Subsequent accesses are free (already Some).
    /// Returns true if the buffer exists (or was just created).
    fn ensure_vt_buffer(&mut self, vt_num: usize) -> bool {
        if vt_num >= MAX_VTS { return false; }
        if self.vt_buffers[vt_num].is_some() { return true; }

        let buf = BackingFramebuffer::new(
            self.hw_fb.width(), self.hw_fb.height(),
            self.hw_fb.stride(), self.hw_fb.format(),
        );
        os_log::println!("[COMP] VT{} buffer: {}KB (on-demand)", vt_num, buf.size() / 1024);
        self.vt_buffers[vt_num] = Some(Arc::new(buf));
        true
    }

    /// Get the backing framebuffer for a VT, allocating on demand.
    /// — SableWire: terminal renderers call this on VT switch. First call
    /// for a new VT triggers the backing buffer allocation.
    pub fn get_vt_framebuffer(&mut self, vt_num: usize) -> Option<Arc<dyn Framebuffer>> {
        if vt_num >= MAX_VTS { return None; }
        self.ensure_vt_buffer(vt_num);
        self.vt_buffers[vt_num]
            .as_ref()
            .map(|b| b.clone() as Arc<dyn Framebuffer>)
    }

    /// Get the backing buffer reference for direct blit access (no lazy alloc)
    fn get_vt_buffer(&self, vt_num: usize) -> Option<&Arc<BackingFramebuffer>> {
        if vt_num >= MAX_VTS { return None; }
        self.vt_buffers[vt_num].as_ref()
    }

    /// Composite visible VT buffers onto the hardware framebuffer.
    /// — SableWire: lazily allocates backing buffers for newly-visible VTs.
    fn composite(&mut self) {
        let viewports = self.layout.compute_viewports();
        let tile_count = self.layout.tile_count();
        let full_redraw = FULL_REDRAW.swap(false, Ordering::AcqRel);

        for slot_idx in 0..tile_count {
            let (vt_idx, viewport) = viewports[slot_idx];
            if viewport.width == 0 || viewport.height == 0 {
                continue;
            }

            // — SableWire: lazy-allocate backing buffer for newly-visible VTs
            self.ensure_vt_buffer(vt_idx);

            // — SableWire: skip clean buffers unless full redraw requested
            if !full_redraw && !VT_DIRTY[vt_idx].swap(false, Ordering::AcqRel) {
                continue;
            }

            if let Some(src_buf) = self.get_vt_buffer(vt_idx) {
                self.blit_vt_to_hw(src_buf, &viewport);
            }
        }

        // — GlassSignal: draw borders between tiles in split modes
        if self.layout.layout() != Layout::Fullscreen {
            self.draw_borders(&viewports, tile_count);
        }

        // — GlassSignal: flush to GPU if VirtIO-GPU is active
        self.hw_fb.flush();
    }

    /// Blit a VT backing buffer into a viewport rectangle on the hardware fb.
    /// — SableWire: the hot inner loop. ~0.3ms for full-screen at 1024×768.
    fn blit_vt_to_hw(&self, src: &BackingFramebuffer, viewport: &Viewport) {
        let src_ptr = src.raw_ptr();
        let dst_ptr = self.hw_fb.buffer();
        let src_stride = src.stride() as usize;
        let dst_stride = self.hw_fb.stride() as usize;
        let bpp = src.format().bytes_per_pixel() as usize;

        // — SableWire: blit min(viewport.width, src.width) × min(viewport.height, src.height)
        let blit_w = viewport.width.min(src.width()) as usize;
        let blit_h = viewport.height.min(src.height()) as usize;
        let row_bytes = blit_w * bpp;

        // — CrashBloom: Validate pointers before blitting. Rust nightly 2024 panics
        // on null/overlap in debug mode. Bail silently rather than crashing the kernel.
        if src_ptr.is_null() || dst_ptr.is_null() {
            return;
        }

        unsafe {
            for row in 0..blit_h {
                let src_offset = row * src_stride;
                let dst_offset = ((viewport.y as usize + row) * dst_stride)
                    + (viewport.x as usize * bpp);

                core::ptr::copy_nonoverlapping(
                    src_ptr.add(src_offset),
                    dst_ptr.add(dst_offset),
                    row_bytes,
                );
            }
        }
    }

    /// Draw border lines between tiles and a focus highlight on the active tile.
    /// — GlassSignal: 2px dark gray dividers + 1px cyan focus border
    fn draw_borders(&self, viewports: &[(usize, Viewport); MAX_TILES], tile_count: usize) {
        let screen_w = self.hw_fb.width() as usize;
        let screen_h = self.hw_fb.height() as usize;
        let focused = self.layout.focused_slot();

        // — GlassSignal: fill gap pixels between tiles with border color
        match self.layout.layout() {
            Layout::HSplit => {
                // Horizontal border between top and bottom tiles
                let (_, top_vp) = viewports[0];
                let border_y = top_vp.y as usize + top_vp.height as usize;
                let border_h = 2usize.min(screen_h.saturating_sub(border_y));
                self.fill_hw_rect(0, border_y, screen_w, border_h, self.border_color);
            }
            Layout::VSplit => {
                // Vertical border between left and right tiles
                let (_, left_vp) = viewports[0];
                let border_x = left_vp.x as usize + left_vp.width as usize;
                let border_w = 2usize.min(screen_w.saturating_sub(border_x));
                self.fill_hw_rect(border_x, 0, border_w, screen_h, self.border_color);
            }
            Layout::Quad => {
                // Cross-shaped border (horizontal + vertical)
                let (_, tl) = viewports[0];
                let border_x = tl.x as usize + tl.width as usize;
                let border_y = tl.y as usize + tl.height as usize;
                let bw = 2usize.min(screen_w.saturating_sub(border_x));
                let bh = 2usize.min(screen_h.saturating_sub(border_y));
                // Vertical bar
                self.fill_hw_rect(border_x, 0, bw, screen_h, self.border_color);
                // Horizontal bar
                self.fill_hw_rect(0, border_y, screen_w, bh, self.border_color);
            }
            _ => {}
        }

        // — GlassSignal: draw 1px focus highlight around the focused tile
        if tile_count > 1 {
            let (_, vp) = viewports[focused];
            let x = vp.x as usize;
            let y = vp.y as usize;
            let w = vp.width as usize;
            let h = vp.height as usize;
            // Top edge
            self.fill_hw_rect(x, y, w, 1, self.focus_color);
            // Bottom edge
            if y + h > 0 {
                self.fill_hw_rect(x, y + h - 1, w, 1, self.focus_color);
            }
            // Left edge
            self.fill_hw_rect(x, y, 1, h, self.focus_color);
            // Right edge
            if x + w > 0 {
                self.fill_hw_rect(x + w - 1, y, 1, h, self.focus_color);
            }
        }
    }

    /// Fill a rectangle on the hardware framebuffer with a raw ARGB color.
    /// — GlassSignal: used for borders and focus highlights only
    fn fill_hw_rect(&self, x: usize, y: usize, w: usize, h: usize, color_argb: u32) {
        let bpp = self.hw_fb.format().bytes_per_pixel() as usize;
        let dst_ptr = self.hw_fb.buffer();
        let dst_stride = self.hw_fb.stride() as usize;
        let screen_w = self.hw_fb.width() as usize;
        let screen_h = self.hw_fb.height() as usize;

        let x_end = (x + w).min(screen_w);
        let y_end = (y + h).min(screen_h);

        // — GlassSignal: convert ARGB to the framebuffer's pixel format
        let pixel_bytes = match self.hw_fb.format() {
            fb::PixelFormat::BGRA8888 => [
                (color_argb & 0xFF) as u8,         // B
                ((color_argb >> 8) & 0xFF) as u8,  // G
                ((color_argb >> 16) & 0xFF) as u8, // R
                ((color_argb >> 24) & 0xFF) as u8, // A
            ],
            _ => [
                ((color_argb >> 16) & 0xFF) as u8, // R
                ((color_argb >> 8) & 0xFF) as u8,  // G
                (color_argb & 0xFF) as u8,         // B
                ((color_argb >> 24) & 0xFF) as u8, // A
            ],
        };

        unsafe {
            for row in y..y_end {
                let row_offset = row * dst_stride;
                for col in x..x_end {
                    let offset = row_offset + col * bpp;
                    core::ptr::copy_nonoverlapping(
                        pixel_bytes.as_ptr(),
                        dst_ptr.add(offset),
                        bpp.min(4),
                    );
                }
            }
        }
    }
}

// ============================================================================
// Public API — called from kernel init, VT switch, terminal write, timer tick
// ============================================================================

/// Initialize the compositor. Called once during kernel boot after fb::init_from_boot().
/// Returns the VT0 backing framebuffer for the terminal renderer.
/// — NeonRoot: only VT0 gets a buffer at init. The rest are lazy-allocated
/// on first split/switch. No upfront memory waste.
pub fn init(hw_fb: Arc<dyn Framebuffer>) -> Option<Arc<dyn Framebuffer>> {
    let mut compositor = Compositor::new(hw_fb);
    let vt0_fb = compositor.get_vt_framebuffer(0);
    *COMPOSITOR.lock() = Some(compositor);
    FULL_REDRAW.store(true, Ordering::Release);
    os_log::println!("[COMP] compositor initialized (VT0 buffer allocated, rest on-demand)");
    vt0_fb
}

/// Get the backing framebuffer for a specific VT (allocates on demand).
/// Used by terminal::update_framebuffer() on VT switch.
/// — SableWire: first call for a new VT triggers ~4MB backing buffer allocation.
pub fn get_vt_framebuffer(vt_num: usize) -> Option<Arc<dyn Framebuffer>> {
    let mut guard = COMPOSITOR.lock();
    guard.as_mut().and_then(|c| c.get_vt_framebuffer(vt_num))
}

/// Mark a VT as dirty (its backing buffer has new content).
/// Called after terminal::write() or /dev/fb0 write.
/// — SableWire: lock-free, ISR-safe
#[inline]
pub fn mark_dirty(vt_num: usize) {
    if vt_num < MAX_VTS {
        VT_DIRTY[vt_num].store(true, Ordering::Release);
    }
}

/// Request a full redraw (e.g., after layout change or VT switch).
/// — SableWire: lock-free, ISR-safe
#[inline]
pub fn request_full_redraw() {
    FULL_REDRAW.store(true, Ordering::Release);
}

/// Composite all dirty VTs onto the hardware framebuffer.
/// Called from the 30Hz timer tick or on explicit flush request.
/// — SableWire: this is the only function that touches the hardware framebuffer.
pub fn tick() {
    // — SableWire: fast path — check if anything is dirty before grabbing the lock
    let any_dirty = FULL_REDRAW.load(Ordering::Acquire)
        || VT_DIRTY.iter().any(|d| d.load(Ordering::Acquire));

    // — GlassSignal: trace tick activity (first few only to avoid serial flood)
    static TICK_TRACE: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(0);
    let tick_n = TICK_TRACE.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
    if tick_n < 5 {
        unsafe {
            os_log::write_str_raw("[COMP-TICK] #");
            os_log::write_u32_raw(tick_n);
            if any_dirty {
                os_log::write_str_raw(" DIRTY — compositing\n");
            } else {
                os_log::write_str_raw(" clean\n");
            }
        }
    }

    if !any_dirty {
        return;
    }

    if let Some(mut guard) = COMPOSITOR.try_lock() {
        if let Some(ref mut compositor) = *guard {
            compositor.composite();
        }
    }
    // — SableWire: if try_lock fails, compositor is busy (terminal write holding it).
    // Next tick will catch up. No data loss — dirty flags persist.
}

/// Switch focus to a VT. Updates layout manager and triggers redraw.
/// Called from Alt+Fn keyboard shortcut.
pub fn focus_vt(vt_num: usize) {
    if let Some(ref mut compositor) = *COMPOSITOR.lock() {
        compositor.layout.focus_vt(vt_num);
        COMPOSITOR_FOCUS_VT.store(compositor.layout.focused_vt(), Ordering::Release);
        request_full_redraw();
    }
}

/// Get the currently focused VT index (lock-free, ISR-safe).
#[inline]
pub fn focused_vt() -> usize {
    COMPOSITOR_FOCUS_VT.load(Ordering::Acquire)
}

/// Set the tiling layout. Triggers full redraw.
pub fn set_layout(layout: Layout) {
    if let Some(ref mut compositor) = *COMPOSITOR.lock() {
        compositor.layout.set_layout(layout);
        COMPOSITOR_FOCUS_VT.store(compositor.layout.focused_vt(), Ordering::Release);
        request_full_redraw();
        os_log::println!("[COMP] layout={:?} tiles={}", layout, compositor.layout.tile_count());
    }
}

/// Toggle fullscreen ↔ last split layout (Alt+Enter).
pub fn toggle_fullscreen() {
    if let Some(ref mut compositor) = *COMPOSITOR.lock() {
        compositor.layout.toggle_fullscreen();
        COMPOSITOR_FOCUS_VT.store(compositor.layout.focused_vt(), Ordering::Release);
        request_full_redraw();
        os_log::println!("[COMP] toggle → {:?}", compositor.layout.layout());
    }
}

/// Cycle focus to next visible tile (Alt+Tab).
pub fn cycle_focus() {
    if let Some(ref mut compositor) = *COMPOSITOR.lock() {
        compositor.layout.cycle_focus();
        COMPOSITOR_FOCUS_VT.store(compositor.layout.focused_vt(), Ordering::Release);
        request_full_redraw();
    }
}

/// Get the current layout mode.
pub fn current_layout() -> Layout {
    COMPOSITOR.lock().as_ref()
        .map(|c| c.layout.layout())
        .unwrap_or(Layout::Fullscreen)
}

/// Get viewport info for a VT (used by /dev/fb0 ioctl to report resolution).
pub fn get_vt_viewport(vt_num: usize) -> Option<Viewport> {
    let guard = COMPOSITOR.lock();
    let compositor = guard.as_ref()?;
    let viewports = compositor.layout.compute_viewports();
    let tile_count = compositor.layout.tile_count();
    for i in 0..tile_count {
        let (idx, viewport) = viewports[i];
        if idx == vt_num {
            return Some(viewport);
        }
    }
    // — GlassSignal: VT not currently visible — return full screen as fallback
    Some(Viewport::new(0, 0, compositor.hw_fb.width(), compositor.hw_fb.height()))
}

/// Set a VT's display mode (Text/Graphics).
pub fn set_vt_mode(vt_num: usize, mode: VtMode) {
    if let Some(ref mut compositor) = *COMPOSITOR.lock() {
        if vt_num < MAX_VTS {
            compositor.vt_modes[vt_num] = mode;
        }
    }
}

/// Get a VT's display mode.
pub fn get_vt_mode(vt_num: usize) -> VtMode {
    COMPOSITOR.lock().as_ref()
        .map(|c| {
            if vt_num < MAX_VTS { c.vt_modes[vt_num] } else { VtMode::Text }
        })
        .unwrap_or(VtMode::Text)
}

/// Check if compositor is initialized (lock-free quick check).
pub fn is_initialized() -> bool {
    // — SableWire: try_lock instead of load(AtomicBool) because we don't
    // want a separate atomic just for this. try_lock is cheap when uncontended.
    COMPOSITOR.try_lock().map_or(false, |g| g.is_some())
}

/// Update the hardware framebuffer reference (e.g., after VirtIO-GPU init).
/// — GlassSignal: hot-swap the compositor's output target
pub fn update_hw_framebuffer(hw_fb: Arc<dyn Framebuffer>) {
    if let Some(ref mut compositor) = *COMPOSITOR.lock() {
        compositor.hw_fb = hw_fb;
        compositor.layout.update_screen_size(
            compositor.hw_fb.width(),
            compositor.hw_fb.height(),
        );
        request_full_redraw();
    }
}
