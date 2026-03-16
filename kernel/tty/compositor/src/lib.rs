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
pub mod events;
pub mod layout;
pub mod scrollbar;

use alloc::sync::Arc;
use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use spin::Mutex;

use backing_fb::BackingFramebuffer;
use events::{EventHandler, DragState, HitZone, MouseState};
use fb::Framebuffer;
use layout::{Layout, LayoutManager, ScrollbarFlags, Viewport, ViewportGeometry, MAX_TILES,
             SCROLLBAR_WIDTH, SCROLLBAR_HEIGHT};
pub use layout::{MAX_VTS, LOG_VT};
use scrollbar::{Scrollbar, Orientation, ScrollContent, ScrollbarHitZone, PartState};

/// — NeonVale: GPU flush bounding box. Tracks the minimal rectangle of pixels
/// that actually changed since last flush. Why shove 3MB through the VirtIO pipe
/// when a cursor blink touched 128 bytes? This is the difference between 60fps
/// and slideshow mode on VirtIO-GPU. Reset after each flush, extended by every
/// write to hw_fb (blit, fill, scrollbar, statusbar, cursor).
#[derive(Clone, Copy, Debug)]
pub struct DirtyRect {
    /// — NeonVale: bounding box edges. u32::MAX/0 sentinel = "nothing dirty yet"
    pub x_min: u32,
    pub y_min: u32,
    pub x_max: u32,
    pub y_max: u32,
    /// — NeonVale: anything actually written since last flush?
    pub pending: bool,
}

impl DirtyRect {
    /// — NeonVale: fresh empty rect. No pixels touched, no flush needed.
    pub const fn new() -> Self {
        DirtyRect {
            x_min: u32::MAX,
            y_min: u32::MAX,
            x_max: 0,
            y_max: 0,
            pending: false,
        }
    }

    /// — NeonVale: extend the dirty rect to include this rectangle.
    /// Called by every function that touches hw_fb pixels.
    #[inline]
    pub fn extend(&mut self, x: u32, y: u32, w: u32, h: u32) {
        if w == 0 || h == 0 { return; }
        let x2 = x.saturating_add(w);
        let y2 = y.saturating_add(h);
        if x < self.x_min { self.x_min = x; }
        if y < self.y_min { self.y_min = y; }
        if x2 > self.x_max { self.x_max = x2; }
        if y2 > self.y_max { self.y_max = y2; }
        self.pending = true;
    }

    /// — NeonVale: mark the entire screen dirty. Used for full redraws.
    #[inline]
    pub fn mark_full(&mut self, screen_w: u32, screen_h: u32) {
        self.x_min = 0;
        self.y_min = 0;
        self.x_max = screen_w;
        self.y_max = screen_h;
        self.pending = true;
    }

    /// — NeonVale: reset after flush. Ready for next frame's damage accumulation.
    #[inline]
    pub fn reset(&mut self) {
        self.x_min = u32::MAX;
        self.y_min = u32::MAX;
        self.x_max = 0;
        self.y_max = 0;
        self.pending = false;
    }

    /// — NeonVale: get clamped flush region. Returns (x, y, w, h) or None if clean.
    #[inline]
    pub fn flush_region(&self, screen_w: u32, screen_h: u32) -> Option<(u32, u32, u32, u32)> {
        if !self.pending { return None; }
        let x = self.x_min.min(screen_w);
        let y = self.y_min.min(screen_h);
        let x2 = self.x_max.min(screen_w);
        let y2 = self.y_max.min(screen_h);
        if x >= x2 || y >= y2 { return None; }
        Some((x, y, x2 - x, y2 - y))
    }
}

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
    AtomicBool::new(false), // — NeonVale: LOG VT
];

/// Global full-redraw flag — set on layout change or VT switch
static FULL_REDRAW: AtomicBool = AtomicBool::new(true);

/// Active VT for input routing (mirrors vt::ACTIVE_VT but compositor-managed)
static COMPOSITOR_FOCUS_VT: AtomicUsize = AtomicUsize::new(0);

/// — SoftGlyph: cursor position changed — needs redraw even if no VT is dirty
static CURSOR_DIRTY: AtomicBool = AtomicBool::new(false);

/// — EchoFrame: scrollbar visual state changed — needs redraw without full VT blit
static SCROLLBAR_DIRTY: AtomicBool = AtomicBool::new(false);

/// — GlassSignal: bottom status bar height in pixels. Always visible.
/// Dark bg, green for active VT, dim for inactive, accent for KB button. — SableWire
const STATUSBAR_HEIGHT: u32 = 24;

/// — GlassSignal: status bar dirty flag. Set on VT switch, OSK toggle, init. — SableWire
static STATUSBAR_DIRTY: AtomicBool = AtomicBool::new(true);

/// — SableWire: deferred layout change flag. Set in tick() (ISR) when reserved_bottom
/// changes but we can't call apply_layout_change (blocking locks). Consumed by next
/// non-ISR focus_vt or explicit layout change. Terminal resize + SIGWINCH happen then.
static PENDING_LAYOUT_CHANGE: AtomicBool = AtomicBool::new(false);

/// — GlassSignal: status bar colors — cyberpunk aesthetic for the strip at the bottom
const SB_BG_COLOR: u32 = 0xFF0D0D1A;        // near-black background
const SB_ACTIVE_VT_COLOR: u32 = 0xFF00AA55; // green for focused VT
const SB_INACTIVE_VT_COLOR: u32 = 0xFF333344; // dim for unfocused VTs
const SB_KB_COLOR: u32 = 0xFF0088CC;        // accent blue for KB toggle button
const SB_TEXT_COLOR: u32 = 0xFFE0E0E0;      // white text
/// — NeonVale: amber color for the LOG VT button in the status bar
const SB_LOG_COLOR: u32 = 0xFFCC8800;

/// — SoftGlyph: lock-free mouse init flag. Set once when compositor creates the
/// cursor. ISR code checks this instead of try_lock() on the compositor mutex —
/// try_lock fails when tick() holds the lock, which killed the entire mouse path.
static MOUSE_INITIALIZED: AtomicBool = AtomicBool::new(false);

/// Callback for notifying the VT layer about per-VT winsize changes.
/// — GlassSignal: compositor can't depend on vt (circular), so the kernel
/// registers this callback during init. Args: (vt_num, rows, cols, xpixel, ypixel).
type WinsizeCallbackFn = fn(usize, u16, u16, u16, u16);
static mut WINSIZE_CALLBACK: Option<WinsizeCallbackFn> = None;

/// Register the winsize callback. Called once during kernel init.
///
/// # Safety
/// Must be called during single-threaded initialization.
pub unsafe fn set_winsize_callback(f: WinsizeCallbackFn) {
    unsafe { WINSIZE_CALLBACK = Some(f); }
}

/// The global compositor instance
static COMPOSITOR: Mutex<Option<Compositor>> = Mutex::new(None);

/// — GlassSignal: Default font cell dimensions (PSF2 8x16). Used for text grid
/// calculations. Updated if a different font is loaded.
const DEFAULT_CELL_WIDTH: u32 = 8;
const DEFAULT_CELL_HEIGHT: u32 = 16;

/// Compositor state
pub struct Compositor {
    /// The real hardware framebuffer — ONLY the compositor touches this
    hw_fb: Arc<dyn Framebuffer>,
    /// Per-VT virtual framebuffers (pixel canvases) — allocated lazily on first use.
    /// — SableWire: only VT0 gets a VFB at init. The rest spawn on demand
    /// when you split the screen or switch VTs. Sized to viewport usable area,
    /// allocated from buddy allocator physical frames, freed on Drop. No waste.
    vt_buffers: [Option<Arc<BackingFramebuffer>>; MAX_VTS],
    /// Per-VT display mode
    vt_modes: [VtMode; MAX_VTS],
    /// Per-VT viewport geometries — None for off-screen VTs.
    /// — GlassSignal: single source of truth for VFB dimensions, text grid sizes,
    /// and compositor blit positions. Recomputed on every layout change.
    vt_geometries: [Option<ViewportGeometry>; MAX_VTS],
    /// Layout manager — viewport geometry
    layout: LayoutManager,
    /// Font cell dimensions for text grid calculations
    cell_width: u32,
    cell_height: u32,
    /// Border color for split-mode dividers (cyan highlight)
    border_color: u32,
    /// Focus highlight color
    focus_color: u32,
    /// — GlassSignal: per-VT scrollbar visibility flags
    vt_scrollbar_flags: [ScrollbarFlags; MAX_VTS],
    /// — EchoFrame: Win95-style scrollbar widgets, one vertical + one horizontal per VT
    vscrollbars: [Scrollbar; MAX_VTS],
    hscrollbars: [Scrollbar; MAX_VTS],
    /// — GlassSignal: mouse event handler — owns drag state, hit-testing, the works
    event_handler: EventHandler,
    /// — SoftGlyph: mouse cursor — compositor draws it last, on top of everything.
    /// Position tracked here, save/restore for cursor-only movement (no VT dirty).
    mouse_cursor: Option<fb::mouse::MouseCursor>,
    /// — NeonVale: GPU flush damage tracking. Only the pixels that changed get
    /// pushed through VirtIO-GPU. A cursor blink used to flush 3MB. Now it flushes
    /// ~128 bytes. The difference between "smooth" and "why is my VM at 100% CPU."
    dirty_rect: DirtyRect,
    /// — GlassSignal: cached viewport computation. Recomputed only on layout
    /// change (resize, VT add/remove, OSK toggle), NOT on every composite/draw call.
    /// Before this cache, compute_viewports() ran 4-6 times per tick. Pure waste.
    cached_viewports: [(usize, Viewport); MAX_TILES],
    /// — GlassSignal: cached viewport generation counter. Bumped on layout change
    /// so we know when to invalidate.
    viewport_generation: u32,
    /// — EchoFrame: cached scroll content per VT for scrollbar skip-redraw.
    /// If thumb position hasn't changed, skip the ~30 fill_rect calls per scrollbar.
    cached_scroll_content: [Option<ScrollContent>; MAX_VTS],
}

impl Compositor {
    /// Create a new compositor. Pre-allocates ALL VT backing buffers at boot.
    /// — SableWire: lazy allocation caused too many init-order headaches and
    /// race conditions. ~24MB upfront is cheap insurance against display bugs.
    fn new(hw_fb: Arc<dyn Framebuffer>) -> Self {
        let width = hw_fb.width();
        let height = hw_fb.height();
        let format = hw_fb.format();

        os_log::println!("[COMP] init {}x{} stride={} bpp={} (eager alloc, all VTs)",
            width, height, hw_fb.stride(), format.bytes_per_pixel() * 8);

        let mut layout = LayoutManager::new(width, height);
        let cell_width = DEFAULT_CELL_WIDTH;
        let cell_height = DEFAULT_CELL_HEIGHT;

        // — GlassSignal: reserve bottom area for status bar (+ OSK if visible)
        let reserved = STATUSBAR_HEIGHT + vkbd::keyboard_height();
        layout.set_reserved_bottom(reserved);
        vkbd::set_bottom_offset(STATUSBAR_HEIGHT);

        // — GlassSignal: all VTs get vertical scrollbar track reserved
        let mut vt_scrollbar_flags = [ScrollbarFlags::default(); MAX_VTS];
        for i in 0..MAX_VTS {
            vt_scrollbar_flags[i] = ScrollbarFlags { vscroll: true, hscroll: false };
        }

        // — GlassSignal: compute initial geometries (Fullscreen, VT0 only visible)
        let vt_geometries = layout.recompute_geometries(cell_width, cell_height, &vt_scrollbar_flags);

        // — SableWire: pre-allocate ALL VT backing buffers. Off-screen VTs
        // use the same dimensions as VT0 (fullscreen usable area) so every VT
        // starts consistent. VT0 is just an alias for the active VT — all VTs
        // should be the same size. — GlassSignal: the old code gave VT0 the
        // scrollbar-reduced width (usable_width) but VT1-5 got raw screen size.
        // That's a 16px mismatch that corrupts stride calculations on VT switch.
        let default_w = vt_geometries[0].map_or(width, |g| g.usable_width);
        let default_h = vt_geometries[0].map_or(height, |g| g.usable_height);
        let mut vt_buffers: [Option<Arc<BackingFramebuffer>>; MAX_VTS] =
            core::array::from_fn(|_| None);
        for i in 0..MAX_VTS {
            let (w, h) = if let Some(geom) = vt_geometries[i] {
                (geom.usable_width, geom.usable_height)
            } else {
                // — SableWire: off-screen VTs get same dims as the visible one.
                // They'll resize on first layout change anyway.
                (default_w, default_h)
            };
            let stride = w * format.bytes_per_pixel() as u32;
            let buf = BackingFramebuffer::new(w, h, stride, format);
            os_log::println!("[COMP] VT{} buffer: {}KB ({}x{})",
                i, buf.size() / 1024, w, h);
            vt_buffers[i] = Some(Arc::new(buf));
        }

        // — GlassSignal: border colors — dark gray divider, cyan focus highlight
        let border_color = 0xFF333333; // dark gray ARGB
        let focus_color = 0xFF00AACC;  // cyan ARGB

        // — EchoFrame: create scrollbar widget instances for each VT
        let vscrollbars: [Scrollbar; MAX_VTS] = core::array::from_fn(|_| Scrollbar::new(Orientation::Vertical));
        let hscrollbars: [Scrollbar; MAX_VTS] = core::array::from_fn(|_| Scrollbar::new(Orientation::Horizontal));

        // — GlassSignal: compute initial viewports for the cache
        let initial_viewports = layout.compute_viewports();

        let mut comp = Compositor {
            hw_fb,
            vt_buffers,
            vt_modes: [VtMode::Text; MAX_VTS],
            vt_geometries,
            layout,
            cell_width,
            cell_height,
            border_color,
            focus_color,
            vt_scrollbar_flags,
            vscrollbars,
            hscrollbars,
            event_handler: EventHandler::new(),
            mouse_cursor: Some(fb::mouse::MouseCursor::new(width, height)),
            dirty_rect: DirtyRect::new(),
            cached_viewports: initial_viewports,
            viewport_generation: 0,
            cached_scroll_content: [None; MAX_VTS],
        };
        // — EchoFrame: position scrollbar widgets based on initial geometry
        comp.update_scrollbar_rects();
        comp
    }

    /// — GlassSignal: invalidate cached viewports. Must be called on any layout
    /// change (split, VT switch, resize, OSK toggle). Next access recomputes.
    fn invalidate_viewport_cache(&mut self) {
        self.cached_viewports = self.layout.compute_viewports();
        self.viewport_generation = self.viewport_generation.wrapping_add(1);
    }

    /// Ensure a VT has a VFB, allocating one on demand sized to its viewport.
    /// — SableWire: the lazy allocation hot path. First split/switch to a VT
    /// triggers a buddy alloc sized to viewport. Subsequent accesses are free.
    /// Returns true if the buffer exists (or was just created).
    fn ensure_vt_buffer(&mut self, vt_num: usize) -> bool {
        if vt_num >= MAX_VTS { return false; }
        if self.vt_buffers[vt_num].is_some() { return true; }

        // — GlassSignal: new text-mode VT gets vertical scrollbar track reserved
        if self.vt_modes[vt_num] == VtMode::Text {
            self.vt_scrollbar_flags[vt_num].vscroll = true;
        }

        // — GlassSignal: size VFB to viewport usable area if geometry exists,
        // otherwise fall back to full screen (off-screen VT being accessed early)
        let (w, h) = if let Some(geom) = self.vt_geometries[vt_num] {
            (geom.usable_width, geom.usable_height)
        } else {
            (self.hw_fb.width(), self.hw_fb.height())
        };
        let format = self.hw_fb.format();
        let stride = w * format.bytes_per_pixel() as u32;

        let buf = BackingFramebuffer::new(w, h, stride, format);
        os_log::println!("[COMP] VT{} buffer: {}KB ({}x{} on-demand)",
            vt_num, buf.size() / 1024, w, h);
        self.vt_buffers[vt_num] = Some(Arc::new(buf));
        true
    }

    /// Resize a VT's VFB to match new viewport dimensions.
    /// — GlassSignal: called on layout change. Allocates new buffer, copies
    /// old content clipped to min dimensions, frees old buffer. If alloc fails,
    /// keeps old buffer (graceful degradation — stale dimensions until next try).
    /// Returns the new Arc<BackingFramebuffer> if resize happened.
    fn resize_vt_buffer(&mut self, vt_num: usize, new_w: u32, new_h: u32) -> bool {
        if vt_num >= MAX_VTS { return false; }

        let old_buf = match self.vt_buffers[vt_num].take() {
            Some(b) => b,
            None => return false,
        };

        // — GlassSignal: skip if dimensions unchanged
        if old_buf.width() == new_w && old_buf.height() == new_h {
            self.vt_buffers[vt_num] = Some(old_buf);
            return false;
        }

        let format = old_buf.format();
        let new_stride = new_w * format.bytes_per_pixel() as u32;
        let new_buf = BackingFramebuffer::new(new_w, new_h, new_stride, format);

        // — GlassSignal: copy old content clipped to min(old, new) dimensions
        let copy_w = old_buf.width().min(new_w) as usize;
        let copy_h = old_buf.height().min(new_h) as usize;
        let bpp = format.bytes_per_pixel() as usize;
        let row_bytes = copy_w * bpp;
        let old_stride = old_buf.stride() as usize;
        let new_stride_usize = new_buf.stride() as usize;

        unsafe {
            let src = old_buf.raw_ptr();
            let dst = new_buf.raw_ptr() as *mut u8;
            for row in 0..copy_h {
                core::ptr::copy_nonoverlapping(
                    src.add(row * old_stride),
                    dst.add(row * new_stride_usize),
                    row_bytes,
                );
            }
        }

        os_log::println!("[COMP] VT{} resized: {}x{} → {}x{} ({}KB)",
            vt_num, old_buf.width(), old_buf.height(), new_w, new_h,
            new_buf.size() / 1024);

        // — GlassSignal: old_buf dropped here, frees physical frames
        self.vt_buffers[vt_num] = Some(Arc::new(new_buf));
        true
    }

    /// Recompute all VT geometries and resize VFBs to match.
    /// — GlassSignal: called on layout change, VT switch, screen resize.
    /// Returns list of (vt_num, old_geom, new_geom) for VTs that changed size,
    /// so the caller can trigger terminal resize + SIGWINCH.
    fn apply_layout_change(&mut self) -> [(usize, Option<ViewportGeometry>, Option<ViewportGeometry>); MAX_VTS] {
        let old_geometries = self.vt_geometries;
        self.vt_geometries = self.layout.recompute_geometries(self.cell_width, self.cell_height, &self.vt_scrollbar_flags);

        let changes: [(usize, Option<ViewportGeometry>, Option<ViewportGeometry>); MAX_VTS] =
            core::array::from_fn(|i| (i, old_geometries[i], self.vt_geometries[i]));

        for vt_num in 0..MAX_VTS {
            let new_geom = self.vt_geometries[vt_num];
            let old_geom = old_geometries[vt_num];

            match (old_geom, new_geom) {
                (_, Some(geom)) => {
                    // — GlassSignal: VT is visible — ensure buffer exists at correct size
                    if self.vt_buffers[vt_num].is_some() {
                        self.resize_vt_buffer(vt_num, geom.usable_width, geom.usable_height);
                    }
                    // — GlassSignal: tell the terminal emulator about the new VFB.
                    // Only if dimensions actually changed and VT has a buffer.
                    let dims_changed = match old_geom {
                        Some(old) => old.usable_width != geom.usable_width
                            || old.usable_height != geom.usable_height,
                        None => true,
                    };
                    if dims_changed {
                        if let Some(ref buf) = self.vt_buffers[vt_num] {
                            terminal::resize_vt(vt_num, buf.clone() as Arc<dyn Framebuffer>);
                        }
                        // — GlassSignal: notify VT layer about new winsize + SIGWINCH
                        unsafe {
                            if let Some(cb) = WINSIZE_CALLBACK {
                                cb(
                                    vt_num,
                                    geom.text_rows as u16,
                                    geom.text_cols as u16,
                                    geom.usable_width as u16,
                                    geom.usable_height as u16,
                                );
                            }
                        }
                    }
                }
                (Some(_), None) => {
                    // — GlassSignal: VT went off-screen — keep buffer, just stop blitting
                }
                (None, None) => {
                    // — GlassSignal: was off-screen, still off-screen — no-op
                }
            }
        }

        // — GlassSignal: refresh viewport cache after geometry recomputation
        self.invalidate_viewport_cache();

        // — EchoFrame: scrollbar widgets need updated positions after layout shift
        self.update_scrollbar_rects();

        changes
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
    #[allow(dead_code)]
    fn get_vt_buffer(&self, vt_num: usize) -> Option<&Arc<BackingFramebuffer>> {
        if vt_num >= MAX_VTS { return None; }
        self.vt_buffers[vt_num].as_ref()
    }

    /// Composite visible VT buffers onto the hardware framebuffer.
    /// — SableWire: only blits VTs that have geometry (are visible). Off-screen
    /// VTs are skipped entirely — no buffer access, no dirty flag check.
    /// Dirty flags are passed in from tick() — never double-consume atomics.
    fn composite(&mut self, full_redraw: bool, sb_dirty: bool) {
        // — GlassSignal: use cached viewports — no recomputation per tick
        let viewports = self.cached_viewports;
        let tile_count = self.layout.tile_count();

        // — NeonVale: Erase cursor before VT blit. The save_buffer may contain
        // stale content, but the VT blit immediately overwrites it in the viewport
        // area. Scrollbar/border draws cover any remaining stale pixels in chrome.
        // The cursor is redrawn LAST in tick() via cursor.redraw() on fresh content.
        if let Some(ref mut cursor) = self.mouse_cursor {
            cursor.erase(&*self.hw_fb);
        }

        // — InputShade: clip VT blit height to avoid overwriting status bar + OSK.
        // The status bar is always visible; OSK renders above it when shown.
        // Without this clip, every cursor blink → overwrites bottom chrome → repaint hell.
        // — GlassSignal: total_bottom_reserved = STATUSBAR_HEIGHT + keyboard_height(). — SableWire
        let vkbd_clip = vkbd::total_bottom_reserved();

        let mut any_vt_blitted = false;
        for slot_idx in 0..tile_count {
            let (vt_idx, viewport) = viewports[slot_idx];
            if viewport.width == 0 || viewport.height == 0 {
                continue;
            }

            // — SableWire: lazy-allocate VFB for newly-visible VTs
            self.ensure_vt_buffer(vt_idx);

            // — SableWire: skip clean buffers unless full redraw requested
            if !full_redraw && !VT_DIRTY[vt_idx].swap(false, Ordering::AcqRel) {
                continue;
            }

            // — NeonVale: clone the Arc to avoid borrow conflict with &mut self in blit_vt_to_hw.
            // Arc::clone is just a refcount bump — ~2ns, not a buffer copy.
            let src_buf = self.vt_buffers[vt_idx].as_ref().cloned();
            if let Some(src_buf) = src_buf {
                // — GlassSignal: blit VFB content into viewport rect on hardware FB.
                let geom = self.vt_geometries[vt_idx];
                let mut blit_vp = if let Some(g) = geom {
                    Viewport::new(
                        g.screen_x + g.border_left,
                        g.screen_y + g.border_top,
                        g.usable_width,
                        g.usable_height,
                    )
                } else {
                    viewport
                };

                // — InputShade: clip blit height to avoid overwriting vkbd area.
                // Only clip when NOT doing full_redraw (full_redraw repaints vkbd too).
                if vkbd_clip > 0 && !full_redraw {
                    let screen_h = self.hw_fb.height();
                    let kb_top = screen_h.saturating_sub(vkbd_clip);
                    let blit_bottom = blit_vp.y + blit_vp.height;
                    if blit_bottom > kb_top {
                        blit_vp.height = kb_top.saturating_sub(blit_vp.y);
                    }
                }

                if blit_vp.height > 0 {
                    self.blit_vt_to_hw(&src_buf, &blit_vp);
                    any_vt_blitted = true;
                }
            }
        }

        // — GlassSignal: draw borders between tiles in split modes
        if self.layout.layout() != Layout::Fullscreen {
            self.draw_borders(&viewports, tile_count);
        }

        // — GlassSignal: draw scrollbar chrome after VT content + borders.
        // Redraw when: scrollbar visual state changed (hover/press), any VT was
        // blitted (thumb position may have changed), or full layout redraw.
        // — EchoFrame: this skips ~30 fill_rect calls per frame when truly idle.
        if full_redraw || sb_dirty || any_vt_blitted {
            self.draw_scrollbars();
        }
    }

    /// Blit a VT backing buffer into a viewport rectangle on the hardware fb.
    /// — SableWire: the hot inner loop. ~0.3ms for full-screen at 1024×768.
    /// — NeonVale: extends dirty_rect so flush_region knows what changed.
    fn blit_vt_to_hw(&mut self, src: &BackingFramebuffer, viewport: &Viewport) {
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

        // — NeonVale: tell the damage tracker what we just scribbled on
        self.dirty_rect.extend(viewport.x, viewport.y, blit_w as u32, blit_h as u32);
    }

    /// — NeonVale: Reblit just the mouse cursor's old+new bounding box from the
    /// active VT's backing buffer to the hw framebuffer. This replaces the broken
    /// save_buffer erase approach for cursor-only frames. save_buffer gets stale
    /// when VT content changes between save and restore, causing trails. This method
    /// blits fresh VT pixels over the cursor area, then the cursor is redrawn on top.
    fn reblit_cursor_area(&mut self) {
        let cursor_bounds = match self.mouse_cursor {
            Some(ref c) => c.bounds(),
            None => return,
        };
        let (cx, cy, cw, ch) = cursor_bounds;
        if cw == 0 || ch == 0 { return; }

        // — NeonVale: find which VT's backing buffer covers the cursor area, then
        // blit just that sub-rect from the VT buffer to the hw framebuffer.
        let active_vt = self.layout.focused_vt();
        let src_buf = self.vt_buffers[active_vt].as_ref().cloned();
        if let Some(src_buf) = src_buf {
            let geom = self.vt_geometries[active_vt];
            let (vp_x, vp_y) = match geom {
                Some(g) => (g.screen_x + g.border_left, g.screen_y + g.border_top),
                None => (0, 0),
            };

            let bpp = self.hw_fb.format().bytes_per_pixel() as usize;
            let dst_stride = self.hw_fb.stride() as usize;
            let src_stride = src_buf.stride() as usize;
            let dst_ptr = self.hw_fb.buffer();
            let src_ptr = src_buf.buffer();

            if src_ptr.is_null() || dst_ptr.is_null() { return; }

            // — NeonVale: clip cursor bounds to VT viewport and screen
            let screen_w = self.hw_fb.width();
            let screen_h = self.hw_fb.height();
            let x_start = cx.max(vp_x);
            let y_start = cy.max(vp_y);
            let x_end = (cx + cw).min(screen_w);
            let y_end = (cy + ch).min(screen_h);

            if x_start >= x_end || y_start >= y_end { return; }

            let row_bytes = (x_end - x_start) as usize * bpp;

            for y in y_start..y_end {
                let src_y = (y - vp_y) as usize;
                let src_x = (x_start - vp_x) as usize;
                let src_offset = src_y * src_stride + src_x * bpp;
                let dst_offset = y as usize * dst_stride + x_start as usize * bpp;

                // — NeonVale: bounds check to avoid reading past VT buffer
                if src_offset + row_bytes > src_buf.size() { continue; }

                unsafe {
                    core::ptr::copy_nonoverlapping(
                        src_ptr.add(src_offset),
                        dst_ptr.add(dst_offset),
                        row_bytes,
                    );
                }
            }
        }
    }

    /// — EchoFrame: update scrollbar widget positions from current viewport geometries.
    /// Called after any layout change (split, VT switch, resize).
    fn update_scrollbar_rects(&mut self) {
        let tile_count = self.layout.tile_count();
        // — GlassSignal: use cached viewports — layout already updated the cache
        let viewports = self.cached_viewports;

        for slot_idx in 0..tile_count {
            let (vt_idx, viewport) = viewports[slot_idx];
            if vt_idx >= MAX_VTS || viewport.width == 0 || viewport.height == 0 {
                continue;
            }
            let geom = match self.vt_geometries[vt_idx] {
                Some(g) => g,
                None => continue,
            };
            let flags = self.vt_scrollbar_flags[vt_idx];

            // — EchoFrame: vertical scrollbar sits on the right edge
            if flags.vscroll {
                let sb_x = (geom.screen_x + geom.total_width).saturating_sub(SCROLLBAR_WIDTH);
                let sb_y = geom.screen_y + geom.border_top;
                let sb_h = geom.usable_height;
                self.vscrollbars[vt_idx].set_rect(sb_x, sb_y, SCROLLBAR_WIDTH, sb_h);
            }

            // — EchoFrame: horizontal scrollbar sits on the bottom edge
            if flags.hscroll {
                let sb_x = geom.screen_x + geom.border_left;
                let sb_y = (geom.screen_y + geom.total_height).saturating_sub(SCROLLBAR_HEIGHT);
                let sb_w = geom.usable_width;
                self.hscrollbars[vt_idx].set_rect(sb_x, sb_y, sb_w, SCROLLBAR_HEIGHT);
            }
        }
    }

    /// — EchoFrame: Draw Win95-style scrollbar widgets for all visible VTs.
    /// Each scrollbar is a self-contained object that knows how to render itself.
    /// — NeonVale: now caches ScrollContent per VT — skips the ~30 fill_rect calls
    /// when thumb position hasn't moved. Combined with dirty rect tracking, this
    /// means idle scrollbars cost exactly zero GPU bandwidth.
    fn draw_scrollbars(&mut self) {
        let tile_count = self.layout.tile_count();
        // — GlassSignal: use cached viewports — no recomputation per draw
        let viewports = self.cached_viewports;

        for slot_idx in 0..tile_count {
            let (vt_idx, viewport) = viewports[slot_idx];
            if vt_idx >= MAX_VTS || viewport.width == 0 || viewport.height == 0 {
                continue;
            }
            let flags = self.vt_scrollbar_flags[vt_idx];
            let geom = match self.vt_geometries[vt_idx] {
                Some(g) => g,
                None => continue,
            };

            // — EchoFrame: query terminal state and update scrollbar content
            let sb_state = terminal::get_scrollbar_state(vt_idx);

            // ── Vertical scrollbar ──
            if flags.vscroll {
                let mut need_redraw = true;
                if let Some(state) = sb_state {
                    let total = state.scrollback_len + state.rows as usize;
                    let visible = state.rows as usize;
                    let new_content = ScrollContent {
                        total,
                        visible,
                        position: state.scroll_offset,
                    };

                    // — EchoFrame: skip redraw if content state is identical to cached version.
                    // Thumb position is derived from content — same content = same pixels.
                    // Also skip if any hover/press state changed (handled by sb_dirty flag).
                    if let Some(cached) = self.cached_scroll_content[vt_idx] {
                        if cached.total == new_content.total
                            && cached.visible == new_content.visible
                            && cached.position == new_content.position
                            && self.vscrollbars[vt_idx].arrow_dec_state == PartState::Normal
                            && self.vscrollbars[vt_idx].arrow_inc_state == PartState::Normal
                            && self.vscrollbars[vt_idx].thumb_state == PartState::Normal
                        {
                            need_redraw = false;
                        }
                    }

                    self.vscrollbars[vt_idx].set_content(new_content);
                    self.cached_scroll_content[vt_idx] = Some(new_content);
                }

                if need_redraw {
                    // — NeonVale: extend dirty rect for the scrollbar's bounding box
                    let sb = &self.vscrollbars[vt_idx];
                    self.dirty_rect.extend(sb.x, sb.y, sb.width, sb.height);

                    // — EchoFrame: render the widget. Closure bridges to fill_hw_rect.
                    let hw_fb = &self.hw_fb;
                    self.vscrollbars[vt_idx].draw(&mut |x, y, w, h, color| {
                        fill_hw_rect_static(hw_fb.as_ref(), x, y, w, h, color);
                    });
                }
            }

            // ── Horizontal scrollbar ──
            if flags.hscroll {
                let mut need_hredraw = true;
                if let Some(state) = sb_state {
                    let total_w = state.max_line_width;
                    let visible_w = state.cols as usize;
                    let new_content = ScrollContent {
                        total: total_w,
                        visible: visible_w,
                        position: state.h_scroll_offset,
                    };

                    // — EchoFrame: same caching logic for horizontal scrollbar.
                    // Index offset by MAX_VTS would be cleaner but we only have one
                    // cache array — horizontal scrollbar uses different state entirely
                    // so compare against the hscrollbar's actual content.
                    let old = &self.hscrollbars[vt_idx].content;
                    if old.total == new_content.total
                        && old.visible == new_content.visible
                        && old.position == new_content.position
                        && self.hscrollbars[vt_idx].arrow_dec_state == PartState::Normal
                        && self.hscrollbars[vt_idx].arrow_inc_state == PartState::Normal
                        && self.hscrollbars[vt_idx].thumb_state == PartState::Normal
                    {
                        need_hredraw = false;
                    }

                    self.hscrollbars[vt_idx].set_content(new_content);
                }

                if need_hredraw {
                    // — NeonVale: extend dirty rect for horizontal scrollbar bounds
                    let sb = &self.hscrollbars[vt_idx];
                    self.dirty_rect.extend(sb.x, sb.y, sb.width, sb.height);

                    let hw_fb = &self.hw_fb;
                    self.hscrollbars[vt_idx].draw(&mut |x, y, w, h, color| {
                        fill_hw_rect_static(hw_fb.as_ref(), x, y, w, h, color);
                    });
                }
            }

            // — EchoFrame: corner block where both scrollbars meet — raised face
            if flags.vscroll && flags.hscroll {
                let corner_x = (geom.screen_x + geom.total_width).saturating_sub(SCROLLBAR_WIDTH) as usize;
                let corner_y = (geom.screen_y + geom.total_height).saturating_sub(SCROLLBAR_HEIGHT) as usize;
                self.fill_hw_rect(corner_x, corner_y, SCROLLBAR_WIDTH as usize, SCROLLBAR_HEIGHT as usize, 0xFFC0C0C0);
            }
        }
    }

    /// Draw border lines between tiles and a focus highlight on the active tile.
    /// — GlassSignal: 2px dark gray dividers + 1px cyan focus border
    fn draw_borders(&mut self, viewports: &[(usize, Viewport); MAX_TILES], tile_count: usize) {
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
    /// — GlassSignal: used for borders, focus highlights, corner blocks.
    /// — SableWire: row-batched like fill_hw_rect_static — see that function's
    /// comment for why per-pixel MMIO writes are a death sentence in ISR context.
    /// — NeonVale: now extends dirty_rect for regional GPU flush.
    fn fill_hw_rect(&mut self, x: usize, y: usize, w: usize, h: usize, color_argb: u32) {
        fill_hw_rect_static(self.hw_fb.as_ref(), x, y, w, h, color_argb);
        self.dirty_rect.extend(x as u32, y as u32, w as u32, h as u32);
    }

    /// — GlassSignal: draw the bottom status bar. VT1-VT6 buttons + KB toggle.
    /// Active VT glows green, others are dim. KB button is accent blue.
    /// Renders at Y = screen_h - STATUSBAR_HEIGHT. Uses PSF2 font for labels.
    /// No heap allocation, no locks beyond what we already hold. — SableWire
    fn draw_statusbar(&mut self) {
        let screen_w = self.hw_fb.width();
        let screen_h = self.hw_fb.height();
        let bar_y = screen_h.saturating_sub(STATUSBAR_HEIGHT) as usize;
        let focused_vt = self.layout.focused_vt();

        // — GlassSignal: fill bar background
        self.fill_hw_rect(0, bar_y, screen_w as usize, STATUSBAR_HEIGHT as usize, SB_BG_COLOR);

        let btn_w = 40usize;
        let btn_h = 18usize;
        let btn_y = bar_y + 3; // 3px top margin within bar
        let btn_gap = 4usize;
        let mut btn_x = 8usize; // left margin

        // — GlassSignal: VT1-VT6 buttons + LOG button — NeonVale
        for vt in 0..MAX_VTS {
            let is_log = vt == LOG_VT;
            let btn_color = if vt == focused_vt {
                SB_ACTIVE_VT_COLOR
            } else if is_log {
                SB_LOG_COLOR
            } else {
                SB_INACTIVE_VT_COLOR
            };
            self.fill_hw_rect(btn_x, btn_y, btn_w, btn_h, btn_color);
            if is_log {
                self.draw_text_on_hw(btn_x, btn_y, btn_w, btn_h, b"LOG", SB_TEXT_COLOR);
            } else {
                let label: [u8; 3] = [b'V', b'T', b'1' + vt as u8];
                self.draw_text_on_hw(btn_x, btn_y, btn_w, btn_h, &label, SB_TEXT_COLOR);
            }
            btn_x += btn_w + btn_gap;
        }

        // — GlassSignal: KB toggle button on the right side
        let kb_x = screen_w as usize - btn_w - 8;
        let kb_color = if vkbd::is_visible() { SB_ACTIVE_VT_COLOR } else { SB_KB_COLOR };
        self.fill_hw_rect(kb_x, btn_y, btn_w, btn_h, kb_color);
        self.draw_text_on_hw(kb_x, btn_y, btn_w, btn_h, b"KB", SB_TEXT_COLOR);
    }

    /// — GlassSignal: draw text centered in a rectangle on the hardware framebuffer.
    /// Uses PSF2 font glyphs. No allocation. — SableWire
    fn draw_text_on_hw(&self, x: usize, y: usize, w: usize, h: usize,
                       text: &[u8], color_argb: u32) {
        let font = &fb::font::PSF2_FONT;
        let glyph_w = font.width as usize;
        let glyph_h = font.height as usize;
        let label_w = text.len() * glyph_w;
        let text_x = x + w.saturating_sub(label_w) / 2;
        let text_y = y + h.saturating_sub(glyph_h) / 2;

        let fg_pixel = match self.hw_fb.format() {
            fb::PixelFormat::BGRA8888 => [
                (color_argb & 0xFF) as u8,
                ((color_argb >> 8) & 0xFF) as u8,
                ((color_argb >> 16) & 0xFF) as u8,
                ((color_argb >> 24) & 0xFF) as u8,
            ],
            _ => [
                ((color_argb >> 16) & 0xFF) as u8,
                ((color_argb >> 8) & 0xFF) as u8,
                (color_argb & 0xFF) as u8,
                ((color_argb >> 24) & 0xFF) as u8,
            ],
        };

        let buf = self.hw_fb.buffer();
        let stride = self.hw_fb.stride() as usize;
        let bpp = self.hw_fb.format().bytes_per_pixel() as usize;

        for (i, &ch) in text.iter().enumerate() {
            if let Some(glyph) = font.glyph(ch as char) {
                let gx = text_x + i * glyph_w;
                let bytes_per_row = ((glyph.width + 7) / 8) as usize;
                unsafe {
                    for row in 0..glyph.height {
                        let row_offset = (text_y + row as usize) * stride;
                        let glyph_row_start = row as usize * bytes_per_row;
                        for col in 0..glyph.width {
                            let byte_idx = glyph_row_start + (col / 8) as usize;
                            let bit_idx = 7 - (col % 8);
                            if byte_idx < glyph.data.len()
                                && (glyph.data[byte_idx] >> bit_idx) & 1 != 0
                            {
                                let offset = row_offset + (gx + col as usize) * bpp;
                                core::ptr::copy_nonoverlapping(
                                    fg_pixel.as_ptr(), buf.add(offset), bpp.min(4),
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    // ════════════════════════════════════════════════════════════════════════
    // Event handling — GlassSignal: the compositor IS the window manager
    // EchoFrame: scrollbar widget event helpers below
    // ════════════════════════════════════════════════════════════════════════

    /// — GlassSignal: get the list of visible VTs for hit-testing.
    /// Returns (vt_num, is_visible) pairs for each tile slot.
    fn visible_vts(&self) -> [(usize, bool); MAX_TILES] {
        // — GlassSignal: use cached viewports for hit-testing
        let viewports = self.cached_viewports;
        let tile_count = self.layout.tile_count();
        let mut result = [(0usize, false); MAX_TILES];
        for i in 0..tile_count {
            let (vt_idx, vp) = viewports[i];
            result[i] = (vt_idx, vp.width > 0 && vp.height > 0);
        }
        result
    }

    /// — GlassSignal: handle mouse button press. Returns what console.rs should do.
    fn handle_mouse_press(&mut self, button: MouseButton, x: i32, y: i32) -> MouseAction {
        let tiles = self.visible_vts();
        let zone = events::hit_test(
            x, y, &self.vt_geometries, &self.vt_scrollbar_flags, &tiles,
        );

        match button {
            MouseButton::Left => {
                self.event_handler.left_pressed = true;

                match zone {
                    HitZone::VScrollbar { vt } => {
                        // — EchoFrame: sub-hit-test with the scrollbar widget
                        let sub_zone = self.vscrollbars[vt].hit_test(x, y);
                        let sb_state = terminal::get_scrollbar_state(vt);
                        let (total, visible, cur_offset) = if let Some(s) = sb_state {
                            (s.scrollback_len + s.rows as usize, s.rows as usize, s.scroll_offset)
                        } else {
                            (0, 0, 0)
                        };
                        let scrollable = total.saturating_sub(visible);

                        match sub_zone {
                            ScrollbarHitZone::ArrowDec => {
                                // — EchoFrame: scroll up one line
                                self.vscrollbars[vt].arrow_dec_state = PartState::Pressed;
                                if scrollable > 0 {
                                    let new_off = (cur_offset + 1).min(scrollable);
                                    terminal::scroll_to_line(vt, new_off);
                                }
                                mark_dirty(vt);
                                MouseAction::Consumed
                            }
                            ScrollbarHitZone::ArrowInc => {
                                // — EchoFrame: scroll down one line
                                self.vscrollbars[vt].arrow_inc_state = PartState::Pressed;
                                if cur_offset > 0 {
                                    terminal::scroll_to_line(vt, cur_offset - 1);
                                }
                                mark_dirty(vt);
                                MouseAction::Consumed
                            }
                            ScrollbarHitZone::TrackBefore => {
                                // — EchoFrame: page up (scroll up by visible rows)
                                if scrollable > 0 {
                                    let page = visible.max(1);
                                    let new_off = (cur_offset + page).min(scrollable);
                                    terminal::scroll_to_line(vt, new_off);
                                }
                                mark_dirty(vt);
                                MouseAction::Consumed
                            }
                            ScrollbarHitZone::TrackAfter => {
                                // — EchoFrame: page down (scroll down by visible rows)
                                if cur_offset > 0 {
                                    let page = visible.max(1);
                                    let new_off = cur_offset.saturating_sub(page);
                                    terminal::scroll_to_line(vt, new_off);
                                }
                                mark_dirty(vt);
                                MouseAction::Consumed
                            }
                            ScrollbarHitZone::Thumb => {
                                // — EchoFrame: start thumb drag
                                self.event_handler.state = MouseState::ScrollbarDrag;
                                self.vscrollbars[vt].thumb_state = PartState::Pressed;
                                let track_h = self.vscrollbars[vt].track_pixel_length() as usize;
                                self.event_handler.drag = Some(DragState {
                                    vt,
                                    vertical: true,
                                    start_pos: y,
                                    start_offset: cur_offset,
                                    track_length: track_h,
                                    total_content: total,
                                    visible_content: visible,
                                });
                                mark_dirty(vt);
                                MouseAction::Consumed
                            }
                            _ => {
                                // — EchoFrame: corner/none hits just consume the event
                                // without entering drag mode. The old catch-all put us
                                // into ScrollbarDrag for ANY sub-zone miss, which meant
                                // clicking the corner dead zone or a hit-test edge case
                                // would start phantom drags. Track clicks are handled by
                                // TrackBefore/TrackAfter above. — GlassSignal
                                MouseAction::Consumed
                            }
                        }
                    }

                    HitZone::HScrollbar { vt } => {
                        // — EchoFrame: sub-hit-test horizontal scrollbar widget
                        let sub_zone = self.hscrollbars[vt].hit_test(x, y);
                        let sb_state = terminal::get_scrollbar_state(vt);
                        let (total_w, visible_w, cur_offset) = if let Some(s) = sb_state {
                            (s.max_line_width, s.cols as usize, s.h_scroll_offset)
                        } else {
                            (0, 0, 0)
                        };
                        let scrollable = total_w.saturating_sub(visible_w);

                        match sub_zone {
                            ScrollbarHitZone::ArrowDec => {
                                self.hscrollbars[vt].arrow_dec_state = PartState::Pressed;
                                if cur_offset > 0 {
                                    terminal::scroll_to_col(vt, cur_offset - 1);
                                }
                                mark_dirty(vt);
                                MouseAction::Consumed
                            }
                            ScrollbarHitZone::ArrowInc => {
                                self.hscrollbars[vt].arrow_inc_state = PartState::Pressed;
                                if cur_offset < scrollable {
                                    terminal::scroll_to_col(vt, cur_offset + 1);
                                }
                                mark_dirty(vt);
                                MouseAction::Consumed
                            }
                            ScrollbarHitZone::TrackBefore => {
                                let page = visible_w.max(1);
                                let new_off = cur_offset.saturating_sub(page);
                                terminal::scroll_to_col(vt, new_off);
                                mark_dirty(vt);
                                MouseAction::Consumed
                            }
                            ScrollbarHitZone::TrackAfter => {
                                let page = visible_w.max(1);
                                let new_off = (cur_offset + page).min(scrollable);
                                terminal::scroll_to_col(vt, new_off);
                                mark_dirty(vt);
                                MouseAction::Consumed
                            }
                            ScrollbarHitZone::Thumb => {
                                self.event_handler.state = MouseState::ScrollbarDrag;
                                self.hscrollbars[vt].thumb_state = PartState::Pressed;
                                let track_w = self.hscrollbars[vt].track_pixel_length() as usize;
                                self.event_handler.drag = Some(DragState {
                                    vt,
                                    vertical: false,
                                    start_pos: x,
                                    start_offset: cur_offset,
                                    track_length: track_w,
                                    total_content: total_w,
                                    visible_content: visible_w,
                                });
                                mark_dirty(vt);
                                MouseAction::Consumed
                            }
                            _ => {
                                // — EchoFrame: corner/none hits consume without drag.
                                // Same fix as vertical — don't phantom-drag on stray clicks.
                                // — GlassSignal
                                MouseAction::Consumed
                            }
                        }
                    }

                    HitZone::ScrollbarCorner { .. } | HitZone::Border => {
                        MouseAction::Consumed
                    }

                    HitZone::VtContent { vt } => {
                        self.event_handler.state = MouseState::ContentPress;
                        MouseAction::ForwardToTerminal { vt }
                    }

                    HitZone::None => MouseAction::Nothing,
                }
            }

            MouseButton::Middle => {
                self.event_handler.middle_pressed = true;
                match zone {
                    HitZone::VtContent { vt } => MouseAction::ForwardToTerminal { vt },
                    _ => MouseAction::Nothing,
                }
            }

            MouseButton::Right => {
                match zone {
                    HitZone::VtContent { vt } => MouseAction::ForwardToTerminal { vt },
                    _ => MouseAction::Nothing,
                }
            }
        }
    }

    /// — GlassSignal: handle mouse button release.
    fn handle_mouse_release(&mut self, button: MouseButton, _x: i32, _y: i32) -> MouseAction {
        match button {
            MouseButton::Left => {
                self.event_handler.left_pressed = false;
                let was_dragging = self.event_handler.state == MouseState::ScrollbarDrag;
                let drag_vt = self.event_handler.drag.as_ref().map(|d| d.vt);
                self.event_handler.state = MouseState::Idle;
                self.event_handler.drag = None;

                // — EchoFrame: reset all scrollbar visual states on release
                for sb in self.vscrollbars.iter_mut() {
                    sb.reset_states();
                }
                for sb in self.hscrollbars.iter_mut() {
                    sb.reset_states();
                }
                if let Some(vt) = drag_vt {
                    mark_dirty(vt);
                }
                SCROLLBAR_DIRTY.store(true, Ordering::Release);

                if was_dragging {
                    MouseAction::Consumed
                } else {
                    // — GlassSignal: was content press — let console.rs finish selection
                    MouseAction::ForwardToTerminal { vt: self.layout.focused_vt() }
                }
            }

            MouseButton::Middle => {
                self.event_handler.middle_pressed = false;
                MouseAction::Nothing
            }

            MouseButton::Right => MouseAction::Nothing,
        }
    }

    /// — GlassSignal: handle mouse motion. Drags scrollbars, hovers, or forwards to terminal.
    fn handle_mouse_move(&mut self, x: i32, y: i32) -> MouseAction {
        match self.event_handler.state {
            MouseState::ScrollbarDrag => {
                if let Some(ref drag) = self.event_handler.drag {
                    let vt = drag.vt;
                    let scrollable = drag.total_content.saturating_sub(drag.visible_content);
                    if scrollable == 0 || drag.track_length == 0 {
                        return MouseAction::Consumed;
                    }

                    if drag.vertical {
                        // — EchoFrame: vertical drag — delta Y maps to scroll lines
                        let delta_px = y - drag.start_pos;
                        let delta_lines = (delta_px as i64 * scrollable as i64) / drag.track_length as i64;
                        let new_offset = (drag.start_offset as i64 - delta_lines)
                            .max(0).min(scrollable as i64) as usize;
                        terminal::scroll_to_line(vt, new_offset);
                    } else {
                        // — EchoFrame: horizontal drag — delta X maps to scroll columns
                        let delta_px = x - drag.start_pos;
                        let delta_cols = (delta_px as i64 * drag.total_content as i64) / drag.track_length as i64;
                        let max_scroll = drag.total_content.saturating_sub(drag.visible_content);
                        let new_offset = (drag.start_offset as i64 + delta_cols)
                            .max(0).min(max_scroll as i64) as usize;
                        terminal::scroll_to_col(vt, new_offset);
                    }
                    mark_dirty(vt);
                    SCROLLBAR_DIRTY.store(true, Ordering::Release);
                    return MouseAction::Consumed;
                }
                MouseAction::Nothing
            }

            MouseState::ContentPress => {
                MouseAction::ForwardToTerminal { vt: self.layout.focused_vt() }
            }

            MouseState::Idle => {
                // — EchoFrame: update hover states on scrollbar widgets
                self.update_hover_states(x, y);
                // — InputShade: update vkbd key hover when keyboard is visible.
                // Cheap: hit_test is pure math, only marks dirty if hover changed.
                if vkbd::is_visible() {
                    vkbd::update_hover(x, y);
                }
                MouseAction::Nothing
            }
        }
    }

    /// — EchoFrame: update hover visual states for scrollbar widgets.
    /// Called on idle mouse move. Only marks dirty if state actually changed.
    fn update_hover_states(&mut self, x: i32, y: i32) {
        let tiles = self.visible_vts();
        let zone = events::hit_test(
            x, y, &self.vt_geometries, &self.vt_scrollbar_flags, &tiles,
        );

        // — EchoFrame: snapshot ALL states BEFORE any changes. Compare after.
        // Old approach reset first then compared — but that meant "old" was always
        // Normal, so any hover = "changed" = SCROLLBAR_DIRTY every mouse move = 2 FPS.
        type StateTriple = (PartState, PartState, PartState);
        let mut old_vstates: [StateTriple; MAX_VTS] = [(PartState::Normal, PartState::Normal, PartState::Normal); MAX_VTS];
        let mut old_hstates: [StateTriple; MAX_VTS] = [(PartState::Normal, PartState::Normal, PartState::Normal); MAX_VTS];
        for (i, sb) in self.vscrollbars.iter().enumerate() {
            old_vstates[i] = (sb.arrow_dec_state, sb.arrow_inc_state, sb.thumb_state);
        }
        for (i, sb) in self.hscrollbars.iter().enumerate() {
            old_hstates[i] = (sb.arrow_dec_state, sb.arrow_inc_state, sb.thumb_state);
        }

        // — EchoFrame: reset all scrollbar states, then set the one being hovered
        for sb in self.vscrollbars.iter_mut() {
            sb.reset_states();
        }
        for sb in self.hscrollbars.iter_mut() {
            sb.reset_states();
        }

        match zone {
            HitZone::VScrollbar { vt } => {
                let sub = self.vscrollbars[vt].hit_test(x, y);
                match sub {
                    ScrollbarHitZone::ArrowDec => self.vscrollbars[vt].arrow_dec_state = PartState::Hovered,
                    ScrollbarHitZone::ArrowInc => self.vscrollbars[vt].arrow_inc_state = PartState::Hovered,
                    ScrollbarHitZone::Thumb => self.vscrollbars[vt].thumb_state = PartState::Hovered,
                    _ => {}
                }
            }
            HitZone::HScrollbar { vt } => {
                let sub = self.hscrollbars[vt].hit_test(x, y);
                match sub {
                    ScrollbarHitZone::ArrowDec => self.hscrollbars[vt].arrow_dec_state = PartState::Hovered,
                    ScrollbarHitZone::ArrowInc => self.hscrollbars[vt].arrow_inc_state = PartState::Hovered,
                    ScrollbarHitZone::Thumb => self.hscrollbars[vt].thumb_state = PartState::Hovered,
                    _ => {}
                }
            }
            _ => {}
        }

        // — EchoFrame: compare final states against pre-reset snapshots.
        // Only mark dirty if something actually changed visually.
        let mut changed = false;
        for (i, sb) in self.vscrollbars.iter().enumerate() {
            if (sb.arrow_dec_state, sb.arrow_inc_state, sb.thumb_state) != old_vstates[i] {
                changed = true;
                break;
            }
        }
        if !changed {
            for (i, sb) in self.hscrollbars.iter().enumerate() {
                if (sb.arrow_dec_state, sb.arrow_inc_state, sb.thumb_state) != old_hstates[i] {
                    changed = true;
                    break;
                }
            }
        }

        if changed {
            SCROLLBAR_DIRTY.store(true, Ordering::Release);
        }
    }

    /// — GlassSignal: handle mouse wheel. Shift+wheel = horizontal scroll.
    fn handle_mouse_wheel(&mut self, delta: i32, _x: i32, _y: i32, shift_held: bool) -> MouseAction {
        let vt = self.layout.focused_vt();
        let scroll_lines = (delta.unsigned_abs() as usize) * 3;

        if shift_held {
            // — GlassSignal: shift+wheel = horizontal scroll
            if delta > 0 {
                terminal::scroll_left(scroll_lines);
            } else {
                terminal::scroll_right(scroll_lines);
            }
            mark_dirty(vt);
            MouseAction::Consumed
        } else {
            // — GlassSignal: normal wheel = vertical scroll (handled by terminal in console.rs)
            // Return Nothing so console.rs can decide based on mouse mode
            MouseAction::Nothing
        }
    }
}

/// — EchoFrame: static fill_rect that doesn't need &self — used by scrollbar widget
/// draw callbacks. Same pixel-format-aware logic as Compositor::fill_hw_rect.
///
/// — SableWire: row-batched MMIO writes. The old per-pixel path did W×H individual
/// MMIO writes at ~1000 cycles each. A 16×400 scrollbar track = 6,400 writes = 6.4M
/// cycles IN THE TIMER ISR. Now we fill one row in a stack buffer (RAM, ~1 cycle/pixel),
/// then blast the whole row to MMIO in one copy_nonoverlapping. Reduces MMIO transaction
/// count by W× (16× for scrollbars). The difference between "works" and "deadlock."
fn fill_hw_rect_static(hw_fb: &dyn Framebuffer, x: usize, y: usize, w: usize, h: usize, color_argb: u32) {
    let bpp = hw_fb.format().bytes_per_pixel() as usize;
    let dst_ptr = hw_fb.buffer();
    let dst_stride = hw_fb.stride() as usize;
    let screen_w = hw_fb.width() as usize;
    let screen_h = hw_fb.height() as usize;

    let x_end = (x + w).min(screen_w);
    let y_end = (y + h).min(screen_h);
    if x >= screen_w || y >= screen_h { return; }
    let actual_w = x_end - x;
    if actual_w == 0 || y >= y_end { return; }

    let pixel_bytes = match hw_fb.format() {
        fb::PixelFormat::BGRA8888 => [
            (color_argb & 0xFF) as u8,
            ((color_argb >> 8) & 0xFF) as u8,
            ((color_argb >> 16) & 0xFF) as u8,
            ((color_argb >> 24) & 0xFF) as u8,
        ],
        _ => [
            ((color_argb >> 16) & 0xFF) as u8,
            ((color_argb >> 8) & 0xFF) as u8,
            (color_argb & 0xFF) as u8,
            ((color_argb >> 24) & 0xFF) as u8,
        ],
    };

    // — SableWire: build one row of pixels in stack RAM, then copy to MMIO per row.
    // 256px × 4bpp = 1024 bytes on stack — covers any scrollbar/widget width.
    // For wider rects, we tile the row buffer. Scrollbars are always 16px wide.
    const MAX_ROW_PX: usize = 256;
    let row_px = actual_w.min(MAX_ROW_PX);
    let mut row_buf = [0u8; MAX_ROW_PX * 4];
    let pb = bpp.min(4);

    // — SableWire: stamp the pixel pattern into the row template
    for col in 0..row_px {
        let off = col * bpp;
        row_buf[off..off + pb].copy_from_slice(&pixel_bytes[..pb]);
    }
    let row_bytes = row_px * bpp;

    unsafe {
        for row in y..y_end {
            let dst_offset = row * dst_stride + x * bpp;
            let dst = dst_ptr.add(dst_offset);

            if actual_w <= MAX_ROW_PX {
                // — SableWire: common case — entire row fits in one blast
                core::ptr::copy_nonoverlapping(row_buf.as_ptr(), dst, row_bytes);
            } else {
                // — SableWire: wide rect — tile the row buffer across the width
                let mut remaining = actual_w;
                let mut col_off = 0usize;
                while remaining > 0 {
                    let chunk = remaining.min(MAX_ROW_PX);
                    let chunk_bytes = chunk * bpp;
                    core::ptr::copy_nonoverlapping(
                        row_buf.as_ptr(),
                        dst.add(col_off * bpp),
                        chunk_bytes,
                    );
                    col_off += chunk;
                    remaining -= chunk;
                }
            }
        }
    }
}

// ============================================================================
// Public API — called from kernel init, VT switch, terminal write, timer tick
// ============================================================================

/// Initialize the compositor. Called once during kernel boot after fb::init_from_boot().
/// Returns all VT backing framebuffers for eager terminal initialization.
/// — SableWire: no more lazy allocation — every VT is ready to render from boot.
pub fn init(hw_fb: Arc<dyn Framebuffer>) -> [Option<Arc<dyn Framebuffer>>; MAX_VTS] {
    let mut compositor = Compositor::new(hw_fb);
    let mut fbs: [Option<Arc<dyn Framebuffer>>; MAX_VTS] = core::array::from_fn(|_| None);
    for i in 0..MAX_VTS {
        fbs[i] = compositor.get_vt_framebuffer(i);
    }
    *COMPOSITOR.lock() = Some(compositor);
    FULL_REDRAW.store(true, Ordering::Release);
    // — SoftGlyph: set lock-free flag so ISR mouse processing knows cursor exists
    MOUSE_INITIALIZED.store(true, Ordering::Release);
    os_log::println!("[COMP] compositor initialized (all {} VT buffers pre-allocated)", MAX_VTS);
    fbs
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

/// — GlassSignal: mark status bar as dirty (VT switch, OSK toggle). — SableWire
#[inline]
pub fn mark_statusbar_dirty() {
    STATUSBAR_DIRTY.store(true, Ordering::Release);
}

/// — SableWire: flush deferred layout change. Call from non-ISR context (e.g. after
/// OSK toggle from userspace). This processes the terminal resize + SIGWINCH that
/// tick() couldn't do because it runs in timer ISR with blocking locks forbidden.
pub fn flush_pending_layout() {
    if PENDING_LAYOUT_CHANGE.swap(false, Ordering::AcqRel) {
        if let Some(ref mut compositor) = *COMPOSITOR.lock() {
            compositor.apply_layout_change();
            request_full_redraw();
        }
    }
}

/// — EchoFrame: ISR-safe version of flush_pending_layout. Uses try_lock on both
/// COMPOSITOR and VT_TERMINALS (via terminal::try_resize_vt). If any lock is
/// contended, re-arms PENDING_LAYOUT_CHANGE for retry next tick. The geometry is
/// already correct (recomputed in tick), this just tells the terminal emulators
/// about the new dimensions so apps render to the right row/col count.
pub fn try_flush_pending_layout() {
    if !PENDING_LAYOUT_CHANGE.load(Ordering::Acquire) {
        return;
    }
    match COMPOSITOR.try_lock() {
        Some(mut guard) => {
            if let Some(ref mut compositor) = *guard {
                // — EchoFrame: geometry already recomputed in tick(). Walk visible VTs
                // and try to resize their terminal emulators. try_resize_vt uses try_lock
                // on VT_TERMINALS — if contended, we'll retry next tick.
                let tile_count = compositor.layout.tile_count();
                // — GlassSignal: use cached viewports instead of recomputing
                let viewports = compositor.cached_viewports;
                let mut all_resized = true;
                for slot_idx in 0..tile_count {
                    let (vt_idx, _) = viewports[slot_idx];
                    if vt_idx >= MAX_VTS { continue; }
                    if let Some(geom) = compositor.vt_geometries[vt_idx] {
                        if let Some(ref buf) = compositor.vt_buffers[vt_idx] {
                            if !terminal::try_resize_vt(vt_idx, buf.clone() as Arc<dyn Framebuffer>) {
                                all_resized = false;
                            } else {
                                // — EchoFrame: notify VT layer about new winsize + SIGWINCH
                                unsafe {
                                    if let Some(cb) = WINSIZE_CALLBACK {
                                        cb(
                                            vt_idx,
                                            geom.text_rows as u16,
                                            geom.text_cols as u16,
                                            geom.usable_width as u16,
                                            geom.usable_height as u16,
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
                if all_resized {
                    PENDING_LAYOUT_CHANGE.store(false, Ordering::Release);
                }
                // — EchoFrame: if not all resized, PENDING_LAYOUT_CHANGE stays true → retry
            }
        }
        None => {
            // — EchoFrame: compositor lock contended — retry next tick
        }
    }
}

/// Composite all dirty VTs onto the hardware framebuffer.
/// Called at ~100 Hz from timer ISR. Only actually composites when something changed.
/// — SableWire: this is the only function that touches the hardware framebuffer.
pub fn tick() {
    // — SableWire: fast path — consume all dirty flags atomically.
    // When nothing is dirty, bail immediately — zero cost idle path.
    let full_r = FULL_REDRAW.swap(false, Ordering::AcqRel);
    let cursor_d = CURSOR_DIRTY.swap(false, Ordering::AcqRel);
    let vkbd_d = vkbd::take_dirty();
    let vkbd_hover_d = vkbd::take_hover_dirty();
    let vt_dirty = VT_DIRTY.iter().any(|d| d.load(Ordering::Acquire));
    let sb_dirty = SCROLLBAR_DIRTY.swap(false, Ordering::AcqRel);
    let statusbar_d = STATUSBAR_DIRTY.swap(false, Ordering::AcqRel);
    let any_dirty = full_r || cursor_d || vkbd_d || vkbd_hover_d || vt_dirty || sb_dirty || statusbar_d;

    if !any_dirty {
        return;
    }

    // — SableWire: content dirty = needs full composite (VT blit, borders, scrollbars).
    // Cursor-only, vkbd-only, or scrollbar-hover-only changes skip the expensive path.
    let content_dirty = full_r || vt_dirty;

    if let Some(mut guard) = COMPOSITOR.try_lock() {
        if let Some(ref mut compositor) = *guard {
            // — GlassSignal: update reserved_bottom if OSK toggled.
            // — SableWire: when reserved_bottom shrinks (OSK closed), the old keyboard
            // pixels linger on hw_fb. Force full composite so VT content reblits over
            // the dead zone. Without this, closing the OSK leaves ghost keys.
            // — SableWire: CRITICAL: do NOT call apply_layout_change() here — we're
            // in the timer ISR. apply_layout_change calls terminal::resize_vt which
            // does a blocking lock on VT_TERMINALS → instant deadlock if held.
            // BUT we CAN recompute geometries (pure math) and reposition scrollbar
            // widgets so the compositor blits to the right places immediately.
            // The terminal resize + SIGWINCH is still deferred to non-ISR context.
            // — EchoFrame: OSK goes UNDER the tileable area. VT viewports shrink
            // when OSK opens, scrollbars shrink with them. No occlusion.
            let mut force_composite = false;
            if vkbd_d || full_r {
                let new_reserved = STATUSBAR_HEIGHT + vkbd::keyboard_height();
                if compositor.layout.set_reserved_bottom(new_reserved) {
                    force_composite = true;
                    // — EchoFrame: recompute viewport geometries so VTs shrink/grow
                    // to accommodate the OSK. Pure math — no locks, no allocs.
                    compositor.vt_geometries = compositor.layout.recompute_geometries(
                        compositor.cell_width, compositor.cell_height,
                        &compositor.vt_scrollbar_flags,
                    );
                    // — GlassSignal: viewport cache must follow new geometry
                    compositor.invalidate_viewport_cache();
                    // — EchoFrame: scrollbar rects must follow the new geometry
                    compositor.update_scrollbar_rects();
                    // — SableWire: mark all VTs dirty so the full blit covers the old OSK area
                    for d in VT_DIRTY.iter() {
                        d.store(true, Ordering::Release);
                    }
                    // — SableWire: defer terminal resize + SIGWINCH to non-ISR context
                    PENDING_LAYOUT_CHANGE.store(true, Ordering::Release);
                }
            }

            // — NeonVale: reset dirty rect at start of frame. Every draw call
            // extends it. At the end, flush only the bounding box of actual changes.
            compositor.dirty_rect.reset();

            // — NeonVale: full redraw = mark entire screen dirty upfront.
            // No point tracking individual rects when we're repainting everything.
            if full_r || force_composite {
                compositor.dirty_rect.mark_full(
                    compositor.hw_fb.width(),
                    compositor.hw_fb.height(),
                );
            }

            // — NeonVale: ALWAYS composite. The dirty rect tracks what actually changed
            // and the GPU flush is regional — so skipping the VT blit saves nothing
            // meaningful. But skipping it BREAKS the mouse cursor because save_buffer
            // goes stale between frames (VT content changes, boot text overwrites, etc.)
            // and cursor.erase() restores the stale pixels → massive trails.
            // Linux XFree86 had the same bug in 1998. The fix is the same: always
            // composite, let the damage tracker handle efficiency. — NeonVale
            compositor.composite(full_r || force_composite, sb_dirty);
            // — GlassSignal: status bar renders below VT content, above OSK.
            // Always redrawn on full_r, VT switch, OSK toggle, or statusbar_d flag.
            // force_composite means layout changed — must repaint status bar too. — SableWire
            if full_r || statusbar_d || vkbd_d || force_composite {
                compositor.draw_statusbar();
            }
            // — InputShade: draw virtual keyboard overlay after status bar.
            // Full repaint on toggle/press or full redraw — the VT blit is clipped to
            // avoid overwriting the keyboard area, so the overlay persists on hw_fb.
            // Hover-only changes use the fast path: repaint just 2 keys, not all ~100.
            if vkbd::is_visible() {
                if vkbd_d || full_r {
                    // — NeonVale: vkbd overlay covers entire keyboard area
                    let screen_h = compositor.hw_fb.height();
                    let vkbd_h = vkbd::keyboard_height();
                    let kb_top = screen_h.saturating_sub(STATUSBAR_HEIGHT + vkbd_h);
                    compositor.dirty_rect.extend(0, kb_top, compositor.hw_fb.width(), vkbd_h);
                    vkbd::draw_overlay(&*compositor.hw_fb);
                } else if vkbd_hover_d {
                    // — NeonVale: hover-only vkbd change — small area, still need to track
                    let screen_h = compositor.hw_fb.height();
                    let vkbd_h = vkbd::keyboard_height();
                    let kb_top = screen_h.saturating_sub(STATUSBAR_HEIGHT + vkbd_h);
                    compositor.dirty_rect.extend(0, kb_top, compositor.hw_fb.width(), vkbd_h);
                    vkbd::redraw_hover_keys(&*compositor.hw_fb);
                }
            }
            // — SoftGlyph: mouse cursor last — the final layer before GPU flush
            if let Some(ref mut cursor) = compositor.mouse_cursor {
                // — NeonVale: track cursor redraw area for regional flush
                let (cx, cy, cw, ch) = cursor.bounds();
                compositor.dirty_rect.extend(cx, cy, cw, ch);
                cursor.redraw(&*compositor.hw_fb);
            }
            // — NeonVale: REGIONAL flush — only the bounding box of actual changes
            // gets pushed through VirtIO-GPU. A cursor blink goes from flushing 3MB
            // (full 1024x768x4) to flushing ~2KB (16x16 cursor area). The GPU
            // transfer_to_host_2d + resource_flush commands specify exact subrect.
            let screen_w = compositor.hw_fb.width();
            let screen_h = compositor.hw_fb.height();
            if let Some((fx, fy, fw, fh)) = compositor.dirty_rect.flush_region(screen_w, screen_h) {
                compositor.hw_fb.flush_region(fx, fy, fw, fh);
            }
        }
    }
}

/// Switch focus to a VT. Updates layout manager, recomputes geometries,
/// resizes VFBs, and triggers redraw.
/// Called from Alt+Fn keyboard shortcut.
pub fn focus_vt(vt_num: usize) {
    if let Some(ref mut compositor) = *COMPOSITOR.lock() {
        // — SableWire: consume deferred layout change from tick() ISR context.
        // Now safe to call apply_layout_change — we're not in ISR, blocking locks OK.
        PENDING_LAYOUT_CHANGE.swap(false, Ordering::AcqRel);
        compositor.layout.focus_vt(vt_num);
        compositor.apply_layout_change();
        COMPOSITOR_FOCUS_VT.store(compositor.layout.focused_vt(), Ordering::Release);
        request_full_redraw();
        mark_statusbar_dirty();
    }
}

/// — GraveShift: ISR-safe VT focus switch. Uses try_lock on COMPOSITOR.
/// Skips apply_layout_change (which calls terminal::resize_vt → VT_TERMINALS.lock())
/// to avoid deadlock. Instead just updates focus index and requests full redraw.
/// The geometry is already computed for all 6 VTs — switching focus in fullscreen
/// mode doesn't change geometry, just which VT gets blitted.
/// Returns false if COMPOSITOR lock is contended — caller should retry next tick.
pub fn try_focus_vt(vt_num: usize) -> bool {
    match COMPOSITOR.try_lock() {
        Some(mut guard) => {
            if let Some(ref mut compositor) = *guard {
                compositor.layout.focus_vt(vt_num);
                // — GraveShift: skip apply_layout_change() — it calls terminal::resize_vt
                // which uses blocking .lock(). In fullscreen mode, geometry doesn't change
                // on VT switch. If we're in split mode and geometry matters, it'll get
                // fixed up on the next non-ISR focus_vt call or layout change.
                // — EchoFrame: BUT we must recompute geometries + reposition scrollbar
                // widgets for the new VT. recompute_geometries and update_scrollbar_rects
                // are pure math — no locks, no allocs, ISR-safe. Without this, only the
                // VT visible at boot gets geometry/scrollbar rects; every other VT's
                // scrollbars stay at (0,0,0,0) and draw() bails silently.
                compositor.vt_geometries = compositor.layout.recompute_geometries(
                    compositor.cell_width, compositor.cell_height, &compositor.vt_scrollbar_flags,
                );
                // — GlassSignal: viewport cache must follow geometry recomputation
                compositor.invalidate_viewport_cache();
                compositor.update_scrollbar_rects();
                COMPOSITOR_FOCUS_VT.store(compositor.layout.focused_vt(), Ordering::Release);
                request_full_redraw();
                mark_statusbar_dirty();
            }
            true
        }
        None => false,
    }
}

/// Get the currently focused VT index (lock-free, ISR-safe).
#[inline]
pub fn focused_vt() -> usize {
    COMPOSITOR_FOCUS_VT.load(Ordering::Acquire)
}

/// Set the tiling layout. Recomputes geometries, resizes VFBs, triggers redraw.
pub fn set_layout(layout: Layout) {
    if let Some(ref mut compositor) = *COMPOSITOR.lock() {
        compositor.layout.set_layout(layout);
        compositor.apply_layout_change();
        COMPOSITOR_FOCUS_VT.store(compositor.layout.focused_vt(), Ordering::Release);
        request_full_redraw();
        os_log::println!("[COMP] layout={:?} tiles={}", layout, compositor.layout.tile_count());
    }
}

/// Toggle fullscreen ↔ last split layout (Alt+Enter).
pub fn toggle_fullscreen() {
    if let Some(ref mut compositor) = *COMPOSITOR.lock() {
        compositor.layout.toggle_fullscreen();
        compositor.apply_layout_change();
        COMPOSITOR_FOCUS_VT.store(compositor.layout.focused_vt(), Ordering::Release);
        request_full_redraw();
        os_log::println!("[COMP] toggle → {:?}", compositor.layout.layout());
    }
}

/// Cycle focus to next visible tile (Alt+Tab).
pub fn cycle_focus() {
    if let Some(ref mut compositor) = *COMPOSITOR.lock() {
        compositor.layout.cycle_focus();
        compositor.apply_layout_change();
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

/// — GlassSignal: update scrollbar flags for a VT based on terminal state.
/// Called after wrap mode toggle — may trigger geometry recomputation + VFB resize
/// if horizontal scrollbar visibility changes.
pub fn update_scrollbar_flags(vt_num: usize) {
    if let Some(ref mut compositor) = *COMPOSITOR.lock() {
        if vt_num >= MAX_VTS { return; }
        let old_flags = compositor.vt_scrollbar_flags[vt_num];
        let mut new_flags = ScrollbarFlags { vscroll: true, hscroll: false };

        // — GlassSignal: horizontal scrollbar only when wrap OFF and content wider than viewport
        if let Some(state) = terminal::get_scrollbar_state(vt_num) {
            if !state.wrap_mode && state.max_line_width > state.cols as usize {
                new_flags.hscroll = true;
            }
        }

        compositor.vt_scrollbar_flags[vt_num] = new_flags;

        // — GlassSignal: if flags changed, recompute geometry (VFB resize)
        if old_flags.hscroll != new_flags.hscroll {
            compositor.apply_layout_change();
            request_full_redraw();
        }
    }
}

/// Get viewport info for a VT (used by /dev/fb0 ioctl to report resolution).
/// — GlassSignal: legacy API, returns raw Viewport for backward compat
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

/// Get the full ViewportGeometry for a VT. None if VT is off-screen.
/// — GlassSignal: the real API. fb0, terminal resize, and winsize all use this.
pub fn get_vt_geometry(vt_num: usize) -> Option<ViewportGeometry> {
    let guard = COMPOSITOR.lock();
    let compositor = guard.as_ref()?;
    if vt_num < MAX_VTS {
        compositor.vt_geometries[vt_num]
    } else {
        None
    }
}

/// Check if a VT is currently visible on screen.
/// — GlassSignal: lock-free would be nicer but geometry changes are rare
pub fn is_vt_visible(vt_num: usize) -> bool {
    let guard = COMPOSITOR.lock();
    guard.as_ref()
        .map(|c| vt_num < MAX_VTS && c.vt_geometries[vt_num].is_some())
        .unwrap_or(false)
}

/// Get VFB dimensions for a VT (usable area). For fb0 ioctl and terminal init.
/// Returns (width, height) — usable pixels, not including chrome.
/// Falls back to full screen size if VT has no geometry (off-screen).
pub fn get_vfb_dimensions(vt_num: usize) -> (u32, u32) {
    let guard = COMPOSITOR.lock();
    if let Some(compositor) = guard.as_ref() {
        if vt_num < MAX_VTS {
            if let Some(geom) = compositor.vt_geometries[vt_num] {
                return (geom.usable_width, geom.usable_height);
            }
        }
        (compositor.hw_fb.width(), compositor.hw_fb.height())
    } else {
        (0, 0)
    }
}

/// Get a VT's VFB info for /dev/fb0 redirection.
/// — GlassSignal: returns (base_ptr, size, width, height, stride, bpp, is_bgr).
/// The kernel's memory module constructs FramebufferDeviceInfo from these.
/// Returns None if VT has no buffer.
pub fn get_vfb_info_raw(vt_num: usize) -> Option<(usize, usize, u32, u32, u32, u32, bool)> {
    let guard = COMPOSITOR.lock();
    let compositor = guard.as_ref()?;
    if vt_num >= MAX_VTS { return None; }
    let buf = compositor.vt_buffers[vt_num].as_ref()?;
    let is_bgr = matches!(compositor.hw_fb.format(), fb::PixelFormat::BGRA8888);
    Some((
        buf.buffer() as usize,
        buf.size(),
        buf.width(),
        buf.height(),
        buf.stride(),
        compositor.hw_fb.format().bytes_per_pixel() as u32 * 8,
        is_bgr,
    ))
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
/// — GlassSignal: hot-swap the compositor's output target, recompute everything
pub fn update_hw_framebuffer(hw_fb: Arc<dyn Framebuffer>) {
    if let Some(ref mut compositor) = *COMPOSITOR.lock() {
        let w = hw_fb.width();
        let h = hw_fb.height();
        compositor.hw_fb = hw_fb;
        compositor.layout.update_screen_size(w, h);
        // — SoftGlyph: re-create cursor for new screen dimensions
        compositor.mouse_cursor = Some(fb::mouse::MouseCursor::new(w, h));
        compositor.apply_layout_change();
        request_full_redraw();
    }
}

// ============================================================================
// Event System — GlassSignal: the compositor as window manager
// ============================================================================
// Console.rs forwards raw mouse events here. The compositor hit-tests against
// its known geometry (viewports, scrollbars, borders) and returns what to do.
// No more geometry math in console.rs — the compositor owns all screen layout.

/// Re-export event types for console.rs
pub use events::{MouseAction, MouseButton};

/// Handle a mouse button press at screen coordinates (x, y).
/// — GlassSignal: ISR-safe via try_lock. Returns Consumed if compositor handled it.
pub fn handle_mouse_press(button: MouseButton, x: i32, y: i32) -> MouseAction {
    if let Some(mut guard) = COMPOSITOR.try_lock() {
        if let Some(ref mut compositor) = *guard {
            return compositor.handle_mouse_press(button, x, y);
        }
    }
    MouseAction::Nothing
}

/// Handle a mouse button release at screen coordinates (x, y).
pub fn handle_mouse_release(button: MouseButton, x: i32, y: i32) -> MouseAction {
    if let Some(mut guard) = COMPOSITOR.try_lock() {
        if let Some(ref mut compositor) = *guard {
            return compositor.handle_mouse_release(button, x, y);
        }
    }
    MouseAction::Nothing
}

/// Handle mouse motion to screen coordinates (x, y).
/// — GlassSignal: during scrollbar drag, compositor handles it entirely.
/// During content press, forwards to terminal for selection tracking.
pub fn handle_mouse_move(x: i32, y: i32) -> MouseAction {
    if let Some(mut guard) = COMPOSITOR.try_lock() {
        if let Some(ref mut compositor) = *guard {
            return compositor.handle_mouse_move(x, y);
        }
    }
    MouseAction::Nothing
}

/// Handle mouse wheel at screen coordinates.
/// — GlassSignal: shift+wheel = horizontal scroll (compositor handles).
/// Normal wheel without shift = vertical scroll (console.rs decides based on mouse mode).
pub fn handle_mouse_wheel(delta: i32, x: i32, y: i32, shift_held: bool) -> MouseAction {
    if let Some(mut guard) = COMPOSITOR.try_lock() {
        if let Some(ref mut compositor) = *guard {
            return compositor.handle_mouse_wheel(delta, x, y, shift_held);
        }
    }
    MouseAction::Nothing
}

/// Hit-test a screen coordinate. Returns which zone (VT content, scrollbar, etc.).
/// — GlassSignal: useful for cursor shape changes (future: resize cursors on borders)
pub fn hit_test(x: i32, y: i32) -> HitZone {
    if let Some(guard) = COMPOSITOR.try_lock() {
        if let Some(ref compositor) = *guard {
            let tiles = compositor.visible_vts();
            return events::hit_test(
                x, y, &compositor.vt_geometries, &compositor.vt_scrollbar_flags, &tiles,
            );
        }
    }
    HitZone::None
}

/// Check if the compositor is currently tracking a scrollbar drag.
/// — GlassSignal: console.rs uses this to suppress selection during drag.
pub fn is_dragging_scrollbar() -> bool {
    if let Some(guard) = COMPOSITOR.try_lock() {
        if let Some(ref compositor) = *guard {
            return compositor.event_handler.state == MouseState::ScrollbarDrag;
        }
    }
    false
}

// ============================================================================
// Mouse Cursor — SoftGlyph: compositor owns the cursor, draws it last
// ============================================================================

/// Check if the mouse cursor is initialized.
/// — SoftGlyph: lock-free atomic check. The old try_lock() approach failed when
/// tick() held the compositor lock, causing the entire mouse input block to be
/// skipped. This was the root cause of the invisible mouse cursor.
#[inline]
pub fn mouse_initialized() -> bool {
    MOUSE_INITIALIZED.load(Ordering::Acquire)
}

/// Move mouse cursor by relative delta.
/// — SoftGlyph: called from ISR context (terminal_tick), ISR-safe via try_lock.
pub fn mouse_move(dx: i32, dy: i32) {
    if dx == 0 && dy == 0 { return; }
    if let Some(mut guard) = COMPOSITOR.try_lock() {
        if let Some(ref mut compositor) = *guard {
            if let Some(ref mut cursor) = compositor.mouse_cursor {
                // — NeonVale: Only update position — do NOT erase/redraw from ISR.
                // The erase writes stale save_buffer pixels (boot text, old VT content)
                // causing trails. tick() does a full composite + cursor.redraw() which
                // saves fresh pixels from the just-blitted VT content. This is the
                // standard deferred rendering model — ISR sets state, tick() paints.
                let new_x = (cursor.x + dx).clamp(0, cursor.screen_w - 1);
                let new_y = (cursor.y + dy).clamp(0, cursor.screen_h - 1);
                cursor.x = new_x;
                cursor.y = new_y;
            }
            CURSOR_DIRTY.store(true, Ordering::Release);
            // — NeonVale: Mark active VT dirty so composite() reblits VT content
            // over the cursor's old position. Without this, cursor.erase() writes
            // stale save_buffer pixels and no VT blit overwrites them → trails.
            let active = compositor.layout.focused_vt();
            VT_DIRTY[active].store(true, Ordering::Release);
        }
    }
}

/// Set mouse cursor to absolute position.
/// — SoftGlyph: for tablet devices (absolute coordinates from virtio-tablet).
/// Only marks dirty if position actually changed — tablets spam events even idle.
pub fn mouse_set_position(x: i32, y: i32) {
    if let Some(mut guard) = COMPOSITOR.try_lock() {
        if let Some(ref mut compositor) = *guard {
            if let Some(ref mut cursor) = compositor.mouse_cursor {
                let (old_x, old_y) = cursor.position();
                if x == old_x && y == old_y { return; }
                cursor.x = x.clamp(0, cursor.screen_w - 1);
                cursor.y = y.clamp(0, cursor.screen_h - 1);
                CURSOR_DIRTY.store(true, Ordering::Release);
                // — NeonVale: Force VT reblit to cover cursor erase area
                let active = compositor.layout.focused_vt();
                VT_DIRTY[active].store(true, Ordering::Release);
            }
        }
    }
}

/// Get current mouse position in screen pixels.
/// — SoftGlyph: ISR-safe via try_lock. Returns None if cursor not initialized.
pub fn mouse_position() -> Option<(i32, i32)> {
    if let Some(guard) = COMPOSITOR.try_lock() {
        if let Some(ref compositor) = *guard {
            if let Some(ref cursor) = compositor.mouse_cursor {
                return Some(cursor.position());
            }
        }
    }
    None
}

/// Get screen dimensions from the compositor's hardware framebuffer.
/// — SoftGlyph: for tablet coordinate scaling (0..32767 → screen pixels).
pub fn screen_dimensions() -> Option<(u32, u32)> {
    if let Some(guard) = COMPOSITOR.try_lock() {
        if let Some(ref compositor) = *guard {
            return Some((compositor.hw_fb.width(), compositor.hw_fb.height()));
        }
    }
    None
}

/// — GlassSignal: status bar click action. Returned by hit_test_statusbar(). — SableWire
pub enum StatusBarAction {
    /// Clicked on a VT button — switch to this VT (0-indexed)
    SwitchVt(usize),
    /// Clicked on the KB toggle button
    ToggleOSK,
    /// Click was in status bar area but didn't hit any button
    None,
}

/// — GlassSignal: hit-test click against the status bar region.
/// Returns the action to take, or None if the click missed the status bar.
/// Lock-free geometry check — button positions match draw_statusbar(). — SableWire
pub fn hit_test_statusbar(x: i32, y: i32) -> Option<StatusBarAction> {
    let guard = COMPOSITOR.try_lock()?;
    let compositor = guard.as_ref()?;
    let screen_w = compositor.hw_fb.width();
    let screen_h = compositor.hw_fb.height();

    let bar_y = screen_h.saturating_sub(STATUSBAR_HEIGHT);
    if (y as u32) < bar_y || y < 0 {
        return None; // — GlassSignal: click above status bar
    }

    let btn_w = 40u32;
    let btn_gap = 4u32;
    let mut btn_x = 8u32;
    let click_x = x as u32;

    // — GlassSignal: check VT buttons
    for vt in 0..MAX_VTS {
        if click_x >= btn_x && (click_x) < btn_x + btn_w {
            return Some(StatusBarAction::SwitchVt(vt));
        }
        btn_x += btn_w + btn_gap;
    }

    // — GlassSignal: check KB button (right side)
    let kb_x = screen_w - btn_w - 8;
    if click_x >= kb_x && (click_x) < kb_x + btn_w {
        return Some(StatusBarAction::ToggleOSK);
    }

    Some(StatusBarAction::None)
}

/// — GlassSignal: get status bar height in pixels. Always visible. — SableWire
#[inline]
pub fn statusbar_height() -> u32 {
    STATUSBAR_HEIGHT
}
